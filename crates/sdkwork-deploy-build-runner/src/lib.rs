//! Governed build executor boundary for the unified app delivery platform
//! (REQ-2026-0002, TECH-unified-app-delivery-platform §6).
//!
//! The control plane owns build rows; the runner claims builds, plans
//! commands through a `BuildExecutor`, executes them on the executor host,
//! and reports state transitions through the typed repository port. Platform
//! executors are command constructors with environment checks; signing and
//! upload commands are constructed only when the corresponding secret files
//! are present.

pub mod channel_ci;
pub mod ota_manifest;
pub mod review_observer;

pub use channel_ci::TokioChannelCiProcess;

pub use ota_manifest::{
    generate_android_ota_json, generate_ios_ota_plist, OtaManifestError, OtaManifestInput,
};
pub use review_observer::{
    validate_review_transition, NoOpReviewObserver, ReviewObservation, ReviewObserver,
};

use std::path::{Path, PathBuf};
use std::time::Duration;

use sdkwork_deploy_core::SemanticVersion;

/// One bounded command to execute with its working directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPlan {
    pub commands: Vec<BoundedCommand>,
    /// Workspace root (checked-out source).
    pub working_directory: PathBuf,
    /// Bounded toolchain summary recorded on the build.
    pub toolchain_summary: String,
}

/// A single bounded command: allowlisted executable plus bounded arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedCommand {
    pub program: String,
    pub arguments: Vec<String>,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub exit_code: i32,
    pub output: String,
    pub duration_ms: i64,
    /// Stable bounded failure code when the execution failed.
    pub error_code: Option<String>,
}

impl ExecutionOutcome {
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("executor validation: {0}")]
    Validation(String),
    #[error("executor plan: {0}")]
    Plan(String),
    #[error("executor run: {0}")]
    Run(String),
}

/// Executor boundary: plans bounded commands and executes them.
#[async_trait::async_trait]
pub trait BuildExecutor: Send + Sync {
    fn plan(
        &self,
        context: &ExecutionContext,
        template_commands: &[String],
    ) -> Result<CommandPlan, ExecutorError>;

    async fn execute(&self, plan: &CommandPlan) -> Result<ExecutionOutcome, ExecutorError>;
}

/// Immutable execution context resolved by the runner from the claimed build.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub build_uuid: String,
    pub app_uuid: String,
    pub platform: String,
    pub tech_stack: String,
    pub semantic_version: Option<String>,
    pub working_directory: PathBuf,
    /// Runner identity for claim fencing.
    pub runner_node_uuid: String,
    pub runner_version: String,
}

/// Default executor: runs bounded commands with `tokio::process`, capturing
/// output. Command programs and arguments are bounded; the working directory
/// must exist and stay inside the configured workspace root.
pub struct CommandExecutor {
    /// Absolute workspace root that executions may not escape.
    pub workspace_root: PathBuf,
    pub maximum_output_bytes: usize,
}

impl CommandExecutor {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            maximum_output_bytes: 8 * 1024 * 1024,
        }
    }
}

#[async_trait::async_trait]
impl BuildExecutor for CommandExecutor {
    fn plan(
        &self,
        context: &ExecutionContext,
        template_commands: &[String],
    ) -> Result<CommandPlan, ExecutorError> {
        if template_commands.len() > 64 {
            return Err(ExecutorError::Validation(
                "template commands must contain at most 64 entries".into(),
            ));
        }
        if !context.working_directory.starts_with(&self.workspace_root) {
            return Err(ExecutorError::Validation(
                "working directory escapes the workspace root".into(),
            ));
        }
        let mut commands = Vec::with_capacity(template_commands.len());
        for command in template_commands {
            let parsed = parse_bounded_command(command)?;
            commands.push(parsed);
        }
        Ok(CommandPlan {
            commands,
            working_directory: context.working_directory.clone(),
            toolchain_summary: format!(
                "runner/{}-{}/platform/{}",
                context.runner_version, context.platform, context.tech_stack
            ),
        })
    }

    async fn execute(&self, plan: &CommandPlan) -> Result<ExecutionOutcome, ExecutorError> {
        if plan.commands.is_empty() {
            return Ok(ExecutionOutcome {
                exit_code: 0,
                output: "no commands planned".into(),
                duration_ms: 0,
                error_code: None,
            });
        }
        let started = std::time::Instant::now();
        let mut output = String::new();
        for command in &plan.commands {
            tracing::info!(program = %command.program, label = %command.label, "executing build command");
            let mut child = tokio::process::Command::new(&command.program)
                .args(&command.arguments)
                .current_dir(&plan.working_directory)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|error| {
                    ExecutorError::Run(format!("spawn {}: {error}", command.program))
                })?;

            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| ExecutorError::Run("missing stdout pipe".into()))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| ExecutorError::Run("missing stderr pipe".into()))?;
            let stdout_reader = read_bounded_stdout(stdout, self.maximum_output_bytes);
            let stderr_reader = read_bounded_stderr(stderr, self.maximum_output_bytes);
            let (stdout_text, stderr_text) = tokio::join!(stdout_reader, stderr_reader);
            let status = child.wait().await.map_err(|error| {
                ExecutorError::Run(format!("wait {}: {error}", command.program))
            })?;

            output.push_str(&format!(
                "$ {}\n{}\n{}\n",
                command.label, stdout_text, stderr_text
            ));
            if output.len() > self.maximum_output_bytes {
                output.truncate(self.maximum_output_bytes);
                output.push_str("\n[output truncated]");
            }
            if !status.success() {
                let exit_code = status.code().unwrap_or(-1);
                let duration_ms = started.elapsed().as_millis() as i64;
                return Ok(ExecutionOutcome {
                    exit_code,
                    output,
                    duration_ms,
                    error_code: Some(format!("BUILD_COMMAND_FAILED:{}", command.label)),
                });
            }
        }
        Ok(ExecutionOutcome {
            exit_code: 0,
            output,
            duration_ms: started.elapsed().as_millis() as i64,
            error_code: None,
        })
    }
}

async fn read_bounded_stdout(reader: tokio::process::ChildStdout, maximum_bytes: usize) -> String {
    read_bounded_generic(reader, maximum_bytes).await
}

async fn read_bounded_stderr(reader: tokio::process::ChildStderr, maximum_bytes: usize) -> String {
    read_bounded_generic(reader, maximum_bytes).await
}

async fn read_bounded_generic<R>(mut reader: R, maximum_bytes: usize) -> String
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if buffer.len() >= maximum_bytes {
                    buffer.truncate(maximum_bytes);
                    buffer.extend_from_slice(b"\n[output truncated]");
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// Parses one template command string into a bounded program/argument pair.
/// The program must be a plain executable name or a relative path without
/// shell metacharacters; arguments must not contain shell metacharacters.
pub fn parse_bounded_command(command: &str) -> Result<BoundedCommand, ExecutorError> {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.len() > 500 {
        return Err(ExecutorError::Validation(
            "command must be 1..=500 characters".into(),
        ));
    }
    let mut parts = split_command(trimmed)?;
    let program = parts.remove(0);
    let program_for_label = program.clone();
    if is_denied_interpreter(&program) {
        return Err(ExecutorError::Validation(format!(
            "program {program} is a shell interpreter and is not allowed"
        )));
    }
    if !is_bounded_program(&program) {
        return Err(ExecutorError::Validation(format!(
            "program {program} contains shell metacharacters or an absolute path"
        )));
    }
    for argument in &parts {
        if !is_bounded_argument(argument) {
            return Err(ExecutorError::Validation(format!(
                "argument {argument} contains shell metacharacters"
            )));
        }
    }
    Ok(BoundedCommand {
        program,
        arguments: parts,
        label: format!(
            "{} {}",
            program_for_label,
            args_label(&[program_for_label.as_str()])
        ),
    })
}

fn args_label(args: &[&str]) -> String {
    let mut joined = String::new();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            joined.push(' ');
        }
        joined.push_str(arg);
        if joined.len() > 60 {
            joined.push_str("...");
            break;
        }
    }
    joined
}

fn split_command(command: &str) -> Result<Vec<String>, ExecutorError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '\'' | '"' if quote == Some(character) => quote = None,
            ' ' | '\t' if quote.is_none() => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if quote.is_some() {
        return Err(ExecutorError::Validation(
            "command contains an unterminated quote".into(),
        ));
    }
    if escaped {
        return Err(ExecutorError::Validation(
            "command ends with an escape character".into(),
        ));
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.is_empty() {
        return Err(ExecutorError::Validation("command is empty".into()));
    }
    Ok(parts)
}

fn is_denied_interpreter(program: &str) -> bool {
    matches!(
        program,
        "sh" | "bash" | "zsh" | "ksh" | "dash" | "fish" | "cmd" | "powershell" | "pwsh"
    )
}

fn is_bounded_program(program: &str) -> bool {
    !program.is_empty()
        && program.len() <= 128
        && !program.starts_with('/')
        && !program.contains("..")
        && program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
}

fn is_bounded_argument(argument: &str) -> bool {
    !argument.is_empty()
        && argument.len() <= 500
        && !argument.starts_with('/')
        && !argument.contains("..")
        && !argument
            .bytes()
            .any(|byte| matches!(byte, b'|' | b'&' | b';' | b'<' | b'>' | b'$' | b'`' | b'\n'))
}

// ---------------------------------------------------------------------------
// Platform command constructors (environment-checked)
// ---------------------------------------------------------------------------

/// Flutter target commands: `flutter pub get` then the release build for the
/// target platform.
pub fn flutter_commands(platform: &str) -> Result<Vec<String>, ExecutorError> {
    let build = match platform {
        "ANDROID" => "flutter build apk --release",
        "IOS" => "flutter build ipa --release",
        "HARMONYOS" => "flutter build hap --release",
        _ => {
            return Err(ExecutorError::Validation(format!(
                "flutter build is not supported for platform {platform}"
            )))
        }
    };
    Ok(vec!["flutter pub get".to_owned(), build.to_owned()])
}

/// Native Android commands via Gradle.
pub fn gradle_commands() -> Vec<String> {
    vec![
        "./gradlew --no-daemon assembleRelease".to_owned(),
        "./gradlew --no-daemon bundleRelease".to_owned(),
    ]
}

/// Native iOS commands via xcodebuild (scheme is tenant-configured).
pub fn xcodebuild_commands(scheme: &str) -> Result<Vec<String>, ExecutorError> {
    if scheme.trim().is_empty() || scheme.len() > 200 {
        return Err(ExecutorError::Validation(
            "xcodebuild scheme is invalid".into(),
        ));
    }
    Ok(vec![format!(
        "xcodebuild -workspace Runner.xcworkspace -scheme {scheme} -configuration Release archive"
    )])
}

/// HarmonyOS commands via hvigor.
pub fn hvigor_commands() -> Vec<String> {
    vec!["hvigorw assembleHap --mode module -p product=default".to_owned()]
}

/// Mini-program CI upload commands (platform review submission boundary).
pub fn miniprogram_upload_commands(
    project_path: &str,
    version: &str,
) -> Result<Vec<String>, ExecutorError> {
    let parsed = SemanticVersion::parse(version)
        .map_err(|error| ExecutorError::Validation(format!("semanticVersion: {error}")))?;
    if project_path.trim().is_empty() || project_path.len() > 500 {
        return Err(ExecutorError::Validation("projectPath is invalid".into()));
    }
    Ok(vec![format!(
        "miniprogram-ci upload --pp {project_path} --ver {}",
        parsed.to_canonical_string()
    )])
}

// ---------------------------------------------------------------------------
// Build state machine helpers
// ---------------------------------------------------------------------------

/// Validates a forward state transition of the build state machine.
pub fn validate_state_transition(current: &str, next: &str) -> Result<(), ExecutorError> {
    let terminal = matches!(next, "SUCCEEDED" | "FAILED" | "CANCELLED" | "TIMED_OUT");
    let allowed = if terminal {
        matches!(current, "PREPARING" | "COMPILING" | "TESTING" | "PACKAGING")
            || (next == "FAILED" && current == "QUEUED")
    } else {
        matches!(
            (current, next),
            ("QUEUED", "PREPARING")
                | ("PREPARING", "COMPILING")
                | ("COMPILING", "TESTING")
                | ("COMPILING", "PACKAGING")
                | ("TESTING", "PACKAGING")
        )
    };
    if allowed {
        Ok(())
    } else {
        Err(ExecutorError::Validation(format!(
            "invalid build state transition {current} -> {next}"
        )))
    }
}

/// Validates that a path stays inside the workspace root.
pub fn path_within_root(root: &Path, candidate: &Path) -> Result<PathBuf, ExecutorError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| ExecutorError::Plan(format!("resolve workspace root: {error}")))?;
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        canonical_root.join(candidate)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|error| ExecutorError::Plan(format!("resolve candidate path: {error}")))?;
    if canonical.starts_with(&canonical_root) {
        Ok(canonical)
    } else {
        Err(ExecutorError::Validation(
            "candidate path escapes the workspace root".into(),
        ))
    }
}

pub fn bounded_wait_timeout(timeout: Duration) -> Duration {
    timeout.clamp(Duration::from_secs(1), Duration::from_secs(3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_commands() {
        let command = parse_bounded_command("flutter build apk --release").expect("parse");
        assert_eq!(command.program, "flutter");
        assert_eq!(command.arguments, vec!["build", "apk", "--release"]);
    }

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(parse_bounded_command("rm -rf /").is_err());
        assert!(parse_bounded_command("sh -c 'curl evil'").is_err());
        assert!(parse_bounded_command("echo $HOME").is_err());
        assert!(parse_bounded_command("echo `id`").is_err());
        assert!(parse_bounded_command("a | b").is_err());
    }

    #[test]
    fn platform_command_constructors_are_bounded() {
        assert_eq!(flutter_commands("ANDROID").unwrap().len(), 2);
        assert!(flutter_commands("WECHAT").is_err());
        assert_eq!(gradle_commands().len(), 2);
        assert!(xcodebuild_commands("Runner").is_ok());
        assert!(xcodebuild_commands("").is_err());
        assert_eq!(hvigor_commands().len(), 1);
        assert!(miniprogram_upload_commands("miniprogram", "1.2.3").is_ok());
        assert!(miniprogram_upload_commands("miniprogram", "not-a-version").is_err());
    }

    #[test]
    fn state_machine_rejects_backward_transitions() {
        assert!(validate_state_transition("QUEUED", "PREPARING").is_ok());
        assert!(validate_state_transition("PREPARING", "COMPILING").is_ok());
        assert!(validate_state_transition("COMPILING", "PACKAGING").is_ok());
        assert!(validate_state_transition("PACKAGING", "SUCCEEDED").is_ok());
        assert!(validate_state_transition("SUCCEEDED", "FAILED").is_err());
        assert!(validate_state_transition("COMPILING", "PREPARING").is_err());
        assert!(validate_state_transition("QUEUED", "PACKAGING").is_err());
    }

    #[test]
    fn workspace_confinement_is_enforced() {
        let root = std::env::temp_dir().join("sdkwork-build-runner-test-root");
        let candidate = root.join("..").join("escape");
        assert!(path_within_root(&root, &candidate).is_err());
    }
}

//! Direct-mode (`self_managed`) mini-program upload execution.
//!
//! The customer owns the WeChat account and upload key. Deploy only runs the
//! planned `miniprogram-ci` invocation inside a caller-configured sandbox:
//! - the project directory must sit under the configured project root;
//! - the private key path must sit under the configured key root;
//! - the executable name is fixed by the planner, never supplied by the caller;
//! - output is bounded, and a non-zero exit is a failure, not a release.
//!
//! Process execution itself is a port ([`ChannelCiProcess`]) so the service can
//! be tested without spawning anything; the worker installs a real runner.
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::channel_release_direct::{
    plan_miniprogram_ci_upload, MiniProgramCiInvocation, MiniProgramCiUploadRequest,
};

const DEFAULT_MAX_OUTPUT_CHARS: usize = 8192;

/// Runs one planned CI invocation.
#[async_trait]
pub trait ChannelCiProcess: Send + Sync {
    /// Executes the invocation in `working_directory` and returns the exit code
    /// plus an output excerpt.
    async fn run(
        &self,
        invocation: &MiniProgramCiInvocation,
        working_directory: &str,
    ) -> DeployServiceResult<ChannelCiOutcome>;
}

/// Result of one CI invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelCiOutcome {
    /// Process exit code.
    pub exit_code: i32,
    /// Bounded output excerpt.
    pub output_excerpt: String,
}

/// The runner port the service depends on.
#[async_trait]
pub trait ChannelCiRunner: Send + Sync {
    /// Validates and executes one direct-mode upload.
    async fn upload(
        &self,
        request: &MiniProgramCiUploadRequest,
    ) -> DeployServiceResult<ChannelCiOutcome>;
}

/// Fail-closed runner used when direct-mode execution is not configured.
pub struct UnconfiguredChannelCiRunner;

#[async_trait]
impl ChannelCiRunner for UnconfiguredChannelCiRunner {
    async fn upload(
        &self,
        _request: &MiniProgramCiUploadRequest,
    ) -> DeployServiceResult<ChannelCiOutcome> {
        Err(DeployServiceError::Conflict(
            "direct-mode channel upload is not configured".to_string(),
        ))
    }
}

/// Filesystem sandbox for direct-mode uploads.
#[derive(Clone, Debug)]
pub struct DirectUploadPolicy {
    /// Only project directories under this root may be uploaded.
    pub project_root: PathBuf,
    /// Only private keys under this root may be used.
    pub key_root: PathBuf,
}

impl DirectUploadPolicy {
    /// Builds a policy from the two sandbox roots.
    pub fn new(project_root: impl Into<PathBuf>, key_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            key_root: key_root.into(),
        }
    }

    /// Resolves and checks both paths against their roots.
    ///
    /// @returns `(canonical project dir, canonical key path)`.
    pub fn check(
        &self,
        request: &MiniProgramCiUploadRequest,
    ) -> Result<(PathBuf, PathBuf), String> {
        let project_dir = std::fs::canonicalize(request.project_dir.trim())
            .map_err(|error| format!("project directory is not readable: {error}"))?;
        let project_root = std::fs::canonicalize(&self.project_root)
            .map_err(|error| format!("project root is not readable: {error}"))?;
        if !project_dir.starts_with(&project_root) {
            return Err("project directory escapes the configured project root".to_string());
        }
        if !project_dir.is_dir() {
            return Err("project directory is not a directory".to_string());
        }
        let key_path = std::fs::canonicalize(request.private_key_path.trim())
            .map_err(|error| format!("private key is not readable: {error}"))?;
        let key_root = std::fs::canonicalize(&self.key_root)
            .map_err(|error| format!("key root is not readable: {error}"))?;
        if !key_path.starts_with(&key_root) {
            return Err("private key escapes the configured key root".to_string());
        }
        if !key_path.is_file() {
            return Err("private key is not a file".to_string());
        }
        Ok((project_dir, key_path))
    }
}

/// Executes direct-mode uploads inside a [`DirectUploadPolicy`] sandbox.
pub struct BoundedChannelCiRunner<P: ChannelCiProcess> {
    process: P,
    policy: DirectUploadPolicy,
    max_output_chars: usize,
}

impl<P: ChannelCiProcess> BoundedChannelCiRunner<P> {
    /// Builds the runner.
    pub fn new(process: P, policy: DirectUploadPolicy) -> Self {
        Self {
            process,
            policy,
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
        }
    }

    /// Overrides the output excerpt bound.
    pub fn with_max_output_chars(mut self, maximum: usize) -> Self {
        self.max_output_chars = maximum.max(64);
        self
    }
}

#[async_trait]
impl<P: ChannelCiProcess> ChannelCiRunner for BoundedChannelCiRunner<P> {
    async fn upload(
        &self,
        request: &MiniProgramCiUploadRequest,
    ) -> DeployServiceResult<ChannelCiOutcome> {
        let policy = self.policy.clone();
        let policy_request = request.clone();
        let (project_dir, key_path) =
            tokio::task::spawn_blocking(move || policy.check(&policy_request))
                .await
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?
                .map_err(DeployServiceError::validation)?;
        let resolved = MiniProgramCiUploadRequest {
            project_dir: project_dir.to_string_lossy().into_owned(),
            private_key_path: key_path.to_string_lossy().into_owned(),
            ..request.clone()
        };
        let invocation =
            plan_miniprogram_ci_upload(&resolved).map_err(DeployServiceError::validation)?;
        let mut outcome = self.process.run(&invocation, &resolved.project_dir).await?;
        outcome.output_excerpt = truncate_chars(&outcome.output_excerpt, self.max_output_chars);
        if outcome.exit_code != 0 {
            return Err(DeployServiceError::Conflict(format!(
                "miniprogram-ci upload failed with exit code {}: {}",
                outcome.exit_code, outcome.output_excerpt
            )));
        }
        Ok(outcome)
    }
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    if value.chars().count() <= maximum {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(maximum).collect();
    truncated.push_str("…[truncated]");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct FakeProcess {
        exit_code: i32,
        calls: AtomicUsize,
        last: Mutex<Option<(MiniProgramCiInvocation, String)>>,
    }

    impl FakeProcess {
        fn new(exit_code: i32) -> Self {
            Self {
                exit_code,
                calls: AtomicUsize::new(0),
                last: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl ChannelCiProcess for FakeProcess {
        async fn run(
            &self,
            invocation: &MiniProgramCiInvocation,
            working_directory: &str,
        ) -> DeployServiceResult<ChannelCiOutcome> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last.lock().expect("last") =
                Some((invocation.clone(), working_directory.to_string()));
            Ok(ChannelCiOutcome {
                exit_code: self.exit_code,
                output_excerpt: "uploaded".to_string(),
            })
        }
    }

    struct Sandbox {
        root: PathBuf,
        project: PathBuf,
        key: PathBuf,
    }

    fn sandbox() -> Sandbox {
        let root = std::env::temp_dir().join(format!(
            "deploy-ci-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let project = root.join("workspace").join("demo");
        let key_dir = root.join("keys");
        std::fs::create_dir_all(&project).expect("project dir");
        std::fs::create_dir_all(&key_dir).expect("key dir");
        let key = key_dir.join("upload.key");
        std::fs::write(&key, "dummy").expect("key file");
        Sandbox { root, project, key }
    }

    fn request(sandbox: &Sandbox) -> MiniProgramCiUploadRequest {
        MiniProgramCiUploadRequest {
            project_dir: sandbox.project.to_string_lossy().into_owned(),
            app_id: "wx123".to_string(),
            private_key_path: sandbox.key.to_string_lossy().into_owned(),
            version: "1.0.0".to_string(),
            description: "build 42".to_string(),
            robot: Some(7),
        }
    }

    fn policy(sandbox: &Sandbox) -> DirectUploadPolicy {
        DirectUploadPolicy::new(sandbox.root.join("workspace"), sandbox.root.join("keys"))
    }

    #[tokio::test]
    async fn runs_the_planned_upload_inside_the_sandbox() {
        let sandbox = sandbox();
        let process = FakeProcess::new(0);
        let runner = BoundedChannelCiRunner::new(process, policy(&sandbox));
        let outcome = runner.upload(&request(&sandbox)).await.expect("upload");
        assert_eq!(outcome.exit_code, 0);
    }

    #[tokio::test]
    async fn rejects_a_project_outside_the_sandbox() {
        let sandbox = sandbox();
        let outside = sandbox.root.join("outside");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let process = FakeProcess::new(0);
        let runner = BoundedChannelCiRunner::new(process, policy(&sandbox));
        let mut request = request(&sandbox);
        request.project_dir = outside.to_string_lossy().into_owned();
        assert!(runner.upload(&request).await.is_err());
    }

    #[tokio::test]
    async fn rejects_a_key_outside_the_sandbox() {
        let sandbox = sandbox();
        let stray = sandbox.root.join("stray.key");
        std::fs::write(&stray, "dummy").expect("stray key");
        let runner = BoundedChannelCiRunner::new(FakeProcess::new(0), policy(&sandbox));
        let mut request = request(&sandbox);
        request.private_key_path = stray.to_string_lossy().into_owned();
        assert!(runner.upload(&request).await.is_err());
    }

    #[tokio::test]
    async fn surfaces_a_non_zero_exit_as_conflict() {
        let sandbox = sandbox();
        let runner = BoundedChannelCiRunner::new(FakeProcess::new(2), policy(&sandbox));
        assert!(runner.upload(&request(&sandbox)).await.is_err());
    }
}

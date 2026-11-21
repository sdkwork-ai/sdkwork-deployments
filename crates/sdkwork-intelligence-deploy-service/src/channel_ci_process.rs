//! Direct-mode `miniprogram-ci` process execution.
//!
//! The worker owns process execution; the service only plans and sandboxes the
//! invocation. This adapter enforces the fixed executable name, pipes both
//! streams with a byte bound, and turns a timeout into a failure instead of a
//! hung publish.
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::channel_ci_runner::{ChannelCiOutcome, ChannelCiProcess};
use crate::channel_release_direct::MiniProgramCiInvocation;

/// The only executable the direct-mode runner will start.
const MINIPROGRAM_CI_PROGRAM: &str = "miniprogram-ci";
const DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT_SECONDS: u64 = 900;

/// Production `miniprogram-ci` runner.
pub struct TokioChannelCiProcess {
    max_output_bytes: usize,
    timeout: Duration,
}

impl TokioChannelCiProcess {
    /// Builds the runner with bounded output and a 15-minute timeout.
    pub fn new() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
        }
    }
}

impl Default for TokioChannelCiProcess {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelCiProcess for TokioChannelCiProcess {
    async fn run(
        &self,
        invocation: &MiniProgramCiInvocation,
        working_directory: &str,
    ) -> DeployServiceResult<ChannelCiOutcome> {
        if invocation.program != MINIPROGRAM_CI_PROGRAM {
            return Err(DeployServiceError::validation(
                "only the miniprogram-ci executable may be started",
            ));
        }
        let mut child = tokio::process::Command::new(MINIPROGRAM_CI_PROGRAM)
            .args(&invocation.args)
            .current_dir(working_directory)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                DeployServiceError::Internal(format!("spawn miniprogram-ci: {error}"))
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DeployServiceError::Internal("missing stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| DeployServiceError::Internal("missing stderr pipe".to_string()))?;
        let reader = read_bounded_output(stdout, stderr, self.max_output_bytes);
        let waited = tokio::time::timeout(self.timeout, async {
            let (stdout_text, stderr_text) = reader.await;
            let status = child.wait().await;
            (stdout_text, stderr_text, status)
        })
        .await;
        let (stdout_text, stderr_text, status) = match waited {
            Ok(value) => value,
            Err(_) => {
                let _ = child.kill().await;
                return Err(DeployServiceError::Conflict(format!(
                    "miniprogram-ci upload timed out after {} seconds",
                    self.timeout.as_secs()
                )));
            }
        };
        let exit_code = status
            .map_err(|error| DeployServiceError::Internal(format!("wait miniprogram-ci: {error}")))?
            .code()
            .unwrap_or(-1);
        let mut output = stdout_text;
        if !stderr_text.is_empty() {
            output.push_str("\n[stderr]\n");
            output.push_str(&stderr_text);
        }
        Ok(ChannelCiOutcome {
            exit_code,
            output_excerpt: output,
        })
    }
}

async fn read_bounded_output(
    mut stdout: tokio::process::ChildStdout,
    mut stderr: tokio::process::ChildStderr,
    maximum: usize,
) -> (String, String) {
    use tokio::io::AsyncReadExt;
    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let mut stdout_limited = (&mut stdout).take(maximum as u64);
    let mut stderr_limited = (&mut stderr).take(maximum as u64);
    let _ = stdout_limited.read_to_end(&mut stdout_buffer).await;
    let _ = stderr_limited.read_to_end(&mut stderr_buffer).await;
    (
        String::from_utf8_lossy(&stdout_buffer).into_owned(),
        String::from_utf8_lossy(&stderr_buffer).into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn refuses_a_non_miniprogram_ci_program() {
        let process = TokioChannelCiProcess::new();
        let invocation = MiniProgramCiInvocation {
            program: "rm".to_string(),
            args: vec!["-rf".to_string(), "/".to_string()],
        };
        assert!(process.run(&invocation, ".").await.is_err());
    }
}

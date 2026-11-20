//! Direct-mode (`self_managed`) mini-program upload planning.
//!
//! A customer who keeps their own WeChat account publishes with
//! `miniprogram-ci` and a code-upload private key, not through a component
//! authorization. Deploy still owns the release record, so it must produce the
//! exact invocation deterministically:
//!
//! - the private key **path** is passed to the tool, never read or logged by
//!   Deploy;
//! - version and description obey WeChat's limits before the tool runs;
//! - the robot number is optional and bounded.
//!
//! Planning is pure. Executing the process (and parsing its output) belongs to
//! the worker that owns the CI runtime.
use crate::channel_release::ChannelReleaseCommand;

/// Maximum provider version length.
pub const MAX_PROVIDER_VERSION_CHARS: usize = 64;
/// Maximum upload description length.
pub const MAX_UPLOAD_DESCRIPTION_CHARS: usize = 200;
/// WeChat's robot range for CI uploads.
pub const MIN_CI_ROBOT: u8 = 1;
/// WeChat's robot range for CI uploads.
pub const MAX_CI_ROBOT: u8 = 30;

/// One direct-mode upload request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramCiUploadRequest {
    /// Path to the projected mini-program project.
    pub project_dir: String,
    /// Target AppID.
    pub app_id: String,
    /// Path to the customer's upload private key.
    pub private_key_path: String,
    /// Version shown to users.
    pub version: String,
    /// Upload description (build identity).
    pub description: String,
    /// Optional CI robot number.
    pub robot: Option<u8>,
}

/// A planned process invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramCiInvocation {
    /// Executable name.
    pub program: String,
    /// Arguments, already ordered.
    pub args: Vec<String>,
}

/// Plans the `miniprogram-ci upload` invocation.
///
/// @param request - project, credentials path, version, description, robot.
/// @returns the executable and argument vector.
pub fn plan_miniprogram_ci_upload(
    request: &MiniProgramCiUploadRequest,
) -> Result<MiniProgramCiInvocation, String> {
    validate_upload_request(request)?;
    let mut args = vec![
        "upload".to_string(),
        "--pp".to_string(),
        request.project_dir.trim().to_string(),
        "--pkp".to_string(),
        request.private_key_path.trim().to_string(),
        "--appid".to_string(),
        request.app_id.trim().to_string(),
        "--uv".to_string(),
        request.version.trim().to_string(),
        "--ud".to_string(),
        request.description.trim().to_string(),
    ];
    if let Some(robot) = request.robot {
        args.push("--robot".to_string());
        args.push(robot.to_string());
    }
    Ok(MiniProgramCiInvocation {
        program: "miniprogram-ci".to_string(),
        args,
    })
}

/// True when the command requires the direct CI path.
pub fn command_uses_direct_ci(mode: sdkwork_deploy_contract::ChannelAccessMode) -> bool {
    matches!(
        mode,
        sdkwork_deploy_contract::ChannelAccessMode::SelfManaged
    )
}

/// The command a direct-mode upload represents.
pub fn direct_upload_command() -> ChannelReleaseCommand {
    ChannelReleaseCommand::Upload
}

fn validate_upload_request(request: &MiniProgramCiUploadRequest) -> Result<(), String> {
    for (field, value) in [
        ("projectDir", request.project_dir.as_str()),
        ("appId", request.app_id.as_str()),
        ("privateKeyPath", request.private_key_path.as_str()),
        ("version", request.version.as_str()),
        ("description", request.description.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} is required"));
        }
    }
    if request.version.trim().chars().count() > MAX_PROVIDER_VERSION_CHARS {
        return Err(format!(
            "version must be at most {MAX_PROVIDER_VERSION_CHARS} characters"
        ));
    }
    if request.version.chars().any(char::is_whitespace) {
        return Err("version must not contain whitespace".to_string());
    }
    if request.description.trim().chars().count() > MAX_UPLOAD_DESCRIPTION_CHARS {
        return Err(format!(
            "description must be at most {MAX_UPLOAD_DESCRIPTION_CHARS} characters"
        ));
    }
    if let Some(robot) = request.robot {
        if !(MIN_CI_ROBOT..=MAX_CI_ROBOT).contains(&robot) {
            return Err(format!(
                "robot must be between {MIN_CI_ROBOT} and {MAX_CI_ROBOT}"
            ));
        }
    }
    Ok(())
}

/// Plans a direct-mode upload: mode check, state transitions, CI invocation.
///
/// @param mode - credential mode of the bound channel account.
/// @param current - current provider release state.
/// @param request - project, credentials path, version, description, robot.
/// @returns the transitions to apply after a successful upload and the process
/// invocation that produces it.
pub fn plan_direct_upload(
    mode: sdkwork_deploy_contract::ChannelAccessMode,
    current: sdkwork_deploy_contract::ChannelReleaseStatus,
    request: &MiniProgramCiUploadRequest,
) -> Result<
    (
        Vec<sdkwork_deploy_contract::ChannelReleaseStatus>,
        MiniProgramCiInvocation,
    ),
    String,
> {
    if !command_uses_direct_ci(mode) {
        return Err(
            "direct CI upload requires a self_managed channel account; authorized accounts publish through the component path"
                .to_string(),
        );
    }
    let transitions = crate::channel_release::plan_command_transitions(
        current,
        &crate::channel_release::ChannelReleaseCommand::Upload,
    )?;
    let invocation = plan_miniprogram_ci_upload(request)?;
    Ok((transitions, invocation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_deploy_contract::ChannelAccessMode;

    fn sample_request() -> MiniProgramCiUploadRequest {
        MiniProgramCiUploadRequest {
            project_dir: "target/projection".to_string(),
            app_id: "wxappid".to_string(),
            private_key_path: "secrets/private.wx.key".to_string(),
            version: "1.2.3".to_string(),
            description: "build-42".to_string(),
            robot: Some(7),
        }
    }

    #[test]
    fn plans_the_documented_argument_order() {
        let invocation = plan_miniprogram_ci_upload(&sample_request()).expect("plan");
        assert_eq!(invocation.program, "miniprogram-ci");
        assert_eq!(
            invocation.args,
            vec![
                "upload",
                "--pp",
                "target/projection",
                "--pkp",
                "secrets/private.wx.key",
                "--appid",
                "wxappid",
                "--uv",
                "1.2.3",
                "--ud",
                "build-42",
                "--robot",
                "7",
            ]
        );
    }

    #[test]
    fn robot_is_optional() {
        let mut request = sample_request();
        request.robot = None;
        let invocation = plan_miniprogram_ci_upload(&request).expect("plan");
        assert!(!invocation.args.contains(&"--robot".to_string()));
    }

    #[test]
    fn rejects_missing_fields() {
        for field in [
            "project_dir",
            "app_id",
            "private_key_path",
            "version",
            "description",
        ] {
            let mut request = sample_request();
            match field {
                "project_dir" => request.project_dir.clear(),
                "app_id" => request.app_id.clear(),
                "private_key_path" => request.private_key_path.clear(),
                "version" => request.version.clear(),
                "description" => request.description.clear(),
                _ => unreachable!("known field"),
            }
            assert!(
                plan_miniprogram_ci_upload(&request).is_err(),
                "{field} must be required"
            );
        }
    }

    #[test]
    fn enforces_version_and_description_limits() {
        let mut request = sample_request();
        request.version = "v".repeat(MAX_PROVIDER_VERSION_CHARS + 1);
        assert!(plan_miniprogram_ci_upload(&request).is_err());
        let mut request = sample_request();
        request.description = "d".repeat(MAX_UPLOAD_DESCRIPTION_CHARS + 1);
        assert!(plan_miniprogram_ci_upload(&request).is_err());
        let mut request = sample_request();
        request.version = "1 2".to_string();
        assert!(plan_miniprogram_ci_upload(&request).is_err());
    }

    #[test]
    fn enforces_the_robot_range() {
        let mut request = sample_request();
        request.robot = Some(0);
        assert!(plan_miniprogram_ci_upload(&request).is_err());
        request.robot = Some(MAX_CI_ROBOT + 1);
        assert!(plan_miniprogram_ci_upload(&request).is_err());
    }

    #[test]
    fn direct_mode_is_the_self_managed_mode() {
        assert!(command_uses_direct_ci(ChannelAccessMode::SelfManaged));
        assert!(!command_uses_direct_ci(
            ChannelAccessMode::OperatorAuthorized
        ));
    }

    #[test]
    fn direct_upload_requires_both_the_mode_and_the_state() {
        use sdkwork_deploy_contract::ChannelReleaseStatus;
        let (transitions, invocation) = plan_direct_upload(
            ChannelAccessMode::SelfManaged,
            ChannelReleaseStatus::Projected,
            &sample_request(),
        )
        .expect("plan");
        assert_eq!(transitions, vec![ChannelReleaseStatus::Uploaded]);
        assert_eq!(invocation.program, "miniprogram-ci");

        assert!(plan_direct_upload(
            ChannelAccessMode::OperatorAuthorized,
            ChannelReleaseStatus::Projected,
            &sample_request(),
        )
        .is_err());
        assert!(plan_direct_upload(
            ChannelAccessMode::SelfManaged,
            ChannelReleaseStatus::Draft,
            &sample_request(),
        )
        .is_err());
    }
}

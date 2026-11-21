//! Channel release publish commands: the app-api command surface over the
//! publisher.
//!
//! The command loads the release once, applies exactly one provider step through
//! [`ChannelReleasePublisher`], and returns the reloaded row so the caller sees
//! the persisted state instead of the intended one.
use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    ChannelAccessMode, ChannelReleasePublishCommandRequest, ChannelReleaseResponse,
    DeployAppRequestContext, DeployServiceError, DeployServiceResult,
    TransitionChannelReleaseRequest, CHANNEL_RELEASE_ACTION_GRAY,
    CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS, CHANNEL_RELEASE_ACTION_RELEASE,
    CHANNEL_RELEASE_ACTION_ROLLBACK, CHANNEL_RELEASE_ACTION_SET_PRIVACY,
    CHANNEL_RELEASE_ACTION_SUBMIT_AUDIT, CHANNEL_RELEASE_ACTION_UPLOAD,
};

use crate::channel_release::{
    MiniProgramAuditSubmission, MiniProgramGrayReleaseRequest, MiniProgramReleaseRequest,
    MiniProgramRollbackRequest, MiniProgramUploadRequest,
};
use crate::channel_release_observer::ChannelReleaseStore;
use crate::channel_release_publisher::ChannelReleasePublisher;
use crate::repository::DeployRepositoryPort;
use crate::DeployService;

/// Adapts the repository port to the narrow publisher store, scoped to one
/// tenant and application so a release id from another app cannot be published.
pub struct PortChannelReleaseStore {
    repository: Arc<dyn DeployRepositoryPort>,
    tenant_id: i64,
    app_id: String,
}

impl PortChannelReleaseStore {
    /// Builds the scoped adapter.
    pub fn new(repository: Arc<dyn DeployRepositoryPort>, tenant_id: i64, app_id: String) -> Self {
        Self {
            repository,
            tenant_id,
            app_id,
        }
    }
}

#[async_trait]
impl ChannelReleaseStore for PortChannelReleaseStore {
    async fn load_channel_release_status(
        &self,
        _tenant_id: i64,
        release_id: &str,
    ) -> DeployServiceResult<(sdkwork_deploy_contract::ChannelReleaseStatus, i64)> {
        let release = self
            .repository
            .retrieve_channel_release(self.tenant_id, &self.app_id, release_id)
            .await?;
        let version = release.version.parse::<i64>().map_err(|_| {
            DeployServiceError::Internal("channel release version is not numeric".to_string())
        })?;
        Ok((release.release_status, version))
    }

    async fn transition_channel_release(
        &self,
        _tenant_id: i64,
        release_id: &str,
        expected_version: i64,
        next: sdkwork_deploy_contract::ChannelReleaseStatus,
        commit_id: Option<&str>,
        audit_id: Option<&str>,
        detail: Option<&str>,
    ) -> DeployServiceResult<()> {
        self.repository
            .transition_channel_release(
                self.tenant_id,
                &self.app_id,
                release_id,
                &TransitionChannelReleaseRequest {
                    expected_version: expected_version.to_string(),
                    release_status: next,
                    commit_id: commit_id.map(str::to_string),
                    audit_id: audit_id.map(str::to_string),
                    error_code: detail.map(str::to_string),
                },
            )
            .await?;
        Ok(())
    }

    async fn set_channel_release_gray_percentage(
        &self,
        _tenant_id: i64,
        release_id: &str,
        gray_percentage: u16,
    ) -> DeployServiceResult<()> {
        self.repository
            .set_channel_release_gray(self.tenant_id, &self.app_id, release_id, gray_percentage)
            .await
    }
}

/// Enforces the direct-mode product boundary.
///
/// A `self_managed` customer keeps the WeChat account; Deploy can only run the
/// `miniprogram-ci` upload. Review and release happen in the WeChat admin console
/// or the customer's own tooling, and the observed status is recorded through the
/// release transition endpoint.
pub(crate) fn validate_action_for_access_mode(
    access_mode: ChannelAccessMode,
    action: &str,
) -> DeployServiceResult<()> {
    if access_mode == ChannelAccessMode::SelfManaged && action != CHANNEL_RELEASE_ACTION_UPLOAD {
        return Err(DeployServiceError::validation(
            "self-managed channel releases only support upload; advance review and release in the \
             WeChat admin console and record the observed status with a release transition",
        ));
    }
    Ok(())
}

/// Requires a command field that the selected provider path needs.
fn required_command_field<'a>(value: Option<&'a str>, field: &str) -> DeployServiceResult<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DeployServiceError::validation(format!("{field} is required")))
}

/// Validates a domain edit with the provider rules before it consumes the
/// monthly quota.
pub(crate) fn validate_domain_command(action: &str, domains: &[String]) -> DeployServiceResult<()> {
    let action =
        crate::wechat_channel_rules::WechatDomainAction::from_wire(action).ok_or_else(|| {
            DeployServiceError::validation("domainAction must be add, set, or delete")
        })?;
    crate::wechat_channel_rules::validate_wechat_domain_change(
        &crate::wechat_channel_rules::WechatDomainChange {
            action,
            domains: domains.to_vec(),
        },
    )
    .map_err(DeployServiceError::validation)
}

/// Concrete publisher used by the publish command.
type PublishPublisher = ChannelReleasePublisher<
    Arc<dyn crate::channel_release::ChannelReleaseProvider>,
    PortChannelReleaseStore,
>;

impl DeployService {
    /// Records one publish command outcome for operations and support.
    async fn audit_channel_release_publish(
        &self,
        context: &DeployAppRequestContext,
        release_id: &str,
        action: &str,
        outcome: &str,
    ) {
        if let Err(error) = self
            .repository
            .insert_audit_log(crate::repository::InsertAuditLogCommand {
                tenant_id: context.tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                operator_id: context.actor_id.unwrap_or(0),
                action: format!("channel_release.publish.{action}.{outcome}"),
                target_type: "channel_release".to_string(),
                target_id: None,
                target_uuid: Some(release_id.to_string()),
            })
            .await
        {
            tracing::warn!(error = %error, action, "channel release publish audit failed");
        }
    }

    /// Marks the release failed (best effort), records the audit, and returns the
    /// original provider error.
    async fn fail_channel_release_publish(
        &self,
        context: &DeployAppRequestContext,
        publisher: &PublishPublisher,
        tenant_id: i64,
        release_id: &str,
        action: &str,
        error: DeployServiceError,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        let _ = publisher
            .mark_failed(tenant_id, release_id, &error.to_string())
            .await;
        self.audit_channel_release_publish(context, release_id, action, "failed")
            .await;
        Err(error)
    }

    /// Records the audit without touching the release state (account-level
    /// configuration steps and rollback/gray, whose live state must survive a
    /// failed attempt).
    async fn fail_channel_release_without_state(
        &self,
        context: &DeployAppRequestContext,
        release_id: &str,
        action: &str,
        error: DeployServiceError,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        self.audit_channel_release_publish(context, release_id, action, "failed")
            .await;
        Err(error)
    }

    /// Plans the direct-mode (`self_managed`) `miniprogram-ci upload` invocation.
    pub async fn plan_channel_direct_upload(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &sdkwork_deploy_contract::CreateChannelDirectUploadPlanRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ChannelDirectUploadPlan> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        let plan = crate::channel_release_direct::plan_miniprogram_ci_upload(
            &crate::channel_release_direct::MiniProgramCiUploadRequest {
                project_dir: request.project_dir.clone(),
                app_id: request.provider_app_id.clone(),
                private_key_path: request.private_key_path.clone(),
                version: request.version.clone(),
                description: request.description.clone(),
                robot: request.robot,
            },
        )
        .map_err(DeployServiceError::validation)?;
        Ok(sdkwork_deploy_contract::ChannelDirectUploadPlan {
            program: plan.program,
            args: plan.args,
        })
    }

    /// Runs one publish step for a channel release.
    pub async fn publish_channel_release(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        release_id: &str,
        request: &ChannelReleasePublishCommandRequest,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        let tenant_id = Self::tenant_id(context)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        request.validate().map_err(DeployServiceError::validation)?;
        let release = self
            .repository
            .retrieve_channel_release(tenant_id, app_id, release_id)
            .await?;
        let store = PortChannelReleaseStore::new(
            Arc::clone(&self.repository),
            tenant_id,
            app_id.to_string(),
        );
        let publisher =
            ChannelReleasePublisher::new(Arc::clone(&self.channel_release_provider), store);
        validate_action_for_access_mode(release.access_mode, request.action.trim())?;
        match request.action.trim() {
            CHANNEL_RELEASE_ACTION_UPLOAD => {
                if release.access_mode == ChannelAccessMode::SelfManaged {
                    let provider_app_id = required_command_field(
                        request.provider_app_id.as_deref(),
                        "providerAppId",
                    )?;
                    let project_dir =
                        required_command_field(request.project_dir.as_deref(), "projectDir")?;
                    let private_key_path = required_command_field(
                        request.private_key_path.as_deref(),
                        "privateKeyPath",
                    )?;
                    let version = required_command_field(
                        request
                            .provider_version
                            .as_deref()
                            .or(release.provider_version.as_deref()),
                        "providerVersion",
                    )?;
                    if let Err(error) = self
                        .channel_ci_runner
                        .upload(&crate::channel_release_direct::MiniProgramCiUploadRequest {
                            project_dir: project_dir.to_string(),
                            app_id: provider_app_id.to_string(),
                            private_key_path: private_key_path.to_string(),
                            version: version.to_string(),
                            description: version.to_string(),
                            robot: request.robot,
                        })
                        .await
                    {
                        return self
                            .fail_channel_release_publish(
                                context,
                                &publisher,
                                tenant_id,
                                release_id,
                                CHANNEL_RELEASE_ACTION_UPLOAD,
                                error,
                            )
                            .await;
                    }
                    if let Err(error) = publisher
                        .record_upload(tenant_id, release_id, &format!("ci:{version}"))
                        .await
                    {
                        return self
                            .fail_channel_release_publish(
                                context,
                                &publisher,
                                tenant_id,
                                release_id,
                                CHANNEL_RELEASE_ACTION_UPLOAD,
                                error,
                            )
                            .await;
                    }
                } else {
                    if let Err(error) = publisher
                        .upload(
                            tenant_id,
                            release_id,
                            &MiniProgramUploadRequest {
                                authorization_id: request.authorization_id.clone(),
                                environment: release.environment.clone(),
                                template_id: request
                                    .template_id
                                    .clone()
                                    .or_else(|| release.template_id.clone()),
                                provider_version: request
                                    .provider_version
                                    .clone()
                                    .or_else(|| release.provider_version.clone()),
                                ext_json: request.ext_json.clone(),
                            },
                        )
                        .await
                    {
                        return self
                            .fail_channel_release_publish(
                                context,
                                &publisher,
                                tenant_id,
                                release_id,
                                CHANNEL_RELEASE_ACTION_UPLOAD,
                                error,
                            )
                            .await;
                    }
                }
            }
            CHANNEL_RELEASE_ACTION_SUBMIT_AUDIT => {
                let commit_id = release.commit_id.clone().ok_or_else(|| {
                    DeployServiceError::Conflict(
                        "channel release has no commit id; upload the code first".to_string(),
                    )
                })?;
                if let Err(error) = publisher
                    .submit_audit(
                        tenant_id,
                        release_id,
                        &MiniProgramAuditSubmission {
                            authorization_id: request.authorization_id.clone(),
                            commit_id,
                            provider_version: release.provider_version.clone().unwrap_or_default(),
                            items: request.items.clone().unwrap_or_default(),
                        },
                    )
                    .await
                {
                    return self
                        .fail_channel_release_publish(
                            context,
                            &publisher,
                            tenant_id,
                            release_id,
                            CHANNEL_RELEASE_ACTION_SUBMIT_AUDIT,
                            error,
                        )
                        .await;
                }
            }
            CHANNEL_RELEASE_ACTION_RELEASE => {
                if let Err(error) = publisher
                    .release(
                        tenant_id,
                        release_id,
                        &MiniProgramReleaseRequest {
                            authorization_id: request.authorization_id.clone(),
                            provider_version: release.provider_version.clone(),
                        },
                    )
                    .await
                {
                    return self
                        .fail_channel_release_publish(
                            context,
                            &publisher,
                            tenant_id,
                            release_id,
                            CHANNEL_RELEASE_ACTION_RELEASE,
                            error,
                        )
                        .await;
                }
            }
            CHANNEL_RELEASE_ACTION_ROLLBACK => {
                if let Err(error) = publisher
                    .rollback(
                        tenant_id,
                        release_id,
                        &MiniProgramRollbackRequest {
                            authorization_id: request.authorization_id.clone(),
                            target_version: request.target_version.clone().unwrap_or_default(),
                        },
                    )
                    .await
                {
                    return self
                        .fail_channel_release_without_state(
                            context,
                            release_id,
                            CHANNEL_RELEASE_ACTION_ROLLBACK,
                            error,
                        )
                        .await;
                }
            }
            CHANNEL_RELEASE_ACTION_GRAY => {
                if let Err(error) = publisher
                    .gray(
                        tenant_id,
                        release_id,
                        &MiniProgramGrayReleaseRequest {
                            authorization_id: request.authorization_id.clone(),
                            gray_percentage: request.gray_percentage.unwrap_or(0),
                        },
                    )
                    .await
                {
                    return self
                        .fail_channel_release_without_state(
                            context,
                            release_id,
                            CHANNEL_RELEASE_ACTION_GRAY,
                            error,
                        )
                        .await;
                }
            }
            CHANNEL_RELEASE_ACTION_SET_PRIVACY => {
                let privacy = request
                    .privacy
                    .clone()
                    .ok_or_else(|| DeployServiceError::validation("privacy is required"))?;
                if let Err(error) = self
                    .channel_release_provider
                    .set_mini_program_privacy(&request.authorization_id, privacy)
                    .await
                {
                    return self
                        .fail_channel_release_without_state(
                            context,
                            release_id,
                            CHANNEL_RELEASE_ACTION_SET_PRIVACY,
                            error,
                        )
                        .await;
                }
            }
            CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS => {
                let action = request
                    .domain_action
                    .clone()
                    .ok_or_else(|| DeployServiceError::validation("domainAction is required"))?;
                let domains = request.domains.clone().unwrap_or_default();
                validate_domain_command(&action, &domains)?;
                if let Err(error) = self
                    .channel_release_provider
                    .modify_mini_program_domains(&request.authorization_id, &action, &domains)
                    .await
                {
                    return self
                        .fail_channel_release_without_state(
                            context,
                            release_id,
                            CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS,
                            error,
                        )
                        .await;
                }
            }
            other => {
                return Err(DeployServiceError::validation(format!(
                    "unknown channel release action {other}"
                )))
            }
        }
        self.audit_channel_release_publish(context, release_id, request.action.trim(), "succeeded")
            .await;
        self.repository
            .retrieve_channel_release(tenant_id, app_id, release_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mode_only_supports_upload() {
        assert!(validate_action_for_access_mode(ChannelAccessMode::SelfManaged, "upload").is_ok());
        for action in [
            "submit_audit",
            "release",
            "rollback",
            "gray",
            "modify_domains",
            "set_privacy",
        ] {
            assert!(
                validate_action_for_access_mode(ChannelAccessMode::SelfManaged, action).is_err(),
                "{action} must be rejected for a self-managed release"
            );
        }
    }

    #[test]
    fn operator_authorized_mode_supports_every_action() {
        for action in [
            "upload",
            "submit_audit",
            "release",
            "rollback",
            "gray",
            "modify_domains",
            "set_privacy",
        ] {
            assert!(
                validate_action_for_access_mode(ChannelAccessMode::OperatorAuthorized, action)
                    .is_ok()
            );
        }
    }

    #[test]
    fn domain_command_uses_provider_rules() {
        assert!(validate_domain_command("add", &["https://api.example.com".to_string()]).is_ok());
        assert!(validate_domain_command("add", &[]).is_err());
        assert!(validate_domain_command("add", &["http://api.example.com".to_string()]).is_err());
        assert!(
            validate_domain_command("replace", &["https://api.example.com".to_string()]).is_err()
        );
        assert!(validate_domain_command(
            "add",
            &[
                "https://api.example.com".to_string(),
                "https://API.example.com/".to_string(),
            ],
        )
        .is_err());
    }
}

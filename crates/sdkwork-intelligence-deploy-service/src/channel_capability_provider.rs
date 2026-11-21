//! Channel release provider backed by IAM capability invocations.
//!
//! Deploy holds no WeChat credential: every provider step is one
//! `capability_invocations` call, and IAM resolves the authorizer token. The
//! payload mapping here is the contract between the two sides, so each field is
//! explicit instead of passing an opaque blob.
use async_trait::async_trait;
use sdkwork_deploy_contract::{
    wechat_audit_status_to_release, DeployServiceError, DeployServiceResult,
};
use serde_json::{json, Value};

use crate::channel_capability_client::IamChannelCapabilityClient;
use crate::channel_release::{
    ChannelReleaseProvider, MiniProgramAuditObservation, MiniProgramAuditOutcome,
    MiniProgramAuditSubmission, MiniProgramGrayReleaseRequest, MiniProgramReleaseOutcome,
    MiniProgramReleaseRequest, MiniProgramRollbackRequest, MiniProgramUploadOutcome,
    MiniProgramUploadRequest,
};

/// Uploads mini-program code from a template.
pub const CAPABILITY_MP_CODE_COMMIT: &str = "mini_program.code.commit";
/// Submits a mini-program review package.
pub const CAPABILITY_MP_AUDIT_SUBMIT: &str = "mini_program.audit.submit";
/// Reads a mini-program review verdict.
pub const CAPABILITY_MP_AUDIT_STATUS: &str = "mini_program.audit.status";
/// Releases the approved version.
pub const CAPABILITY_MP_RELEASE: &str = "mini_program.release";
/// Restores a released version.
pub const CAPABILITY_MP_ROLLBACK: &str = "mini_program.rollback";
/// Adjusts a gray rollout.
pub const CAPABILITY_MP_GRAY_RELEASE: &str = "mini_program.gray_release";
/// Adds, sets, or deletes mini-program server domains.
pub const CAPABILITY_MP_DOMAINS_MODIFY: &str = "mini_program.domains.modify";
/// Sets the mini-program privacy declaration.
pub const CAPABILITY_MP_PRIVACY_SET: &str = "mini_program.privacy.set";

/// Provider that executes channel steps through IAM.
pub struct IamChannelCapabilityChannelReleaseProvider {
    client: IamChannelCapabilityClient,
}

impl IamChannelCapabilityChannelReleaseProvider {
    /// Builds the provider from a configured capability client.
    pub fn new(client: IamChannelCapabilityClient) -> Self {
        Self { client }
    }
}

fn required_field<'a>(value: Option<&'a str>, field: &str) -> DeployServiceResult<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DeployServiceError::validation(format!("{field} is required")))
}

fn required_string(result: &Value, field: &str) -> DeployServiceResult<String> {
    result
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            DeployServiceError::Internal(format!("capability response is missing {field}"))
        })
}

#[async_trait]
impl ChannelReleaseProvider for IamChannelCapabilityChannelReleaseProvider {
    async fn upload_mini_program(
        &self,
        request: &MiniProgramUploadRequest,
    ) -> DeployServiceResult<MiniProgramUploadOutcome> {
        let template_id = required_field(request.template_id.as_deref(), "template_id")?;
        let user_version = required_field(request.provider_version.as_deref(), "provider_version")?;
        let mut payload = json!({
            "templateId": template_id,
            "userVersion": user_version,
            "userDesc": user_version,
        });
        if let Some(ext_json) = &request.ext_json {
            let encoded = serde_json::to_string(ext_json)
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
            payload["extJson"] = json!(encoded);
        }
        let result = self
            .client
            .invoke(
                &request.authorization_id,
                CAPABILITY_MP_CODE_COMMIT,
                payload,
            )
            .await?;
        Ok(MiniProgramUploadOutcome {
            commit_id: required_string(&result, "commitId")?,
        })
    }

    async fn submit_mini_program_audit(
        &self,
        request: &MiniProgramAuditSubmission,
    ) -> DeployServiceResult<MiniProgramAuditOutcome> {
        if request.items.is_empty() {
            return Err(DeployServiceError::validation(
                "review items are required to submit a mini-program audit",
            ));
        }
        let payload = json!({
            "items": request.items,
            "commitId": request.commit_id,
        });
        let result = self
            .client
            .invoke(
                &request.authorization_id,
                CAPABILITY_MP_AUDIT_SUBMIT,
                payload,
            )
            .await?;
        Ok(MiniProgramAuditOutcome {
            audit_id: required_string(&result, "auditId")?,
        })
    }

    async fn query_mini_program_audit(
        &self,
        authorization_id: &str,
        audit_id: &str,
    ) -> DeployServiceResult<MiniProgramAuditObservation> {
        let result = self
            .client
            .invoke(
                authorization_id,
                CAPABILITY_MP_AUDIT_STATUS,
                json!({ "auditId": audit_id }),
            )
            .await?;
        let status = result
            .get("status")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                DeployServiceError::Internal(
                    "audit status response is missing a numeric status".to_string(),
                )
            })?;
        let mapped = wechat_audit_status_to_release(status).ok_or_else(|| {
            DeployServiceError::Internal(format!("unknown WeChat audit status {status}"))
        })?;
        Ok(MiniProgramAuditObservation {
            status: mapped,
            detail: None,
        })
    }

    async fn release_mini_program(
        &self,
        request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
        self.client
            .invoke(&request.authorization_id, CAPABILITY_MP_RELEASE, json!({}))
            .await?;
        Ok(MiniProgramReleaseOutcome {
            released_version: request.provider_version.clone(),
        })
    }

    async fn rollback_mini_program(
        &self,
        request: &MiniProgramRollbackRequest,
    ) -> DeployServiceResult<()> {
        self.client
            .invoke(
                &request.authorization_id,
                CAPABILITY_MP_ROLLBACK,
                json!({ "appVersion": request.target_version }),
            )
            .await?;
        Ok(())
    }

    async fn gray_release_mini_program(
        &self,
        request: &MiniProgramGrayReleaseRequest,
    ) -> DeployServiceResult<()> {
        self.client
            .invoke(
                &request.authorization_id,
                CAPABILITY_MP_GRAY_RELEASE,
                json!({ "grayPercentage": request.gray_percentage }),
            )
            .await?;
        Ok(())
    }

    async fn modify_mini_program_domains(
        &self,
        authorization_id: &str,
        action: &str,
        domains: &[String],
    ) -> DeployServiceResult<()> {
        self.client
            .invoke(
                authorization_id,
                CAPABILITY_MP_DOMAINS_MODIFY,
                json!({ "action": action, "requestDomains": domains }),
            )
            .await?;
        Ok(())
    }

    async fn set_mini_program_privacy(
        &self,
        authorization_id: &str,
        privacy: Value,
    ) -> DeployServiceResult<()> {
        self.client
            .invoke(authorization_id, CAPABILITY_MP_PRIVACY_SET, privacy)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_capability_client::{
        ChannelCapabilityTransport, IamChannelCapabilityClient, StaticChannelCapabilityBearerAuth,
    };
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    struct FakeTransport {
        body: String,
        last: Mutex<Option<(String, Value)>>,
    }

    impl FakeTransport {
        fn reply(result: Value) -> Self {
            Self {
                body: serde_json::json!({ "code": 0, "data": { "item": { "result": result } } })
                    .to_string(),
                last: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl ChannelCapabilityTransport for FakeTransport {
        async fn post_json(
            &self,
            url: &str,
            _bearer: &str,
            body: &Value,
        ) -> DeployServiceResult<(u16, String)> {
            *self.last.lock().expect("last") = Some((url.to_string(), body.clone()));
            Ok((200, self.body.clone()))
        }
    }

    fn provider(
        result: Value,
    ) -> (
        IamChannelCapabilityChannelReleaseProvider,
        Arc<FakeTransport>,
    ) {
        let transport = Arc::new(FakeTransport::reply(result));
        let client = IamChannelCapabilityClient::new(
            "https://iam.example",
            Arc::new(StaticChannelCapabilityBearerAuth::new("bearer-1")),
            transport.clone(),
        );
        (
            IamChannelCapabilityChannelReleaseProvider::new(client),
            transport,
        )
    }

    fn upload_request() -> MiniProgramUploadRequest {
        MiniProgramUploadRequest {
            authorization_id: "auth-1".to_string(),
            environment: "production".to_string(),
            template_id: Some("tpl-1".to_string()),
            provider_version: Some("1.0.0".to_string()),
            ext_json: Some(json!({"extAppid": "wx1"})),
        }
    }

    #[tokio::test]
    async fn upload_maps_the_template_into_the_commit_capability() {
        let (provider, transport) = provider(json!({"committed": true, "commitId": "tpl-1:1.0.0"}));
        let outcome = provider
            .upload_mini_program(&upload_request())
            .await
            .expect("upload");
        assert_eq!(outcome.commit_id, "tpl-1:1.0.0");
        let (url, body) = transport.last.lock().expect("last").clone().expect("call");
        assert!(url.ends_with("/capability_invocations"));
        assert_eq!(body["capability"], CAPABILITY_MP_CODE_COMMIT);
        assert_eq!(body["payload"]["templateId"], "tpl-1");
        assert_eq!(body["payload"]["userVersion"], "1.0.0");
        assert_eq!(body["payload"]["extJson"], r#"{"extAppid":"wx1"}"#);
    }

    #[tokio::test]
    async fn upload_without_a_template_is_rejected_before_the_call() {
        let (provider, transport) = provider(json!({"commitId": "x"}));
        let mut request = upload_request();
        request.template_id = None;
        assert!(provider.upload_mini_program(&request).await.is_err());
        assert!(transport.last.lock().expect("last").is_none());
    }

    #[tokio::test]
    async fn submit_requires_review_items() {
        let (provider, _transport) = provider(json!({"auditId": "audit-1"}));
        let base = MiniProgramAuditSubmission {
            authorization_id: "auth-1".to_string(),
            commit_id: "tpl-1:1.0.0".to_string(),
            provider_version: "1.0.0".to_string(),
            items: Vec::new(),
        };
        assert!(provider.submit_mini_program_audit(&base).await.is_err());
        let with_items = MiniProgramAuditSubmission {
            items: vec![json!({"address": "pages/index"})],
            ..base
        };
        let outcome = provider
            .submit_mini_program_audit(&with_items)
            .await
            .expect("submit");
        assert_eq!(outcome.audit_id, "audit-1");
    }

    #[tokio::test]
    async fn release_and_rollback_map_the_lifecycle_actions() {
        let (provider, transport) = provider(json!({"released": true}));
        let outcome = provider
            .release_mini_program(&MiniProgramReleaseRequest {
                authorization_id: "auth-1".to_string(),
                provider_version: Some("1.0.0".to_string()),
            })
            .await
            .expect("release");
        assert_eq!(outcome.released_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            transport
                .last
                .lock()
                .expect("last")
                .as_ref()
                .expect("call")
                .1["capability"],
            CAPABILITY_MP_RELEASE
        );

        provider
            .rollback_mini_program(&MiniProgramRollbackRequest {
                authorization_id: "auth-1".to_string(),
                target_version: "0.9.0".to_string(),
            })
            .await
            .expect("rollback");
        let (_, body) = transport.last.lock().expect("last").clone().expect("call");
        assert_eq!(body["capability"], CAPABILITY_MP_ROLLBACK);
        assert_eq!(body["payload"]["appVersion"], "0.9.0");
    }

    #[tokio::test]
    async fn gray_maps_the_percentage() {
        let (provider, transport) = provider(json!({"grayPercentage": 30}));
        provider
            .gray_release_mini_program(&MiniProgramGrayReleaseRequest {
                authorization_id: "auth-1".to_string(),
                gray_percentage: 30,
            })
            .await
            .expect("gray");
        let (_, body) = transport.last.lock().expect("last").clone().expect("call");
        assert_eq!(body["capability"], CAPABILITY_MP_GRAY_RELEASE);
        assert_eq!(body["payload"]["grayPercentage"], 30);
    }

    #[tokio::test]
    async fn audit_query_maps_the_provider_status() {
        let (provider, transport) = provider(json!({"status": 2}));
        let observation = provider
            .query_mini_program_audit("auth-1", "audit-1")
            .await
            .expect("query");
        assert_eq!(
            observation.status,
            sdkwork_deploy_contract::ChannelReleaseStatus::InReview
        );
        let (_, body) = transport.last.lock().expect("last").clone().expect("call");
        assert_eq!(body["capability"], CAPABILITY_MP_AUDIT_STATUS);
        assert_eq!(body["payload"]["auditId"], "audit-1");
    }

    #[tokio::test]
    async fn domains_and_privacy_map_to_their_capabilities() {
        let (domain_provider, domain_transport) = provider(json!({"modified": 1}));
        domain_provider
            .modify_mini_program_domains("auth-1", "add", &["https://api.example.com".to_string()])
            .await
            .expect("domains");
        let (_, body) = domain_transport
            .last
            .lock()
            .expect("last")
            .clone()
            .expect("call");
        assert_eq!(body["capability"], CAPABILITY_MP_DOMAINS_MODIFY);
        assert_eq!(body["payload"]["action"], "add");
        assert_eq!(
            body["payload"]["requestDomains"][0],
            "https://api.example.com"
        );

        let (privacy_provider, privacy_transport) = provider(json!({"updated": true}));
        privacy_provider
            .set_mini_program_privacy("auth-1", json!({"privacy_ver": 1}))
            .await
            .expect("privacy");
        let (_, body) = privacy_transport
            .last
            .lock()
            .expect("last")
            .clone()
            .expect("call");
        assert_eq!(body["capability"], CAPABILITY_MP_PRIVACY_SET);
        assert_eq!(body["payload"]["privacy_ver"], 1);
    }

    #[tokio::test]
    async fn audit_query_rejects_an_unknown_provider_status() {
        let (provider, _transport) = provider(json!({"status": 99}));
        assert!(provider
            .query_mini_program_audit("auth-1", "audit-1")
            .await
            .is_err());
    }
}

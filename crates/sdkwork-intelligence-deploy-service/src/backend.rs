//! Backend-api service surface implementation.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    next_order_transition, AcmeAccountPage, AcmeAccountResponse, BuildQueuePage,
    CertificateChallengePage, CertificateOrderPage, CertificateOrderResponse,
    CreateNginxConfigRequest, CreateNodeClusterRequest, CreateServerRequest, DeployBackendApi,
    DeployBackendRequestContext, DeployServiceError, DeployServiceResult,
    EntitlementProjectionPage, IngestUsageEventsRequest, ListNginxConfigsQuery,
    RequestCertificateOrderRequest, RetentionRunRequest, RetentionRunResponse, RunnerHealthPage,
    SigningIdentityHealthPage, SourceEventIngestResponse, SourceEventPage,
    StoreCertificateVersionRequest, UpdateNginxConfigRequest, UpdateNodeClusterRequest,
    UpdateServerRequest, UsageIngestResult, UsageReconciliationRequest,
    UsageReconciliationResponse,
};

use crate::certificate_material::{seal_issued_material, DeclaredCertificateEvidence};
use crate::DeployService;

impl DeployService {
    fn backend_tenant_scope(
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<Option<i64>> {
        Ok(context.tenant_id)
    }

    fn backend_write_tenant(context: &DeployBackendRequestContext) -> DeployServiceResult<i64> {
        context
            .tenant_id
            .filter(|tenant_id| *tenant_id > 0)
            .ok_or(DeployServiceError::validation(
                "tenant context is required for backend write operations",
            ))
    }
}

#[async_trait]
impl DeployBackendApi for DeployService {
    async fn list_nginx_configs(
        &self,
        context: &DeployBackendRequestContext,
        query: &ListNginxConfigsQuery,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxConfigPage> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository.list_nginx_configs(tenant_id, query).await
    }

    async fn create_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateNginxConfigRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxConfigResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .create_nginx_config(tenant_id, request)
            .await
    }

    async fn retrieve_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxConfigResponse> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository
            .retrieve_nginx_config(tenant_id, config_id)
            .await
    }

    async fn update_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxConfigResponse> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository
            .update_nginx_config(tenant_id, config_id, request)
            .await
    }

    async fn validate_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxValidateResponse> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository
            .validate_nginx_config(tenant_id, config_id)
            .await
    }

    async fn deploy_nginx_config(
        &self,
        context: &DeployBackendRequestContext,
        config_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxConfigResponse> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository
            .deploy_nginx_config(tenant_id, config_id)
            .await
    }

    async fn reload_nginx(
        &self,
        _context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxReloadResponse> {
        self.repository.reload_nginx().await
    }

    async fn retrieve_nginx_status(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NginxStatusResponse> {
        let tenant_id = Self::backend_tenant_scope(context)?;
        self.repository.retrieve_nginx_status(tenant_id).await
    }

    async fn list_servers(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
        cluster_id: Option<String>,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ServerPage> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_servers(tenant_id, page, page_size, cluster_id)
            .await
    }

    async fn create_server(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateServerRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ServerResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository.create_server(tenant_id, request).await
    }

    async fn update_server(
        &self,
        context: &DeployBackendRequestContext,
        server_id: &str,
        request: &UpdateServerRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::ServerResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .update_server(tenant_id, server_id, request)
            .await
    }

    async fn list_node_clusters(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NodeClusterPage> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_node_clusters(tenant_id, page, page_size)
            .await
    }

    async fn create_node_cluster(
        &self,
        context: &DeployBackendRequestContext,
        request: &CreateNodeClusterRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NodeClusterResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .create_node_cluster(tenant_id, request)
            .await
    }

    async fn update_node_cluster(
        &self,
        context: &DeployBackendRequestContext,
        cluster_id: &str,
        request: &UpdateNodeClusterRequest,
    ) -> DeployServiceResult<sdkwork_deploy_contract::NodeClusterResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .update_node_cluster(tenant_id, cluster_id, request)
            .await
    }

    async fn list_audit_logs(
        &self,
        context: &DeployBackendRequestContext,
        query: &sdkwork_deploy_contract::AuditLogQuery,
        cursor: Option<&str>,
    ) -> DeployServiceResult<sdkwork_deploy_contract::AuditLogPage> {
        // 审计日志是租户私有数据：无租户上下文的令牌必须被拒绝，绝不回退为
        // 全库枚举（repository 层的 None 分支同时 fail-closed）。
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_audit_logs(Some(tenant_id), query, cursor)
            .await
    }

    async fn list_entitlement_projections(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EntitlementProjectionPage> {
        // Platform management surface: tenant-scoped when the token carries a
        // tenant, platform-wide otherwise (platform operators).
        self.repository
            .list_entitlement_projections(context.tenant_id, page, page_size)
            .await
    }

    async fn list_build_queue(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildQueuePage> {
        self.repository
            .list_build_queue(context.tenant_id, page, page_size)
            .await
    }

    async fn list_runner_health(
        &self,
        _context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<RunnerHealthPage> {
        self.repository.list_runner_health(page, page_size).await
    }
    // -- TLS control plane orchestration (TECH-cloud-site-publishing §4.5) ---------

    async fn create_acme_account(
        &self,
        context: &DeployBackendRequestContext,
        request: &sdkwork_deploy_contract::CreateAcmeAccountRequest,
    ) -> DeployServiceResult<AcmeAccountResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        if !matches!(
            request.ca_profile.as_str(),
            "LETS_ENCRYPT_STAGING" | "LETS_ENCRYPT_PRODUCTION"
        ) {
            return Err(DeployServiceError::validation(
                "caProfile must be LETS_ENCRYPT_STAGING or LETS_ENCRYPT_PRODUCTION",
            ));
        }
        if !request.directory_url.starts_with("https://") || request.directory_url.len() > 2048 {
            return Err(DeployServiceError::validation(
                "directoryUrl must be an https URL (max 2048 characters)",
            ));
        }
        if request.contact_email.is_empty() || request.contact_email.len() > 320 {
            return Err(DeployServiceError::validation(
                "contactEmail must be 1..=320 characters",
            ));
        }
        if let Some(digest) = request.external_account_digest.as_deref() {
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(DeployServiceError::validation(
                    "externalAccountDigest must be 64 hexadecimal characters",
                ));
            }
        }
        self.repository
            .create_acme_account(tenant_id, request)
            .await
    }

    async fn list_acme_accounts(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AcmeAccountPage> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_acme_accounts(tenant_id, page, page_size)
            .await
    }

    async fn request_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        request: &RequestCertificateOrderRequest,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        // Delegated rather than reimplemented: the renewal sweep opens orders
        // through the same function, and a second copy of the CAA pre-flight here
        // would let the two paths drift until only one of them still checked.
        self.open_certificate_order(tenant_id, request).await
    }

    async fn advance_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        // Discover the current state, then advance exactly one canonical step.
        // The retry loop absorbs a concurrent worker that advanced the order
        // between the read and the optimistic UPDATE.
        for _ in 0..2 {
            let current = self
                .repository
                .retrieve_certificate_order(tenant_id, order_id)
                .await?
                .status;
            let Some(next) = next_order_transition(&current) else {
                return Err(DeployServiceError::conflict(format!(
                    "certificate order {order_id} is at terminal state {current}"
                )));
            };
            let applied = self
                .repository
                .advance_certificate_order(tenant_id, order_id, &current, next)
                .await?;
            if applied == next {
                return self
                    .repository
                    .retrieve_certificate_order(tenant_id, order_id)
                    .await;
            }
        }
        Err(DeployServiceError::conflict(format!(
            "certificate order {order_id} is contended; retry"
        )))
    }

    async fn fail_certificate_order(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        error_code: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::backend_write_tenant(context)?;
        if error_code.is_empty() || error_code.len() > 64 {
            return Err(DeployServiceError::validation(
                "errorCode must be 1..=64 characters",
            ));
        }
        self.repository
            .fail_certificate_order(tenant_id, order_id, error_code)
            .await
    }

    async fn record_challenge_result(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        challenge_id: Option<&str>,
        valid: bool,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .record_challenge_result(tenant_id, order_id, challenge_id, valid, None)
            .await
    }

    async fn store_certificate_version(
        &self,
        context: &DeployBackendRequestContext,
        request: &StoreCertificateVersionRequest,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let tenant_id = Self::backend_write_tenant(context)?;
        if request.version_no <= 0 {
            return Err(DeployServiceError::validation("versionNo must be positive"));
        }
        if !matches!(request.key_algorithm.as_str(), "RSA" | "ECDSA") {
            return Err(DeployServiceError::validation(
                "keyAlgorithm must be RSA or ECDSA",
            ));
        }
        if !(request.secret_bundle_ref.starts_with("secret://")
            || request.secret_bundle_ref.starts_with("file:"))
        {
            return Err(DeployServiceError::validation(
                "secretBundleRef must be a secret:// or file: reference",
            ));
        }
        if chrono::DateTime::parse_from_rfc3339(&request.not_before).is_err()
            || chrono::DateTime::parse_from_rfc3339(&request.not_after).is_err()
        {
            return Err(DeployServiceError::validation(
                "notBefore/notAfter must be RFC3339 timestamps",
            ));
        }

        // The version's identity must exist before the material is sealed: the
        // AAD binds each sealed file to this uuid and the row is written under
        // the same value, so the two cannot drift.
        let certificate_version_uuid = sdkwork_database_id::uuid_v4();
        let material = seal_issued_material(
            &request.material,
            &DeclaredCertificateEvidence {
                serial_sha256: &request.serial_sha256,
                fingerprint_sha256: &request.fingerprint_sha256,
                spki_sha256: &request.spki_sha256,
                chain_sha256: &request.chain_sha256,
                issuer: &request.issuer,
                subject: &request.subject,
                key_algorithm: &request.key_algorithm,
                not_before: &request.not_before,
                not_after: &request.not_after,
            },
            &certificate_version_uuid,
            self.certificate_material_key.as_deref(),
            self.certificate_trust_anchors.as_deref(),
        )?;

        self.repository
            .store_certificate_version(
                tenant_id,
                &certificate_version_uuid,
                &request.order_id,
                request.version_no,
                &request.serial_sha256,
                &request.fingerprint_sha256,
                &request.spki_sha256,
                &request.chain_sha256,
                &request.issuer,
                &request.subject,
                &request.key_algorithm,
                &request.not_before,
                &request.not_after,
                &request.secret_bundle_ref,
                &material,
            )
            .await
    }

    async fn list_certificate_orders(
        &self,
        context: &DeployBackendRequestContext,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateOrderPage> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_certificate_orders(tenant_id, certificate_id, page, page_size)
            .await
    }

    async fn list_certificate_challenges(
        &self,
        context: &DeployBackendRequestContext,
        order_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateChallengePage> {
        let tenant_id = Self::backend_write_tenant(context)?;
        self.repository
            .list_certificate_challenges(tenant_id, order_id, page, page_size)
            .await
    }

    async fn run_retention(
        &self,
        context: &DeployBackendRequestContext,
        request: &RetentionRunRequest,
    ) -> DeployServiceResult<RetentionRunResponse> {
        // Retention windows come from platform configuration; zero/absent
        // windows disable that dimension. Dry runs are the safe default.
        let package_days = retention_days("SDKWORK_DEPLOY_RETENTION_PACKAGE_DAYS");
        let release_days = retention_days("SDKWORK_DEPLOY_RETENTION_RELEASE_DAYS");
        let log_days = retention_days("SDKWORK_DEPLOY_RETENTION_BUILD_LOG_DAYS");
        if package_days == 0 && release_days == 0 && log_days == 0 {
            return Err(DeployServiceError::validation(
                "no retention windows configured (SDKWORK_DEPLOY_RETENTION_*_DAYS)",
            ));
        }
        let _ = context;
        self.repository
            .run_retention(request.dry_run, package_days, release_days, log_days)
            .await
    }

    async fn ingest_usage_events(
        &self,
        _context: &DeployBackendRequestContext,
        request: &IngestUsageEventsRequest,
    ) -> DeployServiceResult<UsageIngestResult> {
        if request.events.len() > 10_000 {
            return Err(DeployServiceError::validation(
                "a single usage ingest batch must not exceed 10000 events",
            ));
        }
        if request.events.is_empty() {
            return Ok(UsageIngestResult::default());
        }
        self.repository
            .insert_usage_events_batch(&request.events)
            .await
    }

    async fn rebuild_usage_daily(
        &self,
        _context: &DeployBackendRequestContext,
        request: &UsageReconciliationRequest,
    ) -> DeployServiceResult<UsageReconciliationResponse> {
        self.repository
            .rebuild_usage_daily(
                request.window_start.as_deref(),
                request.window_end.as_deref(),
            )
            .await
    }

    async fn list_signing_identity_health(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityHealthPage> {
        self.repository
            .list_signing_identity_health(context.tenant_id, page, page_size)
            .await
    }
    async fn ingest_source_event(
        &self,
        _context: &DeployBackendRequestContext,
        payload: &[u8],
        signature: Option<&str>,
    ) -> DeployServiceResult<SourceEventIngestResponse> {
        // Signature verification: GitHub sends X-Hub-Signature-256 as
        // "sha256=<hex hmac-sha256(secret, raw body)>". The platform secret is
        // required; without it the endpoint fails closed (an unauthenticated
        // webhook would allow anyone to burn build capacity).
        let secret = std::env::var("SDKWORK_DEPLOY_WEBHOOK_SECRET").unwrap_or_default();
        if secret.is_empty() {
            return Err(DeployServiceError::forbidden(
                "SDKWORK_DEPLOY_WEBHOOK_SECRET is not configured; webhook ingestion is disabled",
            ));
        }
        let Some(signature) = signature else {
            return Err(DeployServiceError::forbidden(
                "missing X-Hub-Signature-256 header",
            ));
        };
        let expected = signature
            .strip_prefix("sha256=")
            .ok_or_else(|| DeployServiceError::forbidden("X-Hub-Signature-256 must use sha256="))?;
        let actual = sdkwork_utils_rust::hmac_sha256(payload, secret.as_bytes());
        if !sdkwork_utils_rust::secure_compare(expected, &actual) {
            return Err(DeployServiceError::forbidden(
                "webhook signature mismatch; payload rejected",
            ));
        }

        // Parse the GitHub push payload subset.
        let parsed: serde_json::Value = serde_json::from_slice(payload).map_err(|error| {
            DeployServiceError::validation(format!("invalid webhook payload: {error}"))
        })?;
        let source_ref = parsed
            .get("ref")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| DeployServiceError::validation("payload is missing ref"))?;
        let clone_url = parsed
            .pointer("/repository/clone_url")
            .or_else(|| parsed.pointer("/repository/html_url"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| DeployServiceError::validation("payload is missing repository url"))?;
        let source_commit = parsed
            .pointer("/head_commit/id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| DeployServiceError::validation("payload is missing head_commit.id"))?;
        if source_commit.len() < 7
            || source_commit.len() > 64
            || !source_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(DeployServiceError::validation(
                "head_commit.id must be a hex commit SHA",
            ));
        }
        let commit_message = parsed
            .pointer("/head_commit/message")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.chars().take(2000).collect::<String>());
        let sender = parsed
            .pointer("/pusher/name")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.chars().take(512).collect::<String>());

        // Match the repository, then ingest (deduplicated per commit).
        let Some(matched) = self.repository.match_repository_by_url(clone_url).await? else {
            return Err(DeployServiceError::not_found(
                "no bound source repository matches the webhook payload",
            ));
        };
        let payload_sha256 = sdkwork_utils_rust::sha256_hash(payload);
        let (event, fresh) = self
            .repository
            .ingest_source_event(
                &matched,
                "PUSH",
                source_ref,
                source_commit,
                commit_message.as_deref(),
                sender.as_deref(),
                &payload_sha256,
            )
            .await?;
        if !fresh {
            // Redelivered webhook: report the existing outcome.
            return Ok(SourceEventIngestResponse {
                event_id: event.id,
                event_status: event.event_status,
                builds_triggered: event.builds_triggered,
                duplicate: true,
            });
        }

        // Trigger builds only for the default branch (feature branches are
        // recorded as SKIPPED for traceability).
        let expected_ref = format!("refs/heads/{}", matched.default_branch);
        if source_ref != expected_ref {
            self.repository
                .update_source_event_result(matched.tenant_id, &event.id, false, 0, None)
                .await?;
            return Ok(SourceEventIngestResponse {
                event_id: event.id,
                event_status: "SKIPPED".to_owned(),
                builds_triggered: 0,
                duplicate: false,
            });
        }

        // Trigger one build per active target with a governed template.
        let mut triggered = 0_i32;
        let targets = self
            .repository
            .list_trigger_targets(&matched.app_id)
            .await?;
        for target in &targets {
            let request = sdkwork_deploy_contract::CreateBuildRequest {
                platform_target_id: target.platform_target_id.clone(),
                source_repository_id: Some(matched.repository_id.clone()),
                source_ref: Some(source_ref.to_owned()),
                template_id: Some(target.template_id.clone()),
                semantic_version: None,
                idempotency_key: format!("event:{}:{}", event.id, target.platform_target_id),
            };
            match self
                .repository
                .create_build(matched.tenant_id, None, &request)
                .await
            {
                Ok(_) => triggered += 1,
                Err(error) => {
                    tracing::warn!(
                        "build trigger skipped for event {} target {}: {error}",
                        event.id,
                        target.platform_target_id
                    );
                }
            }
        }
        self.repository
            .update_source_event_result(matched.tenant_id, &event.id, true, triggered, None)
            .await?;
        Ok(SourceEventIngestResponse {
            event_id: event.id,
            event_status: "PROCESSED".to_owned(),
            builds_triggered: triggered,
            duplicate: false,
        })
    }

    async fn list_source_events(
        &self,
        context: &DeployBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SourceEventPage> {
        self.repository
            .list_source_events(context.tenant_id, page, page_size)
            .await
    }
}

/// Reads a retention window from platform configuration; absent or invalid
/// values disable the dimension (0 = no retention).
fn retention_days(key: &str) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|days| *days > 0)
        .unwrap_or(0)
}

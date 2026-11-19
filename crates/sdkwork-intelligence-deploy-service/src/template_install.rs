//! Free, Git-backed template installation.
//!
//! A marketplace listing names a source application (`app_uuid`). Installing it
//! creates a new application of the same kind and copies the source
//! application's Git remix declaration onto it. The copy is a *current source*
//! remix, not a frozen artifact: archive-backed and paid templates are refused
//! until artifact sharing and order/payment entitlement are designed, so this
//! entry never creates an app whose source it cannot deliver.

use sdkwork_deploy_contract::{
    AppKind, AppSourceGitInput, AppSourceKind, AppTemplateInstallResponse, CreateAppRequest,
    DeployAppApi, DeployAppRequestContext, DeployServiceError, DeployServiceResult,
    InstallAppTemplateRequest, UpdateAppSourceRequest, TEMPLATE_PRICING_FREE, TEMPLATE_TYPE_APP,
};

use crate::DeployService;

impl DeployService {
    /// Installs a free, Git-backed template as a new application.
    ///
    /// Access is the same marketplace read the listing uses: the author sees
    /// every state, a consumer sees `PUBLIC` + `PUBLISHED` only. The source
    /// application must be readable by the same caller; a personal source app
    /// therefore cannot back a public template for another user.
    ///
    /// @param context - caller tenant/user.
    /// @param template_uuid - marketplace listing id.
    /// @param idempotency_key - replay key for the created application.
    /// @param request - optional name/slug overrides.
    /// @returns the created application.
    pub async fn install_app_template(
        &self,
        context: &DeployAppRequestContext,
        template_uuid: &str,
        idempotency_key: &str,
        request: &InstallAppTemplateRequest,
    ) -> DeployServiceResult<AppTemplateInstallResponse> {
        let template = DeployAppApi::retrieve_app_template(self, context, template_uuid).await?;
        ensure_template_installable(
            template.template_type.as_str(),
            template.pricing_model.as_str(),
            template.app_uuid.as_str(),
        )?;

        // Validate the source before creating anything: a created app whose
        // source cannot be copied is a dead application in the catalog.
        let source_app = self
            .retrieve_app(context, template.app_uuid.as_str())
            .await?;
        let source = self
            .retrieve_app_source(context, template.app_uuid.as_str())
            .await?;
        let git = match source.source_kind {
            Some(AppSourceKind::Git) => source.git,
            Some(AppSourceKind::Archive) => {
                return Err(DeployServiceError::conflict(
                    "archive-backed templates cannot be installed until artifact sharing is available; use a Git-backed template",
                ))
            }
            None => {
                return Err(DeployServiceError::conflict(
                    "template source application has no configured source",
                ))
            }
        };
        let git = git.ok_or_else(|| {
            DeployServiceError::conflict("template source application has an incomplete Git source")
        })?;

        let create_request = CreateAppRequest {
            name: request
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(template.display_name.as_str())
                .to_owned(),
            slug: request
                .slug
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            app_kind: AppKind::parse(source_app.app_kind.as_str()).ok_or_else(|| {
                DeployServiceError::conflict(
                    "template source application has an unsupported application kind",
                )
            })?,
            owner_type: None,
            description: Some(template.summary.clone()).filter(|value| !value.trim().is_empty()),
            app_type: None,
            runtime_config: None,
            metadata: Some(serde_json::json!({
                "template": {
                    "uuid": template.id,
                    "versionUuid": template.latest_version_uuid,
                }
            })),
            default_environment: Some(source_app.default_environment.clone()),
            app_domain_label: None,
            app_domain_suffixes: None,
            source_specs: None,
            idempotency_key: None,
        };
        let app = self
            .create_app(context, Some(idempotency_key), &create_request)
            .await?;

        let update = UpdateAppSourceRequest {
            kind: AppSourceKind::Git,
            git: Some(AppSourceGitInput {
                repo_provider: git.repo_provider,
                repo_url: git.repo_url,
                repo_ref: Some(git.repo_ref),
                sub_path: git.sub_path,
                credential_secret_ref: git.credential_secret_ref,
                clone_mode: Some(git.clone_mode),
            }),
            archive: None,
        };
        self.update_app_source(context, &app.id, &update)
            .await
            .map_err(|error| {
                DeployServiceError::Internal(format!(
                    "application {} was created but its template source could not be copied: {error}",
                    app.id
                ))
            })?;
        self.repository
            .record_app_template_install(
                Self::tenant_id(context)?,
                template.id.as_str(),
                app.id.as_str(),
            )
            .await?;
        self.audit_app_action(context, "app.template.install", &app.id)
            .await?;

        Ok(AppTemplateInstallResponse {
            app,
            template_uuid: template.id,
        })
    }
}

/// Refuses a listing this install path cannot deliver.
///
/// Extracted so the commercial refusal (paid, non-APP, missing source) is
/// testable without a database, and so the service cannot drift from it.
pub(crate) fn ensure_template_installable(
    template_type: &str,
    pricing_model: &str,
    app_uuid: &str,
) -> DeployServiceResult<()> {
    if template_type != TEMPLATE_TYPE_APP {
        return Err(DeployServiceError::validation(
            "only APP templates can be installed as an application",
        ));
    }
    if pricing_model != TEMPLATE_PRICING_FREE {
        return Err(DeployServiceError::conflict(
            "paid template installation requires the order/payment entitlement flow and is not available yet",
        ));
    }
    if app_uuid.trim().is_empty() {
        return Err(DeployServiceError::conflict(
            "template has no source application to remix",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_template_installable;

    #[test]
    fn only_free_app_templates_with_a_source_are_installable() {
        assert!(ensure_template_installable("APP", "FREE", "app-1").is_ok());
        assert!(ensure_template_installable("PPT", "FREE", "app-1").is_err());
        assert!(ensure_template_installable("VIDEO", "FREE", "app-1").is_err());
        assert!(ensure_template_installable("APP", "PAID", "app-1").is_err());
        assert!(ensure_template_installable("APP", "FREE", "  ").is_err());
    }
}

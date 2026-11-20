//! Free scaffold installation.
//!
//! A marketplace listing names a source application (`app_uuid`). Installing it
//! creates a new application of the same kind and copies the source:
//! a Git source is remixed as the source application's **current** declaration;
//! an archive source is cloned into a new artifact attributed to the installed
//! app while keeping the same Drive node/checksum. Paid scaffolds are still
//! refused until the order/payment entitlement is verified at install time.

use sdkwork_deploy_contract::{
    AppKind, AppSourceArchiveInput, AppSourceGitInput, AppSourceGitResponse, AppSourceKind,
    ApplicationScaffoldInstallResponse, CreateAppRequest, DeployAppApi, DeployAppRequestContext,
    DeployServiceError, DeployServiceResult, InstallApplicationScaffoldRequest,
    UpdateAppSourceRequest, SCAFFOLD_PRICING_PAID, SCAFFOLD_TYPE_APP,
};

use crate::DeployService;

impl DeployService {
    /// Installs a free, Git-backed scaffold as a new application.
    ///
    /// Access is the same marketplace read the listing uses: the author sees
    /// every state, a consumer sees `PUBLIC` + `PUBLISHED` only. The source
    /// application must be readable by the same caller; a personal source app
    /// therefore cannot back a public scaffold for another user.
    ///
    /// @param context - caller tenant/user.
    /// @param scaffold_uuid - marketplace listing id.
    /// @param idempotency_key - replay key for the created application.
    /// @param request - optional name/slug overrides.
    /// @returns the created application.
    pub async fn install_application_scaffold(
        &self,
        context: &DeployAppRequestContext,
        scaffold_uuid: &str,
        idempotency_key: &str,
        request: &InstallApplicationScaffoldRequest,
    ) -> DeployServiceResult<ApplicationScaffoldInstallResponse> {
        let scaffold =
            DeployAppApi::retrieve_application_scaffold(self, context, scaffold_uuid).await?;
        ensure_scaffold_installable(
            scaffold.template_type.as_str(),
            scaffold.pricing_model.as_str(),
            scaffold.app_uuid.as_str(),
        )?;
        let paid = scaffold.pricing_model == SCAFFOLD_PRICING_PAID;
        let paid_order_id = if paid {
            let order_id = request
                .order_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DeployServiceError::conflict(
                        "a paid scaffold install requires the settled order id",
                    )
                })?;
            let buyer = Self::require_actor(context)?;
            if !self
                .repository
                .verify_paid_application_scaffold_order(
                    Self::tenant_id(context)?,
                    buyer,
                    order_id,
                    scaffold.id.as_str(),
                )
                .await?
            {
                return Err(DeployServiceError::conflict(
                    "the order is not a settled app-scaffold purchase for this buyer and listing",
                ));
            }
            Some(order_id.to_owned())
        } else {
            None
        };

        // Validate the source before creating anything: a created app whose
        // source cannot be copied is a dead application in the catalog.
        let source_app = self
            .retrieve_app(context, scaffold.app_uuid.as_str())
            .await?;
        let source = self
            .retrieve_app_source(context, scaffold.app_uuid.as_str())
            .await?;
        let version_artifact = if let Some(latest_version_uuid) =
            scaffold.latest_version_uuid.as_deref()
        {
            DeployAppApi::list_application_scaffold_versions(self, context, scaffold_uuid, 1, 100)
                .await?
                .items
                .into_iter()
                .find(|version| version.id == latest_version_uuid)
                .and_then(|version| version.artifact_uuid)
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let version_git_snapshot =
            if let Some(latest_version_uuid) = scaffold.latest_version_uuid.as_deref() {
                self.repository
                    .retrieve_application_scaffold_version_source_snapshot(
                        Self::tenant_id(context)?,
                        latest_version_uuid,
                    )
                    .await?
                    .as_ref()
                    .and_then(git_input_from_snapshot)
            } else {
                None
            };
        let (git, archive_artifact_id) = match source.source_kind {
            Some(AppSourceKind::Git) => (
                version_git_snapshot.or_else(|| source.git.map(git_input_from_source)),
                None,
            ),
            Some(AppSourceKind::Archive) => (
                None,
                version_artifact.or_else(|| {
                    source
                        .archive
                        .map(|archive| archive.artifact_id)
                        .filter(|value| !value.trim().is_empty())
                }),
            ),
            None => {
                return Err(DeployServiceError::conflict(
                    "scaffold source application has no configured source",
                ))
            }
        };
        if git.is_none() && archive_artifact_id.is_none() {
            return Err(DeployServiceError::conflict(
                "scaffold source application has an incomplete source",
            ));
        }

        let create_request = CreateAppRequest {
            name: request
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(scaffold.display_name.as_str())
                .to_owned(),
            slug: request
                .slug
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            app_kind: AppKind::parse(source_app.app_kind.as_str()).ok_or_else(|| {
                DeployServiceError::conflict(
                    "scaffold source application has an unsupported application kind",
                )
            })?,
            owner_type: None,
            description: Some(scaffold.summary.clone()).filter(|value| !value.trim().is_empty()),
            app_type: None,
            runtime_config: None,
            metadata: Some(serde_json::json!({
                "scaffold": {
                    "uuid": scaffold.id,
                    "versionUuid": scaffold.latest_version_uuid,
                    "orderId": paid_order_id,
                }
            })),
            default_environment: Some(source_app.default_environment.clone()),
            app_domain_label: None,
            app_domain_suffixes: None,
            source_specs: None,
            idempotency_key: None,
        };
        let app = self
            .create_app(
                context,
                Some(paid_order_id.as_deref().unwrap_or(idempotency_key)),
                &create_request,
            )
            .await?;

        let update = if let Some(git) = git {
            UpdateAppSourceRequest {
                kind: AppSourceKind::Git,
                git: Some(AppSourceGitInput {
                    repo_provider: git.repo_provider,
                    repo_url: git.repo_url,
                    repo_ref: git.repo_ref,
                    sub_path: git.sub_path,
                    credential_secret_ref: git.credential_secret_ref,
                    clone_mode: git.clone_mode,
                }),
                archive: None,
            }
        } else {
            let source_artifact_id = archive_artifact_id.ok_or_else(|| {
                DeployServiceError::conflict("scaffold source archive has no artifact")
            })?;
            let cloned = self
                .repository
                .clone_marketplace_artifact_for_app(
                    Self::tenant_id(context)?,
                    source_artifact_id.as_str(),
                    app.id.as_str(),
                )
                .await?;
            UpdateAppSourceRequest {
                kind: AppSourceKind::Archive,
                git: None,
                archive: Some(AppSourceArchiveInput {
                    artifact_id: cloned,
                }),
            }
        };
        self.update_app_source(context, &app.id, &update)
            .await
            .map_err(|error| {
                DeployServiceError::Internal(format!(
                    "application {} was created but its scaffold source could not be copied: {error}",
                    app.id
                ))
            })?;
        // A paid order was already counted by the order fulfillment; counting
        // again here would double the marketplace metric.
        if !paid {
            self.repository
                .record_app_scaffold_install(
                    Self::tenant_id(context)?,
                    scaffold.id.as_str(),
                    app.id.as_str(),
                )
                .await?;
        }
        self.audit_app_action(context, "app.scaffold.install", &app.id)
            .await?;

        Ok(ApplicationScaffoldInstallResponse {
            app,
            scaffold_uuid: scaffold.id,
        })
    }
}

/// Refuses a listing this install path cannot deliver.
///
/// Only the artifact shape is checked here; a paid listing's order entitlement
/// is verified separately against `commerce_order`.
pub(crate) fn ensure_scaffold_installable(
    template_type: &str,
    _pricing_model: &str,
    app_uuid: &str,
) -> DeployServiceResult<()> {
    if template_type != SCAFFOLD_TYPE_APP {
        return Err(DeployServiceError::validation(
            "only APP scaffolds can be installed as an application",
        ));
    }
    if app_uuid.trim().is_empty() {
        return Err(DeployServiceError::conflict(
            "scaffold has no source application to remix",
        ));
    }
    Ok(())
}

fn git_input_from_source(git: AppSourceGitResponse) -> AppSourceGitInput {
    AppSourceGitInput {
        repo_provider: git.repo_provider,
        repo_url: git.repo_url,
        repo_ref: Some(git.repo_ref),
        sub_path: git.sub_path,
        credential_secret_ref: git.credential_secret_ref,
        clone_mode: Some(git.clone_mode),
    }
}

fn git_input_from_snapshot(snapshot: &serde_json::Value) -> Option<AppSourceGitInput> {
    let git = snapshot.get("git")?;
    let repo_url = git.get("repoUrl")?.as_str()?.trim();
    if repo_url.is_empty() {
        return None;
    }
    let text = |key: &str| {
        git.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Some(AppSourceGitInput {
        repo_provider: text("repoProvider").unwrap_or_else(|| "GITHUB".to_owned()),
        repo_url: repo_url.to_owned(),
        repo_ref: text("repoRef"),
        sub_path: text("subPath"),
        credential_secret_ref: text("credentialSecretRef"),
        clone_mode: text("cloneMode"),
    })
}

#[cfg(test)]
mod tests {
    use super::{ensure_scaffold_installable, git_input_from_snapshot};

    #[test]
    fn a_version_git_snapshot_becomes_an_install_source() {
        let input = git_input_from_snapshot(&serde_json::json!({
            "git": {
                "repoProvider": "GITHUB",
                "repoUrl": "https://github.com/sdkwork/demo",
                "repoRef": "v1.2.0",
                "cloneMode": "FULL",
                "credentialSecretRef": "vault://git/demo"
            }
        }))
        .expect("snapshot");
        assert_eq!(input.repo_provider, "GITHUB");
        assert_eq!(input.repo_url, "https://github.com/sdkwork/demo");
        assert_eq!(input.repo_ref.as_deref(), Some("v1.2.0"));
        assert_eq!(input.clone_mode.as_deref(), Some("FULL"));
        assert!(git_input_from_snapshot(&serde_json::json!({})).is_none());
    }

    #[test]
    fn only_application_scaffolds_with_a_source_are_installable() {
        assert!(ensure_scaffold_installable("APP", "FREE", "app-1").is_ok());
        assert!(ensure_scaffold_installable("APP", "PAID", "app-1").is_ok());
        assert!(ensure_scaffold_installable("PPT", "FREE", "app-1").is_err());
        assert!(ensure_scaffold_installable("VIDEO", "FREE", "app-1").is_err());
        assert!(ensure_scaffold_installable("APP", "FREE", "  ").is_err());
    }
}

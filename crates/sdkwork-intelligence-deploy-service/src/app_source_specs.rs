//! Application source specs: the service surface for the spec dimension.
//!
//! An app serves several sources — a PC bundle, an H5 bundle, a mini-program
//! bundle — and each is uploaded on its own schedule. That is what a *spec*
//! records. It is deliberately not a *variant*: a variant answers "which
//! delivery of one source does this client get", a spec answers "which source
//! is this". Two independently uploaded codebases are two specs even when they
//! serve the same client class.
//!
//! # How a spec reaches the edge
//!
//! Nothing on the request path knows about specs. Inside the next
//! `apps.composition.update` transaction each active spec is projected into a
//! variant, one `CLIENT_CLASS` rule per declared client class, a resource and a
//! mount, and the compiler validates the result like any hand-authored
//! composition. A spec therefore takes effect when the environment is next
//! composed/published, the same moment every other composition change does —
//! which keeps one owner per transaction instead of a second writer racing the
//! first.
//!
//! # Why binding resolves the source first
//!
//! `bindSource` runs the same content-provider validation as
//! `apps.composition.update` before writing the row. A spec may only carry a
//! provider reference the delivery plane has already accepted, so a bad Drive
//! root is refused while the operator is still looking at the dialog rather
//! than at publish time.

use async_trait::async_trait;
use sdkwork_deploy_content_provider_port::{
    ProviderRequestCredentials, ValidateContentProviderResourceCommand,
    ValidatedContentProviderResource,
};
use sdkwork_deploy_contract::{
    AppResourceDefinition, AppSourceSpecDefinition, AppSourceSpecPage, AppSourceSpecResponse,
    BindAppSourceSpecSourceRequest, CreateAppSourceSpecRequest, DeployAppRequestContext,
    DeployServiceError, DeployServiceResult, UpdateAppSourceSpecRequest,
};
use sdkwork_deploy_runtime_compiler::canonical_sha256_excluding_field;

#[derive(Clone, Debug)]
pub struct ListAppSourceSpecsQuery {
    pub tenant_id: i64,
    pub app_uuid: String,
    /// `None` lists every environment; the console passes one when it is
    /// showing a single environment's routing table.
    pub environment: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CreateAppSourceSpecCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    /// Command identity. `apps.sourceSpecs.create` is declared
    /// `x-sdkwork-idempotent`, so the same key must replay the row the first
    /// attempt wrote instead of colliding on the spec key.
    pub idempotency_key: String,
    /// Hash of the fields that land in the row, so a reused key carrying a
    /// *different* spec is refused instead of answered with the wrong row.
    pub request_sha256: String,
    pub generated_at: String,
    pub request: CreateAppSourceSpecRequest,
}

#[derive(Clone, Debug)]
pub struct UpdateAppSourceSpecCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub spec_uuid: String,
    /// Optimistic-concurrency token, from `If-Match`. Compared against the
    /// spec's own `version`, which the caller has just read.
    pub expected_spec_version: i64,
    pub generated_at: String,
    pub request: UpdateAppSourceSpecRequest,
}

#[derive(Clone, Debug)]
pub struct DeleteAppSourceSpecCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub spec_uuid: String,
    pub generated_at: String,
}

/// The stored provider triple, already resolved against the content provider.
#[derive(Clone, Debug)]
pub struct BindAppSourceSpecSourceCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub spec_uuid: String,
    pub generated_at: String,
    pub resource: ValidatedContentProviderResource,
}

/// Materializes the spec set an `apps.create` call declared, in one
/// transaction.
///
/// Separate from [`CreateAppSourceSpecCommand`] because the two agree on
/// everything except *when* they are allowed: a lone `create` adds one spec to
/// whatever is there, whereas this declares the whole set for an environment
/// that a `create_app` just produced. Both land on the same validated upsert so
/// the two entry points cannot drift into two vocabularies.
#[derive(Clone, Debug)]
pub struct DeclareAppSourceSpecsCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub environment: String,
    pub generated_at: String,
    pub definitions: Vec<AppSourceSpecDefinition>,
}

#[async_trait]
pub trait AppSourceSpecRepositoryPort: Send + Sync {
    async fn list_app_source_specs(
        &self,
        query: ListAppSourceSpecsQuery,
    ) -> DeployServiceResult<AppSourceSpecPage>;

    async fn create_app_source_spec(
        &self,
        command: CreateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse>;

    async fn retrieve_app_source_spec(
        &self,
        tenant_id: i64,
        app_uuid: &str,
        spec_uuid: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse>;

    async fn update_app_source_spec(
        &self,
        command: UpdateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse>;

    async fn delete_app_source_spec(
        &self,
        command: DeleteAppSourceSpecCommand,
    ) -> DeployServiceResult<()>;

    async fn bind_app_source_spec_source(
        &self,
        command: BindAppSourceSpecSourceCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse>;

    /// Replaces one environment's whole spec set, atomically.
    async fn declare_app_source_specs(
        &self,
        command: DeclareAppSourceSpecsCommand,
    ) -> DeployServiceResult<AppSourceSpecPage>;
}

impl crate::DeployService {
    /// Materializes the specs an `apps.create` declared.
    ///
    /// Scoped to `pub(crate)`: the only caller is app creation, and exposing it
    /// as a service method would invite a second "declare a set" entry point
    /// alongside the per-spec CRUD, which is exactly the drift this avoids.
    pub(crate) async fn declare_app_source_specs(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: &str,
        definitions: &[AppSourceSpecDefinition],
    ) -> DeployServiceResult<AppSourceSpecPage> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        self.repository
            .declare_app_source_specs(DeclareAppSourceSpecsCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                environment: environment.to_owned(),
                generated_at: now(),
                definitions: definitions.to_vec(),
            })
            .await
    }

    pub async fn list_app_source_specs(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        if let Some(environment) = environment {
            validate_identifier(environment, 64, "environment")?;
        }
        self.repository
            .list_app_source_specs(ListAppSourceSpecsQuery {
                tenant_id,
                app_uuid: app_id.to_owned(),
                environment: environment.map(str::to_owned),
            })
            .await
    }

    pub async fn create_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        idempotency_key: &str,
        request: &CreateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(idempotency_key, 128, "Idempotency-Key")?;
        // Hashed over the request itself: every field of
        // `CreateAppSourceSpecRequest` is a field of the row, so a retry that
        // changed anything observable is a different command.
        let request_value = serde_json::to_value(request)
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let request_sha256 =
            canonical_sha256_excluding_field(&request_value, "__no_excluded_field")
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let spec = self
            .repository
            .create_app_source_spec(CreateAppSourceSpecCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                idempotency_key: idempotency_key.to_owned(),
                request_sha256,
                generated_at: now(),
                request: request.clone(),
            })
            .await?;
        self.audit_app_action(context, "app.source_spec.create", app_id)
            .await?;
        Ok(spec)
    }

    pub async fn retrieve_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(spec_id, 128, "sourceSpecId")?;
        self.repository
            .retrieve_app_source_spec(tenant_id, app_id, spec_id)
            .await
    }

    pub async fn update_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
        expected_spec_version: i64,
        request: &UpdateAppSourceSpecRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(spec_id, 128, "sourceSpecId")?;
        if expected_spec_version < 1 {
            return Err(DeployServiceError::validation(
                "If-Match must carry the source spec's version, which starts at 1",
            ));
        }
        let spec = self
            .repository
            .update_app_source_spec(UpdateAppSourceSpecCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                spec_uuid: spec_id.to_owned(),
                expected_spec_version,
                generated_at: now(),
                request: request.clone(),
            })
            .await?;
        self.audit_app_action(context, "app.source_spec.update", app_id)
            .await?;
        Ok(spec)
    }

    pub async fn delete_app_source_spec(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(spec_id, 128, "sourceSpecId")?;
        self.repository
            .delete_app_source_spec(DeleteAppSourceSpecCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                spec_uuid: spec_id.to_owned(),
                generated_at: now(),
            })
            .await?;
        self.audit_app_action(context, "app.source_spec.delete", app_id)
            .await?;
        Ok(())
    }

    /// Binds an uploaded source to a spec.
    ///
    /// Resolution happens *before* the row is written: the content provider is
    /// the only thing that can say whether a Drive directory actually exists
    /// and is readable by this tenant, and a spec whose provider reference was
    /// never resolved would fail at publish time instead — far from the action
    /// that caused it.
    pub async fn bind_app_source_spec_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        spec_id: &str,
        request: &BindAppSourceSpecSourceRequest,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(spec_id, 128, "sourceSpecId")?;
        let credentials = ProviderRequestCredentials {
            auth_token: context.auth_token.clone(),
            access_token: context.access_token.clone(),
        };
        let resource = self
            .content_provider
            .validate_resource(
                &credentials,
                ValidateContentProviderResourceCommand {
                    tenant_id,
                    app_uuid: app_id.to_owned(),
                    // The provider only reads `source`; the key is what makes
                    // the command's diagnostics name the thing being validated,
                    // so the spec's own id is used rather than a placeholder.
                    resource: AppResourceDefinition {
                        key: spec_id.to_owned(),
                        source: request.source.clone(),
                    },
                },
            )
            .await?;
        let spec = self
            .repository
            .bind_app_source_spec_source(BindAppSourceSpecSourceCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                spec_uuid: spec_id.to_owned(),
                generated_at: now(),
                resource,
            })
            .await?;
        self.audit_app_action(context, "app.source_spec.bind_source", app_id)
            .await?;
        Ok(spec)
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn validate_identifier(value: &str, maximum_len: usize, field: &str) -> DeployServiceResult<()> {
    if value.is_empty()
        || value.len() > maximum_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(DeployServiceError::validation(format!(
            "{field} is invalid"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_path_separators() {
        assert!(validate_identifier("app-1", 128, "appId").is_ok());
        assert!(validate_identifier("", 128, "appId").is_err());
        assert!(validate_identifier("a/b", 128, "appId").is_err());
        assert!(validate_identifier("a b", 128, "appId").is_err());
    }
}

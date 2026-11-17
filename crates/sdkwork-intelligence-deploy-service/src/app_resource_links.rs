//! Application **resource links**: the account bindings one environment
//! publishes through.
//!
//! `deploy_application_resource_link` is the authority for "which official
//! account, mini program and payment accounts does this environment act as".
//! `deploy_app.metadata.integrations` was the only home for those facts; the
//! ordered migration `0013_deploy_application_resource_link_backfill` normalizes
//! it once, after which every read and write goes through this module.
//!
//! # What the service owns that the database cannot
//!
//! * **Reach.** Every route is addressed by `appId`, so every operation proves
//!   the credential's application scope before touching a row
//!   (`APPLICATION_IDENTITY_SPEC.md` §4.1). The database constraint only knows
//!   `tenant_id`.
//! * **A sentence instead of a SQLSTATE.** The uniqueness rules are real database
//!   constraints (`uk_ref`, `uk_single`, `uk_default`); the service validates the
//!   request first so the operator sees a `40003`/`40901` with a reason instead of
//!   a masked `23505`.
//! * **Default movement.** Setting `isDefault` on one payment account clears it
//!   from the previously default account of the same provider in the same
//!   transaction. `uk_deploy_application_resource_link_default` allows exactly one
//!   default per provider, so "make this one the default" cannot be expressed as a
//!   blind write.
//!
//! # Why the upsert is explicit
//!
//! The write is a `PUT` that replaces the addressed binding
//! ([`sdkwork_deploy_contract::UpsertAppResourceLinkRequest`]): a single-instance
//! kind is addressed by `(environment, kind)` and a payment account by
//! `(environment, kind, resourceRef)`. API_SPEC §15.4 requires upsert behavior to
//! be explicit and forbids hiding it behind a normal create, which is why the
//! operation is `apps.resourceLinks.update` and never `create`.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    AppResourceKind, AppResourceLinkPage, AppResourceLinkResponse, DeployAppRequestContext,
    DeployServiceError, DeployServiceResult, ListAppResourceLinksQuery, NormalizedAppResourceLink,
    UpsertAppResourceLinkRequest,
};

/// The account bindings of one application, in stable order.
#[derive(Clone, Debug)]
pub struct ListAppResourceLinksCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
    /// `None` lists every environment.
    pub environment: Option<String>,
    /// `None` lists every kind.
    pub resource_kind: Option<AppResourceKind>,
    pub page: i32,
    pub page_size: i32,
}

/// Bind or replace one account binding.
#[derive(Clone, Debug)]
pub struct UpsertAppResourceLinkCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    /// Already validated and normalized by the service, so the repository never
    /// re-decides what a valid binding is.
    pub binding: NormalizedAppResourceLink,
}

/// Soft-retire one binding.
#[derive(Clone, Debug)]
pub struct DeleteAppResourceLinkCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
    pub resource_link_uuid: String,
}

#[async_trait]
pub trait AppResourceLinkRepositoryPort: Send + Sync {
    async fn list_app_resource_links(
        &self,
        command: ListAppResourceLinksCommand,
    ) -> DeployServiceResult<AppResourceLinkPage>;

    async fn upsert_app_resource_link(
        &self,
        command: UpsertAppResourceLinkCommand,
    ) -> DeployServiceResult<AppResourceLinkResponse>;

    async fn delete_app_resource_link(
        &self,
        command: DeleteAppResourceLinkCommand,
    ) -> DeployServiceResult<()>;
}

impl crate::DeployService {
    /// The account bindings of one application environment.
    pub async fn list_app_resource_links(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        query: &ListAppResourceLinksQuery,
    ) -> DeployServiceResult<AppResourceLinkPage> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        // The scope gate answers "may this credential address that application";
        // the owner gate answers "may this caller reach that app row". Both run,
        // in that order, exactly as the env-variable surface does.
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .list_app_resource_links(ListAppResourceLinksCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
                environment: query.environment.map(|value| value.as_str().to_owned()),
                resource_kind: query.resource_kind,
                page: query.page,
                page_size: query.page_size,
            })
            .await
    }

    /// Bind an account, or replace the binding the request addresses.
    ///
    /// Idempotent by construction: the second identical request converges on the
    /// same row (and the same natural key), so no `Idempotency-Key` is needed —
    /// the same reasoning `apps.source.update` documents.
    pub async fn upsert_app_resource_link(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpsertAppResourceLinkRequest,
    ) -> DeployServiceResult<AppResourceLinkResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        // Normalizes before any I/O: a request that cannot become a row must not
        // open a transaction, and the validation rules belong to the contract so
        // the pure tests cover exactly what the write path enforces.
        let binding = request.normalize()?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        let link = self
            .repository
            .upsert_app_resource_link(UpsertAppResourceLinkCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0),
                actor_id: context.actor_id.unwrap_or(0),
                app_uuid: app_id.to_owned(),
                binding,
            })
            .await?;
        self.audit_app_action(context, "app.resource_link.upsert", app_id)
            .await?;
        Ok(link)
    }

    /// Soft-retire one binding.
    ///
    /// A retired row keeps its audit trail: "this account used to serve
    /// production" is a question an operator asks after an incident, and a hard
    /// delete answers it with nothing.
    pub async fn delete_app_resource_link(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        resource_link_id: &str,
    ) -> DeployServiceResult<()> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_identifier(app_id, 128, "appId")?;
        validate_identifier(resource_link_id, 128, "resourceLinkId")?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .delete_app_resource_link(DeleteAppResourceLinkCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
                resource_link_uuid: resource_link_id.to_owned(),
            })
            .await?;
        self.audit_app_action(context, "app.resource_link.delete", app_id)
            .await
    }
}

fn validate_identifier(value: &str, maximum_len: usize, field: &str) -> DeployServiceResult<()> {
    if value.trim().is_empty() {
        return Err(DeployServiceError::validation(format!(
            "{field} is required"
        )));
    }
    if value.len() > maximum_len {
        return Err(DeployServiceError::validation(format!(
            "{field} must be at most {maximum_len} characters"
        )));
    }
    Ok(())
}

//! Application **source specs**: storage, and the projection that turns them
//! into runtime variants.
//!
//! # Why a spec is not a variant
//!
//! A `deploy_app_variant` answers "which delivery of one source does this client
//! get"; a spec answers "which source is this". A PC bundle and an H5 bundle are
//! two codebases, so they are two specs — collapsing them into variants of one
//! source would lose the fact that they are uploaded independently.
//!
//! # Why routing lives in its own table
//!
//! The requirement is "one default spec per client class, with a fallback chain
//! per class" (`SDKWORK_DEPLOY_SPEC.md` §8,
//! `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2.1). One spec legitimately
//! serves several classes — an H5 bundle serves `MOBILE` and `TABLET` — so "the
//! default for `MOBILE`" is a fact about the class, not a flag on a spec. Two
//! specs both claiming `MOBILE` would be decided by row order. Hence
//! `deploy_app_source_spec_route`, where a spec names the classes it serves and
//! its rank within each. The class default is the lowest rank for that class,
//! and the database — not this module — enforces that no two specs share a
//! `(class, rank)` pair, so that minimum is always unique.
//!
//! # Where the projection happens
//!
//! The runtime descriptor has no `spec` concept and does not need one. Inside the
//! `apps.composition.update` transaction, each active spec is projected into a
//! Variant (`variantUuid` derived from the spec, label from the spec) plus one
//! `CLIENT_CLASS` VariantRule per stored route, plus a Resource and a Mount. The
//! rule's `priority` is the route's `preference`, so the fallback chain is the
//! ordering: a spec whose source has not been uploaded contributes **no rule**,
//! and the next preference for that class therefore wins. The compiler then
//! validates the result exactly like a hand-authored composition, so there is no
//! second descriptor dialect and the edge needs no change at all.
//!
//! Authoring through the source-spec endpoints and authoring through
//! `apps.composition.update` are therefore additive, never competing: the spec
//! table is the spec dimension's single owner, the composition tables are the
//! composition's, and one transaction writes both. A spec change takes effect
//! when the environment is next composed/published — the same moment every other
//! composition change takes effect.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    AppClientArchitecture, AppClientClass, AppMountHandler, AppSourceBindingResponse,
    AppSourceBindingStatus, AppSourceProviderType, AppSourceSpecDefinition, AppSourceSpecPage,
    AppSourceSpecResponse, AppSourceSpecRouteDefinition, AppSourceSpecStatus, DeployServiceError,
    DeployServiceResult, SdkworkRuntimeTarget,
};
use sdkwork_deploy_runtime_compiler::{RuntimeMount, RuntimeVariant, RuntimeVariantRule};
use sdkwork_deploy_runtime_compiler::{
    RuntimeMountMode, RuntimeMountTranslation, RuntimeProviderReference, RuntimeProviderType,
    RuntimeResource, RuntimeResourceCapabilities,
};
use sdkwork_intelligence_deploy_service::{
    AppSourceSpecRepositoryPort, BindAppSourceSpecSourceCommand, CreateAppSourceSpecCommand,
    DeclareAppSourceSpecsCommand, DeleteAppSourceSpecCommand, ListAppSourceSpecsQuery,
    UpdateAppSourceSpecCommand,
};
use sqlx::{AssertSqlSafe, Postgres, Row, Transaction};

use crate::app_composition::composition_store_error;
use crate::support::new_uuid;
use crate::DeployRepository;

/// The highest `preference` the schema accepts; also the deepest fallback chain.
const MAXIMUM_PREFERENCE: u16 = 255;

/// One client class a spec serves, with its rank in that class's preference
/// order.
///
/// The class default is the *lowest* rank present for that class, which the
/// unique index `uk_deploy_app_source_spec_route_rank` makes unique: it admits
/// at most one spec per `(class, rank)`, so a class can never have two specs
/// claiming the same position. `preference = 0` is the conventional way to
/// declare a default and is what the console should offer.
///
/// Ranks are deliberately allowed to be sparse. A single-spec write sees only
/// part of the environment, so requiring every routed class to already have a
/// rank-0 spec would make the natural authoring order illegal — declaring the PC
/// spec as mobile's fallback before the H5 spec that owns mobile's default
/// exists. The class is served by its lowest rank either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StoredSourceSpecRoute {
    pub(crate) client_class: AppClientClass,
    /// `0` is the class default; higher values are fallbacks tried in order.
    pub(crate) preference: u16,
}

/// A spec as stored, with its internal id kept so the projected ids stay stable
/// across recompiles and a rename never changes routing.
#[derive(Clone, Debug)]
pub(crate) struct StoredSourceSpec {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) spec_key: String,
    pub(crate) label: String,
    pub(crate) runtime_target: String,
    pub(crate) client_architecture: String,
    pub(crate) routes: Vec<StoredSourceSpecRoute>,
    pub(crate) path_prefix: String,
    pub(crate) handler: AppMountHandler,
    pub(crate) index_files: Vec<String>,
    pub(crate) spa_fallback: Option<String>,
    pub(crate) provider_type: Option<String>,
    pub(crate) provider_resource_uuid: Option<String>,
    pub(crate) provider_contract_version: Option<String>,
    pub(crate) source_status: String,
    pub(crate) is_default: bool,
    pub(crate) priority: u16,
    pub(crate) status: String,
    pub(crate) version: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// `index_files_json` is JSONB, so it is projected to text here: the mapper reads
/// it as a string, and decoding a JSONB value straight into a `String` is a type
/// mismatch, not a parse that can be recovered from.
const SPEC_COLUMNS: &str = "id, CAST(uuid AS TEXT) AS uuid, spec_key, label, runtime_target,
     client_architecture, path_prefix, handler_type,
     CAST(index_files_json AS TEXT) AS index_files_json, spa_fallback_path,
     source_provider_type, source_provider_resource_uuid, source_contract_version,
     source_status, is_default, priority, status, version, created_at, updated_at";

const ROUTE_COLUMNS: &str = "spec_id, client_class, preference";

#[async_trait]
impl AppSourceSpecRepositoryPort for DeployRepository {
    async fn list_app_source_specs(
        &self,
        query: ListAppSourceSpecsQuery,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        self.list_app_source_specs_repo(query).await
    }

    async fn create_app_source_spec(
        &self,
        command: CreateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.create_app_source_spec_repo(command).await
    }

    async fn retrieve_app_source_spec(
        &self,
        tenant_id: i64,
        app_uuid: &str,
        spec_uuid: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.retrieve_app_source_spec_repo(tenant_id, app_uuid, spec_uuid)
            .await
    }

    async fn update_app_source_spec(
        &self,
        command: UpdateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.update_app_source_spec_repo(command).await
    }

    async fn delete_app_source_spec(
        &self,
        command: DeleteAppSourceSpecCommand,
    ) -> DeployServiceResult<()> {
        self.delete_app_source_spec_repo(command).await
    }

    async fn bind_app_source_spec_source(
        &self,
        command: BindAppSourceSpecSourceCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        self.bind_app_source_spec_source_repo(command).await
    }

    async fn declare_app_source_specs(
        &self,
        command: DeclareAppSourceSpecsCommand,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        self.declare_app_source_specs_repo(command).await
    }
}

impl DeployRepository {
    async fn list_app_source_specs_repo(
        &self,
        query: ListAppSourceSpecsQuery,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        let app_id =
            crate::support::resolve_app_internal_id(&self.pool, query.tenant_id, &query.app_uuid)
                .await?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SPEC_COLUMNS}
             FROM deploy_app_source_spec
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
               AND ($3::text IS NULL OR environment = $3)
             ORDER BY environment, priority, uuid"
        )))
        .bind(query.tenant_id)
        .bind(app_id)
        .bind(query.environment.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| composition_store_error("list app source specs", error))?;
        let mut specs = rows
            .iter()
            .map(map_stored_spec)
            .collect::<DeployServiceResult<Vec<_>>>()?;
        // One extra query for the whole page rather than one per row: a list of
        // eight specs would otherwise issue eight round trips to render a table
        // that the console refreshes on every edit.
        let routes = sqlx::query(AssertSqlSafe(format!(
            "SELECT {ROUTE_COLUMNS} FROM deploy_app_source_spec_route
             WHERE tenant_id = $1 AND app_id = $2
               AND ($3::text IS NULL OR environment = $3)
             ORDER BY client_class, preference"
        )))
        .bind(query.tenant_id)
        .bind(app_id)
        .bind(query.environment.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| composition_store_error("list app source spec routes", error))?;
        attach_routes(&mut specs, &routes)?;
        let items = specs
            .iter()
            .map(map_source_spec_row)
            .collect::<DeployServiceResult<Vec<_>>>()?;
        Ok(AppSourceSpecPage {
            total: items.len() as i64,
            items,
        })
    }

    async fn create_app_source_spec_repo(
        &self,
        command: CreateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let request = command.request;
        validate_spec_key(&request.spec_key)?;
        validate_label(&request.label)?;
        validate_path_prefix(&request.path_prefix)?;
        let routes = validate_declared_routes(&request.client_class_routes)?;
        validate_spa_fallback(request.handler, request.spa_fallback.as_deref())?;
        let environment = request.environment.as_str();

        let mut transaction = self.begin_spec_transaction().await?;
        let app_id =
            lock_app_internal_id(&mut transaction, command.tenant_id, &command.app_uuid).await?;
        // Replay before insert. A unique index can only say "duplicate"; by the
        // time the insert has lost, the caller has no row to show for a command
        // that did succeed, so a console retrying a create it already performed
        // would be told the spec exists instead of being handed it.
        if let Some(replayed) = load_idempotent_spec(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.idempotency_key,
            &command.request_sha256,
        )
        .await?
        {
            let response = map_source_spec_row(&replayed)?;
            transaction
                .commit()
                .await
                .map_err(|error| composition_store_error("commit spec create replay", error))?;
            return Ok(response);
        }
        let spec_uuid = new_uuid();
        let id = self.next_spec_id()?;
        let index_files_json = serde_json::to_string(&request.index_files)
            .map_err(|_| DeployServiceError::Internal("serialize index files".to_owned()))?;
        sqlx::query(
            "INSERT INTO deploy_app_source_spec (
                id,uuid,tenant_id,organization_id,app_id,environment,spec_key,label,
                runtime_target,client_architecture,path_prefix,handler_type,index_files_json,
                spa_fallback_path,is_default,priority,status,idempotency_key,request_sha256,
                metadata,created_by,updated_by,created_at,updated_at,version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,CAST($13 AS JSONB),
                $14,$15,$16,'ACTIVE',$17,$18,'{}',$19,$19,CAST($20 AS TIMESTAMPTZ),
                CAST($20 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(&spec_uuid)
        .bind(command.tenant_id)
        .bind(command.organization_id)
        .bind(app_id)
        .bind(environment)
        .bind(&request.spec_key)
        .bind(&request.label)
        .bind(request.runtime_target.as_str())
        .bind(request.client_architecture.as_str())
        .bind(&request.path_prefix)
        .bind(handler_name(request.handler))
        .bind(index_files_json)
        .bind(&request.spa_fallback)
        .bind(request.is_default)
        .bind(i32::from(request.priority))
        .bind(&command.idempotency_key)
        .bind(&command.request_sha256)
        .bind(command.actor_id)
        .bind(&command.generated_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| map_spec_write_error("create app source spec", error))?;
        write_spec_routes(
            self,
            &mut transaction,
            command.tenant_id,
            command.organization_id,
            app_id,
            environment,
            id,
            &spec_uuid,
            command.actor_id,
            &command.generated_at,
            &routes,
        )
        .await?;
        let stored = load_spec_by_uuid(&mut transaction, command.tenant_id, app_id, &spec_uuid)
            .await?
            .ok_or_else(|| DeployServiceError::Internal("created spec is missing".to_owned()))?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit app source spec", error))?;
        map_source_spec_row(&stored)
    }

    async fn retrieve_app_source_spec_repo(
        &self,
        tenant_id: i64,
        app_uuid: &str,
        spec_uuid: &str,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let app_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_uuid).await?;
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SPEC_COLUMNS}
             FROM deploy_app_source_spec
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(app_id)
        .bind(spec_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| composition_store_error("retrieve app source spec", error))?
        .ok_or_else(|| DeployServiceError::not_found("source spec not found"))?;
        let mut spec = map_stored_spec(&row)?;
        spec.routes = load_spec_routes(&self.pool, spec.id).await?;
        map_source_spec_row(&spec)
    }

    async fn update_app_source_spec_repo(
        &self,
        command: UpdateAppSourceSpecCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let request = command.request;
        if let Some(label) = request.label.as_deref() {
            validate_label(label)?;
        }
        let routes = match request.client_class_routes.as_ref() {
            Some(value) => Some(validate_declared_routes(value)?),
            None => None,
        };
        if let Some(path_prefix) = request.path_prefix.as_deref() {
            validate_path_prefix(path_prefix)?;
        }
        let mut transaction = self.begin_spec_transaction().await?;
        let app_id =
            lock_app_internal_id(&mut transaction, command.tenant_id, &command.app_uuid).await?;
        let existing = load_spec_by_uuid(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.spec_uuid,
        )
        .await?
        .ok_or_else(|| DeployServiceError::not_found("source spec not found"))?;
        // Checked against the row this transaction has already locked, so two
        // concurrent updates cannot both observe the old version and then both
        // write.
        if command.expected_spec_version != existing.version {
            return Err(DeployServiceError::conflict(
                "source spec version changed; refresh and retry",
            ));
        }
        let handler = request.handler.unwrap_or(existing.handler);
        let spa_fallback = match request.spa_fallback.as_ref() {
            // `Some(None)` is an explicit clear; `None` leaves it untouched.
            Some(value) => value.clone(),
            None => existing.spa_fallback.clone(),
        };
        validate_spa_fallback(handler, spa_fallback.as_deref())?;
        let index_files_json = request
            .index_files
            .as_ref()
            .map(|value| {
                serde_json::to_string(value)
                    .map_err(|_| DeployServiceError::Internal("serialize index files".to_owned()))
            })
            .transpose()?;
        sqlx::query(
            "UPDATE deploy_app_source_spec SET
                label = COALESCE($3, label),
                runtime_target = COALESCE($4, runtime_target),
                client_architecture = COALESCE($5, client_architecture),
                path_prefix = COALESCE($6, path_prefix),
                handler_type = COALESCE($7, handler_type),
                index_files_json = COALESCE(CAST($8 AS JSONB), index_files_json),
                spa_fallback_path = $9,
                is_default = COALESCE($10, is_default),
                priority = COALESCE($11, priority),
                status = COALESCE($12, status),
                updated_by = $13,
                updated_at = CAST($14 AS TIMESTAMPTZ),
                version = version + 1
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(existing.id)
        .bind(command.tenant_id)
        .bind(request.label.as_deref())
        .bind(request.runtime_target.map(SdkworkRuntimeTarget::as_str))
        .bind(
            request
                .client_architecture
                .map(AppClientArchitecture::as_str),
        )
        .bind(request.path_prefix.as_deref())
        .bind(request.handler.map(handler_name))
        .bind(index_files_json)
        .bind(spa_fallback.as_deref())
        .bind(request.is_default)
        .bind(request.priority.map(i32::from))
        .bind(request.status.map(AppSourceSpecStatus::as_str))
        .bind(command.actor_id)
        .bind(&command.generated_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| map_spec_write_error("update app source spec", error))?;
        if let Some(routes) = routes.as_ref() {
            let environment: String =
                sqlx::query_scalar("SELECT environment FROM deploy_app_source_spec WHERE id = $1")
                    .bind(existing.id)
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(|error| composition_store_error("read spec environment", error))?;
            write_spec_routes(
                self,
                &mut transaction,
                command.tenant_id,
                command.organization_id,
                app_id,
                &environment,
                existing.id,
                &command.spec_uuid,
                command.actor_id,
                &command.generated_at,
                routes,
            )
            .await?;
        }
        let stored = load_spec_by_uuid(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.spec_uuid,
        )
        .await?
        .ok_or_else(|| DeployServiceError::Internal("updated spec is missing".to_owned()))?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit app source spec update", error))?;
        map_source_spec_row(&stored)
    }

    async fn delete_app_source_spec_repo(
        &self,
        command: DeleteAppSourceSpecCommand,
    ) -> DeployServiceResult<()> {
        let mut transaction = self.begin_spec_transaction().await?;
        let app_id =
            lock_app_internal_id(&mut transaction, command.tenant_id, &command.app_uuid).await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_app_source_spec
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
        )
        .bind(command.tenant_id)
        .bind(app_id)
        .bind(&command.spec_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| composition_store_error("load source spec for delete", error))?;
        let Some(spec_id) = existing else {
            return Err(DeployServiceError::not_found("source spec not found"));
        };
        sqlx::query(
            "UPDATE deploy_app_source_spec SET deleted_at = CAST($4 AS TIMESTAMPTZ),
                updated_at = CAST($4 AS TIMESTAMPTZ), version = version + 1
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
        )
        .bind(command.tenant_id)
        .bind(app_id)
        .bind(&command.spec_uuid)
        .bind(&command.generated_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| composition_store_error("delete app source spec", error))?;
        // Routes are hard-deleted with the spec. Leaving them behind would keep
        // the class pointing at a spec the environment no longer has, and the
        // projection reads routes by environment, not by "specs that still
        // exist" — so a dangling route would project a rule to a variant that
        // was never emitted and fail compilation.
        delete_spec_routes(&mut transaction, command.tenant_id, spec_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit app source spec delete", error))?;
        Ok(())
    }

    async fn bind_app_source_spec_source_repo(
        &self,
        command: BindAppSourceSpecSourceCommand,
    ) -> DeployServiceResult<AppSourceSpecResponse> {
        let mut transaction = self.begin_spec_transaction().await?;
        let app_id =
            lock_app_internal_id(&mut transaction, command.tenant_id, &command.app_uuid).await?;
        let existing = load_spec_by_uuid(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.spec_uuid,
        )
        .await?
        .ok_or_else(|| DeployServiceError::not_found("source spec not found"))?;
        // A provider resource may back exactly one spec: two specs sharing one
        // Drive root would make "which source is this" ambiguous again.
        let claimed: Option<String> = sqlx::query_scalar(
            "SELECT CAST(uuid AS TEXT) FROM deploy_app_source_spec
             WHERE tenant_id = $1 AND source_provider_type = $2
               AND source_provider_resource_uuid = $3 AND deleted_at IS NULL AND id <> $4
             LIMIT 1",
        )
        .bind(command.tenant_id)
        .bind(provider_type_column(command.resource.provider_type))
        .bind(&command.resource.provider_resource_uuid)
        .bind(existing.id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| composition_store_error("check bound source", error))?;
        if let Some(owner) = claimed {
            return Err(DeployServiceError::conflict(format!(
                "source is already bound to spec {owner}"
            )));
        }
        sqlx::query(
            "UPDATE deploy_app_source_spec SET
                source_provider_type = $3, source_provider_resource_uuid = $4,
                source_contract_version = $5, source_status = 'BOUND',
                source_updated_at = CAST($6 AS TIMESTAMPTZ), updated_by = $7,
                updated_at = CAST($6 AS TIMESTAMPTZ), version = version + 1
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(existing.id)
        .bind(command.tenant_id)
        .bind(provider_type_column(command.resource.provider_type))
        .bind(&command.resource.provider_resource_uuid)
        .bind(&command.resource.provider_contract_version)
        .bind(&command.generated_at)
        .bind(command.actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| map_spec_write_error("bind app source spec source", error))?;
        let stored = load_spec_by_uuid(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.spec_uuid,
        )
        .await?
        .ok_or_else(|| DeployServiceError::Internal("bound spec is missing".to_owned()))?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit source binding", error))?;
        map_source_spec_row(&stored)
    }

    async fn declare_app_source_specs_repo(
        &self,
        command: DeclareAppSourceSpecsCommand,
    ) -> DeployServiceResult<AppSourceSpecPage> {
        let mut transaction = self.begin_spec_transaction().await?;
        let app_id =
            lock_app_internal_id(&mut transaction, command.tenant_id, &command.app_uuid).await?;
        replace_environment_source_specs(
            self,
            &mut transaction,
            command.tenant_id,
            command.organization_id,
            command.actor_id,
            app_id,
            &command.environment,
            &command.generated_at,
            &command.definitions,
        )
        .await?;
        let specs = load_environment_source_specs(
            &mut transaction,
            command.tenant_id,
            app_id,
            &command.environment,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit declared source specs", error))?;
        let items = specs
            .iter()
            .map(map_source_spec_row)
            .collect::<DeployServiceResult<Vec<_>>>()?;
        Ok(AppSourceSpecPage {
            total: items.len() as i64,
            items,
        })
    }

    async fn begin_spec_transaction(&self) -> DeployServiceResult<Transaction<'static, Postgres>> {
        self.pool
            .begin()
            .await
            .map_err(|error| composition_store_error("begin source spec transaction", error))
    }

    fn next_spec_id(&self) -> DeployServiceResult<i64> {
        crate::support::next_id(self.id_generator())
    }

    fn next_spec_route_id(&self) -> DeployServiceResult<i64> {
        crate::support::next_id(self.id_generator())
    }
}

/// Replaces one environment's spec set from a composition request.
///
/// `None` means "leave the spec set alone", which is what keeps a composition
/// update from wiping a spec authored through the source-spec endpoints.
pub(crate) async fn replace_environment_source_specs(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    actor_id: i64,
    app_id: i64,
    environment: &str,
    generated_at: &str,
    definitions: &[AppSourceSpecDefinition],
) -> DeployServiceResult<()> {
    let declared = definitions
        .iter()
        .map(|definition| definition.spec_key.clone())
        .collect::<BTreeSet<_>>();
    if declared.len() != definitions.len() {
        return Err(DeployServiceError::validation(
            "source spec keys must be unique within the request",
        ));
    }
    validate_default_spec(definitions)?;

    // Soft-delete the specs this request no longer declares, then upsert the
    // declared ones. Soft delete (not physical) keeps the projected id stable
    // for a spec that is disabled and re-declared.
    sqlx::query(
        "UPDATE deploy_app_source_spec SET deleted_at = CAST($4 AS TIMESTAMPTZ),
            updated_at = CAST($4 AS TIMESTAMPTZ), version = version + 1
         WHERE tenant_id = $1 AND app_id = $2 AND environment = $3
           AND deleted_at IS NULL AND NOT (spec_key = ANY($5))",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(environment)
    .bind(generated_at)
    .bind(declared.iter().cloned().collect::<Vec<_>>())
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("retire undeclared source specs", error))?;

    // Routes follow the specs: a retired spec must not leave a class pointing at
    // a variant that is no longer projected. Rewritten wholesale below anyway,
    // so this only has to clear the rows whose spec just left the set.
    sqlx::query(
        "DELETE FROM deploy_app_source_spec_route
         WHERE tenant_id = $1 AND app_id = $2 AND environment = $3
           AND spec_id NOT IN (
               SELECT id FROM deploy_app_source_spec
               WHERE tenant_id = $1 AND app_id = $2 AND environment = $3
                 AND deleted_at IS NULL
           )",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(environment)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("retire undeclared source spec routes", error))?;

    // Cleared once, before the upserts: the partial unique index allows only one
    // active app-level default per (app, environment), so promoting a different
    // spec in place would collide with the one already there. Inside the loop
    // each iteration would clear the default the previous one had just written,
    // and a set whose non-default spec was declared last would come back with no
    // default at all.
    sqlx::query(
        "UPDATE deploy_app_source_spec SET is_default = FALSE
         WHERE tenant_id = $1 AND app_id = $2 AND environment = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(environment)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("clear default source spec", error))?;

    for definition in definitions {
        validate_spec_key(&definition.spec_key)?;
        validate_label(&definition.label)?;
        validate_path_prefix(&definition.path_prefix)?;
        let routes = validate_declared_routes(&definition.client_class_routes)?;
        validate_spa_fallback(definition.handler, definition.spa_fallback.as_deref())?;
        let index_files_json = serde_json::to_string(&definition.index_files)
            .map_err(|_| DeployServiceError::Internal("serialize index files".to_owned()))?;
        let existing: Option<(i64, String)> = sqlx::query_as(
            "SELECT id, CAST(uuid AS TEXT) AS uuid FROM deploy_app_source_spec
             WHERE tenant_id = $1 AND app_id = $2 AND environment = $3 AND spec_key = $4
               AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_id)
        .bind(environment)
        .bind(&definition.spec_key)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("load declared source spec", error))?;
        let (spec_id, spec_uuid) = match existing {
            Some((id, uuid)) => {
                sqlx::query(
                    "UPDATE deploy_app_source_spec SET label = $2,
                        runtime_target = $3, client_architecture = $4, path_prefix = $5,
                        handler_type = $6, index_files_json = CAST($7 AS JSONB),
                        spa_fallback_path = $8, is_default = $9, priority = $10,
                        updated_by = $11, updated_at = CAST($12 AS TIMESTAMPTZ),
                        version = version + 1
                     WHERE id = $1",
                )
                .bind(id)
                .bind(&definition.label)
                .bind(definition.runtime_target.as_str())
                .bind(definition.client_architecture.as_str())
                .bind(&definition.path_prefix)
                .bind(handler_name(definition.handler))
                .bind(index_files_json)
                .bind(&definition.spa_fallback)
                .bind(definition.is_default)
                .bind(i32::from(definition.priority))
                .bind(actor_id)
                .bind(generated_at)
                .execute(&mut **transaction)
                .await
                .map_err(|error| map_spec_write_error("update declared source spec", error))?;
                (id, uuid)
            }
            None => {
                let uuid = new_uuid();
                // Held in a local rather than regenerated after the insert: a
                // second generator call returns a *different* number, so the
                // routes written below would reference an id no row has.
                let new_id = repository.next_spec_id()?;
                sqlx::query(
                    "INSERT INTO deploy_app_source_spec (
                        id,uuid,tenant_id,organization_id,app_id,environment,spec_key,label,
                        runtime_target,client_architecture,path_prefix,handler_type,
                        index_files_json,spa_fallback_path,is_default,priority,status,metadata,
                        created_by,updated_by,created_at,updated_at,version)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,CAST($13 AS JSONB),
                        $14,$15,$16,'ACTIVE','{}',$17,$17,CAST($18 AS TIMESTAMPTZ),
                        CAST($18 AS TIMESTAMPTZ),1)",
                )
                .bind(new_id)
                .bind(&uuid)
                .bind(tenant_id)
                .bind(organization_id)
                .bind(app_id)
                .bind(environment)
                .bind(&definition.spec_key)
                .bind(&definition.label)
                .bind(definition.runtime_target.as_str())
                .bind(definition.client_architecture.as_str())
                .bind(&definition.path_prefix)
                .bind(handler_name(definition.handler))
                .bind(index_files_json)
                .bind(&definition.spa_fallback)
                .bind(definition.is_default)
                .bind(i32::from(definition.priority))
                .bind(actor_id)
                .bind(generated_at)
                .execute(&mut **transaction)
                .await
                .map_err(|error| map_spec_write_error("insert declared source spec", error))?;
                (new_id, uuid)
            }
        };
        write_spec_routes(
            repository,
            transaction,
            tenant_id,
            organization_id,
            app_id,
            environment,
            spec_id,
            &spec_uuid,
            actor_id,
            generated_at,
            &routes,
        )
        .await?;
    }
    Ok(())
}

/// Loads one environment's active specs in projection order, routes attached.
pub(crate) async fn load_environment_source_specs(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_id: i64,
    environment: &str,
) -> DeployServiceResult<Vec<StoredSourceSpec>> {
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {SPEC_COLUMNS}
         FROM deploy_app_source_spec
         WHERE tenant_id = $1 AND app_id = $2 AND environment = $3
           AND deleted_at IS NULL AND status = 'ACTIVE'
         ORDER BY priority, spec_key"
    )))
    .bind(tenant_id)
    .bind(app_id)
    .bind(environment)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load environment source specs", error))?;
    let mut specs = rows
        .iter()
        .map(map_stored_spec)
        .collect::<DeployServiceResult<Vec<_>>>()?;
    let route_rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {ROUTE_COLUMNS} FROM deploy_app_source_spec_route
         WHERE tenant_id = $1 AND app_id = $2 AND environment = $3
         ORDER BY client_class, preference"
    )))
    .bind(tenant_id)
    .bind(app_id)
    .bind(environment)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load environment source spec routes", error))?;
    attach_routes(&mut specs, &route_rows)?;
    Ok(specs)
}

/// What a spec set projects into, ready to be appended to a composition.
#[derive(Default)]
pub(crate) struct SourceSpecProjection {
    pub(crate) variants: Vec<RuntimeVariant>,
    pub(crate) variant_rules: Vec<RuntimeVariantRule>,
    pub(crate) resources: Vec<RuntimeResource>,
    pub(crate) mounts: Vec<RuntimeMount>,
    /// The default spec's variant, which becomes the app-level default variant.
    pub(crate) default_variant_uuid: Option<String>,
    /// Spec keys that declared no source, so contributed nothing. Surfaced so a
    /// publish that would route nowhere can name the specs still awaiting an
    /// upload instead of reporting an anonymous empty composition.
    pub(crate) skipped_unbound: Vec<String>,
}

/// Projects specs into runtime variants, rules, mounts and resources.
///
/// Ids are derived from the spec's own uuid rather than from a sequence, so a
/// spec keeps the same projected variant/resource/mount identity across
/// recompiles. That identity is what a binding points at and what the edge
/// matches on, so it must not shift merely because an unrelated row was
/// inserted first.
///
/// Rules are emitted only for a spec that has a source behind it. That is what
/// implements the cross-class fallback chain without any request-time logic: a
/// class whose rank-0 spec is declared-but-not-uploaded has no rank-0 rule, so
/// the rank-1 rule for the same class — from a different spec — is the one that
/// matches.
pub(crate) fn project_source_specs(
    specs: &[StoredSourceSpec],
    composition_variant_keys: &BTreeSet<String>,
) -> DeployServiceResult<SourceSpecProjection> {
    let mut projection = SourceSpecProjection::default();
    for spec in specs {
        if composition_variant_keys.contains(&spec.spec_key) {
            return Err(DeployServiceError::conflict(format!(
                "source spec key '{}' collides with a composition variant of the same key; \
                 rename one of them",
                spec.spec_key
            )));
        }
        let provider_type = match spec.provider_type.as_deref() {
            Some(value) => AppSourceProviderType::parse(value).ok_or_else(|| {
                DeployServiceError::Internal(format!("unknown provider type '{value}'"))
            })?,
            // A spec with no upload contributes no resource and no mount: it is
            // a declared routing shape that is not yet servable, and emitting a
            // resource without a provider reference would fail compilation.
            None => {
                projection.skipped_unbound.push(spec.spec_key.clone());
                continue;
            }
        };
        let provider_resource_uuid = spec.provider_resource_uuid.clone().ok_or_else(|| {
            DeployServiceError::Internal("source binding is incomplete".to_owned())
        })?;
        let contract_version = spec.provider_contract_version.clone().ok_or_else(|| {
            DeployServiceError::Internal("source binding is incomplete".to_owned())
        })?;
        let variant_uuid = derived_id(&spec.uuid, "variant");
        let resource_uuid = derived_id(&spec.uuid, "resource");
        let runtime_provider_type = match provider_type {
            AppSourceProviderType::Drive => RuntimeProviderType::Drive,
            AppSourceProviderType::Knowledgebase => RuntimeProviderType::Knowledgebase,
        };
        projection.variants.push(RuntimeVariant {
            variant_uuid: variant_uuid.clone(),
            label: spec.label.clone(),
        });
        projection.resources.push(RuntimeResource {
            resource_uuid: resource_uuid.clone(),
            provider: RuntimeProviderReference {
                provider_type: runtime_provider_type,
                provider_resource_uuid,
                provider_contract_version: contract_version,
            },
            capabilities: source_capabilities(runtime_provider_type),
        });
        projection.mounts.push(RuntimeMount {
            mount_uuid: derived_id(&spec.uuid, "mount"),
            variant_uuid: variant_uuid.clone(),
            path_prefix: spec.path_prefix.clone(),
            resource_uuid,
            handler: crate::app_composition::runtime_handler(spec.handler),
            translation: RuntimeMountTranslation {
                mode: RuntimeMountMode::Root,
                resource_subpath: "/".to_owned(),
            },
            index_files: spec.index_files.clone(),
            spa_fallback: spec.spa_fallback.clone(),
        });
        for route in &spec.routes {
            let class_name = crate::app_composition::client_class_name(route.client_class);
            projection.variant_rules.push(RuntimeVariantRule {
                // Keyed by class, not by position: adding a fallback for another
                // class must not renumber this rule's identity.
                rule_uuid: derived_id(&spec.uuid, &format!("rule-{class_name}")),
                variant_uuid: variant_uuid.clone(),
                priority: route.preference,
                matcher: sdkwork_deploy_runtime_compiler::RuntimeVariantRuleMatcher::ClientClass {
                    client_class: crate::app_composition::runtime_client_class(route.client_class),
                },
            });
        }
        if spec.is_default {
            projection.default_variant_uuid = Some(variant_uuid);
        }
    }
    Ok(projection)
}

/// Writes one spec's routes, replacing whatever it had.
///
/// Replace-in-full rather than a diff: the route set is tiny, and "which classes
/// this spec serves, at which rank" is only ever authored as a whole. Removing a
/// class is therefore expressible by leaving it out, which no additive write can
/// express.
#[allow(clippy::too_many_arguments)]
async fn write_spec_routes(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    app_id: i64,
    environment: &str,
    spec_id: i64,
    spec_uuid: &str,
    actor_id: i64,
    generated_at: &str,
    routes: &[StoredSourceSpecRoute],
) -> DeployServiceResult<()> {
    delete_spec_routes(transaction, tenant_id, spec_id).await?;
    for route in routes {
        let class_name = crate::app_composition::client_class_name(route.client_class);
        sqlx::query(
            "INSERT INTO deploy_app_source_spec_route (
                id,uuid,tenant_id,organization_id,app_id,environment,spec_id,client_class,
                preference,created_by,updated_by,created_at,updated_at,version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,CAST($11 AS TIMESTAMPTZ),
                CAST($11 AS TIMESTAMPTZ),1)",
        )
        .bind(repository.next_spec_route_id()?)
        // Derived from the spec uuid and the class, so re-declaring the same
        // route reuses its identity instead of churning a new row id — which
        // keeps the projected rule uuid stable across a recompile.
        .bind(derived_id(spec_uuid, &format!("route-{class_name}")))
        .bind(tenant_id)
        .bind(organization_id)
        .bind(app_id)
        .bind(environment)
        .bind(spec_id)
        .bind(class_name)
        .bind(i32::from(route.preference))
        .bind(actor_id)
        .bind(generated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| map_spec_write_error("write source spec route", error))?;
    }
    Ok(())
}

async fn delete_spec_routes(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    spec_id: i64,
) -> DeployServiceResult<()> {
    sqlx::query("DELETE FROM deploy_app_source_spec_route WHERE tenant_id = $1 AND spec_id = $2")
        .bind(tenant_id)
        .bind(spec_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("delete source spec routes", error))?;
    Ok(())
}

async fn load_spec_routes(
    executor: &sqlx::PgPool,
    spec_id: i64,
) -> DeployServiceResult<Vec<StoredSourceSpecRoute>> {
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {ROUTE_COLUMNS} FROM deploy_app_source_spec_route
         WHERE spec_id = $1 ORDER BY client_class, preference"
    )))
    .bind(spec_id)
    .fetch_all(executor)
    .await
    .map_err(|error| composition_store_error("load source spec routes", error))?;
    rows.iter().map(map_stored_route).collect()
}

/// Groups flat route rows onto the specs they belong to.
fn attach_routes(
    specs: &mut [StoredSourceSpec],
    rows: &[sqlx::postgres::PgRow],
) -> DeployServiceResult<()> {
    let mut by_spec: BTreeMap<i64, Vec<StoredSourceSpecRoute>> = BTreeMap::new();
    for row in rows {
        let spec_id: i64 = row
            .try_get("spec_id")
            .map_err(|error| composition_store_error("read route spec id", error))?;
        by_spec
            .entry(spec_id)
            .or_default()
            .push(map_stored_route(row)?);
    }
    for spec in specs.iter_mut() {
        if let Some(routes) = by_spec.remove(&spec.id) {
            spec.routes = routes;
        }
    }
    Ok(())
}

fn map_stored_route(row: &sqlx::postgres::PgRow) -> DeployServiceResult<StoredSourceSpecRoute> {
    let client_class: String = row
        .try_get("client_class")
        .map_err(|error| composition_store_error("read route client class", error))?;
    let preference: i32 = row
        .try_get("preference")
        .map_err(|error| composition_store_error("read route preference", error))?;
    Ok(StoredSourceSpecRoute {
        client_class: parse_client_class(&client_class)?,
        preference: u16::try_from(preference).unwrap_or(MAXIMUM_PREFERENCE),
    })
}

fn source_capabilities(provider_type: RuntimeProviderType) -> RuntimeResourceCapabilities {
    match provider_type {
        RuntimeProviderType::Drive => RuntimeResourceCapabilities {
            static_content: true,
            wiki_routes: false,
            wiki_search: false,
            range_requests: true,
        },
        RuntimeProviderType::Knowledgebase => RuntimeResourceCapabilities {
            static_content: true,
            wiki_routes: true,
            wiki_search: true,
            range_requests: false,
        },
    }
}

/// Deterministic, UUID-shaped id for a projected row.
///
/// Derived from the spec uuid so two specs can never collide, and from the role
/// so one spec's variant, resource, mount and rules stay distinct. Not a real
/// UUIDv5 — nothing consumes it as one; the only requirements are shape and
/// stability within one `(app, environment)`.
fn derived_id(spec_uuid: &str, role: &str) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(format!("{spec_uuid}|{role}").as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &digest[0..8],
        &digest[8..12],
        &digest[12..16],
        &digest[16..20],
        &digest[20..32]
    )
}

async fn lock_app_internal_id(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_uuid: &str,
) -> DeployServiceResult<i64> {
    sqlx::query_scalar(
        "SELECT id FROM deploy_app
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(app_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("lock app for source specs", error))?
    .ok_or_else(|| DeployServiceError::not_found("app not found"))
}

/// The row a repeated `Idempotency-Key` refers to, or `None` when the key is new.
///
/// Looked up by `(tenant, key)` to match the partial unique index exactly: the
/// index is tenant-scoped, so an app-scoped lookup would miss a row the index
/// would still reject and turn a clear "already used" into an opaque conflict.
async fn load_idempotent_spec(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_id: i64,
    idempotency_key: &str,
    request_sha256: &str,
) -> DeployServiceResult<Option<StoredSourceSpec>> {
    let row = sqlx::query(
        "SELECT app_id, CAST(uuid AS TEXT) AS uuid, request_sha256
         FROM deploy_app_source_spec
         WHERE tenant_id = $1 AND idempotency_key = $2",
    )
    .bind(tenant_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load idempotent source spec", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_app_id: i64 = row
        .try_get("app_id")
        .map_err(|_| DeployServiceError::Internal("invalid idempotency record".to_owned()))?;
    if stored_app_id != app_id {
        return Err(DeployServiceError::conflict(
            "Idempotency-Key was already used for another app's source spec",
        ));
    }
    let stored_sha256: Option<String> = row.try_get("request_sha256").ok().flatten();
    // Rows written before the key existed, or by `apps.create`, carry no hash;
    // they are still a replay of this command rather than a conflict.
    if let Some(stored_sha256) = stored_sha256 {
        if stored_sha256 != request_sha256 {
            return Err(DeployServiceError::conflict(
                "Idempotency-Key was already used with another source spec create request",
            ));
        }
    }
    let spec_uuid: String = row
        .try_get("uuid")
        .map_err(|_| DeployServiceError::Internal("invalid idempotency record".to_owned()))?;
    load_spec_by_uuid(transaction, tenant_id, app_id, &spec_uuid).await
}

async fn load_spec_by_uuid(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_id: i64,
    spec_uuid: &str,
) -> DeployServiceResult<Option<StoredSourceSpec>> {
    let row = sqlx::query(AssertSqlSafe(format!(
        "SELECT {SPEC_COLUMNS}
         FROM deploy_app_source_spec
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL"
    )))
    .bind(tenant_id)
    .bind(app_id)
    .bind(spec_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load app source spec", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut spec = map_stored_spec(&row)?;
    let route_rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {ROUTE_COLUMNS} FROM deploy_app_source_spec_route
         WHERE tenant_id = $1 AND spec_id = $2 ORDER BY client_class, preference"
    )))
    .bind(tenant_id)
    .bind(spec.id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load app source spec routes", error))?;
    spec.routes = route_rows
        .iter()
        .map(map_stored_route)
        .collect::<DeployServiceResult<Vec<_>>>()?;
    Ok(Some(spec))
}

fn map_stored_spec(row: &sqlx::postgres::PgRow) -> DeployServiceResult<StoredSourceSpec> {
    let index_files_text: String = row
        .try_get("index_files_json")
        .map_err(|error| composition_store_error("read spec index files", error))?;
    let index_files: Vec<String> = serde_json::from_str(&index_files_text).map_err(|error| {
        DeployServiceError::Internal(format!("decode source spec index files: {error}"))
    })?;
    let handler_text: String = row
        .try_get("handler_type")
        .map_err(|error| composition_store_error("read spec handler", error))?;
    Ok(StoredSourceSpec {
        id: row
            .try_get("id")
            .map_err(|error| composition_store_error("read spec id", error))?,
        uuid: row
            .try_get("uuid")
            .map_err(|error| composition_store_error("read spec uuid", error))?,
        spec_key: row
            .try_get("spec_key")
            .map_err(|error| composition_store_error("read spec key", error))?,
        label: row
            .try_get("label")
            .map_err(|error| composition_store_error("read spec label", error))?,
        runtime_target: row
            .try_get("runtime_target")
            .map_err(|error| composition_store_error("read spec runtime target", error))?,
        client_architecture: row
            .try_get("client_architecture")
            .map_err(|error| composition_store_error("read spec client architecture", error))?,
        // Filled by the caller with the routes it loaded alongside; a spec read
        // without them would project a variant that no client can ever reach.
        routes: Vec::new(),
        path_prefix: row
            .try_get("path_prefix")
            .map_err(|error| composition_store_error("read spec path prefix", error))?,
        handler: parse_handler(&handler_text)?,
        index_files,
        spa_fallback: row
            .try_get("spa_fallback_path")
            .map_err(|error| composition_store_error("read spec spa fallback", error))?,
        provider_type: row
            .try_get("source_provider_type")
            .map_err(|error| composition_store_error("read spec provider type", error))?,
        provider_resource_uuid: row
            .try_get("source_provider_resource_uuid")
            .map_err(|error| composition_store_error("read spec provider resource", error))?,
        provider_contract_version: row
            .try_get("source_contract_version")
            .map_err(|error| composition_store_error("read spec contract version", error))?,
        source_status: row
            .try_get("source_status")
            .map_err(|error| composition_store_error("read spec source status", error))?,
        is_default: row
            .try_get("is_default")
            .map_err(|error| composition_store_error("read spec default flag", error))?,
        priority: u16::try_from(
            row.try_get::<i32, _>("priority")
                .map_err(|error| composition_store_error("read spec priority", error))?,
        )
        .unwrap_or(u16::MAX),
        status: row
            .try_get("status")
            .map_err(|error| composition_store_error("read spec status", error))?,
        version: row
            .try_get("version")
            .map_err(|error| composition_store_error("read spec version", error))?,
        created_at: crate::support::required_datetime(row, "created_at")?,
        updated_at: crate::support::required_datetime(row, "updated_at")?,
    })
}

fn map_source_spec_row(spec: &StoredSourceSpec) -> DeployServiceResult<AppSourceSpecResponse> {
    let source = match (&spec.provider_type, &spec.provider_resource_uuid) {
        (Some(provider_type), Some(provider_resource_uuid)) => Some(AppSourceBindingResponse {
            provider_type: provider_type.clone(),
            provider_resource_uuid: provider_resource_uuid.clone(),
            contract_version: spec.provider_contract_version.clone().unwrap_or_default(),
            status: AppSourceBindingStatus::parse(&spec.source_status)
                .unwrap_or(AppSourceBindingStatus::Bound)
                .as_str()
                .to_owned(),
            updated_at: None,
        }),
        _ => None,
    };
    // Not best-effort: a stored value outside the canonical vocabulary means the
    // row predates the alignment or was written by a bypass, and reporting a
    // substituted default would hide it. The CHECK constraint makes this
    // unreachable in practice, which is exactly why it is safe to be strict.
    let runtime_target = SdkworkRuntimeTarget::parse(&spec.runtime_target).ok_or_else(|| {
        DeployServiceError::Internal(format!("unknown runtime target '{}'", spec.runtime_target))
    })?;
    let client_architecture =
        AppClientArchitecture::parse(&spec.client_architecture).ok_or_else(|| {
            DeployServiceError::Internal(format!(
                "unknown client architecture '{}'",
                spec.client_architecture
            ))
        })?;
    Ok(AppSourceSpecResponse {
        id: spec.uuid.clone(),
        spec_key: spec.spec_key.clone(),
        label: spec.label.clone(),
        runtime_target,
        client_architecture,
        client_class_routes: spec
            .routes
            .iter()
            .map(|route| AppSourceSpecRouteDefinition {
                client_class: route.client_class,
                preference: route.preference,
            })
            .collect(),
        path_prefix: spec.path_prefix.clone(),
        handler: spec.handler,
        index_files: spec.index_files.clone(),
        spa_fallback: spec.spa_fallback.clone(),
        is_default: spec.is_default,
        priority: spec.priority,
        status: spec.status.clone(),
        source_status: spec.source_status.clone(),
        source,
        created_at: spec.created_at.clone(),
        updated_at: spec.updated_at.clone(),
        version: spec.version.to_string(),
    })
}

fn validate_spec_key(spec_key: &str) -> DeployServiceResult<()> {
    let valid = (1..=64).contains(&spec_key.len())
        && spec_key == spec_key.to_ascii_lowercase()
        && spec_key
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && spec_key.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        });
    if !valid {
        return Err(DeployServiceError::validation(
            "specKey must be 1..=64 lowercase ascii letters, digits, dots, underscores or hyphens \
             and must start alphanumeric",
        ));
    }
    Ok(())
}

fn validate_label(label: &str) -> DeployServiceResult<()> {
    if label.trim().is_empty() || label.trim().chars().count() > 64 {
        return Err(DeployServiceError::validation(
            "spec label must be 1..=64 characters",
        ));
    }
    Ok(())
}

fn validate_path_prefix(path_prefix: &str) -> DeployServiceResult<()> {
    if !path_prefix.starts_with('/') || path_prefix.len() > 4096 {
        return Err(DeployServiceError::validation(
            "spec pathPrefix must start with '/' and be at most 4096 bytes",
        ));
    }
    Ok(())
}

/// Normalizes one spec's declared routes and rejects shapes the schema would
/// accept but that mean nothing.
///
/// A client class may be named **once** per spec: a spec is either a class's
/// default or one of its fallbacks, never both, and "this spec serves `MOBILE`
/// at rank 0 and rank 1" has no meaning the edge could act on. The fallback
/// chain is formed *across* specs — a lower rank on one spec and a higher rank on
/// another — which is exactly what the schema's `(class, rank)` uniqueness
/// exists to keep unambiguous.
///
/// Checked here rather than left to the unique index so the operator gets a
/// message naming the class instead of an opaque constraint violation.
fn validate_declared_routes(
    routes: &[AppSourceSpecRouteDefinition],
) -> DeployServiceResult<Vec<StoredSourceSpecRoute>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(routes.len());
    for route in routes {
        if route.preference > MAXIMUM_PREFERENCE {
            return Err(DeployServiceError::validation(format!(
                "clientClassRoutes preference must be at most {MAXIMUM_PREFERENCE}"
            )));
        }
        let name = crate::app_composition::client_class_name(route.client_class);
        if !seen.insert(name) {
            return Err(DeployServiceError::validation(format!(
                "clientClassRoutes must name each client class at most once; '{name}' is repeated"
            )));
        }
        normalized.push(StoredSourceSpecRoute {
            client_class: route.client_class,
            preference: route.preference,
        });
    }
    Ok(normalized)
}

fn validate_spa_fallback(
    handler: AppMountHandler,
    spa_fallback: Option<&str>,
) -> DeployServiceResult<()> {
    match (handler, spa_fallback) {
        (AppMountHandler::Spa, None) => Err(DeployServiceError::validation(
            "a SPA spec must declare spaFallback",
        )),
        (AppMountHandler::Spa, Some(path)) if !path.starts_with('/') => Err(
            DeployServiceError::validation("spaFallback must start with '/'"),
        ),
        (_, Some(_)) if handler != AppMountHandler::Spa => Err(DeployServiceError::validation(
            "spaFallback is only meaningful for a SPA spec",
        )),
        _ => Ok(()),
    }
}

fn validate_default_spec(definitions: &[AppSourceSpecDefinition]) -> DeployServiceResult<()> {
    let defaults = definitions
        .iter()
        .filter(|definition| definition.is_default)
        .count();
    if !definitions.is_empty() && defaults != 1 {
        return Err(DeployServiceError::validation(
            "exactly one declared source spec must set isDefault",
        ));
    }
    Ok(())
}

fn parse_client_class(value: &str) -> DeployServiceResult<AppClientClass> {
    match value {
        "DESKTOP" => Ok(AppClientClass::Desktop),
        "MOBILE" => Ok(AppClientClass::Mobile),
        "TABLET" => Ok(AppClientClass::Tablet),
        "TV" => Ok(AppClientClass::Tv),
        "BOT" => Ok(AppClientClass::Bot),
        "OTHER" => Ok(AppClientClass::Other),
        other => Err(DeployServiceError::Internal(format!(
            "unknown client class '{other}'"
        ))),
    }
}

fn parse_handler(value: &str) -> DeployServiceResult<AppMountHandler> {
    match value {
        "STATIC" => Ok(AppMountHandler::Static),
        "SPA" => Ok(AppMountHandler::Spa),
        "WIKI" => Ok(AppMountHandler::Wiki),
        other => Err(DeployServiceError::Internal(format!(
            "unknown spec handler '{other}'"
        ))),
    }
}

fn handler_name(handler: AppMountHandler) -> &'static str {
    crate::app_composition::mount_handler_name(handler)
}

/// Maps a constraint violation onto the message the operator can act on.
///
/// The database is the authority for these rules; the service validates the
/// same shapes so the common cases never reach it, but a race (two requests
/// claiming the same class rank at once) surfaces here and must not be reported
/// as an internal error.
fn map_spec_write_error(context: &str, error: sqlx::Error) -> DeployServiceError {
    if let sqlx::Error::Database(database_error) = &error {
        if let Some(constraint) = database_error.constraint() {
            return match constraint {
                "uk_deploy_app_source_spec_default" => DeployServiceError::conflict(
                    "another source spec is already the app default for this environment",
                ),
                "uk_deploy_app_source_spec_key" => {
                    DeployServiceError::conflict("a source spec with this key already exists")
                }
                "uk_deploy_app_source_spec_idempotency" => DeployServiceError::conflict(
                    "Idempotency-Key was already used for another source spec create request",
                ),
                "uk_deploy_app_source_spec_route_rank" => DeployServiceError::conflict(
                    "another source spec already holds that client class at that preference",
                ),
                "uk_deploy_app_source_spec_route_spec" => DeployServiceError::conflict(
                    "this source spec already routes that client class",
                ),
                _ => composition_store_error(context, error),
            };
        }
    }
    composition_store_error(context, error)
}

/// The `source_provider_type` column value for a resolved provider.
///
/// Written from the validated resource's own provider type rather than echoed
/// from the request, so the column can only ever hold a value the delivery
/// plane has already accepted.
fn provider_type_column(provider_type: RuntimeProviderType) -> &'static str {
    match provider_type {
        RuntimeProviderType::Drive => "DRIVE",
        RuntimeProviderType::Knowledgebase => "KNOWLEDGEBASE",
    }
}

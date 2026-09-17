use std::collections::BTreeMap;

use async_trait::async_trait;
use sdkwork_deploy_content_provider_port::ValidatedContentProviderResource;
use sdkwork_deploy_contract::{
    AppBindingAction, AppClientClass, AppCompositionResponse, AppMountHandler, AppMountMode,
    AppRedirectScheme, AppRevisionResponse, AppRuntimeAssignmentResponse, AppVariantRuleMatcher,
    DeployServiceError, DeployServiceResult,
};
use sdkwork_deploy_runtime_compiler::{
    canonical_sha256_excluding_field, compile_app_revision, compile_runtime_set,
    normalize_runtime_descriptors, runtime_set_size_bytes, AppRuntimeCompilationInput,
    RuntimeBinding, RuntimeBindingAction, RuntimeClientClass, RuntimeDeliveryPolicy,
    RuntimeEnvironment, RuntimeHandler, RuntimeLimits, RuntimeMount, RuntimeMountMode,
    RuntimeMountTranslation, RuntimeObservabilityPolicy, RuntimeProviderReference,
    RuntimeProviderType, RuntimeRedirectScheme, RuntimeResource, RuntimeSecurityPolicy,
    RuntimeSetCompilationInput, RuntimeVariant, RuntimeVariantRule, RuntimeVariantRuleMatcher,
    DESCRIPTOR_COMPILER_VERSION, WEBSITE_RUNTIME_SCHEMA_VERSION,
};
use sdkwork_intelligence_deploy_service::{
    AppCompositionRepositoryPort, ReplaceAppCompositionCommand,
};
use sqlx::{AssertSqlSafe, PgPool, Postgres, Row, Transaction};

use crate::platform_app_domains::{reconcile_app_default_domains_tx, DEFAULT_BINDING_KEY_PREFIX};
use crate::support::{new_uuid, next_id};
use crate::DeployRepository;

const MAXIMUM_RUNTIME_GENERATION: i64 = 9_007_199_254_740_991;
const MAXIMUM_RUNTIME_SITES: usize = 10_000;

#[derive(Clone, Debug)]
struct StoredApp {
    id: i64,
    organization_id: i64,
    version: i64,
}

#[derive(Clone, Debug)]
struct StoredVariant {
    id: i64,
    uuid: String,
}

#[derive(Clone, Debug)]
struct StoredResource {
    id: i64,
    uuid: String,
}

#[derive(Clone, Debug)]
struct StoredDomain {
    id: i64,
    hostname: String,
}

#[derive(Clone, Debug)]
struct StoredTarget {
    id: i64,
    uuid: String,
    node_uuid: String,
    tenant_scope_hash: String,
}

#[async_trait]
impl AppCompositionRepositoryPort for DeployRepository {
    async fn replace_app_composition(
        &self,
        command: ReplaceAppCompositionCommand,
    ) -> DeployServiceResult<AppCompositionResponse> {
        self.replace_site_composition_repo(command).await
    }
}

impl DeployRepository {
    async fn replace_site_composition_repo(
        &self,
        command: ReplaceAppCompositionCommand,
    ) -> DeployServiceResult<AppCompositionResponse> {
        let mut transaction = begin_transaction(&self.pool).await?;
        if let Some(response) = load_idempotent_result(&mut transaction, &command).await? {
            transaction
                .commit()
                .await
                .map_err(|error| composition_store_error("commit idempotent composition", error))?;
            return Ok(response);
        }

        let app = lock_app(&mut transaction, &command).await?;
        reserve_app_version(&mut transaction, &command, &app).await?;
        let new_app_version = app.version + 1;
        let targets = load_targets(
            &mut transaction,
            command.tenant_id,
            command.request.environment.as_str(),
        )
        .await?;
        let tenant_scope_hash = consistent_tenant_scope_hash(&targets)?;
        let environment = command.request.environment.as_str();

        // 1. Platform publishing domains first, so the request never has to
        //    re-declare `<appDomainLabel>.app[-<env>].<suffix>` and an update
        //    can never leave the app unreachable.
        reconcile_app_default_domains_tx(
            self,
            &mut transaction,
            command.tenant_id,
            app.organization_id,
            Some(command.actor_id),
            app.id,
            environment,
        )
        .await?;
        // 2. Reconcile the app's live composition **for this environment only**.
        //    Other environments' bindings, variants, mounts and resources -- and
        //    every already-published revision snapshot -- are untouched.
        let declared_binding_keys = command
            .request
            .bindings
            .iter()
            .map(|binding| binding.key.clone())
            .collect::<Vec<_>>();
        delete_current_composition(
            &mut transaction,
            app.id,
            environment,
            &declared_binding_keys,
        )
        .await?;
        let resources = insert_resources(self, &mut transaction, &command, &app).await?;
        let variants = insert_variants(self, &mut transaction, &command, &app).await?;
        let default_variant = variants
            .get(&command.request.default_variant_key)
            .ok_or_else(|| DeployServiceError::validation("default variant is missing"))?
            .clone();
        let runtime_rules =
            insert_variant_rules(self, &mut transaction, &command, &app, &variants).await?;
        let runtime_mounts = insert_mounts(
            self,
            &mut transaction,
            &command,
            &app,
            &variants,
            &resources,
        )
        .await?;
        reconcile_bindings(self, &mut transaction, &command, &app, &variants).await?;
        // The descriptor's binding set is read back from the DB so the lookup
        // rows and the routing rows are the same rows.
        let runtime_bindings =
            load_environment_bindings(&mut transaction, app.id, environment).await?;

        let revision_id = next_id(self.id_generator())?;
        let revision_uuid = new_uuid();
        let revision_number = next_revision_number(&mut transaction, app.id).await?;
        let runtime_resources = command
            .resources
            .iter()
            .map(|resource| runtime_resource(resource, &resources))
            .collect::<DeployServiceResult<Vec<_>>>()?;
        let runtime_variants = command
            .request
            .variants
            .iter()
            .map(|variant| RuntimeVariant {
                variant_uuid: variants[&variant.key].uuid.clone(),
                label: variant.label.clone(),
            })
            .collect();
        let compiled = compile_app_revision(AppRuntimeCompilationInput {
            revision_uuid: revision_uuid.clone(),
            app_uuid: command.app_uuid.clone(),
            tenant_scope_hash,
            environment: runtime_environment(command.request.environment),
            generated_at: command.generated_at.clone(),
            app_default_variant_uuid: default_variant.uuid.clone(),
            bindings: runtime_bindings,
            variants: runtime_variants,
            variant_rules: runtime_rules,
            resources: runtime_resources,
            mounts: runtime_mounts,
            delivery_policy: RuntimeDeliveryPolicy {
                provider_timeout_ms: command.request.delivery_policy.provider_timeout_ms,
                metadata_cache_ttl_seconds: command
                    .request
                    .delivery_policy
                    .metadata_cache_ttl_seconds,
                negative_cache_ttl_seconds: command
                    .request
                    .delivery_policy
                    .negative_cache_ttl_seconds,
                stale_while_revalidate_seconds: command
                    .request
                    .delivery_policy
                    .stale_while_revalidate_seconds,
                maximum_object_bytes: command.request.delivery_policy.maximum_object_bytes,
            },
            security_policy: RuntimeSecurityPolicy {
                force_https: command.request.security_policy.force_https,
                deny_dot_files: command.request.security_policy.deny_dot_files,
                denied_path_prefixes: command.request.security_policy.denied_path_prefixes.clone(),
            },
            limits: RuntimeLimits {
                maximum_bindings: command.request.limits.maximum_bindings,
                maximum_variants: command.request.limits.maximum_variants,
                maximum_variant_rules: command.request.limits.maximum_variant_rules,
                maximum_resources: command.request.limits.maximum_resources,
                maximum_mounts: command.request.limits.maximum_mounts,
                maximum_index_files_per_mount: command.request.limits.maximum_index_files_per_mount,
                maximum_path_bytes: command.request.limits.maximum_path_bytes,
                maximum_path_segments: command.request.limits.maximum_path_segments,
            },
            observability_policy: RuntimeObservabilityPolicy {
                access_log_enabled: command.request.observability_policy.access_log_enabled,
                usage_metering_enabled: command.request.observability_policy.usage_metering_enabled,
                trace_sample_rate_per_mille: command
                    .request
                    .observability_policy
                    .trace_sample_rate_per_mille,
            },
        })
        .map_err(|error| DeployServiceError::validation(error.to_string()))?;
        insert_revision(
            &mut transaction,
            &command,
            &app,
            revision_id,
            &revision_uuid,
            revision_number,
            new_app_version,
            &compiled.descriptor,
            &compiled.descriptor_sha256,
        )
        .await?;
        update_site_revision_pointers(
            &mut transaction,
            &command,
            app.id,
            default_variant.id,
            revision_id,
        )
        .await?;

        let mut descriptors = load_other_descriptors(
            &mut transaction,
            command.tenant_id,
            app.id,
            command.request.environment.as_str(),
        )
        .await?;
        descriptors.push(compiled.descriptor);
        normalize_runtime_descriptors(&mut descriptors);
        let assignments = insert_runtime_assignments(
            self,
            &mut transaction,
            &command,
            revision_id,
            &targets,
            &descriptors,
        )
        .await?;
        let response = AppCompositionResponse {
            app_id: command.app_uuid.clone(),
            app_version: new_app_version.to_string(),
            revision: AppRevisionResponse {
                id: revision_uuid.clone(),
                number: revision_number.to_string(),
                descriptor_sha256: compiled.descriptor_sha256,
                validation_status: "VALID".to_owned(),
            },
            runtime_assignments: assignments,
        };
        persist_command_result(&mut transaction, revision_id, &response).await?;
        insert_composition_audit(self, &mut transaction, &command, app.id, revision_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error| composition_store_error("commit app composition", error))?;
        Ok(response)
    }
}

async fn begin_transaction(pool: &PgPool) -> DeployServiceResult<Transaction<'static, Postgres>> {
    pool.begin()
        .await
        .map_err(|error| composition_store_error("begin app composition transaction", error))
}

async fn load_idempotent_result(
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
) -> DeployServiceResult<Option<AppCompositionResponse>> {
    let row = sqlx::query(
        "SELECT r.request_sha256, CAST(r.result_json AS TEXT) AS result_json
         FROM deploy_app_revision r
         INNER JOIN deploy_app s ON s.id = r.app_id
         WHERE r.tenant_id = $1 AND s.uuid = $2 AND r.idempotency_key = $3",
    )
    .bind(command.tenant_id)
    .bind(&command.app_uuid)
    .bind(&command.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load app composition idempotency", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let request_sha256: String = row
        .try_get("request_sha256")
        .map_err(|_| DeployServiceError::Internal("invalid idempotency record".to_owned()))?;
    if request_sha256 != command.request_sha256 {
        return Err(DeployServiceError::conflict(
            "Idempotency-Key was already used with another app composition",
        ));
    }
    let result_json: String = row
        .try_get("result_json")
        .map_err(|_| DeployServiceError::Internal("invalid idempotency result".to_owned()))?;
    serde_json::from_str(&result_json)
        .map(Some)
        .map_err(|_| DeployServiceError::Internal("invalid idempotency result".to_owned()))
}

async fn lock_app(
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
) -> DeployServiceResult<StoredApp> {
    let row = sqlx::query(
        "SELECT id, organization_id, version
         FROM deploy_app
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
         FOR UPDATE",
    )
    .bind(command.tenant_id)
    .bind(&command.app_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("lock app composition", error))?
    .ok_or_else(|| DeployServiceError::not_found("app not found"))?;
    let app = StoredApp {
        id: row
            .try_get("id")
            .map_err(|_| DeployServiceError::Internal("invalid app record".to_owned()))?,
        organization_id: row.try_get("organization_id").unwrap_or(0),
        version: row
            .try_get("version")
            .map_err(|_| DeployServiceError::Internal("invalid app version".to_owned()))?,
    };
    if app.version != command.expected_app_version {
        return Err(DeployServiceError::conflict(
            "app composition version changed; refresh and retry",
        ));
    }
    Ok(app)
}

async fn reserve_app_version(
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
) -> DeployServiceResult<()> {
    let result = sqlx::query(
        "UPDATE deploy_app SET version = version + 1, updated_at = CAST($3 AS TIMESTAMPTZ)
         WHERE id = $1 AND version = $2",
    )
    .bind(app.id)
    .bind(command.expected_app_version)
    .bind(&command.generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("reserve app composition version", error))?;
    if result.rows_affected() != 1 {
        return Err(DeployServiceError::conflict(
            "app composition version changed; refresh and retry",
        ));
    }
    Ok(())
}

async fn load_targets(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    environment: &str,
) -> DeployServiceResult<Vec<StoredTarget>> {
    let rows = sqlx::query(
        "SELECT id, uuid, node_uuid, tenant_scope_hash
         FROM deploy_web_node_target
         WHERE tenant_id = $1 AND environment = $2 AND status = 'ACTIVE'
           AND deleted_at IS NULL
         ORDER BY uuid FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(environment)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load Web Node targets", error))?;
    if rows.is_empty() {
        return Err(DeployServiceError::conflict(
            "no active Web Node target exists for the requested environment",
        ));
    }
    rows.into_iter()
        .map(|row| {
            Ok(StoredTarget {
                id: row.try_get("id").map_err(|_| invalid_stored_target())?,
                uuid: row.try_get("uuid").map_err(|_| invalid_stored_target())?,
                node_uuid: row
                    .try_get("node_uuid")
                    .map_err(|_| invalid_stored_target())?,
                tenant_scope_hash: row
                    .try_get("tenant_scope_hash")
                    .map_err(|_| invalid_stored_target())?,
            })
        })
        .collect()
}

fn consistent_tenant_scope_hash(targets: &[StoredTarget]) -> DeployServiceResult<String> {
    let scope = targets
        .first()
        .map(|target| target.tenant_scope_hash.clone())
        .ok_or_else(|| DeployServiceError::conflict("no active Web Node target"))?;
    if scope.len() != 64
        || !scope
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || targets
            .iter()
            .any(|target| target.tenant_scope_hash != scope)
    {
        return Err(DeployServiceError::conflict(
            "Web Node targets have inconsistent tenant scope",
        ));
    }
    Ok(scope)
}

/// Reconcile the app's live composition for **one** lifecycle environment.
///
/// The four composition tables (`deploy_app_resource`, `deploy_app_variant`,
/// `deploy_app_variant_rule`, `deploy_app_mount`) and `deploy_app_binding` are
/// all environment-scoped, so replacing the `development` composition leaves
/// every other environment -- and its already-published `deploy_app_revision`
/// snapshots -- untouched.
///
/// Bindings in the auto-provisioned keyspace (`appd-*`, written by
/// `provision_app_default_domains_repo`) are never deleted here: the platform
/// publishing domains are managed by their own reconciler and are re-created in
/// the same transaction by the caller. Deleting them was the defect that made
/// `<appId>.app.<suffix>` stop resolving after any composition update.
///
/// Deletion order follows the foreign keys: bindings and mounts first (they
/// reference variants and resources), then variant rules, then variants, then
/// resources.
async fn delete_current_composition(
    transaction: &mut Transaction<'static, Postgres>,
    app_id: i64,
    environment: &str,
    declared_binding_keys: &[String],
) -> DeployServiceResult<()> {
    for (table, context) in [
        ("deploy_app_mount", "delete app mounts"),
        ("deploy_app_variant_rule", "delete app variant rules"),
    ] {
        let query = format!("DELETE FROM {table} WHERE app_id = $1 AND environment = $2");
        sqlx::query(AssertSqlSafe(&*query))
            .bind(app_id)
            .bind(environment)
            .execute(&mut **transaction)
            .await
            .map_err(|error| composition_store_error(context, error))?;
    }
    // Bindings: only the ones the request no longer declares, and never the
    // auto-provisioned platform publishing domains.
    sqlx::query(
        "DELETE FROM deploy_app_binding
         WHERE app_id = $1 AND environment = $2
           AND binding_key NOT LIKE $3
           AND binding_key <> ALL($4)",
    )
    .bind(app_id)
    .bind(environment)
    .bind(format!("{DEFAULT_BINDING_KEY_PREFIX}%"))
    .bind(declared_binding_keys)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("delete app bindings", error))?;
    // Surviving bindings may still reference variants that are about to be
    // replaced; the pointers are re-established by `reconcile_bindings` after
    // the new variants exist, so clear them here to keep the FK satisfied.
    sqlx::query(
        "UPDATE deploy_app_binding
         SET default_variant_id = NULL, forced_variant_id = NULL
         WHERE app_id = $1 AND environment = $2 AND deleted_at IS NULL",
    )
    .bind(app_id)
    .bind(environment)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("clear binding variant pointers", error))?;
    for (table, context) in [
        ("deploy_app_variant", "delete app variants"),
        ("deploy_app_resource", "delete app resources"),
    ] {
        let query = format!("DELETE FROM {table} WHERE app_id = $1 AND environment = $2");
        sqlx::query(AssertSqlSafe(&*query))
            .bind(app_id)
            .bind(environment)
            .execute(&mut **transaction)
            .await
            .map_err(|error| composition_store_error(context, error))?;
    }
    Ok(())
}

async fn insert_resources(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
) -> DeployServiceResult<BTreeMap<String, StoredResource>> {
    let mut stored = BTreeMap::new();
    for resource in &command.resources {
        let id = next_id(repository.id_generator())?;
        let uuid = new_uuid();
        let capabilities = serde_json::to_string(&resource.capabilities).map_err(|_| {
            DeployServiceError::Internal("serialize capabilities failed".to_owned())
        })?;
        sqlx::query(
            "INSERT INTO deploy_app_resource (
                id, uuid, tenant_id, organization_id, app_id, environment, resource_key,
                provider_type, provider_resource_uuid, provider_contract_version,
                capabilities_json, status, last_validated_at, metadata, created_by, updated_by,
                created_at, updated_at, version
             ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,CAST($11 AS JSONB),'VALID',
                CAST($12 AS TIMESTAMPTZ),'{}',$13,$13,CAST($12 AS TIMESTAMPTZ),
                CAST($12 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(command.tenant_id)
        .bind(app.organization_id)
        .bind(app.id)
        .bind(command.request.environment.as_str())
        .bind(&resource.key)
        .bind(provider_type_name(resource.provider_type))
        .bind(&resource.provider_resource_uuid)
        .bind(&resource.provider_contract_version)
        .bind(&capabilities)
        .bind(&command.generated_at)
        .bind(command.actor_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert app resource", error))?;
        stored.insert(resource.key.clone(), StoredResource { id, uuid });
    }
    Ok(stored)
}

async fn insert_variants(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
) -> DeployServiceResult<BTreeMap<String, StoredVariant>> {
    let mut stored = BTreeMap::new();
    for variant in &command.request.variants {
        let id = next_id(repository.id_generator())?;
        let uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_app_variant (
                id,uuid,tenant_id,app_id,environment,variant_key,label,client_class,is_default,
                priority,status,metadata,created_by,updated_by,created_at,updated_at,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'ACTIVE','{}',$11,$11,
                CAST($12 AS TIMESTAMPTZ),CAST($12 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(command.tenant_id)
        .bind(app.id)
        .bind(command.request.environment.as_str())
        .bind(&variant.key)
        .bind(&variant.label)
        .bind(client_class_name(variant.client_class))
        .bind(variant.key == command.request.default_variant_key)
        .bind(i32::from(variant.priority))
        .bind(command.actor_id)
        .bind(&command.generated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert app variant", error))?;
        stored.insert(variant.key.clone(), StoredVariant { id, uuid });
    }
    Ok(stored)
}

async fn insert_variant_rules(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
    variants: &BTreeMap<String, StoredVariant>,
) -> DeployServiceResult<Vec<RuntimeVariantRule>> {
    let mut runtime = Vec::with_capacity(command.request.variant_rules.len());
    for rule in &command.request.variant_rules {
        let id = next_id(repository.id_generator())?;
        let uuid = new_uuid();
        let variant = variants
            .get(&rule.target_variant_key)
            .ok_or_else(|| DeployServiceError::validation("unknown variant rule target"))?;
        let (rule_type, match_value, matcher) = match &rule.matcher {
            AppVariantRuleMatcher::PathPrefix { path_prefix } => (
                "PATH_PREFIX",
                path_prefix.clone(),
                RuntimeVariantRuleMatcher::PathPrefix {
                    path_prefix: path_prefix.clone(),
                },
            ),
            AppVariantRuleMatcher::ClientClass { client_class } => (
                "CLIENT_CLASS",
                client_class_name(*client_class).to_owned(),
                RuntimeVariantRuleMatcher::ClientClass {
                    client_class: runtime_client_class(*client_class),
                },
            ),
        };
        sqlx::query(
            "INSERT INTO deploy_app_variant_rule (
                id,uuid,tenant_id,app_id,environment,rule_key,target_variant_id,rule_type,
                match_value,priority,status,created_by,updated_by,created_at,updated_at,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'ACTIVE',$11,$11,
                CAST($12 AS TIMESTAMPTZ),CAST($12 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(command.tenant_id)
        .bind(app.id)
        .bind(command.request.environment.as_str())
        .bind(&rule.key)
        .bind(variant.id)
        .bind(rule_type)
        .bind(match_value)
        .bind(i32::from(rule.priority))
        .bind(command.actor_id)
        .bind(&command.generated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert app variant rule", error))?;
        runtime.push(RuntimeVariantRule {
            rule_uuid: uuid,
            variant_uuid: variant.uuid.clone(),
            priority: rule.priority,
            matcher,
        });
    }
    Ok(runtime)
}

async fn insert_mounts(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
    variants: &BTreeMap<String, StoredVariant>,
    resources: &BTreeMap<String, StoredResource>,
) -> DeployServiceResult<Vec<RuntimeMount>> {
    let mut runtime = Vec::with_capacity(command.request.mounts.len());
    for mount in &command.request.mounts {
        let id = next_id(repository.id_generator())?;
        let uuid = new_uuid();
        let variant = variants
            .get(&mount.variant_key)
            .ok_or_else(|| DeployServiceError::validation("unknown mount variant"))?;
        let resource = resources
            .get(&mount.resource_key)
            .ok_or_else(|| DeployServiceError::validation("unknown mount resource"))?;
        let index_files = serde_json::to_string(&mount.index_files)
            .map_err(|_| DeployServiceError::Internal("serialize index files failed".to_owned()))?;
        sqlx::query(
            "INSERT INTO deploy_app_mount (
                id,uuid,tenant_id,app_id,environment,mount_key,variant_id,resource_id,path_prefix,
                resource_subpath,mount_mode,handler_type,index_files_json,spa_fallback_path,
                priority,status,created_by,updated_by,created_at,updated_at,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,CAST($13 AS JSONB),$14,$15,
                'ACTIVE',$16,$16,CAST($17 AS TIMESTAMPTZ),CAST($17 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(command.tenant_id)
        .bind(app.id)
        .bind(command.request.environment.as_str())
        .bind(&mount.key)
        .bind(variant.id)
        .bind(resource.id)
        .bind(&mount.path_prefix)
        .bind(&mount.resource_subpath)
        .bind(mount_mode_name(mount.mode))
        .bind(mount_handler_name(mount.handler))
        .bind(&index_files)
        .bind(mount.spa_fallback.as_deref())
        .bind(i32::from(mount.priority))
        .bind(command.actor_id)
        .bind(&command.generated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert app mount", error))?;
        runtime.push(RuntimeMount {
            mount_uuid: uuid,
            variant_uuid: variant.uuid.clone(),
            path_prefix: mount.path_prefix.clone(),
            resource_uuid: resource.uuid.clone(),
            handler: runtime_handler(mount.handler),
            translation: RuntimeMountTranslation {
                mode: runtime_mount_mode(mount.mode),
                resource_subpath: mount.resource_subpath.clone(),
            },
            index_files: mount.index_files.clone(),
            spa_fallback: mount.spa_fallback.clone(),
        });
    }
    Ok(runtime)
}

/// Reconcile the bindings the request declares for one environment.
///
/// Each declared binding is matched by `binding_key` first; if the app has no
/// row with that key, an existing ACTIVE row on the same
/// `(hostname_ascii, path_prefix, environment)` route is **adopted** (its
/// `binding_key` is claimed) instead of inserting a second row. That is what
/// lets a composition declare a binding on an auto-provisioned platform
/// hostname without tripping `uk_deploy_app_binding_active_route`, and it is
/// the reason the DB -- not the request -- is the single binding truth source.
async fn reconcile_bindings(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
    variants: &BTreeMap<String, StoredVariant>,
) -> DeployServiceResult<()> {
    let environment = command.request.environment.as_str();
    let mut domains: BTreeMap<String, StoredDomain> = BTreeMap::new();
    for binding in &command.request.bindings {
        let domain = if let Some(domain) = domains.get(&binding.domain_id) {
            domain.clone()
        } else {
            let domain =
                load_verified_domain(transaction, command.tenant_id, &binding.domain_id).await?;
            domains.insert(binding.domain_id.clone(), domain.clone());
            domain
        };
        let persisted = binding_action(&binding.action, variants)?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_app_binding
             WHERE app_id = $1 AND environment = $2 AND deleted_at IS NULL
               AND (binding_key = $3
                    OR (hostname_ascii = $4 AND path_prefix = $5))
             ORDER BY (binding_key = $3) DESC, id DESC
             LIMIT 1",
        )
        .bind(app.id)
        .bind(environment)
        .bind(&binding.key)
        .bind(&domain.hostname)
        .bind(&binding.path_prefix)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("lookup app binding", error))?;
        if let Some(existing_id) = existing {
            sqlx::query(
                "UPDATE deploy_app_binding SET
                    binding_key = $2, domain_id = $3, hostname_ascii = $4, environment = $5,
                    path_prefix = $6, action_type = $7, default_variant_id = $8,
                    forced_variant_id = $9, redirect_scheme = $10, redirect_hostname = $11,
                    redirect_path_prefix = $12, redirect_status_code = $13, preserve_path = $14,
                    preserve_query = $15, status = 'ACTIVE',
                    activated_at = COALESCE(activated_at, CAST($16 AS TIMESTAMPTZ)),
                    updated_by = $17, updated_at = CAST($16 AS TIMESTAMPTZ),
                    version = version + 1
                 WHERE id = $1",
            )
            .bind(existing_id)
            .bind(&binding.key)
            .bind(domain.id)
            .bind(&domain.hostname)
            .bind(environment)
            .bind(&binding.path_prefix)
            .bind(persisted.action_type)
            .bind(persisted.default_variant_id)
            .bind(persisted.forced_variant_id)
            .bind(persisted.redirect_scheme)
            .bind(persisted.redirect_hostname.clone())
            .bind(persisted.redirect_path_prefix.clone())
            .bind(persisted.redirect_status_code)
            .bind(persisted.preserve_path)
            .bind(persisted.preserve_query)
            .bind(&command.generated_at)
            .bind(command.actor_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| composition_store_error("update app binding", error))?;
            continue;
        }
        let id = next_id(repository.id_generator())?;
        sqlx::query(
            "INSERT INTO deploy_app_binding (
                id,uuid,tenant_id,organization_id,app_id,binding_key,domain_id,hostname_ascii,
                environment,path_prefix,action_type,default_variant_id,forced_variant_id,
                redirect_scheme,redirect_hostname,redirect_path_prefix,redirect_status_code,
                preserve_path,preserve_query,status,verified_at,activated_at,created_by,updated_by,
                created_at,updated_at,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                $18,$19,'ACTIVE',CAST($20 AS TIMESTAMPTZ),CAST($20 AS TIMESTAMPTZ),$21,$21,
                CAST($20 AS TIMESTAMPTZ),CAST($20 AS TIMESTAMPTZ),1)",
        )
        .bind(id)
        .bind(new_uuid())
        .bind(command.tenant_id)
        .bind(app.organization_id)
        .bind(app.id)
        .bind(&binding.key)
        .bind(domain.id)
        .bind(&domain.hostname)
        .bind(environment)
        .bind(&binding.path_prefix)
        .bind(persisted.action_type)
        .bind(persisted.default_variant_id)
        .bind(persisted.forced_variant_id)
        .bind(persisted.redirect_scheme)
        .bind(persisted.redirect_hostname)
        .bind(persisted.redirect_path_prefix)
        .bind(persisted.redirect_status_code)
        .bind(persisted.preserve_path)
        .bind(persisted.preserve_query)
        .bind(&command.generated_at)
        .bind(command.actor_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert app binding", error))?;
    }
    Ok(())
}

/// Read back **every** ACTIVE binding of the app for one environment and turn
/// it into the descriptor's binding set.
///
/// This is the single-source-of-truth step for hostname routing: the same
/// `deploy_app_binding` rows the Web Server lookup matches on are the rows the
/// compiled descriptor routes on, so an auto-provisioned platform publishing
/// domain is routable without the client re-declaring it.
async fn load_environment_bindings(
    transaction: &mut Transaction<'static, Postgres>,
    app_id: i64,
    environment: &str,
) -> DeployServiceResult<Vec<RuntimeBinding>> {
    let rows = sqlx::query(
        "SELECT uuid, hostname_ascii, path_prefix, action_type,
                default_variant_id, forced_variant_id,
                redirect_scheme, redirect_hostname, redirect_path_prefix, redirect_status_code,
                preserve_path, preserve_query
         FROM deploy_app_binding
         WHERE app_id = $1 AND environment = $2 AND deleted_at IS NULL AND status = 'ACTIVE'
         ORDER BY hostname_ascii, path_prefix, uuid",
    )
    .bind(app_id)
    .bind(environment)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load environment bindings", error))?;
    let mut variant_uuids: BTreeMap<i64, String> = BTreeMap::new();
    let mut runtime = Vec::with_capacity(rows.len());
    for row in rows {
        let action_type: String = row
            .try_get("action_type")
            .map_err(|error| composition_store_error("read binding action", error))?;
        let action = if action_type == "REDIRECT" {
            let scheme: Option<String> = row.try_get("redirect_scheme").ok().flatten();
            RuntimeBindingAction::Redirect {
                status_code: row
                    .try_get::<i32, _>("redirect_status_code")
                    .unwrap_or(302)
                    .clamp(0, i32::from(u16::MAX)) as u16,
                scheme: match scheme.as_deref() {
                    Some("http") => RuntimeRedirectScheme::Http,
                    _ => RuntimeRedirectScheme::Https,
                },
                hostname: row
                    .try_get::<Option<String>, _>("redirect_hostname")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                path_prefix: row
                    .try_get::<Option<String>, _>("redirect_path_prefix")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "/".to_owned()),
                preserve_path: row.try_get("preserve_path").unwrap_or(true),
                preserve_query: row.try_get("preserve_query").unwrap_or(true),
            }
        } else {
            let default_variant_id: Option<i64> = row.try_get("default_variant_id").ok().flatten();
            let forced_variant_id: Option<i64> = row.try_get("forced_variant_id").ok().flatten();
            let default =
                resolve_variant_uuid(transaction, &mut variant_uuids, default_variant_id).await?;
            let forced =
                resolve_variant_uuid(transaction, &mut variant_uuids, forced_variant_id).await?;
            RuntimeBindingAction::serve(default, forced)
        };
        runtime.push(RuntimeBinding {
            binding_uuid: row
                .try_get("uuid")
                .map_err(|error| composition_store_error("read binding uuid", error))?,
            hostname: row
                .try_get("hostname_ascii")
                .map_err(|error| composition_store_error("read binding hostname", error))?,
            path_prefix: row
                .try_get("path_prefix")
                .map_err(|error| composition_store_error("read binding path prefix", error))?,
            action,
        });
    }
    Ok(runtime)
}

/// Resolve a variant's public uuid, memoized per call.
async fn resolve_variant_uuid(
    transaction: &mut Transaction<'static, Postgres>,
    cache: &mut BTreeMap<i64, String>,
    variant_id: Option<i64>,
) -> DeployServiceResult<Option<String>> {
    let Some(variant_id) = variant_id else {
        return Ok(None);
    };
    if let Some(uuid) = cache.get(&variant_id) {
        return Ok(Some(uuid.clone()));
    }
    let uuid: Option<String> = sqlx::query_scalar(
        "SELECT uuid FROM deploy_app_variant WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(variant_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("resolve binding variant", error))?;
    if let Some(uuid) = uuid {
        cache.insert(variant_id, uuid.clone());
        Ok(Some(uuid))
    } else {
        // The variant was removed by a later composition update. A binding that
        // can no longer resolve its variant must not silently fall through to
        // the app default; surfacing it as a validation error is the safe
        // behaviour for a published composition.
        Err(DeployServiceError::validation(
            "binding references a variant that no longer exists in this environment",
        ))
    }
}

#[derive(Clone, Debug)]
struct PersistedBindingAction {
    action_type: &'static str,
    default_variant_id: Option<i64>,
    forced_variant_id: Option<i64>,
    redirect_scheme: Option<&'static str>,
    redirect_hostname: Option<String>,
    redirect_path_prefix: Option<String>,
    redirect_status_code: Option<i32>,
    preserve_path: bool,
    preserve_query: bool,
}

fn binding_action(
    action: &AppBindingAction,
    variants: &BTreeMap<String, StoredVariant>,
) -> DeployServiceResult<PersistedBindingAction> {
    match action {
        AppBindingAction::Serve {
            default_variant_key,
            forced_variant_key,
        } => {
            let default = optional_variant(default_variant_key.as_deref(), variants)?;
            let forced = optional_variant(forced_variant_key.as_deref(), variants)?;
            Ok(PersistedBindingAction {
                action_type: "SERVE",
                default_variant_id: default.map(|variant| variant.id),
                forced_variant_id: forced.map(|variant| variant.id),
                redirect_scheme: None,
                redirect_hostname: None,
                redirect_path_prefix: None,
                redirect_status_code: None,
                preserve_path: true,
                preserve_query: true,
            })
        }
        AppBindingAction::Redirect {
            status_code,
            scheme,
            hostname,
            path_prefix,
            preserve_path,
            preserve_query,
        } => Ok(PersistedBindingAction {
            action_type: "REDIRECT",
            default_variant_id: None,
            forced_variant_id: None,
            redirect_scheme: Some(redirect_scheme_name(*scheme)),
            redirect_hostname: Some(hostname.clone()),
            redirect_path_prefix: Some(path_prefix.clone()),
            redirect_status_code: Some(i32::from(*status_code)),
            preserve_path: *preserve_path,
            preserve_query: *preserve_query,
        }),
    }
}

fn optional_variant<'a>(
    key: Option<&str>,
    variants: &'a BTreeMap<String, StoredVariant>,
) -> DeployServiceResult<Option<&'a StoredVariant>> {
    key.map(|key| {
        variants
            .get(key)
            .ok_or_else(|| DeployServiceError::validation("binding references unknown variant"))
    })
    .transpose()
}

async fn load_verified_domain(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    domain_uuid: &str,
) -> DeployServiceResult<StoredDomain> {
    let row = sqlx::query(
        "SELECT id, hostname_ascii FROM deploy_domain
         WHERE tenant_id = $1 AND uuid = $2
           AND verification_status = 'VERIFIED' AND status = 'ACTIVE'
           AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(domain_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load app binding domain", error))?
    .ok_or_else(|| DeployServiceError::not_found("domain not found for app"))?;
    Ok(StoredDomain {
        id: row
            .try_get("id")
            .map_err(|_| DeployServiceError::Internal("invalid domain record".to_owned()))?,
        hostname: row
            .try_get("hostname_ascii")
            .map_err(|_| DeployServiceError::Internal("invalid domain record".to_owned()))?,
    })
}

fn runtime_resource(
    resource: &ValidatedContentProviderResource,
    stored: &BTreeMap<String, StoredResource>,
) -> DeployServiceResult<RuntimeResource> {
    let stored = stored
        .get(&resource.key)
        .ok_or_else(|| DeployServiceError::Internal("stored resource is missing".to_owned()))?;
    Ok(RuntimeResource {
        resource_uuid: stored.uuid.clone(),
        provider: RuntimeProviderReference {
            provider_type: resource.provider_type,
            provider_resource_uuid: resource.provider_resource_uuid.clone(),
            provider_contract_version: resource.provider_contract_version.clone(),
        },
        capabilities: resource.capabilities.clone(),
    })
}

async fn next_revision_number(
    transaction: &mut Transaction<'static, Postgres>,
    app_id: i64,
) -> DeployServiceResult<i64> {
    let row = sqlx::query(
        "SELECT COALESCE(MAX(revision_no), 0) + 1 AS revision_no
         FROM deploy_app_revision WHERE app_id = $1",
    )
    .bind(app_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("reserve app revision number", error))?;
    row.try_get("revision_no")
        .map_err(|_| DeployServiceError::Internal("invalid app revision number".to_owned()))
}

#[allow(clippy::too_many_arguments)]
async fn insert_revision(
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app: &StoredApp,
    revision_id: i64,
    revision_uuid: &str,
    revision_number: i64,
    source_config_version: i64,
    descriptor: &serde_json::Value,
    descriptor_sha256: &str,
) -> DeployServiceResult<()> {
    let descriptor_json = serde_json::to_string(descriptor)
        .map_err(|_| DeployServiceError::Internal("serialize app revision failed".to_owned()))?;
    // Supersede the newest VALID revision of the *same environment*, not
    // whichever revision the app-global `desired_revision_id` happens to hold.
    let supersedes_revision_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM deploy_app_revision
         WHERE app_id = $1 AND environment = $2 AND validation_status = 'VALID'
         ORDER BY revision_no DESC
         LIMIT 1",
    )
    .bind(app.id)
    .bind(command.request.environment.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load superseded app revision", error))?;
    sqlx::query(
        "INSERT INTO deploy_app_revision (
            id,uuid,tenant_id,organization_id,app_id,revision_no,environment,
            descriptor_schema_version,descriptor_json,descriptor_sha256,compiler_version,
            source_config_version,idempotency_key,request_sha256,result_json,validation_status,
            validation_report_json,supersedes_revision_id,created_by,created_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,CAST($9 AS JSONB),$10,$11,$12,$13,$14,
            '{}','VALID','{}',$15,$16,CAST($17 AS TIMESTAMPTZ))",
    )
    .bind(revision_id)
    .bind(revision_uuid)
    .bind(command.tenant_id)
    .bind(app.organization_id)
    .bind(app.id)
    .bind(revision_number)
    .bind(command.request.environment.as_str())
    .bind(WEBSITE_RUNTIME_SCHEMA_VERSION)
    .bind(&descriptor_json)
    .bind(descriptor_sha256)
    .bind(DESCRIPTOR_COMPILER_VERSION)
    .bind(source_config_version)
    .bind(&command.idempotency_key)
    .bind(&command.request_sha256)
    .bind(supersedes_revision_id)
    .bind(command.actor_id)
    .bind(&command.generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("insert app revision", error))?;
    Ok(())
}

async fn update_site_revision_pointers(
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app_id: i64,
    default_variant_id: i64,
    revision_id: i64,
) -> DeployServiceResult<()> {
    sqlx::query(
        "UPDATE deploy_app SET default_variant_id = $2, desired_revision_id = $3,
            updated_at = CAST($4 AS TIMESTAMPTZ) WHERE id = $1",
    )
    .bind(app_id)
    .bind(default_variant_id)
    .bind(revision_id)
    .bind(&command.generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("update app revision pointers", error))?;
    Ok(())
}

/// The other ACTIVE apps' current descriptor **for this environment**, used to
/// build the multi-site runtime set.
///
/// Selection is `(app, environment)` scoped -- the newest `VALID` revision of
/// that environment. The app-level `deploy_app.desired_revision_id` pointer is
/// deliberately not used: it holds whichever environment converged last, so a
/// runtime set published for `development` could carry another environment's
/// composition for the sibling apps.
async fn load_other_descriptors(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    excluded_app_id: i64,
    environment: &str,
) -> DeployServiceResult<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT CAST(r.descriptor_json AS TEXT) AS descriptor_json
         FROM deploy_app s
         INNER JOIN LATERAL (
             SELECT revision.descriptor_json
             FROM deploy_app_revision revision
             WHERE revision.app_id = s.id
               AND revision.environment = $3
               AND revision.validation_status = 'VALID'
             ORDER BY revision.revision_no DESC
             LIMIT 1
         ) r ON TRUE
         WHERE s.tenant_id = $1 AND s.id <> $2 AND s.app_status = 'ACTIVE'
           AND s.deleted_at IS NULL
         ORDER BY s.uuid",
    )
    .bind(tenant_id)
    .bind(excluded_app_id)
    .bind(environment)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("load desired app descriptors", error))?;
    rows.into_iter()
        .map(|row| {
            let text: String = row.try_get("descriptor_json").map_err(|_| {
                DeployServiceError::Internal("invalid desired app descriptor".to_owned())
            })?;
            serde_json::from_str(&text).map_err(|_| {
                DeployServiceError::Internal("invalid desired app descriptor".to_owned())
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn insert_runtime_assignments(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    revision_id: i64,
    targets: &[StoredTarget],
    descriptors: &[serde_json::Value],
) -> DeployServiceResult<Vec<AppRuntimeAssignmentResponse>> {
    let desired_state_sha256 = canonical_sha256_excluding_field(
        &serde_json::json!({"descriptors": descriptors}),
        "__no_excluded_field",
    )
    .map_err(|_| DeployServiceError::Internal("hash desired runtime state failed".to_owned()))?;
    let mut responses = Vec::with_capacity(targets.len());
    for target in targets {
        let generation = next_target_generation(transaction, target.id).await?;
        if generation > MAXIMUM_RUNTIME_GENERATION {
            return Err(DeployServiceError::conflict(
                "runtime assignment generation is exhausted",
            ));
        }
        let snapshot_uuid = new_uuid();
        let compiled = compile_runtime_set(RuntimeSetCompilationInput {
            snapshot_uuid: snapshot_uuid.clone(),
            node_uuid: target.node_uuid.clone(),
            environment: runtime_environment(command.request.environment),
            generation: generation as u64,
            generated_at: command.generated_at.clone(),
            maximum_sites: MAXIMUM_RUNTIME_SITES,
            descriptors: descriptors.to_vec(),
        })
        .map_err(|error| DeployServiceError::validation(error.to_string()))?;
        let runtime_set_json = serde_json::to_string(&compiled.snapshot)
            .map_err(|_| DeployServiceError::Internal("serialize runtime set failed".to_owned()))?;
        let runtime_set_bytes = runtime_set_size_bytes(&compiled.snapshot)
            .map_err(|error| DeployServiceError::validation(error.to_string()))?
            as i64;
        let assignment_id = next_id(repository.id_generator())?;
        let assignment_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_runtime_assignment (
                id,uuid,tenant_id,node_target_id,trigger_app_revision_id,generation,
                snapshot_uuid,snapshot_sha256,desired_state_sha256,runtime_set_json,
                runtime_set_bytes,publish_status,attempt_count,created_at,updated_at,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,CAST($10 AS JSONB),$11,'PENDING',0,
                CAST($12 AS TIMESTAMPTZ),CAST($12 AS TIMESTAMPTZ),1)",
        )
        .bind(assignment_id)
        .bind(&assignment_uuid)
        .bind(command.tenant_id)
        .bind(target.id)
        .bind(revision_id)
        .bind(generation)
        .bind(&snapshot_uuid)
        .bind(&compiled.snapshot_sha256)
        .bind(&desired_state_sha256)
        .bind(&runtime_set_json)
        .bind(runtime_set_bytes)
        .bind(&command.generated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("insert runtime assignment", error))?;
        sqlx::query(
            "UPDATE deploy_runtime_assignment SET publish_status = 'SUPERSEDED',
                lease_owner = NULL, lease_expires_at = NULL,
                updated_at = CAST($1 AS TIMESTAMPTZ), version = version + 1
             WHERE node_target_id = $2 AND generation < $3
               AND publish_status <> 'SUPERSEDED'",
        )
        .bind(&command.generated_at)
        .bind(target.id)
        .bind(generation)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("supersede runtime assignments", error))?;
        responses.push(AppRuntimeAssignmentResponse {
            target_id: target.uuid.clone(),
            assignment_id: assignment_uuid,
            generation: generation.to_string(),
            status: "PENDING".to_owned(),
        });
    }
    Ok(responses)
}

async fn next_target_generation(
    transaction: &mut Transaction<'static, Postgres>,
    target_id: i64,
) -> DeployServiceResult<i64> {
    let row = sqlx::query(
        "SELECT COALESCE(MAX(generation), 0) + 1 AS generation
         FROM deploy_runtime_assignment WHERE node_target_id = $1",
    )
    .bind(target_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("reserve runtime generation", error))?;
    row.try_get("generation")
        .map_err(|_| DeployServiceError::Internal("invalid runtime generation".to_owned()))
}

async fn persist_command_result(
    transaction: &mut Transaction<'static, Postgres>,
    revision_id: i64,
    response: &AppCompositionResponse,
) -> DeployServiceResult<()> {
    let result_json = serde_json::to_string(response).map_err(|_| {
        DeployServiceError::Internal("serialize composition result failed".to_owned())
    })?;
    sqlx::query("UPDATE deploy_app_revision SET result_json = CAST($2 AS JSONB) WHERE id = $1")
        .bind(revision_id)
        .bind(result_json)
        .execute(&mut **transaction)
        .await
        .map_err(|error| composition_store_error("store composition result", error))?;
    Ok(())
}

async fn insert_composition_audit(
    repository: &DeployRepository,
    transaction: &mut Transaction<'static, Postgres>,
    command: &ReplaceAppCompositionCommand,
    app_id: i64,
    revision_id: i64,
) -> DeployServiceResult<()> {
    let audit_id = next_id(repository.id_generator())?;
    let audit_uuid = new_uuid();
    let metadata = serde_json::json!({
        "revisionId": revision_id.to_string(),
        "requestSha256": command.request_sha256,
        "environment": command.request.environment.as_str(),
    })
    .to_string();
    sqlx::query(
        "INSERT INTO deploy_audit_log (
            id,uuid,tenant_id,organization_id,operator_id,operator_type,action,target_type,
            target_id,target_uuid,metadata,created_at
         ) VALUES ($1,$2,$3,$4,$5,'USER','sites.composition.update','app',$6,$7,
            CAST($8 AS JSONB),CAST($9 AS TIMESTAMPTZ))",
    )
    .bind(audit_id)
    .bind(audit_uuid)
    .bind(command.tenant_id)
    .bind(command.organization_id)
    .bind(command.actor_id)
    .bind(app_id)
    .bind(&command.app_uuid)
    .bind(metadata)
    .bind(&command.generated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| composition_store_error("insert app composition audit", error))?;
    Ok(())
}

fn runtime_environment(
    environment: sdkwork_deploy_contract::AppPublishEnvironment,
) -> RuntimeEnvironment {
    match environment {
        sdkwork_deploy_contract::AppPublishEnvironment::Development => {
            RuntimeEnvironment::Development
        }
        sdkwork_deploy_contract::AppPublishEnvironment::Test => RuntimeEnvironment::Test,
        sdkwork_deploy_contract::AppPublishEnvironment::Staging => RuntimeEnvironment::Staging,
        sdkwork_deploy_contract::AppPublishEnvironment::Demo => RuntimeEnvironment::Demo,
        sdkwork_deploy_contract::AppPublishEnvironment::Production => {
            RuntimeEnvironment::Production
        }
    }
}

fn provider_type_name(provider_type: RuntimeProviderType) -> &'static str {
    match provider_type {
        RuntimeProviderType::Drive => "DRIVE",
        RuntimeProviderType::Knowledgebase => "KNOWLEDGEBASE",
    }
}

fn client_class_name(client_class: AppClientClass) -> &'static str {
    match client_class {
        AppClientClass::Desktop => "DESKTOP",
        AppClientClass::Mobile => "MOBILE",
        AppClientClass::Tablet => "TABLET",
        AppClientClass::Tv => "TV",
        AppClientClass::Bot => "BOT",
        AppClientClass::Other => "OTHER",
    }
}

fn runtime_client_class(client_class: AppClientClass) -> RuntimeClientClass {
    match client_class {
        AppClientClass::Desktop => RuntimeClientClass::Desktop,
        AppClientClass::Mobile => RuntimeClientClass::Mobile,
        AppClientClass::Tablet => RuntimeClientClass::Tablet,
        AppClientClass::Tv => RuntimeClientClass::Tv,
        AppClientClass::Bot => RuntimeClientClass::Bot,
        AppClientClass::Other => RuntimeClientClass::Other,
    }
}

fn mount_mode_name(mode: AppMountMode) -> &'static str {
    match mode {
        AppMountMode::Root => "ROOT",
        AppMountMode::Alias => "ALIAS",
    }
}

fn runtime_mount_mode(mode: AppMountMode) -> RuntimeMountMode {
    match mode {
        AppMountMode::Root => RuntimeMountMode::Root,
        AppMountMode::Alias => RuntimeMountMode::Alias,
    }
}

fn mount_handler_name(handler: AppMountHandler) -> &'static str {
    match handler {
        AppMountHandler::Static => "STATIC",
        AppMountHandler::Spa => "SPA",
        AppMountHandler::Wiki => "WIKI",
    }
}

fn runtime_handler(handler: AppMountHandler) -> RuntimeHandler {
    match handler {
        AppMountHandler::Static => RuntimeHandler::Static,
        AppMountHandler::Spa => RuntimeHandler::Spa,
        AppMountHandler::Wiki => RuntimeHandler::Wiki,
    }
}

fn redirect_scheme_name(scheme: AppRedirectScheme) -> &'static str {
    match scheme {
        AppRedirectScheme::Http => "http",
        AppRedirectScheme::Https => "https",
    }
}

fn invalid_stored_target() -> DeployServiceError {
    DeployServiceError::Internal("invalid Web Node target".to_owned())
}

fn composition_store_error(context: &str, error: sqlx::Error) -> DeployServiceError {
    tracing::error!(error = %error, "{context}");
    match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            DeployServiceError::conflict("app composition conflicts with current state")
        }
        _ => DeployServiceError::Internal(format!("{context} failed")),
    }
}

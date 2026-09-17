mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_content_provider_port::ValidatedContentProviderResource;
use sdkwork_deploy_contract::{
    AppBindingAction, AppBindingDefinition, AppClientClass, AppDeliveryPolicy, AppMountDefinition,
    AppMountHandler, AppMountMode, AppObservabilityPolicy, AppPublishEnvironment,
    AppResourceDefinition, AppRuntimeLimits, AppSecurityPolicy, AppVariantDefinition,
    AppVariantRuleDefinition, AppVariantRuleMatcher, ContentProviderResourceSource,
    DriveWebsiteContentMode, DriveWebsiteRootSelector, UpdateAppCompositionRequest,
};
use sdkwork_deploy_runtime_compiler::{
    RuntimeProviderType, RuntimeResourceCapabilities, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{
    AppCompositionRepositoryPort, DeployRepositoryPort, ReplaceAppCompositionCommand,
};
use sqlx::{PgPool, Row};

async fn test_repository() -> (DeployRepository, PgPool) {
    let pool = common::postgres_pool().await;
    seed_control_plane(&pool).await;
    (
        DeployRepository::new(
            pool.clone(),
            SnowflakeIdGenerator::new(2).expect("Snowflake generator"),
            common::test_secret_key(),
        ),
        pool,
    )
}

async fn seed_control_plane(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deploy_app (
            id,uuid,tenant_id,organization_id,name,slug,app_kind,app_status,runtime_config,
            metadata,default_environment,created_at,updated_at,version
         ) VALUES (10,'site-1',7,9,'Docs','docs','WEB','ACTIVE','{}','{}',
                   'production','2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',0)",
    )
    .execute(pool)
    .await
    .expect("insert site");
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status,
            created_at,updated_at,version
         ) VALUES (19,'zone-1',7,9,'example.com','ACTIVE',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(pool)
    .await
    .expect("insert DNS zone");
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status,metadata,created_at,updated_at,version
         ) VALUES (20,'domain-1',7,9,19,'docs.example.com','EXACT','VERIFIED',
                   '2026-07-22T00:00:00Z','ACTIVE','{}',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(pool)
    .await
    .expect("insert domain");
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id,uuid,tenant_id,node_uuid,environment,tenant_scope_hash,status,
            created_at,updated_at,version
         ) VALUES (30,'target-1',7,'node-1','production',$1,'ACTIVE',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .bind("a".repeat(64))
    .execute(pool)
    .await
    .expect("insert target");
}

fn request(handler: AppMountHandler) -> UpdateAppCompositionRequest {
    UpdateAppCompositionRequest {
        environment: AppPublishEnvironment::Production,
        default_variant_key: "default".to_owned(),
        resources: vec![AppResourceDefinition {
            key: "content".to_owned(),
            source: ContentProviderResourceSource::drive_directory(
                "space-1".to_owned(),
                DriveWebsiteRootSelector::SpaceRoot,
                DriveWebsiteContentMode::LiveTree,
            ),
        }],
        variants: vec![AppVariantDefinition {
            key: "default".to_owned(),
            label: "Default".to_owned(),
            client_class: AppClientClass::Other,
            priority: 0,
        }],
        variant_rules: vec![],
        mounts: vec![AppMountDefinition {
            key: "root".to_owned(),
            variant_key: "default".to_owned(),
            resource_key: "content".to_owned(),
            path_prefix: "/".to_owned(),
            resource_subpath: "/".to_owned(),
            mode: AppMountMode::Root,
            handler,
            index_files: vec!["index.html".to_owned()],
            spa_fallback: None,
            priority: 0,
        }],
        bindings: vec![AppBindingDefinition {
            key: "primary".to_owned(),
            domain_id: "domain-1".to_owned(),
            path_prefix: "/".to_owned(),
            action: AppBindingAction::Serve {
                default_variant_key: None,
                forced_variant_key: None,
            },
        }],
        delivery_policy: AppDeliveryPolicy::default(),
        security_policy: AppSecurityPolicy::default(),
        limits: AppRuntimeLimits::default(),
        observability_policy: AppObservabilityPolicy::default(),
    }
}

fn command(
    expected_version: i64,
    idempotency_key: &str,
    request_sha256: &str,
    handler: AppMountHandler,
) -> ReplaceAppCompositionCommand {
    let request = request(handler);
    ReplaceAppCompositionCommand {
        tenant_id: 7,
        organization_id: 9,
        actor_id: 11,
        app_uuid: "site-1".to_owned(),
        expected_app_version: expected_version,
        idempotency_key: idempotency_key.to_owned(),
        request_sha256: request_sha256.to_owned(),
        generated_at: "2026-07-22T00:00:01Z".to_owned(),
        resources: vec![ValidatedContentProviderResource {
            key: "content".to_owned(),
            source: request.resources[0].source.clone(),
            provider_type: RuntimeProviderType::Drive,
            provider_resource_uuid: "website-root-1".to_owned(),
            provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION.to_owned(),
            capabilities: RuntimeResourceCapabilities {
                static_content: true,
                wiki_routes: false,
                wiki_search: false,
                range_requests: true,
            },
        }],
        request,
    }
}

fn tv_command(
    expected_version: i64,
    idempotency_key: &str,
    request_sha256: &str,
) -> ReplaceAppCompositionCommand {
    let mut command = command(
        expected_version,
        idempotency_key,
        request_sha256,
        AppMountHandler::Static,
    );
    command.request.variants[0].client_class = AppClientClass::Tv;
    command
        .request
        .variant_rules
        .push(AppVariantRuleDefinition {
            key: "tv-client".to_owned(),
            target_variant_key: "default".to_owned(),
            priority: 100,
            matcher: AppVariantRuleMatcher::ClientClass {
                client_class: AppClientClass::Tv,
            },
        });
    command
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn composition_is_atomic_idempotent_and_does_not_create_releases() {
    let (repository, pool) = test_repository().await;
    let first = repository
        .replace_app_composition(command(
            0,
            "composition-1",
            &"1".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect("publish first composition");
    assert_eq!(first.app_version, "1");
    assert_eq!(first.revision.number, "1");
    assert_eq!(first.runtime_assignments.len(), 1);
    assert_eq!(first.runtime_assignments[0].generation, "1");

    let replay = repository
        .replace_app_composition(command(
            0,
            "composition-1",
            &"1".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect("replay composition");
    assert_eq!(replay.revision.id, first.revision.id);
    assert_eq!(
        replay.runtime_assignments[0].assignment_id,
        first.runtime_assignments[0].assignment_id
    );

    let conflicting_key = repository
        .replace_app_composition(command(
            0,
            "composition-1",
            &"2".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect_err("same key with another request must conflict");
    assert!(conflicting_key.to_string().contains("Idempotency-Key"));

    repository
        .replace_app_composition(command(
            0,
            "composition-stale",
            &"3".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect_err("stale site version must conflict");

    repository
        .replace_app_composition(command(
            1,
            "composition-invalid",
            &"4".repeat(64),
            AppMountHandler::Wiki,
        ))
        .await
        .expect_err("incompatible provider and handler must roll back");

    let site = sqlx::query(
        "SELECT version, desired_revision_id, default_variant_id FROM deploy_app WHERE id = 10",
    )
    .fetch_one(&pool)
    .await
    .expect("load site");
    assert_eq!(site.try_get::<i64, _>("version").unwrap(), 1);
    assert!(site.try_get::<i64, _>("desired_revision_id").is_ok());
    assert!(site.try_get::<i64, _>("default_variant_id").is_ok());

    let counts = sqlx::query(
        "SELECT
            (SELECT COUNT(*) FROM deploy_app_revision) AS revisions,
            (SELECT COUNT(*) FROM deploy_runtime_assignment) AS assignments,
            (SELECT COUNT(*) FROM deploy_release) AS releases,
            (SELECT COUNT(*) FROM deploy_deployment) AS deployments",
    )
    .fetch_one(&pool)
    .await
    .expect("load counts");
    assert_eq!(counts.try_get::<i64, _>("revisions").unwrap(), 1);
    assert_eq!(counts.try_get::<i64, _>("assignments").unwrap(), 1);
    assert_eq!(counts.try_get::<i64, _>("releases").unwrap(), 0);
    assert_eq!(counts.try_get::<i64, _>("deployments").unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn composition_persists_tv_client_class_and_compiles_the_runtime_rule() {
    let (repository, pool) = test_repository().await;
    repository
        .replace_app_composition(tv_command(0, "composition-tv", &"9".repeat(64)))
        .await
        .expect("publish TV composition");

    let stored = sqlx::query(
        "SELECT v.client_class, r.match_value,
                CAST(revision.descriptor_json AS TEXT) AS descriptor_json
         FROM deploy_app_variant v
         INNER JOIN deploy_app_variant_rule r ON r.app_id = v.app_id
         INNER JOIN deploy_app app ON app.id = v.app_id
         INNER JOIN LATERAL (
             SELECT revision.descriptor_json
             FROM deploy_app_revision revision
             WHERE revision.app_id = app.id
               AND revision.environment = 'production'
               AND revision.validation_status = 'VALID'
             ORDER BY revision.revision_no DESC
             LIMIT 1
         ) revision ON TRUE
         WHERE v.app_id = 10",
    )
    .fetch_one(&pool)
    .await
    .expect("load TV composition");
    assert_eq!(stored.try_get::<String, _>("client_class").unwrap(), "TV");
    assert_eq!(stored.try_get::<String, _>("match_value").unwrap(), "TV");
    let descriptor: serde_json::Value =
        serde_json::from_str(&stored.try_get::<String, _>("descriptor_json").unwrap())
            .expect("parse stored runtime descriptor");
    assert_eq!(descriptor["variantRules"][0]["match"]["clientClass"], "TV");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn composition_rejects_a_domain_owned_by_another_tenant() {
    let (repository, pool) = test_repository().await;
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status,
            created_at,updated_at,version
         ) VALUES (29,'zone-foreign',8,9,'foreign.example.com','ACTIVE',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(&pool)
    .await
    .expect("insert foreign tenant DNS zone");
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status,metadata,created_at,updated_at,version
         ) VALUES (21,'domain-foreign',8,9,29,'foreign.example.com','EXACT','VERIFIED',
                   '2026-07-22T00:00:00Z','ACTIVE','{}',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(&pool)
    .await
    .expect("insert foreign tenant domain");

    let mut command = command(
        0,
        "composition-foreign-domain",
        &"6".repeat(64),
        AppMountHandler::Static,
    );
    command.request.bindings[0].domain_id = "domain-foreign".to_owned();
    let error = repository
        .replace_app_composition(command)
        .await
        .expect_err("cross-tenant domain must not bind");
    assert!(error.to_string().contains("domain not found for app"));
    assert_composition_was_not_committed(&pool).await;
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn composition_requires_an_active_web_target() {
    let (repository, pool) = test_repository().await;
    sqlx::query("DELETE FROM deploy_web_node_target WHERE tenant_id = 7")
        .execute(&pool)
        .await
        .expect("remove Web target");

    let error = repository
        .replace_app_composition(command(
            0,
            "composition-no-target",
            &"7".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect_err("composition without target must fail");
    assert!(error.to_string().contains("no active Web Node target"));
    assert_composition_was_not_committed(&pool).await;
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn composition_rejects_inconsistent_target_tenant_scope() {
    let (repository, pool) = test_repository().await;
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id,uuid,tenant_id,node_uuid,environment,tenant_scope_hash,status,
            created_at,updated_at,version
         ) VALUES (31,'target-2',7,'node-2','production',$1,'ACTIVE',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .bind("b".repeat(64))
    .execute(&pool)
    .await
    .expect("insert inconsistent Web target");

    let error = repository
        .replace_app_composition(command(
            0,
            "composition-scope-conflict",
            &"8".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect_err("inconsistent target scope must fail");
    assert!(error
        .to_string()
        .contains("Web Node targets have inconsistent tenant scope"));
    assert_composition_was_not_committed(&pool).await;
}

async fn assert_composition_was_not_committed(pool: &PgPool) {
    let row = sqlx::query(
        "SELECT version,
                (SELECT COUNT(*) FROM deploy_app_revision) AS revisions,
                (SELECT COUNT(*) FROM deploy_runtime_assignment) AS assignments
         FROM deploy_app WHERE id = 10",
    )
    .fetch_one(pool)
    .await
    .expect("load rolled-back composition state");
    assert_eq!(row.try_get::<i64, _>("version").unwrap(), 0);
    assert_eq!(row.try_get::<i64, _>("revisions").unwrap(), 0);
    assert_eq!(row.try_get::<i64, _>("assignments").unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_composition_is_atomic_and_idempotent() {
    let pool = common::postgres_pool().await;
    seed_control_plane(&pool).await;
    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
        common::test_secret_key(),
    );

    let first = repository
        .replace_app_composition(tv_command(0, "composition-postgres-1", &"5".repeat(64)))
        .await
        .expect("publish PostgreSQL composition");
    assert_eq!(first.app_version, "1");
    assert_eq!(first.runtime_assignments[0].generation, "1");
    let replay = repository
        .replace_app_composition(tv_command(0, "composition-postgres-1", &"5".repeat(64)))
        .await
        .expect("replay PostgreSQL composition");
    assert_eq!(replay.revision.id, first.revision.id);

    let row = sqlx::query(
        "SELECT s.version, r.request_sha256, CAST(r.result_json AS TEXT) AS result_json,
                a.generation, a.publish_status, v.client_class, vr.match_value
         FROM deploy_app s
         INNER JOIN deploy_app_revision r ON r.id = s.desired_revision_id
         INNER JOIN deploy_runtime_assignment a ON a.trigger_app_revision_id = r.id
         INNER JOIN deploy_app_variant v ON v.app_id = s.id
         INNER JOIN deploy_app_variant_rule vr ON vr.app_id = s.id
         WHERE s.id = 10",
    )
    .fetch_one(&pool)
    .await
    .expect("load PostgreSQL composition state");
    assert_eq!(row.try_get::<i64, _>("version").unwrap(), 1);
    assert_eq!(row.try_get::<i64, _>("generation").unwrap(), 1);
    assert_eq!(
        row.try_get::<String, _>("publish_status").unwrap(),
        "PENDING"
    );
    assert_eq!(row.try_get::<String, _>("client_class").unwrap(), "TV");
    assert_eq!(row.try_get::<String, _>("match_value").unwrap(), "TV");
    assert!(row
        .try_get::<String, _>("result_json")
        .unwrap()
        .contains(&first.revision.id));
}

/// P0 regression: replacing one environment's composition must not destroy
/// another environment's publishable hostnames.
///
/// Before the environment-scoped reconcile, `delete_current_composition`
/// deleted every `deploy_app_binding` of the app — including the 14
/// auto-provisioned platform publishing domains — whatever environment the
/// request targeted, so any composition update silently made
/// `<appId>.app.<suffix>` stop resolving.
#[tokio::test]
async fn composition_replace_preserves_other_environments_and_default_domains() {
    let (repository, pool) = test_repository().await;
    // A Web Node target is required for every environment that publishes.
    sqlx::query(
        "INSERT INTO deploy_web_node_target (
            id,uuid,tenant_id,node_uuid,environment,tenant_scope_hash,status,
            created_at,updated_at,version
         ) VALUES (31,'target-dev',7,'node-dev','development',$1,'ACTIVE',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .bind("b".repeat(64))
    .execute(&pool)
    .await
    .expect("insert development target");

    let production = repository
        .replace_app_composition(command(
            0,
            "composition-prod-1",
            &"6".repeat(64),
            AppMountHandler::Static,
        ))
        .await
        .expect("publish production composition");
    assert_eq!(production.app_version, "1");

    let count = |pool: PgPool, environment: &'static str| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM deploy_app_binding
             WHERE app_id = 10 AND environment = $1 AND deleted_at IS NULL
               AND status = 'ACTIVE'",
        )
        .bind(environment)
        .fetch_one(&pool)
        .await
        .expect("count bindings")
    };
    let production_before = count(pool.clone(), "production").await;
    assert_eq!(
        production_before, 15,
        "14 platform publishing domains + 1 declared binding"
    );

    let mut development_request = request(AppMountHandler::Static);
    development_request.environment = AppPublishEnvironment::Development;
    let mut development_command = tv_command(1, "composition-dev-1", &"7".repeat(64));
    development_command.request = development_request;
    repository
        .replace_app_composition(development_command)
        .await
        .expect("publish development composition");

    let production_after = count(pool.clone(), "production").await;
    assert_eq!(
        production_after, production_before,
        "a development publish must not delete production bindings"
    );
    let production_revisions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_revision WHERE app_id = 10 AND environment = 'production'",
    )
    .fetch_one(&pool)
    .await
    .expect("count production revisions");
    assert_eq!(production_revisions, 1);
    let development_bindings = count(pool.clone(), "development").await;
    assert_eq!(development_bindings, 15);

    // The production descriptor still carries the production hostnames, and the
    // platform publishing domains resolve for production.
    let resolved = repository
        .resolve_server_by_hostname("docs.app.sdkwork.com", "production")
        .await
        .expect("resolve")
        .expect("the platform publishing domain must keep resolving");
    assert_eq!(resolved.environment, "production");
}

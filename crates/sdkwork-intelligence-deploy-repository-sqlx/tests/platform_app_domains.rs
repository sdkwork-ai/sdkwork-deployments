mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::app_domains::platform_app_domain_suffixes;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::{PgPool, Row};

async fn test_repository() -> (DeployRepository, PgPool) {
    let pool = common::postgres_pool().await;
    seed_control_plane(&pool).await;
    (
        DeployRepository::new(
            pool.clone(),
            SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
            common::test_secret_key(),
        ),
        pool,
    )
}

async fn seed_control_plane(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deploy_app (
            id,uuid,tenant_id,organization_id,name,slug,app_kind,app_status,
            default_environment,created_at,updated_at,version
         ) VALUES (10,'site-10',7,9,'Shop','shop','WEB','ACTIVE','production',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(pool)
    .await
    .expect("seed deploy_app");
}

/// Inserts a compiled revision for one lifecycle environment. Version
/// resolution is per `(app, environment)` and reads the newest `VALID`
/// revision, so no app-level pointer is written.
async fn seed_revision(
    pool: &PgPool,
    revision_no: i64,
    environment: &str,
    descriptor: &serde_json::Value,
) -> String {
    let sha256 = sdkwork_utils_rust::crypto::sha256_hash(&serde_json::to_vec(descriptor).unwrap());
    sqlx::query(
        "INSERT INTO deploy_app_revision (
            id,uuid,tenant_id,organization_id,app_id,revision_no,environment,
            descriptor_schema_version,descriptor_json,descriptor_sha256,compiler_version,
            source_config_version,idempotency_key,request_sha256,validation_status,created_by,
            created_at
         ) VALUES ($1,$2,7,9,10,$3,$4,
            'sdkwork.website-runtime.v1',$5,$6,'test-compiler/1',1,$7,$8,'VALID',NULL,
            '2026-07-22T00:00:00Z')",
    )
    .bind(revision_no)
    .bind(format!("revision-{environment}-{revision_no}"))
    .bind(revision_no)
    .bind(environment)
    .bind(descriptor)
    .bind(&sha256)
    .bind(format!("key-{environment}-{revision_no}"))
    .bind(format!("req-{environment}-{revision_no}"))
    .execute(pool)
    .await
    .expect("seed deploy_app_revision");
    sha256
}

fn descriptor(app_uuid: &str, marker: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": "revision-0001",
        "appUuid": app_uuid,
        "tenantScopeHash": "1111111111111111111111111111111111111111111111111111111111111111",
        "environment": "production",
        "compilerVersion": "test-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-desktop",
        "marker": marker,
        "bindings": [],
        "variants": [],
        "variantRules": [],
        "resources": [],
        "mounts": [],
        "deliveryPolicy": {},
        "securityPolicy": {},
        "limits": {},
        "observabilityPolicy": {}
    })
}

#[tokio::test]
async fn provisions_default_domains_and_bindings_idempotently() {
    let (repository, pool) = test_repository().await;
    let zones = repository
        .ensure_platform_app_zones(7, 9, Some(1), &platform_app_domain_suffixes())
        .await
        .expect("ensure zones");
    assert_eq!(zones, 14, "one platform zone per suffix");
    // Idempotent zone ensure.
    let zones_again = repository
        .ensure_platform_app_zones(7, 9, Some(1), &platform_app_domain_suffixes())
        .await
        .expect("ensure zones again");
    assert_eq!(zones_again, 0);

    let first = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    assert_eq!(first.created_zones, 0);
    assert_eq!(first.created_domains, 14);
    assert_eq!(first.created_bindings, 14);
    assert_eq!(first.existing_domains, 0);
    assert_eq!(first.existing_bindings, 0);
    assert_eq!(first.hostnames.len(), 14);
    assert!(first.hostnames.contains(&"shop.app.sdkwork.com".to_owned()));
    assert!(first.hostnames.contains(&"shop.app.86offer.cn".to_owned()));

    // Idempotent second pass: nothing new, everything reported as existing.
    let second = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("re-provision");
    assert_eq!(second.created_domains, 0);
    assert_eq!(second.created_bindings, 0);
    assert_eq!(second.existing_domains, 14);
    assert_eq!(second.existing_bindings, 14);

    // Zones were created by the explicit ensure call.
    let zones: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_dns_zone WHERE tenant_id = 7 AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count zones");
    assert_eq!(zones, 14);
    let domains: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_domain
         WHERE tenant_id = 7 AND verification_status = 'VERIFIED' AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count domains");
    assert_eq!(domains, 14 + 14, "14 zone apexes + 14 app domains");
}

#[tokio::test]
async fn every_environment_gets_its_own_publishable_hostname() {
    let (repository, _) = test_repository().await;
    for environment in ["development", "test", "staging", "demo", "production"] {
        repository
            .provision_app_default_domains(7, 9, Some(1), "site-10", environment)
            .await
            .unwrap_or_else(|error| panic!("provision {environment}: {error}"));
    }
    let hostnames: Vec<String> = sqlx::query_scalar(
        "SELECT hostname_ascii FROM deploy_app_binding
         WHERE app_id = 10 AND deleted_at IS NULL ORDER BY hostname_ascii",
    )
    .fetch_all(repository.pool())
    .await
    .expect("list hostnames");
    // 14 suffixes x 5 environments, all distinct.
    assert_eq!(hostnames.len(), 70);
    for expected in [
        "shop.app.sdkwork.com",
        "shop.app-dev.sdkwork.com",
        "shop.app-test.sdkwork.com",
        "shop.app-staging.sdkwork.com",
        "shop.app-demo.sdkwork.com",
    ] {
        assert!(
            hostnames.contains(&expected.to_owned()),
            "missing hostname {expected}"
        );
    }
}

#[tokio::test]
async fn custom_app_domain_label_replaces_the_app_id_prefix() {
    let (repository, _) = test_repository().await;
    sqlx::query("UPDATE deploy_app SET app_domain_label = $1 WHERE id = 10")
        .bind("shop-front")
        .execute(repository.pool())
        .await
        .expect("set app domain label");
    let provisioned = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    assert!(provisioned
        .hostnames
        .contains(&"shop-front.app.sdkwork.com".to_owned()));
    assert!(!provisioned
        .hostnames
        .contains(&"shop.app.sdkwork.com".to_owned()));

    // The app's own uuid is a legal prefix (the "appId" reading of the spec).
    sqlx::query("UPDATE deploy_app SET app_domain_label = $1 WHERE id = 10")
        .bind("site-10")
        .execute(repository.pool())
        .await
        .expect("set uuid prefix");
    let renamed = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("re-provision");
    assert!(renamed
        .hostnames
        .contains(&"site-10.app.sdkwork.com".to_owned()));

    // The previous prefix is retired, not left behind as a second publishing
    // surface.
    let stale: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_binding
         WHERE app_id = 10 AND environment = 'production' AND deleted_at IS NULL
           AND hostname_ascii IN ('shop.app.sdkwork.com', 'shop-front.app.sdkwork.com')",
    )
    .fetch_one(repository.pool())
    .await
    .expect("count stale bindings");
    assert_eq!(stale, 0);
}

#[tokio::test]
async fn per_app_suffix_override_replaces_the_platform_catalog() {
    let (repository, _) = test_repository().await;
    sqlx::query("UPDATE deploy_app SET app_domain_suffixes = $1 WHERE id = 10")
        .bind(serde_json::json!(["example.com", "example.cn"]))
        .execute(repository.pool())
        .await
        .expect("set suffix override");
    let provisioned = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    assert_eq!(provisioned.hostnames.len(), 2);
    assert!(provisioned
        .hostnames
        .contains(&"shop.app.example.com".to_owned()));
    assert!(provisioned
        .hostnames
        .contains(&"shop.app.example.cn".to_owned()));
    // The override suffix zone is created on demand rather than failing.
    let zones: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_dns_zone
         WHERE tenant_id = 7 AND apex_hostname IN ('app.example.com', 'app.example.cn')
           AND deleted_at IS NULL",
    )
    .fetch_one(repository.pool())
    .await
    .expect("count override zones");
    assert_eq!(zones, 2);
}

/// Regression: the per-app suffix override could only ever be *seeded*, never
/// *written*. Every existing test sets `app_domain_suffixes` with a raw
/// `UPDATE ... $1` bound to a `serde_json::Value`, which never exercises the
/// repository's own INSERT/UPDATE. Those bound a bare `Vec<String>`, so sqlx
/// inferred `TEXT[]` against a `JSONB` column and PostgreSQL rejected the
/// statement with
///
/// ```text
/// column "app_domain_suffixes" is of type jsonb but expression is of type text[]
/// ```
///
/// `apps.create` / `apps.update` therefore returned a masked 500 for any payload
/// carrying `appDomainSuffixes`. This drives both writes through the real port
/// and asserts the value round-trips out of the JSONB column.
#[tokio::test]
async fn app_domain_suffix_overrides_round_trip_through_the_write_port() {
    use sdkwork_deploy_contract::{CreateAppRequest, DeployAppRequestContext, UpdateAppRequest};
    use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
    use sdkwork_intelligence_deploy_service::DeployService;
    use std::sync::Arc;

    let (repository, pool) = test_repository().await;
    let service = DeployService::new(Arc::new(repository), Arc::new(MemoryDeployDrivePort));
    let context = DeployAppRequestContext {
        tenant_id: 7,
        actor_id: Some(1),
        organization_id: Some(9),
        ..DeployAppRequestContext::default()
    };

    // `apps.create` with an explicit override: the INSERT JSONB write path.
    let created = service
        .create_app(
            &context,
            Some("create-with-override"),
            &CreateAppRequest {
                name: "With Override".to_owned(),
                slug: Some("with-override".to_owned()),
                app_kind: sdkwork_deploy_contract::AppKind::StaticWeb,
                app_type: None,
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: Some("with-override".to_owned()),
                app_domain_suffixes: Some(vec!["example.com".to_owned(), "example.cn".to_owned()]),
                idempotency_key: Some("create-with-override".to_owned()),
            },
        )
        .await
        .expect("creating an app with appDomainSuffixes must not fail on the JSONB bind");

    // The effective catalog comes back out of the JSONB column. The read path
    // re-normalizes (lowercase, deduplicate, sort), so the assertion is on the
    // set the app will actually publish on rather than on insertion order.
    assert_eq!(
        created.app_domain_suffixes,
        vec!["example.cn".to_owned(), "example.com".to_owned()],
        "the override survives the JSONB round trip (normalized and sorted)"
    );

    // And the column really holds a JSON array — not a text-array-shaped value.
    let stored_kind: String = sqlx::query_scalar(
        "SELECT jsonb_typeof(app_domain_suffixes) FROM deploy_app WHERE uuid = $1",
    )
    .bind(&created.id)
    .fetch_one(&pool)
    .await
    .expect("read the stored JSONB type");
    assert_eq!(
        stored_kind, "array",
        "the column must hold a JSON array, not a text array"
    );

    // The override actually drives provisioning (the reason the column exists).
    let provisioned = service
        .provision_app_default_domains(&context, &created.id, "production")
        .await
        .expect("provision from the override");
    assert!(
        provisioned
            .hostnames
            .contains(&"with-override.app.example.com".to_owned()),
        "the override catalog composes the hostnames; got {:?}",
        provisioned.hostnames
    );

    // `apps.update` is the second, independent JSONB write site. A payload that
    // replaces the list must persist the replacement...
    let updated = service
        .update_app(
            &context,
            &created.id,
            &UpdateAppRequest {
                name: None,
                description: None,
                app_type: None,
                runtime_config: None,
                metadata: None,
                app_status: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: Some(Some(vec!["updated.example.net".to_owned()])),
            },
        )
        .await
        .expect("updating appDomainSuffixes must not fail on the JSONB bind");
    assert_eq!(
        updated.app_domain_suffixes,
        vec!["updated.example.net".to_owned()],
        "the update replaced the override"
    );

    // ...and `Some(None)` means "clear the override", which restores the
    // platform catalog rather than leaving an empty array behind.
    let cleared = service
        .update_app(
            &context,
            &created.id,
            &UpdateAppRequest {
                name: None,
                description: None,
                app_type: None,
                runtime_config: None,
                metadata: None,
                app_status: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: Some(None),
            },
        )
        .await
        .expect("clearing appDomainSuffixes must not fail");
    assert_eq!(
        cleared.app_domain_suffixes,
        platform_app_domain_suffixes(),
        "clearing falls back to the platform catalog"
    );
}

/// Regression: `apps.domains.list` (`GET /app/v3/api/apps/{appId}/domains`)
/// returned a masked 500 because the read projection selected `z.apex`, a
/// column `deploy_dns_zone` never had (it is `apex_hostname`). The handler is
/// the only consumer of `list_app_domains_repo`, so nothing but a direct call
/// catches a typo here — every other test in this file exercises provisioning
/// and hostname resolution instead.
#[tokio::test]
async fn lists_app_domains_with_the_zone_apex_projected() {
    let (repository, _pool) = test_repository().await;
    repository
        .ensure_platform_app_zones(7, 9, Some(1), &platform_app_domain_suffixes())
        .await
        .expect("ensure zones");
    repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");

    let page = repository
        .list_app_domains(7, "site-10")
        .await
        .expect("list app domains must not fail on the zone apex projection");

    assert_eq!(
        page.total, 14,
        "every provisioned publishing hostname is listed"
    );
    assert_eq!(page.items.len(), 14);

    // A default hostname is platform-owned: no DNS step, and its `dnsRecordName`
    // is derived by stripping the zone apex (an `apex_hostname`, not `apex`).
    let default_host = page
        .items
        .iter()
        .find(|item| item.hostname == "shop.app.sdkwork.com")
        .expect("the canonical default hostname is listed");
    assert_eq!(default_host.kind, "DEFAULT");
    assert_eq!(default_host.verification_status, "NOT_REQUIRED");
    assert_eq!(default_host.dns_record_name, None);
    assert_eq!(
        default_host.cname_target.as_deref(),
        Some("shop.app.sdkwork.com")
    );
    assert_eq!(default_host.environment, "production");
    assert!(default_host.domain_id.is_some());
    assert_eq!(default_host.is_canonical, Some(true));

    // Every provisioned hostname is a DEFAULT (platform-owned) row, and they
    // sort alphabetically by hostname.
    assert!(page.items.iter().all(|item| item.kind == "DEFAULT"));
    let mut sorted = page
        .items
        .iter()
        .map(|item| item.hostname.clone())
        .collect::<Vec<_>>();
    sorted.sort();
    let listed = page
        .items
        .iter()
        .map(|item| item.hostname.clone())
        .collect::<Vec<_>>();
    assert_eq!(listed, sorted, "defaults sort by hostname");
}

#[tokio::test]
async fn resolves_the_newest_valid_revision_of_the_matching_environment() {
    let (repository, pool) = test_repository().await;
    for environment in ["production", "development"] {
        repository
            .provision_app_default_domains(7, 9, Some(1), "site-10", environment)
            .await
            .expect("provision");
    }
    // development publishes twice; production once.
    seed_revision(&pool, 1, "development", &descriptor("site-10", "dev-1")).await;
    seed_revision(&pool, 2, "development", &descriptor("site-10", "dev-2")).await;
    let production_sha =
        seed_revision(&pool, 3, "production", &descriptor("site-10", "prod-1")).await;

    let production = repository
        .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
        .await
        .expect("resolve production")
        .expect("production host must resolve");
    assert_eq!(production.app_uuid, "site-10");
    assert_eq!(production.app_slug, "shop");
    assert_eq!(production.environment, "production");
    assert_eq!(production.revision_no, 3);
    assert_eq!(production.descriptor_sha256, production_sha);
    assert_eq!(
        production.descriptor_json["marker"],
        serde_json::json!("prod-1"),
        "the production host must serve the production revision, not the newest app-global one"
    );

    let development = repository
        .resolve_server_by_hostname("shop.app-dev.sdkwork.com", "development")
        .await
        .expect("resolve development")
        .expect("development host must resolve");
    assert_eq!(development.environment, "development");
    assert_eq!(development.revision_no, 2);
    assert_eq!(
        development.descriptor_json["marker"],
        serde_json::json!("dev-2"),
        "the development host must serve the newest development revision"
    );

    // An environment with no revision of its own must not fall back to another
    // environment's descriptor.
    let staging = repository
        .resolve_server_by_hostname("shop.app-staging.sdkwork.com", "staging")
        .await
        .expect("resolve staging");
    assert!(
        staging.is_none(),
        "an environment with no VALID revision must not resolve"
    );

    // Case-insensitive lookup of an unknown suffix still resolves to None.
    let other = repository
        .resolve_server_by_hostname("SHOP.APP.BIRDBODER.COM", "production")
        .await
        .expect("resolve case-insensitive");
    assert!(other.is_none(), "unknown suffix must not resolve");

    // Unmatched custom hostnames resolve to None, not an error.
    let none = repository
        .resolve_server_by_hostname("mysite.example.com", "production")
        .await
        .expect("resolve custom");
    assert!(none.is_none());
}

#[tokio::test]
async fn paused_and_archived_apps_stop_resolving() {
    let (repository, pool) = test_repository().await;
    repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    seed_revision(&pool, 1, "production", &descriptor("site-10", "prod-1")).await;
    assert!(repository
        .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
        .await
        .expect("resolve")
        .is_some());
    sqlx::query("UPDATE deploy_app SET app_status = 'PAUSED' WHERE id = 10")
        .execute(&pool)
        .await
        .expect("pause app");
    assert!(
        repository
            .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
            .await
            .expect("resolve paused")
            .is_none(),
        "a PAUSED app must not be served through the fallback"
    );
}

#[tokio::test]
async fn app_nginx_conf_and_environment_override_are_resolved() {
    let (repository, pool) = test_repository().await;
    repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    seed_revision(&pool, 1, "production", &descriptor("site-10", "prod-1")).await;
    let app_conf = "server {\n  listen 80;\n  server_name shop.app.sdkwork.com;\n}\n";
    sqlx::query(
        "UPDATE deploy_app SET nginx_conf = $1, nginx_conf_sha256 = $2,
            nginx_conf_updated_at = NOW() WHERE id = 10",
    )
    .bind(app_conf)
    .bind(sdkwork_utils_rust::crypto::sha256_hash(app_conf.as_bytes()))
    .execute(&pool)
    .await
    .expect("set app nginx conf");

    // No override: the app-level conf is the answer.
    let resolved = repository
        .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
        .await
        .expect("resolve")
        .expect("must resolve");
    assert_eq!(resolved.nginx_conf.as_deref(), Some(app_conf));
    assert!(resolved.nginx_conf_sha256.is_some());
    assert_eq!(resolved.app_domain_label, "shop");

    // An environment-scoped override wins over the app-level base.
    let override_conf = "server {\n  listen 80;\n  return 418;\n}\n";
    sqlx::query(
        "INSERT INTO deploy_nginx_config (
            id,uuid,tenant_id,app_id,environment,hostname_ascii,config_type,config_name,
            config_content,config_hash,is_active,version_no,status,metadata,created_at,
            updated_at,version
         ) VALUES (700,'nginx-700',7,10,'production',NULL,1,'shop production override',
            $1,$2,TRUE,1,1,'{}',NOW(),NOW(),1)",
    )
    .bind(override_conf)
    .bind(sdkwork_utils_rust::crypto::sha256_hash(
        override_conf.as_bytes(),
    ))
    .execute(&pool)
    .await
    .expect("insert nginx override");
    let overridden = repository
        .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
        .await
        .expect("resolve")
        .expect("must resolve");
    assert_eq!(overridden.nginx_conf.as_deref(), Some(override_conf));

    // A hostname-scoped override wins over the environment-scoped one.
    let host_conf = "server {\n  listen 80;\n  return 410;\n}\n";
    sqlx::query(
        "INSERT INTO deploy_nginx_config (
            id,uuid,tenant_id,app_id,environment,hostname_ascii,config_type,config_name,
            config_content,config_hash,is_active,version_no,status,metadata,created_at,
            updated_at,version
         ) VALUES (701,'nginx-701',7,10,'production','shop.app.sdkwork.com',1,
            'shop host override',$1,$2,TRUE,1,1,'{}',NOW(),NOW(),1)",
    )
    .bind(host_conf)
    .bind(sdkwork_utils_rust::crypto::sha256_hash(
        host_conf.as_bytes(),
    ))
    .execute(&pool)
    .await
    .expect("insert hostname nginx override");
    let host_scoped = repository
        .resolve_server_by_hostname("shop.app.sdkwork.com", "production")
        .await
        .expect("resolve")
        .expect("must resolve");
    assert_eq!(host_scoped.nginx_conf.as_deref(), Some(host_conf));
}

#[tokio::test]
async fn resolves_custom_domains_and_respects_environment() {
    let (repository, pool) = test_repository().await;
    repository
        .ensure_platform_app_zones(7, 9, Some(1), &platform_app_domain_suffixes())
        .await
        .expect("ensure zones");
    repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision");
    // A user custom domain binding (VERIFIED domain + SERVE binding).
    let domain_row = sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status,created_by,updated_by,created_at,updated_at,version
         ) VALUES (90,'domain-90',7,9,
            (SELECT id FROM deploy_dns_zone WHERE tenant_id = 7 AND apex_hostname = 'app.sdkwork.com' LIMIT 1),
            'mysite.example.com','EXACT','VERIFIED','2026-07-22T00:00:00Z','ACTIVE',1,1,
            '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert custom domain");
    let domain_id: i64 = domain_row.try_get("id").expect("domain id");
    sqlx::query(
        "INSERT INTO deploy_app_binding (
            id,uuid,tenant_id,organization_id,app_id,binding_key,domain_id,hostname_ascii,
            environment,path_prefix,action_type,is_canonical,status,verified_at,activated_at,
            created_by,updated_by,created_at,updated_at,version
         ) VALUES (95,'binding-95',7,9,10,'custom-1',$1,'mysite.example.com','production',
            '/','SERVE',FALSE,'ACTIVE','2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1,1,
            '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .bind(domain_id)
    .execute(&pool)
    .await
    .expect("insert custom binding");
    let sha256 = seed_revision(&pool, 2, "production", &descriptor("site-10", "prod-1")).await;

    let resolved = repository
        .resolve_server_by_hostname("mysite.example.com", "production")
        .await
        .expect("resolve")
        .expect("custom domain must resolve");
    assert_eq!(resolved.hostname, "mysite.example.com");
    assert_eq!(resolved.action_type, "SERVE");
    assert_eq!(resolved.descriptor_sha256, sha256);

    // The same hostname in another environment must not resolve.
    let other_env = repository
        .resolve_server_by_hostname("mysite.example.com", "development")
        .await
        .expect("resolve");
    assert!(other_env.is_none());
}

/// Regression: `POST /backend/v3/api/nginx/configs/{configId}/deploy` failed
/// before doing anything because `load_nginx_publish_context` resolved the
/// publish domain with `SELECT d.hostname ... FROM deploy_domain d WHERE
/// d.app_id = s.id AND d.is_primary = 1` — none of which `deploy_domain` has.
/// The table carries `hostname_ascii` and reaches the app through
/// `deploy_app_binding`, so the projection has to come from the binding row.
///
/// The site file is redirected to a temp path so the test exercises the real
/// read + publish path without touching an nginx installation.
#[tokio::test]
async fn deploy_nginx_config_projects_the_primary_hostname_from_the_binding() {
    let (repository, pool) = test_repository().await;
    repository
        .ensure_platform_app_zones(7, 9, Some(1), &platform_app_domain_suffixes())
        .await
        .expect("ensure zones");
    repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect("provision default domains");

    // A hostname-scoped deployed config so `deploy_nginx_config` finds a row.
    let conf = "server {\n  listen 80;\n  server_name shop.app.sdkwork.com;\n  return 200;\n}\n";
    sqlx::query(
        "INSERT INTO deploy_nginx_config (
            id,uuid,tenant_id,app_id,environment,hostname_ascii,config_type,config_name,
            config_content,config_hash,is_active,version_no,status,metadata,created_at,
            updated_at,version
         ) VALUES (810,'nginx-810',7,10,'production','shop.app.sdkwork.com',1,
            'shop production',$1,$2,FALSE,1,0,'{}',NOW(),NOW(),1)",
    )
    .bind(conf)
    .bind(sdkwork_utils_rust::crypto::sha256_hash(conf.as_bytes()))
    .execute(&pool)
    .await
    .expect("insert nginx config to deploy");

    let site_dir = std::env::temp_dir().join(format!("sdkwork-nginx-{}", std::process::id()));
    std::fs::create_dir_all(&site_dir).expect("create temp site dir");
    let site_file = site_dir.join("shop.app.sdkwork.com.conf");
    // SAFETY: the test process is single-threaded for this binary
    // (`--test-threads=1` is the documented way to run this suite), and the
    // value is removed again below.
    unsafe {
        std::env::set_var("SDKWORK_DEPLOY_NGINX_SITE_FILE", &site_file);
        std::env::set_var("SDKWORK_DEPLOY_NGINX_RELOAD", "false");
        std::env::set_var("SDKWORK_DEPLOY_NGINX_ORCHESTRATION", "false");
    }

    let deployed = repository
        .deploy_nginx_config(Some(7), "nginx-810")
        .await
        .expect("deploying an nginx config must not fail on the primary-domain projection");
    assert_eq!(deployed.id, "nginx-810");
    assert!(deployed.is_active, "the config is activated");
    assert_eq!(
        std::fs::read_to_string(&site_file).expect("the published site file exists"),
        conf,
        "the published content is the stored config"
    );

    unsafe {
        std::env::remove_var("SDKWORK_DEPLOY_NGINX_SITE_FILE");
        std::env::remove_var("SDKWORK_DEPLOY_NGINX_RELOAD");
        std::env::remove_var("SDKWORK_DEPLOY_NGINX_ORCHESTRATION");
    }
    let _ = std::fs::remove_dir_all(&site_dir);
}

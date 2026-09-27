//! Source specs: the authoring surface, its idempotency and version
//! preconditions, the per-client-class default, and the projection that turns a
//! spec set into runtime variants, rules, resources and mounts.
//!
//! A spec is the *source* dimension of an app. A PC bundle and an H5 bundle are
//! two independently uploaded codebases, so they are two specs rather than two
//! variants of one source. Which client class reaches which spec — and in what
//! order a class falls back when its default spec has no upload yet — is stored
//! per class, because "the default spec for MOBILE" is a fact about the class
//! and not a flag on one spec.
//!
//! Nothing on the request path knows that. Inside the `apps.composition.update`
//! transaction every active spec is projected into a Variant plus one
//! `CLIENT_CLASS` rule per stored route, plus a Resource and a Mount, and the
//! compiler then validates the result exactly like a hand-authored composition.
//! These tests pin both halves — the authoring surface and the derived
//! descriptor — because a projection that quietly emits nothing is
//! indistinguishable from a spec that was never declared at all.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_content_provider_port::ValidatedContentProviderResource;
use sdkwork_deploy_contract::{
    AppBindingAction, AppBindingDefinition, AppClientArchitecture, AppClientClass,
    AppDeliveryPolicy, AppKind, AppMountHandler, AppObservabilityPolicy, AppPublishEnvironment,
    AppRuntimeLimits, AppSecurityPolicy, AppSourceSpecDefinition, AppSourceSpecResponse,
    AppSourceSpecRouteDefinition, ContentProviderResourceSource, CreateAppRequest,
    CreateAppSourceSpecRequest, DeployServiceError, DriveWebsiteContentMode,
    DriveWebsiteRootSelector, SdkworkRuntimeTarget, UpdateAppCompositionRequest,
    UpdateAppSourceSpecRequest,
};
use sdkwork_deploy_runtime_compiler::{
    RuntimeProviderType, RuntimeResourceCapabilities, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{
    AppCompositionRepositoryPort, AppSourceSpecRepositoryPort, BindAppSourceSpecSourceCommand,
    CreateAppSourceSpecCommand, DeclareAppSourceSpecsCommand, DeleteAppSourceSpecCommand,
    DeployRepositoryPort, ListAppSourceSpecsQuery, ReplaceAppCompositionCommand,
    UpdateAppSourceSpecCommand,
};
use sqlx::PgPool;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ACTOR_ID: i64 = 11;
const GENERATED_AT: &str = "2026-07-22T00:00:01Z";

/// A schema at the full forward migration chain, plus the pool it was built on.
///
/// The pool is what lets a test assert on the *descriptor*: the projected
/// variants, rules, resources and mounts are deliberately not mirrored into the
/// composition tables — the descriptor is their only representation, and reading
/// it is the only way to prove the projection ran.
async fn repository() -> (DeployRepository, PgPool) {
    common::migrated_repository_with_pool("sdkwork-deploy-source-spec-test").await
}

async fn create_app(repository: &DeployRepository, slug: &str) -> String {
    create_app_declaring(repository, slug, None)
        .await
        .expect("create app")
}

/// `apps.create`, with the spec set the caller declared.
///
/// Returns the error rather than panicking so the refusal cases can assert on it.
async fn create_app_declaring(
    repository: &DeployRepository,
    slug: &str,
    source_specs: Option<Vec<AppSourceSpecDefinition>>,
) -> Result<String, DeployServiceError> {
    repository
        .create_app(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(ACTOR_ID),
            None,
            &CreateAppRequest {
                name: format!("spec-app-{slug}"),
                slug: Some(slug.to_owned()),
                app_kind: AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                source_specs,
                owner_type: None,
                idempotency_key: None,
            },
        )
        .await
        .map(|app| app.id)
}

/// One client class this spec serves, at the given rank in that class's
/// preference order. `0` is the class default; higher values are fallbacks.
fn route(client_class: AppClientClass, preference: u16) -> AppSourceSpecRouteDefinition {
    AppSourceSpecRouteDefinition {
        client_class,
        preference,
    }
}

/// The PC specification: `runtimeTarget = browser`, architecture `react`.
///
/// Named so the two browser architectures are visibly *different* specs while
/// sharing a runtime target — the whole reason `client_architecture` exists
/// (`CONFIG_SPEC.md` section 2.1).
const PC_TARGET: SdkworkRuntimeTarget = SdkworkRuntimeTarget::Browser;
const PC_ARCHITECTURE: AppClientArchitecture = AppClientArchitecture::React;
/// The H5 specification: same runtime target, architecture `react-h5`.
const H5_ARCHITECTURE: AppClientArchitecture = AppClientArchitecture::ReactH5;

fn spec_request(
    spec_key: &str,
    label: &str,
    client_architecture: AppClientArchitecture,
    client_class_routes: Vec<AppSourceSpecRouteDefinition>,
    is_default: bool,
) -> CreateAppSourceSpecRequest {
    CreateAppSourceSpecRequest {
        environment: AppPublishEnvironment::Production,
        spec_key: spec_key.to_owned(),
        label: label.to_owned(),
        runtime_target: PC_TARGET,
        client_architecture,
        client_class_routes,
        path_prefix: "/".to_owned(),
        handler: AppMountHandler::Spa,
        index_files: vec!["index.html".to_owned()],
        spa_fallback: Some("/index.html".to_owned()),
        is_default,
        priority: 0,
    }
}

/// The same spec expressed for `apps.create`, which declares a whole set.
fn definition(
    spec_key: &str,
    label: &str,
    client_architecture: AppClientArchitecture,
    client_class_routes: Vec<AppSourceSpecRouteDefinition>,
    is_default: bool,
) -> AppSourceSpecDefinition {
    AppSourceSpecDefinition {
        spec_key: spec_key.to_owned(),
        label: label.to_owned(),
        runtime_target: PC_TARGET,
        client_architecture,
        client_class_routes,
        path_prefix: "/".to_owned(),
        handler: AppMountHandler::Spa,
        index_files: vec!["index.html".to_owned()],
        spa_fallback: Some("/index.html".to_owned()),
        is_default,
        priority: 0,
    }
}

fn create_command(
    app_uuid: &str,
    request: CreateAppSourceSpecRequest,
    idempotency_key: &str,
    request_sha256: &str,
) -> CreateAppSourceSpecCommand {
    CreateAppSourceSpecCommand {
        tenant_id: TENANT_ID,
        organization_id: ORGANIZATION_ID,
        actor_id: ACTOR_ID,
        app_uuid: app_uuid.to_owned(),
        idempotency_key: idempotency_key.to_owned(),
        request_sha256: request_sha256.to_owned(),
        generated_at: GENERATED_AT.to_owned(),
        request,
    }
}

async fn create_spec(
    repository: &DeployRepository,
    app_uuid: &str,
    request: CreateAppSourceSpecRequest,
    idempotency_key: &str,
) -> AppSourceSpecResponse {
    repository
        .create_app_source_spec(create_command(
            app_uuid,
            request,
            idempotency_key,
            &"9".repeat(64),
        ))
        .await
        .expect("create source spec")
}

/// The provider triple a binding writes, already resolved.
///
/// The service resolves a raw `source` through the content provider before it
/// reaches the repository, so a repository-level test supplies the resolved form
/// directly — the port's type is what guarantees the column can only hold a value
/// the delivery plane accepted.
fn validated_source(key: &str, provider_resource_uuid: &str) -> ValidatedContentProviderResource {
    ValidatedContentProviderResource {
        key: key.to_owned(),
        source: ContentProviderResourceSource::drive_directory(
            format!("space-{provider_resource_uuid}"),
            DriveWebsiteRootSelector::SpaceRoot,
            DriveWebsiteContentMode::LiveTree,
        ),
        provider_type: RuntimeProviderType::Drive,
        provider_resource_uuid: provider_resource_uuid.to_owned(),
        provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION.to_owned(),
        capabilities: RuntimeResourceCapabilities {
            static_content: true,
            wiki_routes: false,
            wiki_search: false,
            range_requests: true,
        },
    }
}

async fn bind(
    repository: &DeployRepository,
    app_uuid: &str,
    spec_uuid: &str,
    provider_resource_uuid: &str,
) -> AppSourceSpecResponse {
    repository
        .bind_app_source_spec_source(BindAppSourceSpecSourceCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_uuid.to_owned(),
            spec_uuid: spec_uuid.to_owned(),
            generated_at: GENERATED_AT.to_owned(),
            resource: validated_source(spec_uuid, provider_resource_uuid),
        })
        .await
        .expect("bind source")
}

/// The DNS zone, verified domain and web target a publish binds through.
///
/// `reconcile_bindings` resolves the binding's `domainId` to a **verified**
/// domain, and the publish resolves an active web target before it inspects the
/// variant set at all, so no composition can publish without both. They are
/// seeded by hand because no app-level endpoint creates them in this crate.
async fn seed_ingress_plane(pool: &PgPool) {
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
         ) VALUES (20,'domain-1',7,9,19,'specs.example.com','EXACT','VERIFIED',
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
    .expect("insert web target");
}

/// A publish that declares no composition of its own.
///
/// No hand-authored variant, resource or mount — only the domain the app is
/// served on. That is the shape the source-spec dimension exists for: every
/// servable piece of the descriptor then comes from the stored spec set.
fn spec_only_composition(
    app_uuid: &str,
    idempotency_key: &str,
    expected_app_version: i64,
) -> ReplaceAppCompositionCommand {
    ReplaceAppCompositionCommand {
        tenant_id: TENANT_ID,
        organization_id: ORGANIZATION_ID,
        actor_id: ACTOR_ID,
        app_uuid: app_uuid.to_owned(),
        // Read from the app row rather than assumed: `create_app` seeds the
        // column's own default, and the commands issued after it may have moved
        // the version on.
        expected_app_version,
        idempotency_key: idempotency_key.to_owned(),
        request_sha256: "c".repeat(64),
        generated_at: GENERATED_AT.to_owned(),
        resources: Vec::new(),
        request: UpdateAppCompositionRequest {
            environment: AppPublishEnvironment::Production,
            default_variant_key: String::new(),
            resources: Vec::new(),
            variants: Vec::new(),
            variant_rules: Vec::new(),
            mounts: Vec::new(),
            bindings: vec![AppBindingDefinition {
                key: "primary".to_owned(),
                domain_id: "domain-1".to_owned(),
                path_prefix: "/".to_owned(),
                action: AppBindingAction::Serve {
                    default_variant_key: None,
                    forced_variant_key: None,
                },
            }],
            source_specs: None,
            delivery_policy: AppDeliveryPolicy::default(),
            security_policy: AppSecurityPolicy::default(),
            limits: AppRuntimeLimits::default(),
            observability_policy: AppObservabilityPolicy::default(),
        },
    }
}

async fn latest_descriptor(pool: &PgPool, app_uuid: &str) -> serde_json::Value {
    let text: String = sqlx::query_scalar(
        "SELECT CAST(revision.descriptor_json AS TEXT)
         FROM deploy_app_revision revision
         JOIN deploy_app app ON app.id = revision.app_id
         WHERE app.uuid = $1
         ORDER BY revision.revision_no DESC
         LIMIT 1",
    )
    .bind(app_uuid)
    .fetch_one(pool)
    .await
    .expect("read the published descriptor");
    serde_json::from_str(&text).expect("the descriptor is JSON")
}

/// `(client class, priority)` for every projected client-class rule.
///
/// Read as a set of pairs rather than a list so an assertion does not depend on
/// the order the projection happened to emit them in.
async fn projected_class_rules(pool: &PgPool, app_uuid: &str) -> Vec<(String, u64)> {
    let descriptor = latest_descriptor(pool, app_uuid).await;
    let mut pairs = descriptor["variantRules"]
        .as_array()
        .expect("the descriptor carries variant rules")
        .iter()
        .map(|rule| {
            assert_eq!(
                rule["match"]["type"].as_str(),
                Some("CLIENT_CLASS"),
                "a source spec projects into client-class routing, not a path prefix"
            );
            (
                rule["match"]["clientClass"]
                    .as_str()
                    .expect("client class")
                    .to_owned(),
                rule["priority"].as_u64().expect("rule priority"),
            )
        })
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
}

/// The app's current optimistic-concurrency token.
///
/// A publish is a command like any other, so it carries the version the caller
/// last observed; hard-coding it would make the test depend on `create_app`'s
/// default rather than on the token it is actually testing.
async fn app_version(pool: &PgPool, app_uuid: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM deploy_app WHERE uuid = $1 AND deleted_at IS NULL")
        .bind(app_uuid)
        .fetch_one(pool)
        .await
        .expect("read the app version")
}

async fn spec_row_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM deploy_app_source_spec WHERE tenant_id = $1")
        .bind(TENANT_ID)
        .fetch_one(pool)
        .await
        .expect("count source specs")
}

async fn revision_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM deploy_app_revision WHERE tenant_id = $1")
        .bind(TENANT_ID)
        .fetch_one(pool)
        .await
        .expect("count revisions")
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_repeated_idempotency_key_replays_the_row_and_a_new_payload_conflicts() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-idem").await;
    let request = spec_request(
        "pc",
        "PC",
        PC_ARCHITECTURE,
        vec![route(AppClientClass::Desktop, 0)],
        true,
    );

    let first = repository
        .create_app_source_spec(create_command(
            &app_id,
            request.clone(),
            "spec-create-1",
            &"1".repeat(64),
        ))
        .await
        .expect("first create");
    // The same command again: `apps.sourceSpecs.create` is declared
    // `x-sdkwork-idempotent`, so a console retrying a create it already
    // performed must be handed the row it wrote rather than told the key exists.
    let replay = repository
        .create_app_source_spec(create_command(
            &app_id,
            request,
            "spec-create-1",
            &"1".repeat(64),
        ))
        .await
        .expect("replayed create");
    assert_eq!(
        first.id, replay.id,
        "a repeated key must replay the row the first attempt wrote"
    );
    assert_eq!(
        first.version, replay.version,
        "a replay must not bump the row's version"
    );
    assert_eq!(
        spec_row_count(&pool).await,
        1,
        "a replayed create must not insert a second row"
    );

    // Same key, different payload. Answering with the first row would silently
    // discard the caller's change; the only safe answer is a conflict.
    let error = repository
        .create_app_source_spec(create_command(
            &app_id,
            spec_request(
                "pc",
                "PC (renamed)",
                PC_ARCHITECTURE,
                vec![route(AppClientClass::Desktop, 0)],
                true,
            ),
            "spec-create-1",
            &"2".repeat(64),
        ))
        .await
        .expect_err("a reused key carrying another payload must be refused");
    assert!(
        matches!(error, DeployServiceError::Conflict(_)),
        "expected a conflict, got {error:?}"
    );
    assert_eq!(
        spec_row_count(&pool).await,
        1,
        "the refused create must not have written anything"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_update_precondition_is_the_specs_own_version_not_the_apps() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-crud").await;
    let created = create_spec(
        &repository,
        &app_id,
        spec_request(
            "pc",
            "PC",
            PC_ARCHITECTURE,
            vec![route(AppClientClass::Desktop, 0)],
            true,
        ),
        "spec-crud-1",
    )
    .await;
    assert_eq!(created.version, "1", "a new spec starts at version 1");
    assert_eq!(created.status, "ACTIVE");
    assert_eq!(created.source_status, "EMPTY");
    assert!(
        created.source.is_none(),
        "a spec with no upload reports no source object rather than one full of nulls"
    );
    // Both browser architectures share `runtimeTarget = browser`; the
    // architecture column is the only thing that tells them apart, so it must
    // survive the round trip.
    assert_eq!(created.runtime_target, SdkworkRuntimeTarget::Browser);
    assert_eq!(created.client_architecture, AppClientArchitecture::React);
    assert_eq!(
        created.client_class_routes,
        vec![route(AppClientClass::Desktop, 0)],
        "the stored route set is reported back with its preference rank"
    );

    // The console reads the token from this very field, so the version the
    // precondition carries is the spec's, never the app's.
    let updated = repository
        .update_app_source_spec(UpdateAppSourceSpecCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            spec_uuid: created.id.clone(),
            expected_spec_version: 1,
            generated_at: GENERATED_AT.to_owned(),
            request: UpdateAppSourceSpecRequest {
                label: Some("PC (relabelled)".to_owned()),
                // Replace the route set rather than add to it: a class left out
                // must lose its rule, which is how "stop serving desktops from
                // this spec" is expressed.
                client_class_routes: Some(vec![route(AppClientClass::Mobile, 0)]),
                priority: Some(20),
                ..UpdateAppSourceSpecRequest::default()
            },
        })
        .await
        .expect("update at the current version");
    assert_eq!(updated.version, "2");
    assert_eq!(updated.label, "PC (relabelled)");
    assert_eq!(updated.priority, 20);
    assert_eq!(
        updated.client_class_routes,
        vec![route(AppClientClass::Mobile, 0)],
        "the route set is replaced wholesale, not merged"
    );
    assert_eq!(
        updated.spec_key, "pc",
        "the projected variant key is immutable, so a rename cannot re-point its rules"
    );

    // A stale token must be refused rather than silently overwriting the newer
    // row the other writer produced.
    let stale = repository
        .update_app_source_spec(UpdateAppSourceSpecCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            spec_uuid: created.id.clone(),
            expected_spec_version: 1,
            generated_at: GENERATED_AT.to_owned(),
            request: UpdateAppSourceSpecRequest {
                label: Some("stale write".to_owned()),
                ..UpdateAppSourceSpecRequest::default()
            },
        })
        .await
        .expect_err("a stale version must be refused");
    assert!(
        matches!(stale, DeployServiceError::Conflict(_)),
        "expected a conflict, got {stale:?}"
    );

    let listed = repository
        .list_app_source_specs(ListAppSourceSpecsQuery {
            tenant_id: TENANT_ID,
            app_uuid: app_id.clone(),
            environment: None,
        })
        .await
        .expect("list specs");
    assert_eq!(listed.total, 1);
    assert_eq!(listed.items[0].id, created.id);
    assert_eq!(
        listed.items[0].client_class_routes, updated.client_class_routes,
        "the list surface must carry the same route set as the retrieve surface"
    );

    // Deleting is a soft delete: the row leaves every read surface, and the
    // projection stops seeing it, but its identity is not recycled — that is what
    // keeps a disabled-then-redeclared spec's projected ids stable.
    repository
        .delete_app_source_spec(DeleteAppSourceSpecCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            spec_uuid: created.id.clone(),
            generated_at: GENERATED_AT.to_owned(),
        })
        .await
        .expect("delete spec");

    let after_delete = repository
        .list_app_source_specs(ListAppSourceSpecsQuery {
            tenant_id: TENANT_ID,
            app_uuid: app_id.clone(),
            environment: None,
        })
        .await
        .expect("list specs after delete");
    assert_eq!(after_delete.total, 0, "a deleted spec must leave the list");

    let retrieved = repository
        .retrieve_app_source_spec(TENANT_ID, &app_id, &created.id)
        .await
        .expect_err("a deleted spec must not be retrievable");
    assert!(
        matches!(retrieved, DeployServiceError::NotFound(_)),
        "expected not-found, got {retrieved:?}"
    );

    let live_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_source_spec
         WHERE tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(TENANT_ID)
    .fetch_one(&pool)
    .await
    .expect("count live specs");
    assert_eq!(live_rows, 0);
    // The row itself survives, which is what makes the delete reversible and the
    // projected identity stable.
    assert_eq!(spec_row_count(&pool).await, 1);

    // A soft-deleted spec must not keep its class routes, or the projection
    // would emit a rule pointing at a variant that is no longer in the
    // descriptor.
    let route_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_source_spec_route WHERE tenant_id = $1",
    )
    .bind(TENANT_ID)
    .fetch_one(&pool)
    .await
    .expect("count spec routes");
    assert_eq!(route_rows, 0, "routes go with the spec they route to");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn bound_specs_project_into_variants_rules_resources_and_mounts() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-project").await;

    // Two independently uploaded codebases: a PC bundle serving desktop clients
    // and an H5 bundle serving mobile and tablet.
    let pc = create_spec(
        &repository,
        &app_id,
        spec_request(
            "pc",
            "PC",
            PC_ARCHITECTURE,
            vec![route(AppClientClass::Desktop, 0)],
            true,
        ),
        "spec-project-pc",
    )
    .await;
    let h5 = create_spec(
        &repository,
        &app_id,
        spec_request(
            "h5",
            "H5",
            H5_ARCHITECTURE,
            vec![
                route(AppClientClass::Mobile, 0),
                route(AppClientClass::Tablet, 0),
            ],
            false,
        ),
        "spec-project-h5",
    )
    .await;
    assert_eq!(h5.client_architecture, AppClientArchitecture::ReactH5);
    assert_eq!(
        h5.runtime_target, pc.runtime_target,
        "the PC and H5 builds share a runtime target, so only the architecture \
         distinguishes them"
    );

    let bound_pc = bind(&repository, &app_id, &pc.id, "pc-root-1").await;
    assert_eq!(bound_pc.source_status, "BOUND");
    let bound = bound_pc
        .source
        .as_ref()
        .expect("a bound spec reports its source");
    assert_eq!(bound.provider_type, "DRIVE");
    assert_eq!(bound.provider_resource_uuid, "pc-root-1");
    bind(&repository, &app_id, &h5.id, "h5-root-1").await;

    // One provider resource may back exactly one spec: sharing a Drive root would
    // make "which source is this" ambiguous again.
    let stolen = repository
        .bind_app_source_spec_source(BindAppSourceSpecSourceCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            spec_uuid: h5.id.clone(),
            generated_at: GENERATED_AT.to_owned(),
            resource: validated_source(&h5.id, "pc-root-1"),
        })
        .await
        .expect_err("a source already bound to another spec must be refused");
    assert!(
        matches!(stolen, DeployServiceError::Conflict(_)),
        "expected a conflict, got {stolen:?}"
    );

    seed_ingress_plane(&pool).await;
    let expected_app_version = app_version(&pool, &app_id).await;
    repository
        .replace_app_composition(spec_only_composition(
            &app_id,
            "spec-project-publish",
            expected_app_version,
        ))
        .await
        .expect("publish a composition declared entirely from source specs");

    let descriptor = latest_descriptor(&pool, &app_id).await;

    let variants = descriptor["variants"]
        .as_array()
        .expect("the descriptor carries variants");
    assert_eq!(variants.len(), 2, "one variant per bound spec");
    let mut labels = variants
        .iter()
        .map(|variant| variant["label"].as_str().expect("variant label").to_owned())
        .collect::<Vec<_>>();
    labels.sort();
    assert_eq!(labels, vec!["H5".to_owned(), "PC".to_owned()]);

    // One `CLIENT_CLASS` rule per stored route: 1 for the PC spec plus 2 for the
    // H5 spec. If the projection only ever emitted the default spec, this would
    // read 1 and the H5 bundle would be unreachable. The priority is the route's
    // preference, which is what carries the fallback order to the edge.
    let rules = projected_class_rules(&pool, &app_id).await;
    assert_eq!(
        rules,
        vec![
            ("DESKTOP".to_owned(), 0),
            ("MOBILE".to_owned(), 0),
            ("TABLET".to_owned(), 0),
        ],
        "one routing rule per stored route, ranked by its preference"
    );

    let resources = descriptor["resources"]
        .as_array()
        .expect("the descriptor carries resources");
    assert_eq!(resources.len(), 2, "one resource per bound spec");
    let provider_types = resources
        .iter()
        .map(|resource| {
            resource["provider"]["providerType"]
                .as_str()
                .expect("provider type")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        provider_types.iter().all(|value| value == "DRIVE"),
        "the provider triple is written from the validated resource, got {provider_types:?}"
    );

    let mounts = descriptor["mounts"]
        .as_array()
        .expect("the descriptor carries mounts");
    assert_eq!(mounts.len(), 2, "one mount per bound spec");
    assert!(
        mounts
            .iter()
            .all(|mount| mount["pathPrefix"].as_str() == Some("/")),
        "each spec mounts the root path it declared"
    );

    // A composition declared from source specs has no hand-authored variant, so
    // the app-level default has to come from the spec marked `isDefault`.
    let default_uuid = descriptor["appDefaultVariantUuid"]
        .as_str()
        .expect("appDefaultVariantUuid");
    let default_label = variants
        .iter()
        .find(|variant| variant["variantUuid"].as_str() == Some(default_uuid))
        .map(|variant| variant["label"].as_str().expect("variant label").to_owned())
        .expect("the app default must reference a projected variant");
    assert_eq!(
        default_label, "PC",
        "the spec marked isDefault supplies the app-level default"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_client_class_falls_through_to_its_next_rank_when_the_default_is_unuploaded() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-fallback").await;

    // The normative Adaptive Web preference table (`SDKWORK_DEPLOY_SPEC.md`
    // section 8): mobile prefers H5 and falls back to PC, desktop prefers PC,
    // and a tablet prefers PC while still being able to reach H5. Expressed
    // entirely as data — each class names its default at rank 0 and its fallback
    // at rank 1.
    let pc = create_spec(
        &repository,
        &app_id,
        spec_request(
            "pc",
            "PC",
            PC_ARCHITECTURE,
            vec![
                route(AppClientClass::Desktop, 0),
                // The fallback that makes a phone work before the H5 bundle is
                // uploaded.
                route(AppClientClass::Mobile, 1),
                route(AppClientClass::Tablet, 0),
            ],
            true,
        ),
        "spec-fallback-pc",
    )
    .await;
    let h5 = create_spec(
        &repository,
        &app_id,
        spec_request(
            "h5",
            "H5",
            H5_ARCHITECTURE,
            vec![
                route(AppClientClass::Mobile, 0),
                route(AppClientClass::Tablet, 1),
            ],
            false,
        ),
        "spec-fallback-h5",
    )
    .await;

    seed_ingress_plane(&pool).await;

    // Only the PC bundle exists. MOBILE has no rank-0 rule because the spec that
    // owns rank 0 has no source, so its rank-1 rule is what a phone reaches —
    // the fallback chain, with no request-time logic anywhere.
    bind(&repository, &app_id, &pc.id, "pc-root-1").await;
    let expected_app_version = app_version(&pool, &app_id).await;
    repository
        .replace_app_composition(spec_only_composition(
            &app_id,
            "spec-fallback-publish-pc-only",
            expected_app_version,
        ))
        .await
        .expect("publish with only the PC bundle uploaded");
    assert_eq!(
        projected_class_rules(&pool, &app_id).await,
        vec![
            ("DESKTOP".to_owned(), 0),
            // Rank 1, not 0: the H5 default is declared but unservable, so this
            // is the rule that actually matches a phone.
            ("MOBILE".to_owned(), 1),
            ("TABLET".to_owned(), 0),
        ],
        "an unuploaded default must contribute no rule, leaving its fallback to serve the class"
    );

    // Now the H5 bundle arrives. MOBILE gains a rank-0 rule that outranks the PC
    // fallback, and TABLET gains a rank-1 rule behind PC's rank 0 — so a tablet
    // still gets the PC surface, exactly as the spec's table requires.
    bind(&repository, &app_id, &h5.id, "h5-root-1").await;
    let expected_app_version = app_version(&pool, &app_id).await;
    repository
        .replace_app_composition(spec_only_composition(
            &app_id,
            "spec-fallback-publish-both",
            expected_app_version,
        ))
        .await
        .expect("publish with both bundles uploaded");
    assert_eq!(
        projected_class_rules(&pool, &app_id).await,
        vec![
            ("DESKTOP".to_owned(), 0),
            ("MOBILE".to_owned(), 0),
            ("MOBILE".to_owned(), 1),
            ("TABLET".to_owned(), 0),
            ("TABLET".to_owned(), 1),
        ],
        "each class carries its full preference chain once both sources exist"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn one_client_class_has_one_default_and_the_database_is_what_says_so() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "spec-one-default").await;

    create_spec(
        &repository,
        &app_id,
        spec_request(
            "pc",
            "PC",
            PC_ARCHITECTURE,
            vec![route(AppClientClass::Desktop, 0)],
            true,
        ),
        "spec-one-default-pc",
    )
    .await;

    // A second spec claiming the same class at the same rank. The unique index
    // `uk_deploy_app_source_spec_route_rank` is what refuses it — not a service
    // check — so "one default per client class" holds even under a race.
    let duplicate = repository
        .create_app_source_spec(create_command(
            &app_id,
            spec_request(
                "h5",
                "H5",
                H5_ARCHITECTURE,
                vec![route(AppClientClass::Desktop, 0)],
                false,
            ),
            "spec-one-default-h5",
            &"5".repeat(64),
        ))
        .await
        .expect_err("two specs cannot both be the default for one client class");
    assert!(
        matches!(duplicate, DeployServiceError::Conflict(_)),
        "the database, not the service, owns this rule; expected a conflict, got {duplicate:?}"
    );

    // A *different* rank for the same class is the fallback chain, and a rank
    // above zero with no rank-zero spec yet is legitimate: a per-spec write sees
    // only part of the environment, so requiring completeness here would make the
    // natural authoring order illegal — declaring PC as mobile's fallback before
    // the H5 spec that owns mobile's default exists. The class is served by its
    // lowest rank either way, and the index just shown keeps that rank unique.
    let fallback = repository
        .create_app_source_spec(create_command(
            &app_id,
            spec_request(
                "h5",
                "H5",
                H5_ARCHITECTURE,
                vec![
                    route(AppClientClass::Mobile, 1),
                    // A second class at rank 0 on the same spec: one spec may be
                    // the default for one class and a fallback for another.
                    route(AppClientClass::Tablet, 0),
                ],
                false,
            ),
            "spec-one-default-sparse",
            &"6".repeat(64),
        ))
        .await
        .expect("a fallback rank must be accepted before its default exists");
    assert_eq!(
        fallback.client_class_routes,
        vec![
            route(AppClientClass::Mobile, 1),
            route(AppClientClass::Tablet, 0),
        ],
        "a spec may hold different ranks for different classes"
    );

    // The same spec cannot hold two ranks for one class: a spec is either the
    // default for a class or one of its fallbacks, never both.
    let twice = repository
        .update_app_source_spec(UpdateAppSourceSpecCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            spec_uuid: fallback.id.clone(),
            expected_spec_version: 1,
            generated_at: GENERATED_AT.to_owned(),
            request: UpdateAppSourceSpecRequest {
                client_class_routes: Some(vec![
                    route(AppClientClass::Mobile, 0),
                    route(AppClientClass::Mobile, 1),
                ]),
                ..UpdateAppSourceSpecRequest::default()
            },
        })
        .await
        .expect_err("one spec cannot rank the same class twice");
    assert!(
        matches!(twice, DeployServiceError::Validation(_)),
        "expected a validation failure naming the repeat, got {twice:?}"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn publishing_with_nothing_bound_names_the_specs_awaiting_an_upload() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-unbound").await;

    create_spec(
        &repository,
        &app_id,
        spec_request(
            "pc",
            "PC",
            PC_ARCHITECTURE,
            vec![route(AppClientClass::Desktop, 0)],
            true,
        ),
        "spec-unbound-pc",
    )
    .await;
    create_spec(
        &repository,
        &app_id,
        spec_request(
            "h5",
            "H5",
            H5_ARCHITECTURE,
            vec![route(AppClientClass::Mobile, 0)],
            false,
        ),
        "spec-unbound-h5",
    )
    .await;

    // The web target is resolved before the variant set is inspected, so the
    // publish has to get past that to reach the message under test.
    seed_ingress_plane(&pool).await;
    let expected_app_version = app_version(&pool, &app_id).await;

    // Both specs are declared but neither has an upload. Dropping them silently
    // would compile an empty composition and report an anonymous "no variants",
    // sending the operator to the wrong place; the names are the actionable part.
    let error = repository
        .replace_app_composition(spec_only_composition(
            &app_id,
            "spec-unbound-publish",
            expected_app_version,
        ))
        .await
        .expect_err("publishing with nothing bound must fail");
    let message = match &error {
        DeployServiceError::Validation(message) => message.clone(),
        other => panic!("expected a validation failure, got {other:?}"),
    };
    assert!(
        message.contains("pc") && message.contains("h5"),
        "the failure must name every spec still awaiting an upload, got: {message}"
    );

    assert_eq!(
        revision_count(&pool).await,
        0,
        "the refused publish must not have committed a revision"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn declaring_a_spec_set_materializes_it_and_retires_what_it_drops() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "spec-inline").await;

    // `apps.create` reaches this through the service layer, which is where
    // declaring a whole set belongs; the per-spec endpoints deliberately cannot
    // declare one. Exercising the port the service calls is what makes the
    // behaviour testable without standing up the whole API stack.
    let declared = repository
        .declare_app_source_specs(DeclareAppSourceSpecsCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            environment: "production".to_owned(),
            generated_at: GENERATED_AT.to_owned(),
            definitions: vec![
                definition(
                    "pc",
                    "PC",
                    PC_ARCHITECTURE,
                    vec![route(AppClientClass::Desktop, 0)],
                    true,
                ),
                definition(
                    "h5",
                    "H5",
                    H5_ARCHITECTURE,
                    vec![route(AppClientClass::Mobile, 0)],
                    false,
                ),
            ],
        })
        .await
        .expect("declare the spec set");
    assert_eq!(declared.total, 2, "the whole declared set is materialized");
    assert_eq!(
        declared.items.iter().filter(|spec| spec.is_default).count(),
        1,
        "exactly one declared spec is the app-level default"
    );
    let pc_id = declared
        .items
        .iter()
        .find(|spec| spec.spec_key == "pc")
        .expect("the pc spec")
        .id
        .clone();
    let mut declared_architectures = declared
        .items
        .iter()
        .map(|spec| spec.client_architecture)
        .collect::<Vec<_>>();
    declared_architectures.sort_by_key(|architecture| architecture.as_str());
    assert_eq!(
        declared_architectures,
        vec![AppClientArchitecture::React, AppClientArchitecture::ReactH5],
        "the declared set keeps one PC and one H5 architecture under a shared runtime target"
    );

    // Declaring specs is not publishing: they route nowhere until each has a
    // source, so nothing is compiled.
    assert_eq!(
        revision_count(&pool).await,
        0,
        "declaring specs must not publish anything"
    );

    // Two app-level defaults would leave "which source serves an unclassified
    // client" undefined, so the set is refused as a whole rather than one being
    // picked.
    let refused = repository
        .declare_app_source_specs(DeclareAppSourceSpecsCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            environment: "production".to_owned(),
            generated_at: GENERATED_AT.to_owned(),
            definitions: vec![
                definition(
                    "pc",
                    "PC",
                    PC_ARCHITECTURE,
                    vec![route(AppClientClass::Desktop, 0)],
                    true,
                ),
                definition(
                    "h5",
                    "H5",
                    H5_ARCHITECTURE,
                    vec![route(AppClientClass::Mobile, 0)],
                    true,
                ),
            ],
        })
        .await
        .expect_err("two default specs must be refused");
    assert!(
        matches!(refused, DeployServiceError::Validation(_)),
        "expected a validation failure, got {refused:?}"
    );

    // A later declaration replaces the set rather than adding to it: the spec it
    // drops leaves the read surface, and the spec it keeps keeps both its
    // identity and the source already uploaded behind it.
    bind(&repository, &app_id, &pc_id, "pc-root-1").await;
    let replaced = repository
        .declare_app_source_specs(DeclareAppSourceSpecsCommand {
            tenant_id: TENANT_ID,
            organization_id: ORGANIZATION_ID,
            actor_id: ACTOR_ID,
            app_uuid: app_id.clone(),
            environment: "production".to_owned(),
            generated_at: GENERATED_AT.to_owned(),
            definitions: vec![definition(
                "pc",
                "PC",
                PC_ARCHITECTURE,
                vec![route(AppClientClass::Desktop, 0)],
                true,
            )],
        })
        .await
        .expect("redeclare a smaller set");
    assert_eq!(replaced.total, 1, "the dropped spec must leave the set");
    assert_eq!(replaced.items[0].spec_key, "pc");
    assert_eq!(
        replaced.items[0].id, pc_id,
        "a re-declared spec keeps its identity, so its projected routing ids stay stable"
    );
    assert_eq!(
        replaced.items[0].source_status, "BOUND",
        "declaring a set must not discard a source that is already uploaded"
    );
    assert_eq!(
        spec_row_count(&pool).await,
        2,
        "the dropped spec is soft-deleted rather than erased"
    );
    // The dropped spec's routes go with it: a route left pointing at a retired
    // spec would project a rule to a variant that is no longer emitted.
    let route_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_source_spec_route WHERE tenant_id = $1",
    )
    .bind(TENANT_ID)
    .fetch_one(&pool)
    .await
    .expect("count spec routes");
    assert_eq!(route_rows, 1, "only the surviving spec keeps a route");
}

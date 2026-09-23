mod common;

use std::sync::Arc;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    CreateDomainHostnameRequest, CreateDomainZoneRequest, DeployAppApi, DeployAppRequestContext,
    ListDomainZonesQuery, UpdateDomainHostnameRequest, UpdateDomainZoneRequest, ZoneScope,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{DeployRepositoryPort, DeployService};

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_domain_zone_lifecycle_enforces_resource_boundaries() {
    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let apex = format!(
        "zone{}.dev",
        sdkwork_database_id::uuid_v4().replace('-', "")
    );

    let zone = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(11),
            &CreateDomainZoneRequest {
                apex_hostname: apex.clone(),
                display_name: Some("Production zone".to_owned()),
                dns_provider: Some("manual".to_owned()),
                provider_zone_ref: None,
                // Left unset: `dns_provider = manual` means an operator publishes the
                // record by hand, so there is no account for the zone to name.
                provider_account_id: None,
            },
        )
        .await
        .expect("create root domain zone");
    assert_eq!(zone.apex_hostname, apex);
    assert_eq!(zone.hostname_count, 1);

    let listed = repository
        .list_domain_zones(
            7,
            Some(11),
            &ListDomainZonesQuery {
                page: 1,
                page_size: 20,
                status: Some("ACTIVE".to_owned()),
                keyword: Some("Production".to_owned()),
                scope: None,
            },
        )
        .await
        .expect("list root domain zones");
    assert!(listed.items.iter().any(|item| item.id == zone.id));

    let hostname = repository
        .create_domain_hostname(
            7,
            Some(11),
            &zone.id,
            &CreateDomainHostnameRequest {
                relative_name: "docs".to_owned(),
            },
        )
        .await
        .expect("create child hostname");
    assert_eq!(hostname.hostname, format!("docs.{apex}"));

    let first_challenge = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("create verification challenge");
    assert!(first_challenge.token.is_some());
    let repeated_challenge = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("reload verification challenge");
    // The published value is `base64url(sha256(attempt id))`, so reloading the
    // attempt must hand back the *same* value: an operator who publishes the
    // record and then asks again (or reloads the page) still has to be told what
    // to put in it. Returning `None` on reload left every re-ask with a record
    // name and no value, which is a domain that can never leave `PENDING` and a
    // certificate that can never be ordered.
    assert_eq!(
        repeated_challenge.token, first_challenge.token,
        "reloading a challenge must repeat the value the operator publishes"
    );
    assert!(repeated_challenge.token.is_some());
    assert_eq!(
        repeated_challenge.verification_id,
        first_challenge.verification_id
    );

    // Renaming keeps the hostname in the zone but resets ownership
    // verification and expires the previous challenge (the TXT record
    // changes with the DNS name).
    let renamed = repository
        .update_domain_hostname(
            7,
            Some(11),
            &zone.id,
            &hostname.id,
            &UpdateDomainHostnameRequest {
                relative_name: "docs2".to_owned(),
            },
        )
        .await
        .expect("rename child hostname");
    assert_eq!(renamed.hostname, format!("docs2.{apex}"));
    assert_eq!(renamed.hostname_type, "EXACT");
    assert_eq!(renamed.verification_status, "PENDING");
    assert!(renamed.verified_at.is_none());
    let renamed_challenge = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("challenge after rename");
    assert_ne!(
        renamed_challenge.verification_id, first_challenge.verification_id,
        "rename must expire the old verification challenge"
    );
    assert!(
        renamed_challenge
            .record_name
            .as_deref()
            .is_some_and(|record| record.contains("docs2")),
        "challenge record must target the renamed hostname"
    );

    // The apex hostname is owned by the zone and cannot be renamed, and a
    // hostname cannot be renamed back onto the apex.
    let apex_hostname = repository
        .list_domain_hostnames(7, Some(11), &zone.id, 1, 20)
        .await
        .expect("list apex hostname")
        .items
        .into_iter()
        .find(|item| item.relative_name == "@")
        .expect("apex hostname");
    assert!(
        repository
            .update_domain_hostname(
                7,
                Some(11),
                &zone.id,
                &apex_hostname.id,
                &UpdateDomainHostnameRequest {
                    relative_name: "renamed-apex".to_owned(),
                },
            )
            .await
            .is_err(),
        "apex hostname must not be renamed"
    );
    assert!(
        repository
            .update_domain_hostname(
                7,
                Some(11),
                &zone.id,
                &hostname.id,
                &UpdateDomainHostnameRequest {
                    relative_name: "@".to_owned(),
                },
            )
            .await
            .is_err(),
        "hostname must not be renamed onto the zone apex"
    );

    assert!(repository
        .delete_domain_zone(7, Some(11), &zone.id)
        .await
        .is_err());
    repository
        .delete_domain_hostname(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("delete unbound child hostname");
    let apex_hostname = repository
        .list_domain_hostnames(7, Some(11), &zone.id, 1, 20)
        .await
        .expect("list apex hostname")
        .items
        .into_iter()
        .find(|item| item.relative_name == "@")
        .expect("apex hostname");
    assert!(
        repository
            .delete_domain_hostname(7, Some(11), &zone.id, &apex_hostname.id)
            .await
            .is_err(),
        "the apex hostname belongs to the zone and cannot be deleted independently"
    );
    repository
        .delete_domain_zone(7, Some(11), &zone.id)
        .await
        .expect("delete root domain zone with its apex hostname");

    let recreated = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(11),
            &CreateDomainZoneRequest {
                apex_hostname: apex,
                display_name: None,
                dns_provider: None,
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("reuse soft-deleted apex");
    let recreated_apex = repository
        .list_domain_hostnames(7, Some(11), &recreated.id, 1, 20)
        .await
        .expect("list recreated apex")
        .items
        .into_iter()
        .next()
        .expect("recreated apex hostname");
    assert!(
        repository
            .delete_domain_hostname(7, Some(11), &recreated.id, &recreated_apex.id)
            .await
            .is_err(),
        "the recreated apex hostname belongs to the zone and cannot be deleted independently"
    );
    repository
        .delete_domain_zone(7, Some(11), &recreated.id)
        .await
        .expect("delete recreated zone");
}

/// The console's domain inventory is user-private.
///
/// A zone belongs to the user who created it, so a second user in the same
/// tenant reaches neither the listing nor the by-id read, rename, pause or
/// delete paths — that is the whole point of the page being "my domains". A
/// tenant-level zone (no owner; the platform provisions `app.<suffix>` that way
/// for every member) stays visible to all of them, and a caller with no user
/// subject reaches exactly those.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn postgres_domain_inventory_is_private_to_its_owner() {
    fn ids(page: sdkwork_deploy_contract::DomainZonePage) -> Vec<String> {
        page.items.into_iter().map(|item| item.id).collect()
    }

    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let suffix = sdkwork_database_id::uuid_v4().replace('-', "");
    let request = |apex: &str| CreateDomainZoneRequest {
        apex_hostname: apex.to_owned(),
        display_name: None,
        dns_provider: Some("manual".to_owned()),
        provider_zone_ref: None,
        provider_account_id: None,
    };
    let listing = || ListDomainZonesQuery {
        page: 1,
        page_size: 50,
        status: None,
        keyword: None,
        scope: None,
    };

    // User 11 owns one zone, user 12 owns another in the same tenant, and the
    // tenant-level zone is created without a user subject at all.
    let owner_zone = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(11),
            &request(&format!("owned{suffix}.dev")),
        )
        .await
        .expect("create the owner's zone");
    let neighbour_zone = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(12),
            &request(&format!("neighbour{suffix}.dev")),
        )
        .await
        .expect("create the neighbour's zone");
    let shared_zone = repository
        .create_domain_zone(7, Some(9), None, &request(&format!("app{suffix}.dev")))
        .await
        .expect("create a tenant-level zone");

    let owner_ids = ids(repository
        .list_domain_zones(7, Some(11), &listing())
        .await
        .expect("list as the owner"));
    assert!(
        owner_ids.contains(&owner_zone.id),
        "the owner must see the zone they created"
    );
    assert!(
        !owner_ids.contains(&neighbour_zone.id),
        "another user's zone must not appear in this user's inventory"
    );
    assert!(
        owner_ids.contains(&shared_zone.id),
        "a tenant-level zone is visible to every member"
    );

    let neighbour_ids = ids(repository
        .list_domain_zones(7, Some(12), &listing())
        .await
        .expect("list as the neighbour"));
    assert!(
        !neighbour_ids.contains(&owner_zone.id),
        "the neighbour must not see the owner's zone"
    );
    assert!(neighbour_ids.contains(&neighbour_zone.id));

    let unowned_ids = ids(repository
        .list_domain_zones(7, None, &listing())
        .await
        .expect("list without a user subject"));
    assert_eq!(
        unowned_ids,
        vec![shared_zone.id.clone()],
        "a caller with no user subject reaches the tenant-level zone and nothing else"
    );

    // Every by-id path is refused for a zone the caller does not own. Filtering
    // only the listing would leave a deep link into the hostname page open.
    assert!(
        repository
            .retrieve_domain_zone(7, Some(12), &owner_zone.id)
            .await
            .is_err(),
        "reading another user's zone must be refused"
    );
    assert!(
        repository
            .list_domain_hostnames(7, Some(12), &owner_zone.id, 1, 20)
            .await
            .is_err(),
        "listing another user's hostnames must be refused"
    );
    assert!(
        repository
            .delete_domain_zone(7, Some(12), &owner_zone.id)
            .await
            .is_err(),
        "deleting another user's zone must be refused"
    );
    assert!(
        repository
            .update_domain_zone(
                7,
                Some(12),
                &owner_zone.id,
                &UpdateDomainZoneRequest {
                    display_name: None,
                    dns_provider: None,
                    provider_zone_ref: None,
                    provider_account_id: None,
                    status: Some("PAUSED".to_owned()),
                },
            )
            .await
            .is_err(),
        "pausing another user's root domain must be refused"
    );
    assert!(
        repository
            .create_domain_hostname(
                7,
                Some(12),
                &owner_zone.id,
                &CreateDomainHostnameRequest {
                    relative_name: "docs".to_owned(),
                },
            )
            .await
            .is_err(),
        "adding a hostname under another user's zone must be refused"
    );
    assert!(
        repository
            .ensure_domain_hostname(7, Some(12), &owner_zone.id, "docs")
            .await
            .is_err(),
        "claiming a hostname under another user's zone must be refused"
    );
    assert_eq!(
        repository
            .retrieve_domain_zone(7, Some(11), &owner_zone.id)
            .await
            .expect("the owner still reaches it")
            .status,
        "ACTIVE",
        "a refused write must leave the zone untouched"
    );

    // The owner's hostname and proof paths keep working, and stay closed to the
    // neighbour at the hostname level too.
    let hostname = repository
        .create_domain_hostname(
            7,
            Some(11),
            &owner_zone.id,
            &CreateDomainHostnameRequest {
                relative_name: "docs".to_owned(),
            },
        )
        .await
        .expect("create the owner's hostname");
    assert!(
        repository
            .domain_hostname_verification_challenge(7, Some(12), &owner_zone.id, &hostname.id)
            .await
            .is_err(),
        "starting another user's ownership proof must be refused"
    );
    assert!(
        repository
            .retrieve_domain_hostname(7, Some(12), &owner_zone.id, &hostname.id)
            .await
            .is_err(),
        "reading another user's hostname must be refused"
    );
    assert!(
        repository
            .delete_domain_hostname(7, Some(12), &owner_zone.id, &hostname.id)
            .await
            .is_err(),
        "deleting another user's hostname must be refused"
    );
    assert!(
        repository
            .domain_hostname_verification_challenge(7, Some(11), &owner_zone.id, &hostname.id)
            .await
            .is_ok(),
        "the owner's own proof path stays open"
    );

    // The tenant-level zone is reachable from both members and from a caller
    // with no user subject, which is what keeps the platform inventory usable.
    assert!(repository
        .retrieve_domain_zone(7, Some(12), &shared_zone.id)
        .await
        .is_ok());
    assert!(repository
        .retrieve_domain_zone(7, None, &shared_zone.id)
        .await
        .is_ok());
}

/// The console reaches the owner gate through the service, not through the
/// repository, so the gate only protects anyone if the service actually hands
/// the caller's subject down. Asserting on `DeployRepository` alone proves the
/// gate exists but not that production goes through it: the service could pass
/// `None` for every caller and each user would once again see the whole tenant.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn service_domain_inventory_is_scoped_to_the_calling_subject() {
    let pool = common::postgres_pool().await;
    let repository = Arc::new(DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    ));
    let service = DeployService::new(repository, Arc::new(MemoryDeployDrivePort));

    let caller = |actor_id: Option<i64>| DeployAppRequestContext {
        tenant_id: 7,
        actor_id,
        organization_id: Some(9),
        ..DeployAppRequestContext::default()
    };
    // A fresh apex per zone: active apexes are globally exclusive, so a literal
    // name would collide with the other tests sharing this database.
    let request = |label: &str| CreateDomainZoneRequest {
        apex_hostname: format!(
            "{label}{}.dev",
            sdkwork_database_id::uuid_v4().replace('-', "")
        ),
        display_name: Some(format!("{label} zone")),
        dns_provider: Some("manual".to_owned()),
        provider_zone_ref: None,
        provider_account_id: None,
    };
    let query = ListDomainZonesQuery {
        page: 1,
        page_size: 50,
        status: None,
        keyword: None,
        scope: None,
    };

    let alice = service
        .create_domain_zone(&caller(Some(11)), &request("alice"))
        .await
        .expect("alice claims her own root domain");
    let bob = service
        .create_domain_zone(&caller(Some(12)), &request("bob"))
        .await
        .expect("bob claims his own root domain");
    // No user subject: this is the platform-owned tenant-level zone the
    // deployment provisions for the whole tenant (`app.<suffix>`).
    let platform = service
        .create_domain_zone(&caller(None), &request("platform"))
        .await
        .expect("the deployment provisions a tenant-level zone");

    let ids = |page: &sdkwork_deploy_contract::DomainZonePage| {
        page.items
            .iter()
            .map(|zone| zone.id.clone())
            .collect::<Vec<_>>()
    };

    let alice_ids = ids(&service
        .list_domain_zones(&caller(Some(11)), &query)
        .await
        .expect("alice lists the domains she maintains"));
    assert!(
        alice_ids.contains(&alice.id),
        "alice keeps the zone she claimed"
    );
    assert!(
        alice_ids.contains(&platform.id),
        "the tenant-level zone stays visible to every member"
    );
    assert!(
        !alice_ids.contains(&bob.id),
        "alice must not see the zone bob maintains"
    );

    let bob_ids = ids(&service
        .list_domain_zones(&caller(Some(12)), &query)
        .await
        .expect("bob lists the domains he maintains"));
    assert!(bob_ids.contains(&bob.id), "bob keeps the zone he claimed");
    assert!(
        !bob_ids.contains(&alice.id),
        "bob must not see the zone alice maintains"
    );

    // A caller with no user subject gets the platform inventory and nothing
    // else — never the whole tenant by accident.
    let anonymous_ids = ids(&service
        .list_domain_zones(&caller(None), &query)
        .await
        .expect("a subjectless caller reads the tenant-level inventory"));
    assert_eq!(
        anonymous_ids,
        vec![platform.id.clone()],
        "the subjectless inventory is exactly the tenant-level zone"
    );

    // Every console action on a zone runs through the service too, so one
    // foreign zone has to stay closed for reads and for writes alike.
    assert!(
        service
            .retrieve_domain_zone(&caller(Some(12)), &alice.id)
            .await
            .is_err(),
        "reading another user's zone must be refused"
    );
    assert!(
        service
            .list_domain_hostnames(&caller(Some(12)), &alice.id, 1, 20)
            .await
            .is_err(),
        "listing another user's hostnames must be refused"
    );
    assert!(
        service
            .delete_domain_zone(&caller(Some(12)), &alice.id)
            .await
            .is_err(),
        "deleting another user's zone must be refused"
    );
    // The refusals above must be refusals, not silent successes: alice's zone is
    // still there and still hers.
    let alice_still_there = service
        .retrieve_domain_zone(&caller(Some(11)), &alice.id)
        .await
        .expect("the owner's own zone is untouched");
    assert_eq!(alice_still_there.id, alice.id);
    assert_eq!(alice_still_there.status, "ACTIVE");
}

/// The console's root-domain list must show the operator's own zones *first*,
/// and label which zones are the operator's and which the deployment
/// provisioned.
///
/// Two separate failures are covered here, because the page looked equally
/// wrong for both reasons:
///
/// 1. `scope` did not exist, so the console could not tell an operator's
///    `example.com` from the platform's provisioned `app.example.com` and
///    presented them as if both were the operator's.
/// 2. the listing ordered by `updated_at DESC` alone. The platform provisions
///    its entire `app.<suffix>` catalog in one transaction, so every such zone
///    shares an `updated_at` *newer* than any operator zone typed by hand —
///    which pushed the operator's own root domains off the first page.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn domain_zone_listing_puts_operator_zones_first_and_labels_their_scope() {
    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(6).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let suffix = sdkwork_database_id::uuid_v4().replace('-', "");
    let request = |apex: &str| CreateDomainZoneRequest {
        apex_hostname: apex.to_owned(),
        display_name: None,
        dns_provider: Some("manual".to_owned()),
        provider_zone_ref: None,
        provider_account_id: None,
    };
    let listing = || ListDomainZonesQuery {
        page: 1,
        page_size: 50,
        status: None,
        keyword: None,
        scope: None,
    };

    // The operator's own root domain, created first.
    let operator_apex = format!("operator{suffix}.dev");
    repository
        .create_domain_zone(7, Some(9), Some(11), &request(&operator_apex))
        .await
        .expect("create the operator's root domain");

    // The platform's provisioned zone, created second. Written directly so the
    // row carries the same shape the provisioner writes (`user_id` NULL,
    // `dns_provider` platform), and a *newer* `updated_at` than the operator's.
    let platform_apex = format!("app.{operator_apex}");
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id, uuid, tenant_id, organization_id, apex_hostname, display_name,
            dns_provider, provider_zone_ref, status, user_id, created_by, updated_by
         ) VALUES (
            90001, 'zone-90001', 7, 9, $1, 'Platform app domain zone', 'platform', $2,
            'ACTIVE', NULL, 1, 1
         )",
    )
    .bind(&platform_apex)
    .bind(format!("app.*.{suffix}.dev"))
    .execute(repository.pool())
    .await
    .expect("seed the platform zone");

    let page = repository
        .list_domain_zones(7, Some(11), &listing())
        .await
        .expect("list the operator's zones");

    let seen: Vec<(String, String)> = page
        .items
        .iter()
        .map(|zone| (zone.scope.as_str().to_owned(), zone.apex_hostname.clone()))
        .collect();

    let operator_index = seen
        .iter()
        .position(|(_, apex)| *apex == operator_apex)
        .unwrap_or_else(|| panic!("the operator's root domain is missing from {seen:?}"));
    let platform_index = seen
        .iter()
        .position(|(_, apex)| *apex == platform_apex)
        .unwrap_or_else(|| panic!("the platform zone is missing from {seen:?}"));

    // This is the regression the console reported: the operator's own zone was
    // sorted behind the platform catalog and fell off page one.
    assert!(
        operator_index < platform_index,
        "an operator zone must precede every platform zone; got {seen:?}"
    );

    // The labels have to be right, or the console shows the platform's zones as
    // the operator's own inventory.
    assert_eq!(seen[operator_index].0, "USER", "got {seen:?}");
    assert_eq!(seen[platform_index].0, "PLATFORM", "got {seen:?}");

    // Every platform zone in the page carries PLATFORM, and every USER zone is
    // one the operator owns (never a bare `app.*` provisioning apex).
    for (scope, apex) in &seen {
        if scope == "PLATFORM" {
            assert!(
                apex.starts_with("app."),
                "a PLATFORM zone must be a provisioning apex; got {apex}"
            );
        }
    }
}

/// The console's "Domains" page must be able to ask for one ownership kind.
///
/// Labeling the scope (the test above) is not enough on its own: both kinds are
/// still returned, and because the platform provisions its whole `app.<suffix>`
/// catalog in one transaction, those rows crowd the operator's own root domains
/// out of the first page. `scope` is the filter that makes "my domains" and
/// "the platform's domains" two honest, separately paginated lists.
///
/// The platform zone is seeded by SQL because no caller-facing entry point can
/// produce a zone without an owner: `create_domain_zone` always attributes the
/// new row to the calling subject, and the provisioning apex is by construction
/// `app.<suffix>` rather than a registrable root domain.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn domain_zone_listing_filters_by_scope() {
    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(7).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let suffix = sdkwork_database_id::uuid_v4().replace('-', "");
    let request = |apex: &str| CreateDomainZoneRequest {
        apex_hostname: apex.to_owned(),
        display_name: None,
        dns_provider: Some("manual".to_owned()),
        provider_zone_ref: None,
        provider_account_id: None,
    };
    let listing = |scope: Option<ZoneScope>| ListDomainZonesQuery {
        page: 1,
        page_size: 50,
        status: None,
        keyword: None,
        scope,
    };

    let operator_apex = format!("scoped{suffix}.dev");
    repository
        .create_domain_zone(7, Some(9), Some(11), &request(&operator_apex))
        .await
        .expect("create the operator's root domain");

    let platform_apex = format!("app.{operator_apex}");
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id, uuid, tenant_id, organization_id, apex_hostname, display_name,
            dns_provider, provider_zone_ref, status, user_id, created_by, updated_by
         ) VALUES (
            90002, 'zone-90002', 7, 9, $1, 'Platform app domain zone', 'platform', $2,
            'ACTIVE', NULL, 1, 1
         )",
    )
    .bind(&platform_apex)
    .bind(format!("app.*.{suffix}.dev"))
    .execute(repository.pool())
    .await
    .expect("seed the platform zone");

    let user_page = repository
        .list_domain_zones(7, Some(11), &listing(Some(ZoneScope::User)))
        .await
        .expect("list with scope=USER");
    assert_eq!(
        user_page.total,
        1,
        "scope=USER must count only the operator's own zone; got {:?}",
        user_page
            .items
            .iter()
            .map(|zone| &zone.apex_hostname)
            .collect::<Vec<_>>()
    );
    assert_eq!(user_page.items[0].apex_hostname, operator_apex);
    assert_eq!(user_page.items[0].scope, ZoneScope::User);

    let platform_page = repository
        .list_domain_zones(7, Some(11), &listing(Some(ZoneScope::Platform)))
        .await
        .expect("list with scope=PLATFORM");
    assert_eq!(
        platform_page.total,
        1,
        "scope=PLATFORM must count only the provisioning zone; got {:?}",
        platform_page
            .items
            .iter()
            .map(|zone| &zone.apex_hostname)
            .collect::<Vec<_>>()
    );
    assert_eq!(platform_page.items[0].apex_hostname, platform_apex);
    assert_eq!(platform_page.items[0].scope, ZoneScope::Platform);

    // Omitting the filter keeps the audit view: both kinds, which is what a
    // caller inspecting the whole inventory needs — the certificate coverage
    // picker in particular, since a certificate over a platform zone is
    // legitimate.
    let both = repository
        .list_domain_zones(7, Some(11), &listing(None))
        .await
        .expect("list without a scope filter");
    assert_eq!(both.total, 2, "no filter must return both kinds");
}

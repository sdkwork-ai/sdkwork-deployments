mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{CreateDomainHostnameRequest, CreateDomainZoneRequest};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn domain_activation_requires_external_evidence_for_the_current_attempt() {
    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let apex = format!(
        "verify{}.dev",
        sdkwork_database_id::uuid_v4().replace('-', "")
    );
    let zone = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(11),
            &CreateDomainZoneRequest {
                apex_hostname: apex,
                display_name: Some("Verification test".to_owned()),
                dns_provider: Some("manual".to_owned()),
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create root domain zone");
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
        .expect("create pending hostname");

    let pending = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("load pending challenge");
    let token = pending
        .token
        .clone()
        .expect("new challenge returns the proof");
    let verification_id = pending
        .verification_id
        .clone()
        .expect("pending challenge verification id");
    let proof_sha256 = pending.proof_sha256.clone().expect("pending proof digest");
    assert!(!pending.verified);
    assert!(
        pending.created,
        "the call that opens the attempt reports so"
    );
    assert_eq!(
        sdkwork_utils_rust::crypto::sha256_hash(token.as_bytes()),
        proof_sha256
    );

    // The record value is `base64url(sha256(attempt id))` and therefore
    // deterministic, so re-reading the *same* attempt has to repeat it. Until it
    // did, the value was shown exactly once — on the click that opened the
    // attempt — and every later check (a second click, the page reloaded) handed
    // the operator a record name with nothing to publish. That is an ownership
    // proof that can never be completed, a hostname stuck at `PENDING`, and a
    // certificate that can never be ordered, which is the whole loop this pins.
    let reloaded = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("reload pending challenge");
    assert!(!reloaded.created, "a reload did not open the attempt");
    assert_eq!(
        reloaded.verification_id.as_deref(),
        Some(verification_id.as_str()),
        "reload must target the same attempt"
    );
    assert_eq!(
        reloaded.token.as_deref(),
        Some(token.as_str()),
        "reload must repeat the value the operator publishes"
    );
    assert_eq!(
        reloaded.proof_sha256.as_deref(),
        Some(proof_sha256.as_str())
    );

    assert!(!repository
        .confirm_domain_hostname_verification(
            7,
            Some(11),
            &zone.id,
            &hostname.id,
            &verification_id,
            &"0".repeat(64),
            "test-resolver",
        )
        .await
        .expect("reject mismatched observation"));
    assert!(
        !repository
            .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
            .await
            .expect("reload pending challenge")
            .verified
    );

    assert!(repository
        .confirm_domain_hostname_verification(
            7,
            Some(11),
            &zone.id,
            &hostname.id,
            &verification_id,
            &proof_sha256,
            "test-resolver",
        )
        .await
        .expect("confirm exact observed digest"));
    let verified = repository
        .domain_hostname_verification_challenge(7, Some(11), &zone.id, &hostname.id)
        .await
        .expect("load verified hostname");
    assert!(verified.verified);
    assert!(verified.token.is_none());
    assert!(!repository
        .confirm_domain_hostname_verification(
            7,
            Some(11),
            &zone.id,
            &hostname.id,
            &verification_id,
            &proof_sha256,
            "test-resolver",
        )
        .await
        .expect("repeat confirmation is idempotent"));
}

/// `ensure_domain_hostname` is the claim path a certificate order uses, and its
/// whole value is that re-stating the same SAN list is not an error.
///
/// The strict `create_domain_hostname` next to it is the operator path, where
/// "this already exists" is the right answer; keeping both honest is the point of
/// this test, because collapsing them either way breaks one of the two callers.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn ensure_domain_hostname_reuses_existing_rows_and_leaves_create_strict() {
    let pool = common::postgres_pool().await;
    let repository = DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    let apex = format!(
        "claim{}.dev",
        sdkwork_database_id::uuid_v4().replace('-', "")
    );
    let zone = repository
        .create_domain_zone(
            7,
            Some(9),
            Some(11),
            &CreateDomainZoneRequest {
                apex_hostname: apex.clone(),
                display_name: Some("Claim test".to_owned()),
                dns_provider: Some("manual".to_owned()),
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create root domain zone");

    // The apex row ships with the zone, so `@` must resolve to it rather than
    // failing the way `create_domain_hostname` deliberately does.
    let apex_row = repository
        .ensure_domain_hostname(7, Some(11), &zone.id, "@")
        .await
        .expect("ensure the apex reuses the row the zone created");
    assert_eq!(apex_row.hostname, apex);

    let first = repository
        .ensure_domain_hostname(7, Some(11), &zone.id, "www")
        .await
        .expect("ensure declares the missing hostname");
    assert_eq!(first.hostname, format!("www.{apex}"));
    assert_eq!(first.verification_status, "PENDING");

    let second = repository
        .ensure_domain_hostname(7, Some(11), &zone.id, "www")
        .await
        .expect("ensure is idempotent for an owned hostname");
    assert_eq!(
        first.id, second.id,
        "re-ensuring the same name must land on the same row, not fail or duplicate"
    );

    // A wildcard keeps its leading label through the fold, which is the defect
    // the batch endpoint exists to make impossible for callers to reintroduce.
    let wildcard = repository
        .ensure_domain_hostname(7, Some(11), &zone.id, "*.shop")
        .await
        .expect("ensure declares the wildcard hostname");
    assert_eq!(wildcard.hostname, format!("*.shop.{apex}"));
    assert_eq!(wildcard.hostname_type, "WILDCARD");

    // Same name through the operator path: still refused, because a manual "add
    // hostname" that silently returns someone else's row would hide the typo.
    let strict = repository
        .create_domain_hostname(
            7,
            Some(11),
            &zone.id,
            &CreateDomainHostnameRequest {
                relative_name: "www".to_owned(),
            },
        )
        .await;
    assert!(
        strict.is_err(),
        "create_domain_hostname must stay strict while ensure stays idempotent"
    );
}

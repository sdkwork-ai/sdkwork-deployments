//! Domains that verify themselves through the DNS account bound to their zone.
//!
//! The claim under test is the one an operator makes when they configure a DNS
//! provider once: "this zone's records are served by this account, so nothing
//! about owning a name in it should need my hands". Two things have to hold for
//! that to be true, and they fail in different places, so both are asserted here:
//!
//! * **The right account is chosen.** A zone pinned to one account must present
//!   through *that* account and not through a default of the same provider — the
//!   difference between two Aliyun keys is invisible in a provider-family
//!   assertion and is exactly the mistake that would publish a challenge into the
//!   wrong one of a tenant's two zones.
//! * **The record really is published.** The proof the service confirms is the
//!   digest of whatever the *verifier* observed, so the verifier here reads back
//!   what the provider was actually handed. Asserting "the service said it
//!   published" would pass against a stub that publishes nothing.
//!
//! The provider adapters are the one thing swapped (at the factory the resolver
//! already owns), because the alternative is a live Aliyun or Cloudflare account
//! — the same reason the controlled-CA suite swaps this port rather than teaching
//! the engine a test dialect.
//!
//! Requires PostgreSQL; ignored by default like the crate's other integration
//! tests.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sdkwork_deploy_cloud_account_port::memory::MemoryCloudAccountSeed;
use sdkwork_deploy_cloud_account_port::MemoryCloudAccountPort;
use sdkwork_deploy_contract::{
    CreateCloudAccountRequest, CreateDomainHostnameRequest, CreateDomainZoneRequest, DeployAppApi,
    DeployAppRequestContext, DeployServiceErrorKind, DeployServiceResult, DomainVerifyResponse,
    ListCloudAccountsQuery, OwnershipReach, UpdateDomainHostnameRequest,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{
    dns_txt_record_name, AccountBackedDns01PresenterResolver, CertificateDns01Context,
    CertificateDns01PresenterPort, CertificateDns01Selector, DeployDnsProviderCredential,
    DeployRepositoryPort, DeployService, Dns01PresenterFactory, DomainOwnershipVerifierPort,
    DomainVerificationObservation, UnconfiguredCertificateDns01Presenter,
};
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_acme_service::{
    AcmeServiceResult, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest,
    DnsAccountVerification, DnsProviderKind, InMemoryDns01Presenter,
};

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ACTOR_ID: i64 = 11;

fn context() -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(ACTOR_ID),
        organization_id: Some(ORGANIZATION_ID),
        session_id: None,
        // The console this fixture models is the delivery console, whose caller is
        // the tenant operator reconciling the account its zones resolve through —
        // so it carries the shared-account right. A plain member is a different
        // fixture, and `the_console_sees_the_tenants_dns_accounts` is about what the
        // operator sees.
        can_manage_shared_accounts: true,
        // The delivery console does not hold the tenant-wide ownership grant; the
        // admin surface does. Keeping this `false` is what makes the ownership
        // assertions below describe the console's reach rather than the admin's.
        can_manage_all_ownership: false,
        auth_token: None,
        access_token: None,
    }
}

/// A fresh apex per test, so a shared schema cannot make one test's zone answer
/// another's lookup.
fn unique_apex(prefix: &str) -> String {
    format!(
        "{prefix}{}.dev",
        sdkwork_database_id::uuid_v4().replace('-', "")
    )
}

/// What one account's credential looked like by the time a presenter was built
/// from it. The secret is deliberately not recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PresentedAs {
    family: String,
    access_key_id: String,
}

/// The deployment-level fallback: a recorder that publishes nowhere external.
struct RecordingFallback {
    presenter: Arc<InMemoryDns01Presenter>,
    zone_apex: String,
}

#[async_trait]
impl CertificateDns01PresenterPort for RecordingFallback {
    async fn resolve(
        &self,
        _selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>> {
        Ok(Some(CertificateDns01Context {
            presenter: self.presenter.clone() as Arc<dyn Dns01Presenter>,
            zone_apex: self.zone_apex.clone(),
        }))
    }
}

/// The external vantage point: it hashes the values the provider was asked to
/// publish, so a confirmation is evidence about the wire, not about a return
/// value.
struct PublishedRecordVerifier {
    presenter: Arc<InMemoryDns01Presenter>,
}

#[async_trait]
impl DomainOwnershipVerifierPort for PublishedRecordVerifier {
    async fn verify_dns_txt(
        &self,
        hostname: &str,
        expected_sha256: &str,
    ) -> DeployServiceResult<DomainVerificationObservation> {
        let record_name = dns_txt_record_name(hostname)
            .expect("the ownership record name derives from the hostname");
        let matched = self
            .presenter
            .values_at(&record_name)
            .iter()
            .any(|value| sha256_hash(value.as_bytes()) == expected_sha256);
        Ok(DomainVerificationObservation {
            matched,
            observed_sha256: matched.then(|| expected_sha256.to_owned()),
            verifier_identity: "test/recording-resolver".to_owned(),
        })
    }
}

/// A seam that records which credential each presenter was built from.
type RecordedCredentials = Arc<Mutex<Vec<PresentedAs>>>;

fn recording_factory(
    presenter: Arc<InMemoryDns01Presenter>,
    seen: RecordedCredentials,
) -> Arc<Dns01PresenterFactory> {
    Arc::new(
        move |credential: &DeployDnsProviderCredential, _zone_ref: Option<&str>| {
            let (family, access_key_id) = match credential {
                DeployDnsProviderCredential::AliyunDns { access_key_id, .. } => {
                    ("ALIYUN_DNS", access_key_id.clone())
                }
                DeployDnsProviderCredential::Dnspod { login_id, .. } => {
                    ("DNSPOD", login_id.clone())
                }
                DeployDnsProviderCredential::Cloudflare { .. } => ("CLOUDFLARE", String::new()),
                // The generic family has no public half: its endpoints and secrets
                // both live inside the configuration document, so there is no
                // identifier to record.
                DeployDnsProviderCredential::HttpRequest { .. } => ("HTTP_REQUEST", String::new()),
            };
            seen.lock()
                .expect("recorded credentials lock")
                .push(PresentedAs {
                    family: family.to_owned(),
                    access_key_id,
                });
            Ok(presenter.clone() as Arc<dyn Dns01Presenter>)
        },
    )
}

/// One Aliyun account, distinguishable by its credential half.
fn aliyun_account(id: &str, access_key_id: &str, is_default: bool) -> MemoryCloudAccountSeed {
    let mut seed = MemoryCloudAccountSeed::tenant(id, TENANT_ID, "aliyun");
    seed.access_key_id = Some(access_key_id.to_owned());
    seed.secret_access_key = Some(format!("{access_key_id}-secret"));
    seed.is_default = is_default;
    seed
}

/// Builds the production service shape: the account-backed resolver over an
/// in-memory account center, with a recorder standing in for the provider
/// adapters and for the deployment-level fallback.
struct Harness {
    service: DeployService,
    presenter: Arc<InMemoryDns01Presenter>,
    presented: RecordedCredentials,
}

fn harness(
    repository: Arc<DeployRepository>,
    accounts: MemoryCloudAccountPort,
    fallback_zone_apex: &str,
) -> Harness {
    let repository_port = Arc::clone(&repository) as Arc<dyn DeployRepositoryPort>;
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let presented: RecordedCredentials = Arc::new(Mutex::new(Vec::new()));
    // One account center, two readers. `MemoryCloudAccountPort` is a handle over a
    // shared map, so the clone sees the same accounts and a test cannot "seed the
    // resolver's port separately from the service's" and end up proving nothing.
    let resolver = AccountBackedDns01PresenterResolver::new(
        Arc::clone(&repository_port),
        Arc::new(accounts.clone()),
        Arc::new(RecordingFallback {
            presenter: Arc::clone(&presenter),
            zone_apex: fallback_zone_apex.to_owned(),
        }),
    )
    .with_presenter_factory(recording_factory(
        Arc::clone(&presenter),
        Arc::clone(&presented),
    ));
    let service = DeployService::new(repository_port, Arc::new(MemoryDeployDrivePort))
        .with_cloud_accounts(Arc::new(accounts))
        .with_certificate_dns01_presenter(Arc::new(resolver))
        .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
            presenter: Arc::clone(&presenter),
        }));
    Harness {
        service,
        presenter,
        presented,
    }
}

/// A zone whose records are served by `account_id`.
async fn zone_pinned_to(
    service: &DeployService,
    apex: &str,
    dns_provider: &str,
    account_id: &str,
) -> String {
    service
        .create_domain_zone(
            &context(),
            &CreateDomainZoneRequest {
                apex_hostname: apex.to_owned(),
                display_name: Some(format!("Ownership automation {apex}")),
                dns_provider: Some(dns_provider.to_owned()),
                provider_zone_ref: None,
                provider_account_id: Some(account_id.to_owned()),
            },
        )
        .await
        .expect("create the zone through the operator path")
        .id
}

async fn declare_hostname(service: &DeployService, zone_id: &str, relative_name: &str) -> String {
    service
        .create_domain_hostname(
            &context(),
            zone_id,
            &CreateDomainHostnameRequest {
                relative_name: relative_name.to_owned(),
            },
        )
        .await
        .expect("declare the hostname")
        .id
}

async fn verify(service: &DeployService, zone_id: &str, hostname_id: &str) -> DomainVerifyResponse {
    service
        .verify_domain_hostname(&context(), zone_id, hostname_id)
        .await
        .expect("the verification pass must run")
}

/// The whole point of the feature: a zone bound to a cloud account publishes its
/// own ownership record and the hostname comes back verified, with no operator
/// having touched DNS.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_zone_bound_to_a_cloud_account_verifies_its_own_hostname() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("auto");
    // Two Aliyun accounts, only one of them pinned: a resolver that ignored the pin
    // would fall through to the default and be caught by the credential assertion
    // rather than by a provider-kind one.
    let accounts = MemoryCloudAccountPort::new()
        .with_account(aliyun_account("acct-pinned", "LTAI-pinned", false))
        .with_account(aliyun_account("acct-default", "LTAI-default", true));
    let harness = harness(Arc::new(repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-pinned").await;
    let hostname_id = declare_hostname(&harness.service, &zone_id, "docs").await;

    let response = verify(&harness.service, &zone_id, &hostname_id).await;

    assert!(
        response.verified,
        "a zone bound to a usable account must verify its own hostname: {response:?}"
    );
    assert_eq!(
        response.auto_published,
        Some(true),
        "the pass must report that it published the record through the zone's account"
    );
    let record_name = format!("_sdkwork-verification.docs.{apex}");
    assert!(
        !harness.presenter.values_at(&record_name).is_empty(),
        "the ownership record must have been published at {record_name}"
    );
    assert_eq!(
        harness
            .presented
            .lock()
            .expect("credentials lock")
            .as_slice(),
        &[PresentedAs {
            family: "ALIYUN_DNS".to_owned(),
            access_key_id: "LTAI-pinned".to_owned(),
        }],
        "the zone's own pin must win over the account center's default"
    );
}

/// Re-checking an already verified hostname answers with the verified state rather
/// than failing.
///
/// The attempt's proof is spent once it is answered — the challenge no longer
/// carries a digest to look up — so a second check has to return before the lookup
/// instead of falling into it. The observable difference is between the verified
/// response and an internal error, which is what an operator clicking "verify"
/// twice would otherwise see.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn verifying_an_already_verified_hostname_stays_verified() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("idempotent");
    let harness = harness(
        Arc::new(repository),
        MemoryCloudAccountPort::new().with_account(aliyun_account(
            "acct-idempotent",
            "LTAI-idempotent",
            false,
        )),
        &apex,
    );
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-idempotent").await;
    let hostname_id = declare_hostname(&harness.service, &zone_id, "docs").await;

    let first = verify(&harness.service, &zone_id, &hostname_id).await;
    assert!(first.verified, "the first pass must verify the name");

    let second = verify(&harness.service, &zone_id, &hostname_id).await;
    assert!(
        second.verified,
        "a repeated check must stay verified: {second:?}"
    );
    assert!(
        second.token.is_none(),
        "an answered attempt publishes no value to copy: {second:?}"
    );
    assert!(
        second.record_name.is_none(),
        "an answered attempt has no record left to publish: {second:?}"
    );
    assert_eq!(second.method, first.method);
}

/// A zone with no account keeps the operator path exactly as it was: the record is
/// returned, nothing is published, and the request is not an error.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_zone_without_an_account_returns_the_record_for_the_operator() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("manual");
    let accounts = MemoryCloudAccountPort::new();
    let resolver = AccountBackedDns01PresenterResolver::new(
        Arc::new(repository.clone()) as Arc<dyn DeployRepositoryPort>,
        Arc::new(accounts),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    );
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let service = DeployService::new(
        Arc::new(repository) as Arc<dyn DeployRepositoryPort>,
        Arc::new(MemoryDeployDrivePort),
    )
    .with_certificate_dns01_presenter(Arc::new(resolver))
    .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
        presenter: Arc::clone(&presenter),
    }));
    let zone_id = service
        .create_domain_zone(
            &context(),
            &CreateDomainZoneRequest {
                apex_hostname: apex.clone(),
                display_name: Some("Manual ownership".to_owned()),
                dns_provider: None,
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create an unbound zone")
        .id;
    let hostname_id = declare_hostname(&service, &zone_id, "docs").await;

    let response = verify(&service, &zone_id, &hostname_id).await;

    assert!(
        !response.verified,
        "nothing published means nothing verified"
    );
    assert_eq!(
        response.record_name.as_deref(),
        Some(format!("_sdkwork-verification.docs.{apex}").as_str()),
        "the operator must still be told what to publish"
    );
    assert!(
        response.token.is_some(),
        "the operator must still be told what value to publish"
    );
    assert!(
        presenter.record_names().is_empty(),
        "an unbound zone must publish nothing: {:?}",
        presenter.record_names()
    );
}

/// A pin whose account no longer serves a usable credential degrades to the manual
/// path instead of failing the request — a rotated key must not make a domain
/// unownable.
///
/// The pin is stored against a center that still has the credential, because a pin
/// is proved at the write that supplied it: that is the only way a pin ever comes
/// to exist. The challenge is then run against a center where the credential has
/// been withdrawn, which is the state an operator reaches by rotating a key and
/// forgetting to re-register it.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_pin_without_a_usable_credential_degrades_to_the_operator_path() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("degrade");
    let repository_port = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let service = DeployService::new(
        Arc::clone(&repository_port),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(
        MemoryCloudAccountPort::new().with_account(aliyun_account(
            "acct-rotated",
            "LTAI-rotated",
            false,
        )),
    ));
    let zone_id = zone_pinned_to(&service, &apex, "ALIYUN_DNS", "acct-rotated").await;
    let hostname_id = declare_hostname(&service, &zone_id, "docs").await;

    // The same account as the center sees it after the secret was withdrawn.
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let resolver = AccountBackedDns01PresenterResolver::new(
        repository_port,
        Arc::new(MemoryCloudAccountPort::new().with_account(
            aliyun_account("acct-rotated", "LTAI-rotated", false).without_credential(),
        )),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    );
    let service = service
        .with_certificate_dns01_presenter(Arc::new(resolver))
        .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
            presenter: Arc::clone(&presenter),
        }));

    let response = verify(&service, &zone_id, &hostname_id).await;

    assert!(
        !response.verified,
        "an unusable credential cannot verify anything"
    );
    assert!(
        response.token.is_some() && response.record_name.is_some(),
        "the request must degrade to the operator path, not fail: {response:?}"
    );
    assert!(
        presenter.record_names().is_empty(),
        "nothing may be published through a credential that does not exist"
    );
}

/// Each zone presents through the account it named, and a certificate's own pin
/// overrides its zone's — the two facts that let one tenant run several provider
/// accounts at once.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn each_zone_presents_through_the_account_it_named() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let aliyun_apex = unique_apex("zal");
    let cloudflare_apex = unique_apex("zcf");
    let accounts = MemoryCloudAccountPort::new()
        .with_account(aliyun_account("acct-aliyun-zone", "LTAI-zone", true))
        .with_account(aliyun_account("acct-aliyun-cert", "LTAI-cert", false))
        .with_account(MemoryCloudAccountSeed::tenant(
            "acct-cloudflare",
            TENANT_ID,
            "cloudflare",
        ));
    let repository_port = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let service = DeployService::new(
        Arc::clone(&repository_port),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(accounts.clone()));
    let aliyun_zone =
        zone_pinned_to(&service, &aliyun_apex, "ALIYUN_DNS", "acct-aliyun-zone").await;
    let cloudflare_zone =
        zone_pinned_to(&service, &cloudflare_apex, "CLOUDFLARE", "acct-cloudflare").await;
    // A zone is only found for a hostname that has been declared in it: the lookup
    // joins the hostname row, so declaring and resolving are two separate steps and
    // the resolution under test happens against a name the operator actually added.
    declare_hostname(&service, &aliyun_zone, "docs").await;
    declare_hostname(&service, &cloudflare_zone, "docs").await;

    // The resolver is called the way issuance calls it, against the same zones. The
    // production factory is kept here on purpose: "which adapter" is the question, and
    // a recorder cannot answer it.
    let resolver = AccountBackedDns01PresenterResolver::new(
        Arc::clone(&repository_port),
        Arc::new(accounts.clone()),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    );
    let aliyun = resolver
        .resolve(CertificateDns01Selector {
            tenant_id: TENANT_ID,
            certificate_provider_account_id: None,
            hostname: &format!("docs.{aliyun_apex}"),
        })
        .await
        .expect("resolving the Aliyun zone must not fail")
        .expect("the Aliyun zone names an account");
    assert_eq!(
        aliyun.zone_apex, aliyun_apex,
        "the zone apex owns the record"
    );
    assert_eq!(
        aliyun.presenter.provider_kind(),
        Some(DnsProviderKind::AliyunDns),
        "the Aliyun zone must present through the Aliyun adapter"
    );

    let cloudflare = resolver
        .resolve(CertificateDns01Selector {
            tenant_id: TENANT_ID,
            certificate_provider_account_id: None,
            hostname: &format!("docs.{cloudflare_apex}"),
        })
        .await
        .expect("resolving the Cloudflare zone must not fail")
        .expect("the Cloudflare zone names an account");
    assert_eq!(cloudflare.zone_apex, cloudflare_apex);
    assert_eq!(
        cloudflare.presenter.provider_kind(),
        Some(DnsProviderKind::Cloudflare),
        "the Cloudflare zone must present through the Cloudflare adapter"
    );

    // One zone, two accounts **of the same family**: the certificate's own pin wins
    // over the zone's. Two Aliyun keys are indistinguishable by provider kind, which
    // is exactly why this case has to be asserted on the credential that was used —
    // the difference between them is what the operator pinned, and nothing else.
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let presented: RecordedCredentials = Arc::new(Mutex::new(Vec::new()));
    let recording = AccountBackedDns01PresenterResolver::new(
        repository_port,
        Arc::new(accounts),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    )
    .with_presenter_factory(recording_factory(
        Arc::clone(&presenter),
        Arc::clone(&presented),
    ));
    let pinned = recording
        .resolve(CertificateDns01Selector {
            tenant_id: TENANT_ID,
            certificate_provider_account_id: Some("acct-aliyun-cert"),
            hostname: &format!("docs.{aliyun_apex}"),
        })
        .await
        .expect("resolving a certificate pin must not fail")
        .expect("the pinned account exists");
    assert_eq!(
        pinned.zone_apex, aliyun_apex,
        "the zone still owns the record, whichever account is asked to write it"
    );
    assert_eq!(
        presented.lock().expect("credentials lock").as_slice(),
        &[PresentedAs {
            family: "ALIYUN_DNS".to_owned(),
            access_key_id: "LTAI-cert".to_owned(),
        }],
        "the certificate's pin must override its zone's"
    );

    assert_ne!(aliyun_zone, cloudflare_zone);
}

/// A pin naming an account of another family is not obeyed.
///
/// The declared family decides which provider API is called, and the secret only
/// means anything to the provider that issued it: presenting a Cloudflare token as if
/// it were an Aliyun key pair would not merely fail, it would hand that secret to a
/// third party. The contradiction is refused, and because the chain treats a pin that
/// cannot present as "continue", the order still goes out through the account the
/// center picks for the zone's declared family — the operator's mistake costs them the
/// pin, not the issuance.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_pin_of_another_family_is_not_obeyed() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("mismatch");
    let accounts = MemoryCloudAccountPort::new()
        .with_account(aliyun_account("acct-aliyun-out", "LTAI-out", false))
        .with_account(MemoryCloudAccountSeed::tenant(
            "acct-cloudflare",
            TENANT_ID,
            "cloudflare",
        ));
    let repository_port = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let service = DeployService::new(
        Arc::clone(&repository_port),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(accounts.clone()));
    // The zone is Aliyun-hosted, and the Cloudflare account is pinned on it: a
    // combination no operator can mean, and the one that would leak a token.
    let zone_id = zone_pinned_to(&service, &apex, "ALIYUN_DNS", "acct-cloudflare").await;
    declare_hostname(&service, &zone_id, "docs").await;

    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let presented: RecordedCredentials = Arc::new(Mutex::new(Vec::new()));
    let resolver = AccountBackedDns01PresenterResolver::new(
        repository_port,
        Arc::new(accounts),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    )
    .with_presenter_factory(recording_factory(
        Arc::clone(&presenter),
        Arc::clone(&presented),
    ));

    let resolved = resolver
        .resolve(CertificateDns01Selector {
            tenant_id: TENANT_ID,
            certificate_provider_account_id: Some("acct-cloudflare"),
            hostname: &format!("docs.{apex}"),
        })
        .await
        .expect("a contradictory pin must not fail the order")
        .expect("the chain must still find the zone's own family account");

    assert_eq!(resolved.zone_apex, apex);
    assert_eq!(
        presented.lock().expect("credentials lock").as_slice(),
        &[PresentedAs {
            family: "ALIYUN_DNS".to_owned(),
            access_key_id: "LTAI-out".to_owned(),
        }],
        "the Aliyun account must be used, and the Cloudflare token never presented as an Aliyun key"
    );
}

/// A pin whose target no longer resolves hands over to the next step instead of
/// failing the order (`ADR-20260917` §1).
///
/// The pin is written against a center that still has the account — that is the only
/// way a pin ever comes to exist — and the resolution then runs against a center that
/// has lost it while another account of the same family remains. The order must still
/// find a presenter, and it must be the surviving account: re-pinning the missing one
/// is what the "never store a derived pin" rule exists to prevent.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_pin_that_no_longer_resolves_falls_through_to_the_center() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("dangling");
    let repository_port = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let service = DeployService::new(
        Arc::clone(&repository_port),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(
        MemoryCloudAccountPort::new().with_account(aliyun_account(
            "acct-deleted",
            "LTAI-deleted",
            false,
        )),
    ));
    let zone_id = zone_pinned_to(&service, &apex, "ALIYUN_DNS", "acct-deleted").await;
    declare_hostname(&service, &zone_id, "docs").await;

    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let presented: RecordedCredentials = Arc::new(Mutex::new(Vec::new()));
    let resolver = AccountBackedDns01PresenterResolver::new(
        repository_port,
        Arc::new(MemoryCloudAccountPort::new().with_account(aliyun_account(
            "acct-survivor",
            "LTAI-survivor",
            true,
        ))),
        // The deployment-level fallback shares the recorder, so "the chain reached
        // step 4" would show up as an empty credential list rather than as success.
        Arc::new(RecordingFallback {
            presenter: Arc::clone(&presenter),
            zone_apex: apex.clone(),
        }),
    )
    .with_presenter_factory(recording_factory(
        Arc::clone(&presenter),
        Arc::clone(&presented),
    ));

    let resolved = resolver
        .resolve(CertificateDns01Selector {
            tenant_id: TENANT_ID,
            certificate_provider_account_id: None,
            hostname: &format!("docs.{apex}"),
        })
        .await
        .expect("a dangling pin must not fail the order")
        .expect("the surviving account must present");

    assert_eq!(resolved.zone_apex, apex);
    assert_eq!(
        presented.lock().expect("credentials lock").as_slice(),
        &[PresentedAs {
            family: "ALIYUN_DNS".to_owned(),
            access_key_id: "LTAI-survivor".to_owned(),
        }],
        "the chain must move on to the center's pick"
    );
}

/// The accounts a caller may pick are the ones the center reports for the tenant,
/// not an empty list invented by a missing port.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_console_sees_the_tenants_dns_accounts() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-listed",
        "LTAI-listed",
        true,
    ));
    let service = DeployService::new(
        Arc::new(repository) as Arc<dyn DeployRepositoryPort>,
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(accounts));

    let page = service
        .list_cloud_accounts(
            &context(),
            &ListCloudAccountsQuery {
                dns_provider: Some("ALIYUN_DNS".to_owned()),
                page: 1,
                page_size: 20,
                scope_type: None,
                mine: false,
                keyword: None,
            },
        )
        .await
        .expect("listing must not fail");
    assert_eq!(page.items.len(), 1, "the tenant's Aliyun account: {page:?}");
    assert_eq!(page.items[0].id, "acct-listed");
    assert_eq!(
        page.items[0].dns_provider.as_deref(),
        Some("ALIYUN_DNS"),
        "the console is told which family the account drives, not just its vendor"
    );
}

/// The reach the console gets is the caller's entitlement, not the caller's request.
///
/// One seeded tenant-wide account, two callers: the operator the delivery console
/// serves sees it, and a caller without the shared-account right sees nothing — and
/// cannot publish a tenant-wide account either. Before this rule the service passed
/// `include_platform: true` and the port passed `include_tenant_shared: true`
/// outright, so the second caller saw the first caller's accounts *and* could
/// register the level every zone in the tenant resolves through.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_console_reach_follows_the_callers_shared_account_right() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-cloud-account-entitlement-test")
            .await;
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-tenant",
        "LTAI-tenant",
        true,
    ));
    let service = DeployService::new(
        Arc::new(repository) as Arc<dyn DeployRepositoryPort>,
        Arc::new(MemoryDeployDrivePort),
    )
    .with_cloud_accounts(Arc::new(accounts));

    let query = || ListCloudAccountsQuery {
        dns_provider: Some("ALIYUN_DNS".to_owned()),
        page: 1,
        page_size: 20,
        scope_type: None,
        mine: false,
        keyword: None,
    };
    let member = DeployAppRequestContext {
        can_manage_shared_accounts: false,
        ..context()
    };
    let page = service
        .list_cloud_accounts(&member, &query())
        .await
        .expect("listing must not fail");
    assert!(
        page.items.is_empty(),
        "a caller without the shared-account right must not see the tenant-wide account: {page:?}"
    );
    let page = service
        .list_cloud_accounts(&context(), &query())
        .await
        .expect("listing must not fail");
    assert_eq!(page.items.len(), 1, "the operator does see it: {page:?}");

    // The write half, which is the takeover this rule closes: the member cannot make
    // itself the account the tenant resolves through.
    let error = service
        .create_cloud_account(
            &member,
            &CreateCloudAccountRequest {
                display_name: "我的阿里云 DNS".to_owned(),
                account_code: Some("acct_member".to_owned()),
                dns_provider: "ALIYUN_DNS".to_owned(),
                scope_type: Some("tenant".to_owned()),
                environment: None,
                is_default: false,
                access_key_id: Some("LTAI-member".to_owned()),
                secret_access_key: "member-secret".to_owned(),
                session_token: None,
                confirms_credential: true,
            },
        )
        .await
        .expect_err("a tenant-wide account must not be published by a plain member");
    // Pinned to the entitlement's own wording, not merely to `Validation`. Any
    // unrelated rejection also arrives as `Validation` — an `accountCode` the
    // validator turns away, say — and would read as a gate that had held when the
    // request never reached it.
    assert_eq!(
        error.kind(),
        DeployServiceErrorKind::Validation,
        "unexpected refusal: {error:?}"
    );
    assert!(
        format!("{error:?}").contains("requires permission to manage shared provider accounts"),
        "the refusal must be the entitlement's own: {error:?}"
    );

    // The other direction of the same field, and the reason this test is not
    // satisfied by a hard-wired value: the entitlement has to be *carried through*
    // to the port. A caller who holds the right must reach the account center, so a
    // service that stopped passing the context on — in either direction — fails one
    // half or the other.
    let registration = service
        .create_cloud_account(
            &context(),
            &CreateCloudAccountRequest {
                display_name: "运维阿里云 DNS".to_owned(),
                account_code: Some("acct_operator".to_owned()),
                dns_provider: "ALIYUN_DNS".to_owned(),
                scope_type: Some("tenant".to_owned()),
                environment: None,
                is_default: false,
                access_key_id: Some("LTAI-operator".to_owned()),
                secret_access_key: "operator-secret".to_owned(),
                session_token: None,
                confirms_credential: true,
            },
        )
        .await
        .expect("an operator holding the shared-account right must be able to register");
    assert_eq!(
        registration.account.scope_type, "tenant",
        "the registration must land at the tenant level: {registration:?}"
    );
}

// `ListCloudAccountsQuery` is constructed by name, so a field added to it must be
// a deliberate decision here rather than a silent `..Default::default()`.
#[allow(dead_code)]
fn _query_fields_are_explicit(query: &ListCloudAccountsQuery) {
    let _ = (
        &query.dns_provider,
        query.page,
        query.page_size,
        &query.scope_type,
        query.mine,
        &query.keyword,
    );
}

/// Creating a subdomain in a zone bound to a cloud account materializes it at the
/// provider, which is what turns the first "verify" into one click: the record is
/// already published, so the click only has to read it back.
///
/// The ledger is asserted alongside the provider's own state on purpose. The two
/// describe the same publish from opposite ends — what went out, and what this
/// deployment remembers about it — and a build that wrote one without the other
/// would look correct at create time and only fail at the first rename or delete,
/// when the record can no longer be named.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn creating_a_subdomain_publishes_its_ownership_record() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("create");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-create",
        "LTAI-create",
        true,
    ));
    let repository = Arc::new(repository);
    let harness = harness(Arc::clone(&repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-create").await;
    let hostname_id = declare_hostname(&harness.service, &zone_id, "docs").await;

    let record_name = format!("_sdkwork-verification.docs.{apex}");
    let published = harness.presenter.values_at(&record_name);
    assert_eq!(
        published.len(),
        1,
        "creating the subdomain must publish exactly one value; the provider holds {:?}",
        harness.presenter.record_names()
    );
    assert_eq!(
        harness
            .presented
            .lock()
            .expect("credentials lock")
            .as_slice(),
        &[PresentedAs {
            family: "ALIYUN_DNS".to_owned(),
            access_key_id: "LTAI-create".to_owned(),
        }],
        "the publish must go through the zone's own account"
    );

    let ledger = repository
        .domain_hostname_dns_record(
            TENANT_ID,
            OwnershipReach::new(Some(ACTOR_ID), Some(ORGANIZATION_ID), false),
            &zone_id,
            &hostname_id,
        )
        .await
        .expect("reading the recorded provider record must not fail")
        .expect("a published record must be recorded, or it can never be withdrawn");
    assert_eq!(ledger.record_name, record_name);
    assert_eq!(ledger.zone_apex, apex);
    assert_eq!(
        Some(ledger.record_value.clone()),
        published.first().cloned(),
        "the ledger must describe the value the provider was actually given"
    );

    // One click: nothing has to be published between the create and the check.
    let response = verify(&harness.service, &zone_id, &hostname_id).await;
    assert!(
        response.verified,
        "the record was published by the create, so the first check must verify: {response:?}"
    );
}

/// Renaming a subdomain moves its record: the new name is published and the old
/// one is taken back.
///
/// Without the withdrawal the provider keeps answering for a name this deployment
/// has stopped managing, and the stale record is invisible from the console —
/// which is the failure mode this whole ledger exists to prevent.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn renaming_a_subdomain_moves_its_record() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("rename");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-rename",
        "LTAI-rename",
        true,
    ));
    let repository = Arc::new(repository);
    let harness = harness(Arc::clone(&repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-rename").await;
    let hostname_id = declare_hostname(&harness.service, &zone_id, "docs").await;
    let old_name = format!("_sdkwork-verification.docs.{apex}");
    assert_eq!(harness.presenter.values_at(&old_name).len(), 1);

    harness
        .service
        .update_domain_hostname(
            &context(),
            &zone_id,
            &hostname_id,
            &UpdateDomainHostnameRequest {
                relative_name: "api".to_owned(),
            },
        )
        .await
        .expect("the rename must succeed");

    let new_name = format!("_sdkwork-verification.api.{apex}");
    assert_eq!(
        harness.presenter.values_at(&new_name).len(),
        1,
        "the renamed hostname must be published; the provider holds {:?}",
        harness.presenter.record_names()
    );
    assert!(
        harness.presenter.values_at(&old_name).is_empty(),
        "the old name's record must not survive the rename; the provider holds {:?}",
        harness.presenter.record_names()
    );
    // The whole record set, not just the two names: a rename that published the new
    // value without withdrawing the old one would still pass the two assertions
    // above if some third record had appeared, and this is the claim the operator
    // actually relies on — the provider holds what the console shows, no more.
    assert_eq!(
        harness.presenter.record_names(),
        vec![new_name.clone()],
        "the provider must hold exactly the renamed hostname's record"
    );

    let ledger = repository
        .domain_hostname_dns_record(
            TENANT_ID,
            OwnershipReach::new(Some(ACTOR_ID), Some(ORGANIZATION_ID), false),
            &zone_id,
            &hostname_id,
        )
        .await
        .expect("reading the recorded provider record must not fail")
        .expect("the rename republishes, so a record must be on file");
    assert_eq!(
        ledger.record_name, new_name,
        "the ledger must follow the hostname, not stay on the old name"
    );
}

/// Deleting a subdomain takes its record out of DNS, and leaves its neighbours
/// alone.
///
/// The neighbour is the point. A cleanup that removed *every* record for the zone,
/// or that withdrew through a handle belonging to another hostname, would pass a
/// single-hostname test and destroy a live name the first time an operator tidied
/// up one subdomain.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn deleting_a_subdomain_withdraws_only_its_own_record() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("delete");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-delete",
        "LTAI-delete",
        true,
    ));
    let repository = Arc::new(repository);
    let harness = harness(Arc::clone(&repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-delete").await;
    let doomed = declare_hostname(&harness.service, &zone_id, "docs").await;
    let survivor_name = format!("_sdkwork-verification.api.{apex}");
    declare_hostname(&harness.service, &zone_id, "api").await;
    assert_eq!(
        harness.presenter.record_names().len(),
        2,
        "both subdomains must be published before one of them is deleted"
    );

    harness
        .service
        .delete_domain_hostname(&context(), &zone_id, &doomed)
        .await
        .expect("the delete must succeed");

    let doomed_name = format!("_sdkwork-verification.docs.{apex}");
    assert!(
        harness.presenter.values_at(&doomed_name).is_empty(),
        "the deleted hostname's record must be withdrawn; the provider holds {:?}",
        harness.presenter.record_names()
    );
    assert_eq!(
        harness.presenter.values_at(&survivor_name).len(),
        1,
        "the neighbouring hostname's record must be untouched; the provider holds {:?}",
        harness.presenter.record_names()
    );
}

/// A zone with no cloud account writes nothing at the provider on create or on
/// delete, and both operations still succeed.
///
/// This is the pre-existing behaviour the whole feature has to leave alone: an
/// operator who publishes their records by hand must not gain provider traffic,
/// a verification attempt nobody asked for, or a delete that fails because there
/// was nothing to clean up.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_zone_without_an_account_writes_nothing_at_the_provider() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("manual-sync");
    let repository = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let service = DeployService::new(Arc::clone(&repository), Arc::new(MemoryDeployDrivePort))
        .with_certificate_dns01_presenter(Arc::new(AccountBackedDns01PresenterResolver::new(
            repository,
            Arc::new(MemoryCloudAccountPort::new()),
            Arc::new(UnconfiguredCertificateDns01Presenter),
        )))
        .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
            presenter: Arc::clone(&presenter),
        }));
    let zone_id = service
        .create_domain_zone(
            &context(),
            &CreateDomainZoneRequest {
                apex_hostname: apex.clone(),
                display_name: Some("Manual sync".to_owned()),
                dns_provider: None,
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create an unbound zone")
        .id;

    let hostname_id = declare_hostname(&service, &zone_id, "docs").await;
    assert!(
        presenter.record_names().is_empty(),
        "an unbound zone must publish nothing when a subdomain is created: {:?}",
        presenter.record_names()
    );

    service
        .delete_domain_hostname(&context(), &zone_id, &hostname_id)
        .await
        .expect("a delete with nothing to withdraw must still succeed");
    assert!(
        presenter.record_names().is_empty(),
        "an unbound zone must publish nothing when a subdomain is deleted: {:?}",
        presenter.record_names()
    );
}

/// An in-memory presenter whose family *can* prove an account read-only. The
/// production families that support it answer through their provider; what the
/// zone-verify response owes the operator is the same shape, so the probe's
/// confirmed path is asserted against a presenter that models exactly that.
#[derive(Debug, Default)]
struct ProbeConfirmedPresenter {
    inner: Arc<InMemoryDns01Presenter>,
}

#[async_trait]
impl Dns01Presenter for ProbeConfirmedPresenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::AliyunDns)
    }
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        self.inner.publish(request).await
    }
    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        self.inner.withdraw(handle).await
    }
    async fn verify_account(&self, _zone_apex: &str) -> AcmeServiceResult<DnsAccountVerification> {
        Ok(DnsAccountVerification::Verified)
    }
}

/// The operation the console's 根域名「验证」button drives on a zone whose
/// cloud account answered at entry time: the record is already at the provider,
/// so the click *confirms* — it reads DNS, proves the apex's ownership, and
/// reports honestly that it published nothing itself. The provider fields
/// describe what the account probe could and could not answer; the in-memory
/// family has no read-only probe, so it reports "not checked" rather than a
/// fabricated confirmation.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn verifying_an_entered_zone_confirms_without_republishing() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("zoneverify");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-zone-verify",
        "LTAI-zone-verify",
        false,
    ));
    let harness = harness(Arc::new(repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-zone-verify").await;

    // Zone entry already published the apex record — the same one-click promise
    // a subdomain's create makes — so the ledger on the apex row is populated
    // before any verify click.
    let record_name = format!("_sdkwork-verification.{apex}");
    assert!(
        !harness.presenter.values_at(&record_name).is_empty(),
        "zone entry must have published the apex record at {record_name}"
    );

    let response = harness
        .service
        .verify_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone verification pass must run");

    assert!(
        response.verified,
        "a zone whose record is already at the provider verifies outright: {response:?}"
    );
    assert_eq!(
        response.verification_status, "VERIFIED",
        "the response must carry the row's post-pass state"
    );
    assert!(
        !response.auto_published,
        "this pass only read DNS; the record was published at entry: {response:?}"
    );
    // The in-memory family cannot probe, so nothing was checked and nothing is
    // claimed — the same rule a provider outage follows.
    assert!(!response.provider_account_checked);
    assert!(!response.provider_account_verified);
    assert_eq!(response.provider_account_refusal, None);

    // The zone inventory reads the same fact, so the console badge refreshes
    // from the list alone.
    let zone = harness
        .service
        .retrieve_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone must stay readable");
    assert_eq!(zone.verification_status, "VERIFIED");
    assert!(
        zone.verified_at.is_some(),
        "a verified zone carries its instant"
    );
}

/// The half of the zone verify a TXT lookup cannot answer: whether the domain
/// really is registered at the provider the account belongs to. A family that
/// can prove it read-only comes back confirmed, and that answer travels in the
/// response next to the verification result — never folded into it.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_zone_verify_reports_a_provider_that_confirms_the_registration() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("zoneprobe");
    let repository = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let presenter = Arc::new(ProbeConfirmedPresenter::default());
    let seen: RecordedCredentials = Arc::new(Mutex::new(Vec::new()));
    let resolver = AccountBackedDns01PresenterResolver::new(
        Arc::clone(&repository),
        Arc::new(MemoryCloudAccountPort::new().with_account(aliyun_account(
            "acct-zone-probe",
            "LTAI-zone-probe",
            false,
        ))),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    )
    .with_presenter_factory({
        let presenter = Arc::clone(&presenter);
        let seen = Arc::clone(&seen);
        Arc::new(
            move |credential: &DeployDnsProviderCredential, _zone_ref: Option<&str>| {
                let DeployDnsProviderCredential::AliyunDns { access_key_id, .. } = credential
                else {
                    panic!("the fixture pins an Aliyun account");
                };
                seen.lock()
                    .expect("recorded credentials lock")
                    .push(PresentedAs {
                        family: "ALIYUN_DNS".to_owned(),
                        access_key_id: access_key_id.clone(),
                    });
                Ok(presenter.clone() as Arc<dyn Dns01Presenter>)
            },
        )
    });
    let service = DeployService::new(repository, Arc::new(MemoryDeployDrivePort))
        .with_cloud_accounts(Arc::new(
            MemoryCloudAccountPort::new().with_account(aliyun_account(
                "acct-zone-probe",
                "LTAI-zone-probe",
                false,
            )),
        ))
        .with_certificate_dns01_presenter(Arc::new(resolver))
        .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
            presenter: Arc::clone(&presenter.inner),
        }));
    let zone_id = zone_pinned_to(&service, &apex, "ALIYUN_DNS", "acct-zone-probe").await;

    let response = service
        .verify_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone verification pass must run");

    assert!(response.provider_account_checked, "the family can probe");
    assert!(
        response.provider_account_verified,
        "the probe confirmed the registration: {response:?}"
    );
    assert_eq!(response.provider_account_refusal, None);
    assert!(
        response.verified,
        "the probe adds information; it does not gate the TXT path: {response:?}"
    );
    assert!(
        !response.auto_published,
        "the record was published when the zone was entered, not by this pass"
    );
}

/// The manual path at zone level: no account, so the record is returned for the
/// operator, the zone stays PENDING, and re-running the check re-derives the
/// same value instead of invalidating whatever they already published.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_zone_without_an_account_returns_the_manual_record_and_keeps_the_token_stable() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("zonemanual");
    let repository = Arc::new(repository) as Arc<dyn DeployRepositoryPort>;
    let presenter = Arc::new(InMemoryDns01Presenter::default());
    let service = DeployService::new(Arc::clone(&repository), Arc::new(MemoryDeployDrivePort))
        .with_certificate_dns01_presenter(Arc::new(AccountBackedDns01PresenterResolver::new(
            repository,
            Arc::new(MemoryCloudAccountPort::new()),
            Arc::new(UnconfiguredCertificateDns01Presenter),
        )))
        .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
            presenter: Arc::clone(&presenter),
        }));
    let zone_id = service
        .create_domain_zone(
            &context(),
            &CreateDomainZoneRequest {
                apex_hostname: apex.clone(),
                display_name: Some("Zone manual".to_owned()),
                dns_provider: None,
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create an unbound zone")
        .id;

    let first = service
        .verify_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone verification pass must run");
    let second = service
        .verify_domain_zone(&context(), &zone_id)
        .await
        .expect("a re-check must re-read the challenge");

    assert!(!first.verified && !second.verified);
    assert!(!first.auto_published && !second.auto_published);
    assert_eq!(
        first.record_name.as_deref(),
        Some(format!("_sdkwork-verification.{apex}").as_str()),
        "the operator must be told where the apex record goes"
    );
    assert!(
        first.token.is_some(),
        "the operator must be told what value to publish"
    );
    assert_eq!(
        first.token, second.token,
        "the value must not churn between checks, or a published record goes stale"
    );
    assert!(!first.provider_account_checked && !first.provider_account_verified);
    let zone = service
        .retrieve_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone must stay readable");
    assert_eq!(
        zone.verification_status, "PENDING",
        "an unpublished record leaves the zone unverified"
    );
    assert!(
        presenter.record_names().is_empty(),
        "an unbound zone must publish nothing: {:?}",
        presenter.record_names()
    );
}

/// Deleting the zone takes the record the automation published for its apex
/// back out of the provider, mirroring what a hostname delete already does for
/// its own record.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn deleting_a_verified_zone_withdraws_the_apex_record() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("zonedelete");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-zone-delete",
        "LTAI-zone-delete",
        false,
    ));
    let harness = harness(Arc::new(repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-zone-delete").await;
    let response = harness
        .service
        .verify_domain_zone(&context(), &zone_id)
        .await
        .expect("the zone verification pass must run");
    assert!(response.verified, "the fixture needs a verified zone");
    let record_name = format!("_sdkwork-verification.{apex}");
    assert!(
        !harness.presenter.values_at(&record_name).is_empty(),
        "the fixture needs the apex record published"
    );

    harness
        .service
        .delete_domain_zone(&context(), &zone_id)
        .await
        .expect("the delete must succeed once nothing references the zone");

    assert!(
        harness.presenter.values_at(&record_name).is_empty(),
        "the automation's own record must be withdrawn with the zone"
    );
}

/// 域名录入与功能对齐：创建绑定云账号的根域名时，apex 的所有权记录随创建
/// 即被发布——与子域名的创建路径同一承诺；未绑定账号（且无任何可用凭据）的
/// zone 在录入时什么都不写。
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn entering_a_zone_publishes_the_apex_record_only_through_an_account() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await;
    let apex = unique_apex("zoneentry");
    let accounts = MemoryCloudAccountPort::new().with_account(aliyun_account(
        "acct-zone-entry",
        "LTAI-zone-entry",
        false,
    ));
    let harness = harness(Arc::new(repository), accounts, &apex);
    let zone_id = zone_pinned_to(&harness.service, &apex, "ALIYUN_DNS", "acct-zone-entry").await;

    let record_name = format!("_sdkwork-verification.{apex}");
    assert!(
        !harness.presenter.values_at(&record_name).is_empty(),
        "entering a zone bound to an account must publish its apex record at {record_name}"
    );

    // The manual half: a zone entered without any account — modelled by the
    // unconfigured resolver, the same shape the operator path uses — writes
    // nothing, and the operator is told what to publish instead.
    let manual_apex = unique_apex("zoneentrymanual");
    let manual_repository = Arc::new(
        common::migrated_repository_with_pool("sdkwork-deploy-domain-ownership-automation-test")
            .await
            .0,
    ) as Arc<dyn DeployRepositoryPort>;
    let manual_presenter = Arc::new(InMemoryDns01Presenter::default());
    let manual_service = DeployService::new(
        Arc::clone(&manual_repository),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_certificate_dns01_presenter(Arc::new(AccountBackedDns01PresenterResolver::new(
        manual_repository,
        Arc::new(MemoryCloudAccountPort::new()),
        Arc::new(UnconfiguredCertificateDns01Presenter),
    )))
    .with_domain_ownership_verifier(Arc::new(PublishedRecordVerifier {
        presenter: Arc::clone(&manual_presenter),
    }));
    manual_service
        .create_domain_zone(
            &context(),
            &CreateDomainZoneRequest {
                apex_hostname: manual_apex.clone(),
                display_name: Some("Zone entry manual".to_owned()),
                dns_provider: None,
                provider_zone_ref: None,
                provider_account_id: None,
            },
        )
        .await
        .expect("create an unbound zone");
    assert!(
        manual_presenter.record_names().is_empty(),
        "an unbound zone must publish nothing at entry: {:?}",
        manual_presenter.record_names()
    );
}

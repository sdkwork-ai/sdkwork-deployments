//! Deploy business service orchestrating repository ports and HTTP API traits.

pub mod app;
pub mod app_composition;
pub mod app_delivery;
pub mod app_domains;
pub mod backend;
pub mod certificate_caa;
pub mod certificate_dns;
pub mod certificate_dns_account;
pub mod certificate_issuance;
pub mod certificate_material;
pub mod certificate_renewal;
pub mod cloud_accounts;
pub mod domain_verification;
pub mod entitlement;
pub mod repository;
pub mod runtime_publication;

pub use app_composition::{AppCompositionRepositoryPort, ReplaceAppCompositionCommand};
pub use certificate_caa::{
    caa_base_name, caa_parent_name, combine_order_caa_observations, evaluate_caa_policy,
    issuer_domains_for_directory, observe_identifier_caa, observe_order_caa,
    observe_relevant_caa_rrset, reduce_caa_observation, CaaDecisionObservation,
    CaaIndeterminateReason, CaaPolicyOutcome, CaaRecord, CaaRefusalReason, CaaRrset,
    CaaRrsetObservation, CertificateAuthorityAuthorizationPort, CertificateOrderCaaSubject,
    UnconfiguredCertificateAuthorityAuthorization, ACME_METHOD_DNS_01, ACME_METHOD_HTTP_01,
    CAA_PARAM_ACCOUNT_URI, CAA_PARAM_VALIDATION_METHODS, CAA_SUPPORTED_TAGS, CAA_TAG_IODEF,
    CAA_TAG_ISSUE, CAA_TAG_ISSUEWILD, MAX_CAA_TREE_WALK_LABELS,
};
pub use certificate_dns::{build_dns01_presenter, DeployDnsProviderCredential};
pub use certificate_dns_account::AccountBackedDns01PresenterResolver;
pub use certificate_issuance::{
    resolve_challenge_type, CertificateDns01Context, CertificateDns01PresenterPort,
    CertificateDns01Selector, CertificateIssuancePort, CertificateIssuanceRequest,
    CertificateOrderBatchResult, UnconfiguredCertificateDns01Presenter,
};
pub use certificate_material::{seal_issued_material, DeclaredCertificateEvidence};
pub use certificate_renewal::{
    renewal_retry_delay, validate_renew_before_days, CertificateRenewalBatchResult, ValidityPhase,
    ValidityWindow, DEFAULT_RENEW_BEFORE_DAYS, MAXIMUM_RENEWAL_RETRY_SECONDS,
    MAXIMUM_RENEW_BEFORE_DAYS, MINIMUM_RENEW_BEFORE_DAYS, RENEWAL_OVERDUE_GRACE_DAYS,
    SWEEP_LOOKAHEAD_DAYS,
};
pub use domain_verification::{
    dns_txt_record_name, dns_txt_record_value, dns_txt_relative_name, normalize_domain_hostname,
    normalize_zone_apex, relative_name_for_hostname, DomainOwnershipVerifierPort,
    DomainVerificationChallenge, DomainVerificationObservation,
    UnconfiguredDomainOwnershipVerifier,
};
pub use repository::{
    CertificateOrderClaim, CertificateRenewalClaim, DeployRepositoryPort, DnsChallengeZone,
    ExpiredCertificateSweep,
};
pub use runtime_publication::{
    DeployRuntimeAssignmentMutationPort, DeployRuntimeAssignmentRepositoryPort,
    RuntimeObservationEvidence, RuntimeObservationPersistenceResult, RuntimeObservationState,
    RuntimePublicationBatchResult, RuntimePublicationService,
};

use std::sync::Arc;

use sdkwork_deploy_certificate_material::{MaterialKeyProvider, TrustAnchorBundle};
use sdkwork_deploy_cloud_account_port::{DeployCloudAccountPort, DeployCloudAccountPortAdapter};
use sdkwork_deploy_content_provider_port::{ContentProviderPort, MemoryContentProviderPort};
use sdkwork_deploy_contract::DeployServiceResult;
use sdkwork_deploy_drive_port::DeployDrivePort;

/// Application service for SDKWork Deploy control plane operations.
pub struct DeployService {
    pub(crate) repository: Arc<dyn DeployRepositoryPort>,
    pub(crate) drive: Arc<dyn DeployDrivePort>,
    pub(crate) content_provider: Arc<dyn ContentProviderPort>,
    pub(crate) domain_ownership_verifier: Arc<dyn DomainOwnershipVerifierPort>,
    pub(crate) certificate_caa: Arc<dyn CertificateAuthorityAuthorizationPort>,
    /// Custody root for certificate material at rest.
    ///
    /// `Option`, not a default implementation, because there is no safe default:
    /// storing a private key without a master key is exactly the failure this
    /// exists to prevent. An unconfigured service therefore refuses to store a
    /// version rather than silently keeping key material in the clear.
    pub(crate) certificate_material_key: Option<Arc<dyn MaterialKeyProvider>>,
    /// Local trust anchors, used to resolve the root of a chain that stops at an
    /// intermediate — which is what most CAs return.
    ///
    /// Absent by default, and that is safe: without it the service still resolves
    /// an anchor from the chain itself when the chain ends at a self-signed
    /// certificate, and otherwise refuses the request rather than storing a
    /// bundle whose `root.pem` is empty.
    pub(crate) certificate_trust_anchors: Option<Arc<TrustAnchorBundle>>,
    /// The ACME execution engine (PLAN-2026-0003 §9).
    ///
    /// `Option`, not a default, because there is no honest default: an issuance
    /// worker with no engine must refuse to run rather than pretend an order made
    /// progress. A deployment that does not host certificates simply never
    /// installs one.
    pub(crate) certificate_issuer: Option<Arc<dyn CertificateIssuancePort>>,
    /// Resolves the DNS-01 presenter for a hostname, so wildcard and explicitly
    /// DNS-01 certificates can be validated without an operator pasting records.
    /// The default answers "no provider", which is a fact rather than a failure.
    pub(crate) certificate_dns01: Arc<dyn CertificateDns01PresenterPort>,
    /// The cloud account center, as Deploy consumes it.
    ///
    /// The default reports that no account center is reachable rather than an
    /// empty inventory: "this tenant has no DNS account" and "nobody wired the
    /// account center" look the same to an operator and mean opposite things.
    pub(crate) cloud_accounts: Arc<dyn DeployCloudAccountPort>,
    runtime_publication: Option<Arc<RuntimePublicationService>>,
}

impl DeployService {
    pub fn new(repository: Arc<dyn DeployRepositoryPort>, drive: Arc<dyn DeployDrivePort>) -> Self {
        Self {
            repository,
            drive,
            content_provider: Arc::new(MemoryContentProviderPort),
            domain_ownership_verifier: Arc::new(UnconfiguredDomainOwnershipVerifier),
            certificate_caa: Arc::new(UnconfiguredCertificateAuthorityAuthorization),
            certificate_material_key: None,
            certificate_trust_anchors: None,
            certificate_issuer: None,
            certificate_dns01: Arc::new(UnconfiguredCertificateDns01Presenter),
            cloud_accounts: Arc::new(DeployCloudAccountPortAdapter::Unconfigured),
            runtime_publication: None,
        }
    }

    pub fn new_with_runtime_publication(
        repository: Arc<dyn DeployRepositoryPort>,
        drive: Arc<dyn DeployDrivePort>,
        content_provider: Arc<dyn ContentProviderPort>,
        domain_ownership_verifier: Arc<dyn DomainOwnershipVerifierPort>,
        runtime_publication: Arc<RuntimePublicationService>,
    ) -> Self {
        Self {
            repository,
            drive,
            content_provider,
            domain_ownership_verifier,
            certificate_caa: Arc::new(UnconfiguredCertificateAuthorityAuthorization),
            certificate_material_key: None,
            certificate_trust_anchors: None,
            certificate_issuer: None,
            certificate_dns01: Arc::new(UnconfiguredCertificateDns01Presenter),
            cloud_accounts: Arc::new(DeployCloudAccountPortAdapter::Unconfigured),
            runtime_publication: Some(runtime_publication),
        }
    }

    /// Replaces the CAA resolver.
    ///
    /// Until a resolver is supplied, every CAA observation is
    /// [`CaaRrset::Unavailable`](crate::CaaRrset::Unavailable): the check is
    /// recorded as unevaluable and enforcement stays with the CA, rather than
    /// the control plane reporting a permission it never established.
    pub fn with_certificate_caa_authorization(
        mut self,
        port: Arc<dyn CertificateAuthorityAuthorizationPort>,
    ) -> Self {
        self.certificate_caa = port;
        self
    }

    /// Installs the master key that certificate material is sealed under.
    ///
    /// Without it the service refuses to store a certificate version at all: a
    /// version whose private key never reached storage is a broken certificate,
    /// whereas a version stored in the clear is a compromised one. Failing is
    /// the lesser harm, and it fails loudly at the first issuance rather than
    /// being discovered during an incident.
    pub fn with_certificate_material_key_provider(
        mut self,
        provider: Arc<dyn MaterialKeyProvider>,
    ) -> Self {
        self.certificate_material_key = Some(provider);
        self
    }

    /// Installs the local trust anchor bundle.
    ///
    /// Needed whenever a CA's chain stops at an intermediate and the worker did
    /// not send the root — Let's Encrypt being the obvious case. Without it,
    /// issuance still succeeds for self-signed and root-including chains.
    pub fn with_certificate_trust_anchors(mut self, anchors: Arc<TrustAnchorBundle>) -> Self {
        self.certificate_trust_anchors = Some(anchors);
        self
    }

    /// Installs the ACME execution engine (PLAN-2026-0003 §9).
    ///
    /// Until it is supplied the issuance worker refuses to run rather than
    /// reporting an order as progress it never made.
    pub fn with_certificate_issuance(mut self, port: Arc<dyn CertificateIssuancePort>) -> Self {
        self.certificate_issuer = Some(port);
        self
    }

    /// Installs the resolver that turns a hostname into a DNS-01 presenter.
    ///
    /// Without one, wildcard and explicitly DNS-01 certificates cannot be issued
    /// automatically. The worker reports the missing credential and fails the
    /// order; it never downgrades the validation method, because a downgrade
    /// would be refused by the CA only after spending an order against the
    /// tenant's rate-limit budget.
    pub fn with_certificate_dns01_presenter(
        mut self,
        port: Arc<dyn CertificateDns01PresenterPort>,
    ) -> Self {
        self.certificate_dns01 = port;
        self
    }

    /// Installs the cloud account center this deployment reads DNS credentials
    /// from.
    ///
    /// Until it is supplied, listing accounts and resolving a credential both
    /// report that no account center is reachable. That is deliberately not the
    /// same as "no accounts": a zone may be created without one (its DNS-01 then
    /// falls back to the deployment-level provider configuration), and an operator
    /// seeing an empty inventory instead of a wiring error would conclude the
    /// tenant has no credentials when the truth is that nobody configured the
    /// port.
    pub fn with_cloud_accounts(mut self, port: Arc<dyn DeployCloudAccountPort>) -> Self {
        self.cloud_accounts = port;
        self
    }

    /// Whether an ACME engine is installed.
    ///
    /// Exposed so a worker can decide *not to start* rather than discover the
    /// absence one failed batch at a time: refusing to claim is the only way to
    /// leave the order backlog untouched.
    pub fn certificate_issuance_configured(&self) -> bool {
        self.certificate_issuer.is_some()
    }

    pub fn runtime_publication(&self) -> Option<&Arc<RuntimePublicationService>> {
        self.runtime_publication.as_ref()
    }

    pub async fn ready_check(&self) -> DeployServiceResult<()> {
        self.repository.ready_check().await
    }
}

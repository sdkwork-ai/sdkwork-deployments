//! ACME issuance wiring for the Deploy service host.
//!
//! The control plane's issuance port is satisfied by the *same* ACME engine the
//! Web Server standalone manager uses (`sdkwork-webserver-acme-service`), which is
//! deliberate: one ACME implementation across the fleet means a fix or a policy
//! change lands once, and the two control planes cannot drift into disagreeing
//! about what a valid challenge is.
//!
//! The engine is where the two validation methods actually differ:
//!
//! * **HTTP-01** needs a webroot the public edge already serves. The worker and
//!   the edge are co-located in a standalone deployment, so the path comes from
//!   the *same* environment variable the Web Server uses — a second, private
//!   webroot would publish the challenge somewhere no CA can read it.
//! * **DNS-01** needs a presenter that can publish a TXT record. It is required
//!   for every wildcard identifier, because a wildcard name does not resolve to a
//!   host and therefore has no HTTP proof path at all.

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_intelligence_deploy_service::{
    CertificateDns01Context, CertificateDns01PresenterPort, CertificateDns01Selector,
    CertificateIssuancePort, CertificateIssuanceRequest,
};
use sdkwork_webserver_acme_service::{
    AcmeAccountStore, AcmeConfig, AcmeDns01Context, AcmeHttpClientFactory, AcmeServiceError,
    CertificateIssuer, EncryptedFileAcmeAccountStore, ExtraRootsClientFactory,
    IssuedCertificateMaterial, MemoryAcmeAccountStore, PlatformVerifierClientFactory,
    DEFAULT_ACME_OPERATION_TIMEOUT_MS,
};

/// Let's Encrypt (`certType = 1`) as the engine's certificate type.
const CERT_TYPE_LETS_ENCRYPT: i32 = 1;

/// Wraps the shared ACME engine as the control plane's issuance port.
pub struct AcmeCertificateIssuance {
    issuer: CertificateIssuer,
}

#[async_trait]
impl CertificateIssuancePort for AcmeCertificateIssuance {
    async fn issue(
        &self,
        request: CertificateIssuanceRequest,
    ) -> DeployServiceResult<IssuedCertificateMaterial> {
        // The borrowed context is built here rather than carried on the request:
        // the engine wants `&dyn Dns01Presenter`, and a request that crosses an
        // `async` port boundary cannot hold that borrow.
        // The zone resolver is owned in this scope and borrowed by the
        // context: the single-zone adapter preserves the deploy-side
        // contract while the engine moves to per-identifier resolution.
        let zone_holder = request.dns01.as_ref().map(|context| {
            sdkwork_webserver_acme_service::SingleZoneResolver {
                zone_apex: context.zone_apex.clone(),
            }
        });
        let dns01 = zone_holder
            .as_ref()
            .zip(request.dns01.as_ref())
            .map(|(zones, context)| AcmeDns01Context {
                presenter: context.presenter.as_ref(),
                zones,
            });
        self.issuer
            .issue_with_challenge(
                CERT_TYPE_LETS_ENCRYPT,
                &request.hostnames,
                &request.cert_name,
                &request.key_algorithm,
                dns01,
            )
            .await
            .map_err(map_acme_error)
    }
}

/// Maps an engine error onto the control plane's error vocabulary.
///
/// A provider refusal is a `Conflict`, not an `Internal`: the CA answered, it
/// just said no, and collapsing that into "internal error" would send an operator
/// looking for a bug in our process instead of at the CA's reason.
fn map_acme_error(error: AcmeServiceError) -> DeployServiceError {
    match error {
        AcmeServiceError::Config(message) => {
            DeployServiceError::Internal(format!("ACME configuration: {message}"))
        }
        AcmeServiceError::Validation(message) => DeployServiceError::Validation(message),
        AcmeServiceError::Provider(message) => DeployServiceError::Conflict(format!(
            "certificate authority refused the order: {message}"
        )),
        AcmeServiceError::Internal(message) => DeployServiceError::Internal(message),
    }
}

/// Builds the issuance port from the environment, or `None` when the deployment
/// does not host certificates.
///
/// `None` is an honest answer and not a degraded one: a deployment that never
/// issues is expected, and the issuance worker refuses to run against a service
/// without an engine rather than reporting progress it never made.
pub fn certificate_issuance_from_env() -> Result<Option<Arc<dyn CertificateIssuancePort>>, String> {
    if env_flag_disabled("SDKWORK_DEPLOY_CERTIFICATE_ISSUANCE") {
        return Ok(None);
    }
    // Staging is the default in every environment, including production-like ones.
    // A deployment that means to issue publicly must say so explicitly, because the
    // failure mode of guessing wrong in this direction is a burned rate limit and a
    // publicly logged certificate, while guessing wrong the other way costs one
    // restart.
    let profile =
        std::env::var("SDKWORK_DEPLOY_ACME_PROFILE").unwrap_or_else(|_| "staging".to_owned());
    let use_production = match profile.trim().to_ascii_lowercase().as_str() {
        "production" | "prod" => true,
        "staging" | "stage" | "test" | "demo" | "development" | "dev" => false,
        other => {
            return Err(format!(
                "invalid SDKWORK_DEPLOY_ACME_PROFILE {other}; expected production, staging, test, demo, or development"
            ));
        }
    };
    let directory_url = std::env::var("SDKWORK_DEPLOY_ACME_DIRECTORY_URL").unwrap_or_else(|_| {
        if use_production {
            "https://acme-v02.api.letsencrypt.org/directory".to_owned()
        } else {
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned()
        }
    });
    let contact_email = std::env::var("SDKWORK_DEPLOY_ACME_CONTACT_EMAIL")
        .unwrap_or_else(|_| "admin@localhost".to_owned());
    let renew_before_days: u32 = parse_env_or("SDKWORK_DEPLOY_CERT_RENEW_BEFORE_DAYS", 30_u32)?;
    // Same variable the Web Server edge reads: an HTTP-01 token written anywhere
    // else is a token no CA will ever fetch.
    let webroot = std::env::var("SDKWORK_WEBSERVER_ACME_WEBROOT")
        .or_else(|_| std::env::var("SDKWORK_DEPLOY_ACME_WEBROOT"))
        .ok();
    let cert_root = std::env::var("SDKWORK_DEPLOY_CERT_LIVE_ROOT")
        .unwrap_or_else(|_| "/var/lib/sdkwork/deploy-certificates".to_owned());
    let operation_timeout_ms = parse_env_or(
        "SDKWORK_DEPLOY_ACME_OPERATION_TIMEOUT_MS",
        DEFAULT_ACME_OPERATION_TIMEOUT_MS,
    )?;

    // One account per CA directory, reused across restarts: a fresh account on
    // every boot would burn the CA's account-creation limit and lose the account
    // identity the CA uses for its own history.
    let account_store: Arc<dyn AcmeAccountStore> =
        match std::env::var("SDKWORK_DEPLOY_ACME_ACCOUNT_ROOT") {
            Ok(root) => {
                let master = std::env::var("SDKWORK_DEPLOY_ACME_ACCOUNT_KEY").map_err(|_| {
                    "SDKWORK_DEPLOY_ACME_ACCOUNT_ROOT is set but \
                     SDKWORK_DEPLOY_ACME_ACCOUNT_KEY is missing"
                        .to_owned()
                })?;
                Arc::new(EncryptedFileAcmeAccountStore::new(
                    std::path::PathBuf::from(root),
                    master.as_bytes(),
                ))
            }
            Err(_) => {
                tracing::warn!(
                    "SDKWORK_DEPLOY_ACME_ACCOUNT_ROOT is not set; ACME account credentials \
                     will live only in this process, so a restart creates a new CA account"
                );
                Arc::new(MemoryAcmeAccountStore::default())
            }
        };

    let config = AcmeConfig::new(
        directory_url,
        contact_email,
        renew_before_days,
        webroot,
        use_production,
    )
    .map_err(|error| format!("ACME configuration failed: {error}"))?;
    let client_factory = acme_client_factory_from_env()?;
    let issuer = CertificateIssuer::new_with_client_factory(
        config,
        cert_root,
        operation_timeout_ms,
        account_store,
        client_factory,
    )
    .map_err(|error| format!("certificate issuer bootstrap failed: {error}"))?;
    tracing::info!(use_production, "certificate issuance engine configured");
    Ok(Some(Arc::new(AcmeCertificateIssuance { issuer })))
}

/// Builds the ACME HTTP client factory, adding local trust roots when configured.
///
/// Needed whenever the directory is not served by a publicly trusted CA: an
/// internal ACME directory (step-ca, an enterprise PKI) or a local test CA such as
/// Pebble. Without a way to trust that root the engine cannot even read the
/// directory, so a private CA is unusable rather than merely inconvenient.
///
/// The file is PEM — the form every CA publishes its root in — and a file that is
/// configured but unusable is a startup failure, not a silent fallback to the public
/// trust store: falling back would turn "the path is wrong" into "the CA is
/// unreachable", which is a much harder thing to diagnose.
fn acme_client_factory_from_env() -> Result<Arc<dyn AcmeHttpClientFactory>, String> {
    let path = match std::env::var("SDKWORK_DEPLOY_ACME_TRUST_ROOTS_FILE") {
        Ok(path) if !path.trim().is_empty() => path,
        _ => return Ok(Arc::new(PlatformVerifierClientFactory)),
    };
    let pem = std::fs::read(&path)
        .map_err(|error| format!("read ACME trust roots {path} failed: {error}"))?;
    let ders = sdkwork_deploy_certificate_material::facts::certificate_ders(&pem)
        .map_err(|error| format!("parse ACME trust roots {path} failed: {error}"))?;
    let roots: Vec<rustls_pki_types::CertificateDer<'static>> = ders
        .into_iter()
        .map(rustls_pki_types::CertificateDer::from)
        .collect();
    tracing::info!(path, roots = roots.len(), "loaded ACME trust roots");
    Ok(Arc::new(ExtraRootsClientFactory::new(roots)))
}

/// Resolves the DNS-01 presenter from a single configured provider credential.
///
/// This is the deployment-level answer, and it is now the *last* one tried: the
/// host wraps it in `AccountBackedDns01PresenterResolver`, which first consults
/// the certificate's own account pin, then its zone's, then the IAM account
/// center. This resolver only sees the requests those gave up on, which is
/// correct because it knows nothing about tenants — it is the right answer for an
/// installation that owns exactly one DNS zone and never registered a cloud
/// account, and the wrong one to prefer over a credential an operator bound on
/// purpose.
///
/// Being last is also why an unconfigured host still answers "no provider" rather
/// than failing: a deployment that hosts no zone and registered no account is a
/// legitimate one, and the worker reports the missing credential honestly instead
/// of presenting a record nobody can see.
pub struct EnvDns01PresenterResolver {
    credential: sdkwork_intelligence_deploy_service::DeployDnsProviderCredential,
    zone_apex: String,
}

impl EnvDns01PresenterResolver {
    pub fn from_env() -> Result<Option<Self>, String> {
        let kind = match std::env::var("SDKWORK_DEPLOY_DNS_PROVIDER_KIND") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => return Ok(None),
        };
        let payload = std::env::var("SDKWORK_DEPLOY_DNS_PROVIDER_CREDENTIAL").map_err(|_| {
            "SDKWORK_DEPLOY_DNS_PROVIDER_KIND is set but \
             SDKWORK_DEPLOY_DNS_PROVIDER_CREDENTIAL is missing"
                .to_owned()
        })?;
        let zone_apex = std::env::var("SDKWORK_DEPLOY_DNS_ZONE_APEX").map_err(|_| {
            "SDKWORK_DEPLOY_DNS_PROVIDER_KIND is set but SDKWORK_DEPLOY_DNS_ZONE_APEX is missing; \
             the worker cannot tell which zone owns the _acme-challenge record"
                .to_owned()
        })?;
        let credential =
            sdkwork_intelligence_deploy_service::DeployDnsProviderCredential::from_secret_payload(
                kind.trim(),
                &payload,
            )
            .map_err(|error| format!("DNS provider credential is invalid: {error}"))?;
        Ok(Some(Self {
            credential,
            zone_apex: zone_apex.trim().trim_end_matches('.').to_ascii_lowercase(),
        }))
    }
}

#[async_trait]
impl CertificateDns01PresenterPort for EnvDns01PresenterResolver {
    async fn resolve(
        &self,
        selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>> {
        let name = selector
            .hostname
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        // Only names inside the configured zone can be presented. A hostname
        // outside it belongs to a zone this credential has no authority over, and
        // answering "yes" would have the worker wait on a record the provider
        // silently refused to create.
        if name != self.zone_apex && !name.ends_with(&format!(".{}", self.zone_apex)) {
            return Ok(None);
        }
        let presenter =
            sdkwork_intelligence_deploy_service::build_dns01_presenter(&self.credential, None)
                .map_err(|error| {
                    DeployServiceError::Internal(format!("DNS provider presenter failed: {error}"))
                })?;
        Ok(Some(CertificateDns01Context {
            presenter,
            zone_apex: self.zone_apex.clone(),
        }))
    }
}

fn env_flag_disabled(key: &str) -> bool {
    matches!(
        std::env::var(key).map(|value| value.trim().to_ascii_lowercase()),
        Ok(value) if matches!(value.as_str(), "0" | "false" | "off" | "disabled" | "none")
    )
}

fn parse_env_or<T>(key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(value) => value
            .parse::<T>()
            .map_err(|error| format!("invalid {key}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(format!("read {key} failed: {error}")),
    }
}

//! In-process Deploy service and runtime-publication composition.

use std::sync::Arc;

use sdkwork_database_id::{NodeLease, SnowflakeIdGenerator, SnowflakeNodeAllocator};
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_deploy_certificate_material::{
    FileKeyProvider, MaterialKeyProvider, TrustAnchorBundle,
};
use sdkwork_deploy_cloud_account_port::cloud_account_port_from_env;
use sdkwork_deploy_content_provider_port::{
    content_provider_port_from_env, website_provider_event_delivery_port_from_env,
};
use sdkwork_deploy_database_host::bootstrap_deploy_database_from_env;
use sdkwork_deploy_drive_port::deploy_drive_port_from_env;
use sdkwork_deploy_web_port::{
    DeployWebRuntimePort, SdkWebRuntimeFacade, UnconfiguredWebRuntimePort,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{
    AccountBackedDns01PresenterResolver, CertificateAuthorityAuthorizationPort,
    CertificateDns01PresenterPort, DeployRepositoryPort, DeployRuntimeAssignmentRepositoryPort,
    DeployService, RuntimePublicationService, UnconfiguredCertificateAuthorityAuthorization,
    UnconfiguredCertificateDns01Presenter,
};

mod certificate_caa;
mod certificate_issuance;
mod domain_verification;

use certificate_caa::HickoryCaaResolver;
use certificate_issuance::{certificate_issuance_from_env, EnvDns01PresenterResolver};
use domain_verification::DnsTxtDomainOwnershipVerifier;

// Exported so an integration test can install the *same* object the host installs.
// A test that assembled its own engine would prove the engine works and say nothing
// about whether this wiring reaches it — which is exactly the failure mode a
// configuration bug produces.
pub use certificate_issuance::{
    certificate_issuance_from_env as acme_issuance_from_env, AcmeCertificateIssuance,
    EnvDns01PresenterResolver as EnvCertificateDns01Presenter,
};

pub struct DeployServiceHost {
    pub service: Arc<DeployService>,
}

pub struct RuntimePublicationHost {
    pub publication: Arc<RuntimePublicationService>,
}

async fn snowflake_from_env() -> Result<(SnowflakeIdGenerator, Option<NodeLease>), String> {
    if sdkwork_deploy_core::deploy_is_production_like_environment() {
        if std::env::var("SDKWORK_DEPLOY_SNOWFLAKE_NODE_ID").is_ok() {
            return Err(
                "static SDKWORK_DEPLOY_SNOWFLAKE_NODE_ID is forbidden in production-like environments"
                    .to_owned(),
            );
        }
        let (generator, lease) =
            SnowflakeNodeAllocator::allocate_generator_from_env("sdkwork-deploy", "DEPLOY")
                .await
                .map_err(|error| {
                    format!("allocate Deploy Snowflake database node lease failed: {error}")
                })?;
        return Ok((generator, Some(lease)));
    }

    let node_id = match std::env::var("SDKWORK_DEPLOY_SNOWFLAKE_NODE_ID") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|error| format!("invalid SDKWORK_DEPLOY_SNOWFLAKE_NODE_ID: {error}"))?,
        Err(_) => 1,
    };
    SnowflakeIdGenerator::new(node_id)
        .map(|generator| (generator, None))
        .map_err(|error| error.to_string())
}

async fn repository_from_env() -> Result<Arc<DeployRepository>, String> {
    let database = bootstrap_deploy_database_from_env().await?;
    repository_from_pool(database.pool().clone()).await
}

/// Build the Deploy repository against a caller-provided database pool so the
/// platform cloud gateway can share its process-wide PostgreSQL pool.
async fn repository_from_pool(pool: DatabasePool) -> Result<Arc<DeployRepository>, String> {
    let pool = pool
        .as_postgres()
        .cloned()
        .ok_or_else(|| "Deploy authoritative database must use PostgreSQL".to_owned())?;
    let (id_generator, node_lease) = snowflake_from_env().await?;
    let secret_key = secret_key_from_env()?;
    Ok(Arc::new(match node_lease {
        Some(node_lease) => {
            DeployRepository::new_with_node_lease(pool, id_generator, node_lease, secret_key)
        }
        None => DeployRepository::new(pool, id_generator, secret_key),
    }))
}

/// AES-256 key derivation for secrets at rest, aligned with the Web Server
/// repository contract: production-like environments require
/// `SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY`; development falls back to a
/// derived constant with a warning so local runs stay functional.
fn secret_key_from_env() -> Result<[u8; 32], String> {
    let production_like = sdkwork_deploy_core::deploy_is_production_like_environment();
    let raw = match std::env::var("SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY") {
        Ok(value) => value,
        Err(_) if !production_like => {
            tracing::warn!(
                "SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY missing; using development-only derived key"
            );
            "sdkwork-deploy-development-secret-key".to_string()
        }
        Err(_) => {
            return Err(
                "SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY is required in production-like environments"
                    .to_string(),
            );
        }
    };
    Ok(sdkwork_utils_rust::crypto::derive_aes_256_key(
        raw.as_bytes(),
        b"sdkwork-deploy-env",
        b"env-variable-encryption",
    ))
}

fn web_runtime_from_env() -> Result<Arc<dyn DeployWebRuntimePort>, String> {
    match SdkWebRuntimeFacade::from_env() {
        Ok(facade) => Ok(Arc::new(facade)),
        Err(error) if sdkwork_deploy_core::deploy_is_production_like_environment() => {
            Err(format!("configure Web runtime publication failed: {error}"))
        }
        Err(_) => Ok(Arc::new(UnconfiguredWebRuntimePort)),
    }
}

/// CAA resolver from the host's DNS configuration.
///
/// A host that cannot load a resolver keeps serving: CAA observations are then
/// recorded as unevaluable and enforcement stays with the CA. That is a
/// deliberate degradation, not an authorization — the alternative is refusing
/// every certificate order on a machine whose resolver configuration is
/// unreadable, which is a far larger outage than the CA-side check it replaces.
/// Production-like environments still fail closed on the configuration itself so
/// a broken deployment cannot hide behind the fallback.
fn certificate_caa_from_env() -> Result<Arc<dyn CertificateAuthorityAuthorizationPort>, String> {
    match HickoryCaaResolver::from_system_config() {
        Ok(resolver) => Ok(Arc::new(resolver)),
        Err(error) if sdkwork_deploy_core::deploy_is_production_like_environment() => {
            Err(format!("configure CAA resolver failed: {error}"))
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "CAA resolver unavailable; CAA checks will be recorded as unevaluable"
            );
            Ok(Arc::new(UnconfiguredCertificateAuthorityAuthorization))
        }
    }
}

pub async fn bootstrap_runtime_publication_host_from_env() -> Result<RuntimePublicationHost, String>
{
    let repository = repository_from_env().await?;
    let runtime_repository = repository as Arc<dyn DeployRuntimeAssignmentRepositoryPort>;
    Ok(RuntimePublicationHost {
        publication: Arc::new(RuntimePublicationService::new_with_provider_event_delivery(
            runtime_repository,
            web_runtime_from_env()?,
            website_provider_event_delivery_port_from_env()?,
        )),
    })
}

/// Master key for certificate material at rest, loaded from a protected file.
///
/// Only the **path** is configuration. The key material itself is read off disk
/// and never passes through an environment variable, a command line, or the
/// database — `ADR-20260723` §4 forbids all three, and for good reason: an
/// environment value is visible to every process listing and container inspect,
/// and a key stored in the database it protects protects nothing.
///
/// Returns `None` when no path is configured, which leaves the service unable to
/// store certificate material at all. That is the honest outcome: the alternative
/// is keeping private keys in the clear. Production-like environments refuse to
/// start instead, so a misconfigured deployment cannot hide behind the fallback.
fn certificate_material_key_from_env() -> Result<Option<Arc<dyn MaterialKeyProvider>>, String> {
    let path = match std::env::var("SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE") {
        Ok(path) if !path.trim().is_empty() => path,
        Ok(_) => {
            return if sdkwork_deploy_core::deploy_is_production_like_environment() {
                Err("SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE must name a key file".to_owned())
            } else {
                warn_certificate_material_unavailable("the master key path is empty");
                Ok(None)
            };
        }
        Err(_) => {
            return if sdkwork_deploy_core::deploy_is_production_like_environment() {
                Err(
                    "SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE is required in production-like \
                     environments"
                        .to_owned(),
                )
            } else {
                warn_certificate_material_unavailable(
                    "SDKWORK_DEPLOY_CERTIFICATE_MASTER_KEY_FILE is not set",
                );
                Ok(None)
            };
        }
    };
    match FileKeyProvider::from_file(&path) {
        Ok(provider) => Ok(Some(Arc::new(provider))),
        // A configured-but-unusable key is never a degradation: the operator
        // asked for custody and would otherwise get a silently weaker service.
        Err(error) => Err(format!("load certificate master key failed: {error}")),
    }
}

fn warn_certificate_material_unavailable(reason: &str) {
    tracing::warn!(
        reason,
        "certificate material custody is not configured; storing an issued certificate version \
         will fail with an internal error until a master key is provided"
    );
}

/// Local trust anchors, used to resolve the root of a chain that stops at an
/// intermediate.
///
/// Optional even in production: a chain that already ends at a self-signed
/// certificate needs no bundle. But when an operator points at a file, a file
/// that cannot be read is a startup failure — silently running without anchors
/// would turn every Let's Encrypt issuance into a request-time refusal with no
/// hint as to why.
fn certificate_trust_anchors_from_env() -> Result<Option<Arc<TrustAnchorBundle>>, String> {
    let path = match std::env::var("SDKWORK_DEPLOY_TRUST_ANCHOR_FILE") {
        Ok(path) if !path.trim().is_empty() => path,
        _ => return Ok(None),
    };
    let pem = std::fs::read(&path)
        .map_err(|error| format!("read trust anchor bundle {path} failed: {error}"))?;
    let anchors = TrustAnchorBundle::from_pem(&pem)
        .map_err(|error| format!("parse trust anchor bundle {path} failed: {error}"))?;
    tracing::info!(
        path,
        anchors = anchors.len(),
        "loaded certificate trust anchors"
    );
    Ok(Some(Arc::new(anchors)))
}

pub async fn bootstrap_deploy_service_host_from_env() -> Result<DeployServiceHost, String> {
    let database = bootstrap_deploy_database_from_env().await?;
    bootstrap_deploy_service_host_with_pool(database.pool().clone()).await
}

/// Assemble the Deploy service host against a caller-provided database pool so
/// the platform cloud gateway can share its process-wide PostgreSQL pool.
pub async fn bootstrap_deploy_service_host_with_pool(
    pool: DatabasePool,
) -> Result<DeployServiceHost, String> {
    // Read before `repository_from_pool` consumes the handle. The account center
    // lives in the same schema the platform gateway already owns, so the port takes
    // the pool rather than a URL — and it takes a *clone* because the repository
    // needs the original to keep its own connections.
    let account_pool = pool.as_postgres().cloned();
    let repository = repository_from_pool(pool).await?;
    let service_repository = repository.clone() as Arc<dyn DeployRepositoryPort>;
    let runtime_repository = repository as Arc<dyn DeployRuntimeAssignmentRepositoryPort>;
    let runtime_publication = Arc::new(RuntimePublicationService::new(
        runtime_repository,
        web_runtime_from_env()?,
    ));
    let cloud_accounts = cloud_account_port_from_env(account_pool)?;
    let mut service = DeployService::new_with_runtime_publication(
        service_repository.clone(),
        deploy_drive_port_from_env()?,
        content_provider_port_from_env()?,
        Arc::new(DnsTxtDomainOwnershipVerifier::from_system_config()?),
        runtime_publication,
    )
    .with_certificate_caa_authorization(certificate_caa_from_env()?)
    .with_cloud_accounts(cloud_accounts.clone());
    if let Some(key_provider) = certificate_material_key_from_env()? {
        service = service.with_certificate_material_key_provider(key_provider);
    }
    if let Some(anchors) = certificate_trust_anchors_from_env()? {
        service = service.with_certificate_trust_anchors(anchors);
    }
    // The issuance engine and the DNS-01 resolver are separate decisions, and both
    // are allowed to be absent. A deployment can host certificates without owning a
    // DNS zone (HTTP-01 only, exact names), and it can own a zone while not issuing.
    // Wiring them independently keeps "no DNS provider configured" from reading as
    // "certificates are unsupported here".
    if let Some(issuance) = certificate_issuance_from_env()? {
        service = service.with_certificate_issuance(issuance);
    }
    // The DNS-01 chain, in the order an issuance falls through it: the certificate's
    // account pin, its zone's pin, the account the center picks for the zone's
    // declared provider, then this deployment's own credential. The deployment-level
    // resolver is built first because it is the one that is allowed to be absent;
    // wrapping it keeps the single-zone installation that never registered a cloud
    // account working unchanged, while giving every installation that did register
    // one the narrower answer. An `Unconfigured` account port is not an error here:
    // it makes the middle two steps decline, which is exactly what an installation
    // with no account center should do.
    let deployment_dns01: Arc<dyn CertificateDns01PresenterPort> =
        match EnvDns01PresenterResolver::from_env()? {
            Some(presenter) => Arc::new(presenter),
            None => Arc::new(UnconfiguredCertificateDns01Presenter),
        };
    service = service.with_certificate_dns01_presenter(Arc::new(
        AccountBackedDns01PresenterResolver::new(
            service_repository,
            cloud_accounts,
            deployment_dns01,
        ),
    ));
    Ok(DeployServiceHost {
        service: Arc::new(service),
    })
}

/// Bootstraps the Deploy repository for executor-style workers (build runner)
/// that need direct repository port access without the full service host.
pub async fn bootstrap_deploy_repository_from_env() -> Result<Arc<DeployRepository>, String> {
    repository_from_env().await
}

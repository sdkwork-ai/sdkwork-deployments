use sdkwork_database_id::{NodeLease, SnowflakeIdGenerator};
use sqlx::PgPool;

mod app_composition;
mod app_deployments;
mod app_releases;
mod apps;
mod artifacts;
mod audit;
mod builds;
mod certificate_renewals;
mod certificates;
mod database_profiles;
mod domain_zones;
mod entitlement;
mod env_variables;
mod environments;
mod health_checks;
mod nginx_configs;
mod nginx_orchestrator;
mod nginx_security;
mod node_clusters;
mod platform_app_domains;
mod port;
mod retention;
mod runtime_assignments;
mod servers;

mod source_events;
mod support;
mod tls_control;
mod upload_sessions;
mod usage;

#[derive(Clone)]
pub struct DeployRepository {
    pool: PgPool,
    id_generator: SnowflakeIdGenerator,
    secret_key: [u8; 32],
    _node_lease: Option<NodeLease>,
}

impl DeployRepository {
    pub fn new(pool: PgPool, id_generator: SnowflakeIdGenerator, secret_key: [u8; 32]) -> Self {
        Self {
            pool,
            id_generator,
            secret_key,
            _node_lease: None,
        }
    }

    pub fn new_with_node_lease(
        pool: PgPool,
        id_generator: SnowflakeIdGenerator,
        node_lease: NodeLease,
        secret_key: [u8; 32],
    ) -> Self {
        Self {
            pool,
            id_generator,
            secret_key,
            _node_lease: Some(node_lease),
        }
    }

    /// Read-only constructor for consumers that only resolve deployed sites
    /// (for example the Web Server app-domain fallback). Ids minted through
    /// this handle are process-local placeholders; write provisioning flows
    /// belong to the control plane process.
    pub fn new_lookup(pool: PgPool) -> Self {
        Self::new(
            pool,
            SnowflakeIdGenerator::new(0).expect("lookup snowflake node id"),
            [0u8; 32],
        )
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn id_generator(&self) -> &SnowflakeIdGenerator {
        &self.id_generator
    }

    /// AES-256 key used to protect environment-variable secrets at rest
    /// (derived from `SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY` at bootstrap).
    pub fn secret_key(&self) -> &[u8; 32] {
        &self.secret_key
    }
}

//! Which account center this process talks to.
//!
//! The shape follows `sdkwork-deploy-drive-port::selection`: a pure decision
//! function that is trivially testable, and a thin `from_env` wrapper that reads
//! the environment and then refuses to degrade silently.
//!
//! The one difference is that this port's real adapter is an in-process Rust
//! dependency on IAM rather than an HTTP facade, so the "configuration" that can
//! be missing is the database handle, not a URL. `database_available` is what the
//! assembly point actually knows: the platform gateway hands Deploy its
//! process-wide PostgreSQL pool, and IAM's account center lives in the same
//! schema there.

use std::sync::Arc;

use sdkwork_utils_rust::parse_bool;
use sqlx::PgPool;

#[cfg(test)]
use crate::ListCloudAccountsCommand;
use crate::{
    DeployCloudAccountPort, DeployCloudAccountPortAdapter, IamCloudAccountPort,
    MemoryCloudAccountPort,
};

/// Opt out of the real account center and use the in-memory one.
///
/// Unlike the other Deploy ports this variable is *not* how local development
/// reaches the real thing — a development stack has a pool, so the account center
/// is selected by default and this flag is the exception. It exists for tests and
/// for smoke runs that must not touch a shared account center.
pub const USE_MEMORY_CLOUD_ACCOUNTS_ENV: &str = "SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS";

/// Everything the decision depends on. No environment reads inside, so the whole
/// matrix is exercised by ordinary unit tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeployCloudAccountPortSelectionInput {
    /// `None` when the variable is absent or not a boolean.
    pub use_memory_accounts: Option<bool>,
    /// Whether a PostgreSQL pool reached the assembly point.
    pub database_available: bool,
    pub production_like: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeployCloudAccountPortSelection {
    Iam,
    Memory,
    Unconfigured,
}

pub fn select_deploy_cloud_account_port(
    input: DeployCloudAccountPortSelectionInput,
) -> DeployCloudAccountPortSelection {
    // Memory is an explicit opt-in, and production never gets it even when asked.
    // Folding "asked for memory in production" into `Unconfigured` rather than into
    // an error keeps the two production failures distinguishable at the call site:
    // "you asked for a fake account center where that is forbidden" reads very
    // differently from "no pool reached the assembler".
    if input.use_memory_accounts == Some(true) {
        return if input.production_like {
            DeployCloudAccountPortSelection::Unconfigured
        } else {
            DeployCloudAccountPortSelection::Memory
        };
    }
    if input.database_available {
        return DeployCloudAccountPortSelection::Iam;
    }
    DeployCloudAccountPortSelection::Unconfigured
}

/// Builds the account center port this process should use.
///
/// `pool` is the same handle the caller gave the Deploy repository, so the account
/// center reads the one schema the platform gateway already owns.
pub fn cloud_account_port_from_env(
    pool: Option<PgPool>,
) -> Result<Arc<dyn DeployCloudAccountPort>, String> {
    let use_memory_accounts = std::env::var(USE_MEMORY_CLOUD_ACCOUNTS_ENV)
        .ok()
        .and_then(|value| parse_bool(&value));
    let production_like = sdkwork_deploy_core::deploy_is_production_like_environment();
    let adapter = match select_deploy_cloud_account_port(DeployCloudAccountPortSelectionInput {
        use_memory_accounts,
        database_available: pool.is_some(),
        production_like,
    }) {
        DeployCloudAccountPortSelection::Memory => {
            DeployCloudAccountPortAdapter::Memory(MemoryCloudAccountPort::new())
        }
        DeployCloudAccountPortSelection::Iam => match pool {
            Some(pool) => DeployCloudAccountPortAdapter::Iam(IamCloudAccountPort::new(pool)),
            // Unreachable: the selection only returns `Iam` when a pool exists.
            // Spelled out rather than unwrapped so a future rule change degrades to
            // an honest "not configured" instead of a panic at start-up.
            None => DeployCloudAccountPortAdapter::Unconfigured,
        },
        DeployCloudAccountPortSelection::Unconfigured if production_like => {
            return Err(format!(
                "production Deploy requires the IAM cloud account center: unset \
                 {USE_MEMORY_CLOUD_ACCOUNTS_ENV} (or set it to false) and assemble the service host \
                 against a PostgreSQL pool so DNS provider credentials resolve through IAM rather \
                 than the deprecated deploy_dns_provider_credential table"
            ));
        }
        DeployCloudAccountPortSelection::Unconfigured => {
            DeployCloudAccountPortAdapter::Unconfigured
        }
    };
    Ok(Arc::new(adapter))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        use_memory_accounts: Option<bool>,
        database_available: bool,
        production_like: bool,
    ) -> DeployCloudAccountPortSelectionInput {
        DeployCloudAccountPortSelectionInput {
            use_memory_accounts,
            database_available,
            production_like,
        }
    }

    #[test]
    fn production_never_selects_memory_accounts() {
        // Every combination, not just the opt-in one: a production process must not
        // be able to reach a fake account center by any route, including a stray
        // `SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=1` left over from a dev env file.
        for use_memory_accounts in [None, Some(true), Some(false)] {
            for database_available in [false, true] {
                assert_ne!(
                    select_deploy_cloud_account_port(input(
                        use_memory_accounts,
                        database_available,
                        true
                    )),
                    DeployCloudAccountPortSelection::Memory
                );
            }
        }
        assert_eq!(
            select_deploy_cloud_account_port(input(Some(true), true, true)),
            DeployCloudAccountPortSelection::Unconfigured,
            "an explicit ask for memory is refused in production, not honoured"
        );
    }

    #[test]
    fn production_selects_the_account_center_when_it_can_reach_one() {
        assert_eq!(
            select_deploy_cloud_account_port(input(None, true, true)),
            DeployCloudAccountPortSelection::Iam
        );
        assert_eq!(
            select_deploy_cloud_account_port(input(Some(false), true, true)),
            DeployCloudAccountPortSelection::Iam
        );
    }

    #[test]
    fn production_without_a_pool_is_unconfigured() {
        assert_eq!(
            select_deploy_cloud_account_port(input(Some(false), false, true)),
            DeployCloudAccountPortSelection::Unconfigured
        );
    }

    #[test]
    fn a_development_stack_defaults_to_the_real_account_center() {
        // The co-located case: dev has a pool, so the console lists actual accounts
        // without anyone setting a variable. Memory stays opt-in precisely so an
        // empty listing means "this tenant has none", never "nobody wired the port".
        assert_eq!(
            select_deploy_cloud_account_port(input(None, true, false)),
            DeployCloudAccountPortSelection::Iam
        );
    }

    #[test]
    fn development_can_opt_into_memory_and_falls_back_when_nothing_is_wired() {
        assert_eq!(
            select_deploy_cloud_account_port(input(Some(true), true, false)),
            DeployCloudAccountPortSelection::Memory
        );
        assert_eq!(
            select_deploy_cloud_account_port(input(None, false, false)),
            DeployCloudAccountPortSelection::Unconfigured
        );
        assert_eq!(
            select_deploy_cloud_account_port(input(Some(true), false, false)),
            DeployCloudAccountPortSelection::Memory
        );
    }

    #[tokio::test]
    async fn an_unconfigured_port_reports_unavailable_rather_than_an_empty_tenant() {
        // The port is allowed to be absent in development. What it must not do is
        // answer "no accounts", which an operator would read as a tenant fact and
        // which would hide a wiring fault behind a plausible-looking empty list.
        let port = DeployCloudAccountPortAdapter::Unconfigured;
        let error = port
            .list_accounts(ListCloudAccountsCommand {
                tenant_id: 1_000_001,
                page: 1,
                page_size: 20,
                ..Default::default()
            })
            .await
            .expect_err("an unconfigured account center must fail loudly");
        let message = format!("{error:?}");
        assert!(
            message.contains("not configured"),
            "unexpected error: {message}"
        );
    }
}

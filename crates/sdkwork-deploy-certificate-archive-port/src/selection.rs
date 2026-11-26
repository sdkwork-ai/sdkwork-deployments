//! Which archive store this deployment uses, from `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE`.
//!
//! The switch is deliberately *not* inferred from the Drive mode. Drive is
//! configured in every deployment that has an in-process port, but writing a
//! tenant's certificates into a space is a product decision an operator has to
//! make, and the design fixes the default as `off` (§7): unset means
//! `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE=off`, which stores each version's archive row
//! as `SKIPPED` — "nobody asked for this" rather than a failure an operator would
//! chase, and never a success that did not happen.
//!
//! * `off` → [`CertificateArchiveSelection::Off`]: rows are inserted `SKIPPED`.
//! * `drive` → [`CertificateArchiveSelection::Drive`]: rows are inserted `PENDING`
//!   with the configured bundle policy, and the in-process store writes them.
//! * `memory` → [`CertificateArchiveSelection::Memory`]: the test double. Refused
//!   in production-like environments, like every other in-memory port here.
//!
//! Anything else is a startup error: a typo must not silently leave archiving off
//! (which would look like "the feature does nothing") or on.

use std::sync::Arc;

use sdkwork_deploy_contract::deploy_is_production_like_environment;
use sqlx::PgPool;

use crate::layout::ArchiveBundleKind;
use crate::memory::MemoryCertificateArchiveStore;
use crate::{in_process::InProcessCertificateArchiveStore, CertificateArchiveStore};

/// `off` (default) | `drive` | `memory` (dev only).
pub const CERTIFICATE_ARCHIVE_ENV: &str = "SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE";

/// `PUBLIC` (default) | `FULL` (refused today).
pub const CERTIFICATE_ARCHIVE_BUNDLE_ENV: &str = "SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_BUNDLE";

/// Display name override for the tenant's certificate space.
pub const CERTIFICATE_ARCHIVE_SPACE_NAME_ENV: &str = "SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_SPACE_NAME";

/// Attempt ceiling before an archive row becomes `FAILED`.
pub const CERTIFICATE_ARCHIVE_MAX_ATTEMPTS_ENV: &str =
    "SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_MAX_ATTEMPTS";

/// Wall-clock ceiling for the attempt made inside an issuance.
pub const CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_ENV: &str =
    "SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_MILLIS";

/// Per-issuance attempt ceiling, in milliseconds (design §4.6.3: 30s).
pub const DEFAULT_ATTEMPT_TIMEOUT_MILLIS: u64 = 30_000;

/// Lower bound for the attempt ceiling.
///
/// Below a second the attempt would time out before Drive could even resolve a
/// storage provider, so every version would need the worker to finish it.
pub const MINIMUM_ATTEMPT_TIMEOUT_MILLIS: u64 = 1_000;

/// Upper bound for the attempt ceiling.
///
/// The issuance worker's lease defaults to 900 s and must outlast the CA
/// conversation; an archive attempt that could run for ten minutes inside it
/// would compete with that invariant.
pub const MAXIMUM_ATTEMPT_TIMEOUT_MILLIS: u64 = 120_000;

/// The parsed switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertificateArchiveSelection {
    /// Archiving is switched off; rows are written `SKIPPED`.
    Off,
    /// Archiving runs through the in-process Drive store.
    Drive {
        bundle: ArchiveBundleKind,
        space_name: String,
    },
    /// The in-memory test double (development only).
    Memory {
        bundle: ArchiveBundleKind,
        space_name: String,
    },
}

impl CertificateArchiveSelection {
    /// Whether a version's archive row starts as `PENDING`.
    #[must_use]
    pub fn archives(&self) -> bool {
        !matches!(self, Self::Off)
    }

    /// The bundle policy, defaulting to `PUBLIC` when archiving is off.
    #[must_use]
    pub fn bundle(&self) -> ArchiveBundleKind {
        match self {
            Self::Off => ArchiveBundleKind::Public,
            Self::Drive { bundle, .. } | Self::Memory { bundle, .. } => *bundle,
        }
    }

    /// The space display name used when the space is first created.
    #[must_use]
    pub fn space_name(&self) -> &str {
        match self {
            Self::Off => crate::layout::CERTIFICATE_SPACE_NAME,
            Self::Drive { space_name, .. } | Self::Memory { space_name, .. } => space_name,
        }
    }
}

/// The parsed, validated runtime configuration of the archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateArchiveConfig {
    pub selection: CertificateArchiveSelection,
    pub retry: crate::retry::ArchiveRetryPolicy,
    pub attempt_timeout_millis: u64,
}

/// Resolves the switch.
///
/// # Errors
///
/// An unrecognised mode, an unsupported bundle policy, or a production-like
/// deployment asking for the in-memory store.
pub fn select_certificate_archive(
    mode: Option<&str>,
    bundle: Option<&str>,
    space_name: Option<&str>,
    production_like: bool,
) -> Result<CertificateArchiveSelection, String> {
    let mode = mode
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "off".to_owned());
    // Validated even when archiving is off, so a typo surfaces at startup rather
    // than on the day somebody flips the switch.
    let bundle = match bundle
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .as_deref()
    {
        None | Some("PUBLIC") => ArchiveBundleKind::Public,
        Some("FULL") => ArchiveBundleKind::Full,
        Some(other) => {
            return Err(format!(
                "{CERTIFICATE_ARCHIVE_BUNDLE_ENV} must be PUBLIC or FULL (got {other})"
            ))
        }
    };
    let space_name = space_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(crate::layout::CERTIFICATE_SPACE_NAME)
        .to_owned();
    match mode.as_str() {
        "off" | "false" | "0" | "disabled" | "none" => Ok(CertificateArchiveSelection::Off),
        "drive" | "on" | "true" | "1" => Ok(CertificateArchiveSelection::Drive {
            bundle,
            space_name,
        }),
        "memory" if production_like => Err(format!(
            "production-like environments cannot set {CERTIFICATE_ARCHIVE_ENV}=memory"
        )),
        "memory" => Ok(CertificateArchiveSelection::Memory {
            bundle,
            space_name,
        }),
        other => Err(format!(
            "{CERTIFICATE_ARCHIVE_ENV} must be drive, off, or memory (got {other})"
        )),
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn parse_u64(name: &str, default: u64) -> Result<u64, String> {
    match env(name) {
        None => Ok(default),
        Some(value) if value.trim().is_empty() => Ok(default),
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|error| format!("invalid {name}: {error}")),
    }
}

/// Reads and validates the archive configuration from the environment.
///
/// # Errors
///
/// Any validation failure, including a `FULL` bundle policy. `FULL` is refused at
/// startup rather than at the first issuance: an operator who asked for the
/// private key to be archived must find out before the first certificate is
/// issued, not after a version has been stored with a policy that could not be
/// applied.
pub fn certificate_archive_config_from_env() -> Result<CertificateArchiveConfig, String> {
    let selection = select_certificate_archive(
        env(CERTIFICATE_ARCHIVE_ENV).as_deref(),
        env(CERTIFICATE_ARCHIVE_BUNDLE_ENV).as_deref(),
        env(CERTIFICATE_ARCHIVE_SPACE_NAME_ENV).as_deref(),
        deploy_is_production_like_environment(),
    )?;
    if selection.archives() && selection.bundle() == ArchiveBundleKind::Full {
        return Err(format!(
            "{CERTIFICATE_ARCHIVE_BUNDLE_ENV}=FULL is not supported: {}",
            crate::bundle_policy::private_key_refusal_detail()
        ));
    }
    let max_attempts = parse_u64(
        CERTIFICATE_ARCHIVE_MAX_ATTEMPTS_ENV,
        u64::try_from(crate::retry::DEFAULT_MAX_ATTEMPTS).unwrap_or(5),
    )?;
    let max_attempts = i32::try_from(max_attempts)
        .map_err(|_| format!("{CERTIFICATE_ARCHIVE_MAX_ATTEMPTS_ENV} is out of range"))?;
    let retry = crate::retry::ArchiveRetryPolicy::new(max_attempts).map_err(|error| {
        format!("{CERTIFICATE_ARCHIVE_MAX_ATTEMPTS_ENV}: {error}")
    })?;
    let attempt_timeout_millis = parse_u64(
        CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_ENV,
        DEFAULT_ATTEMPT_TIMEOUT_MILLIS,
    )?;
    if !(MINIMUM_ATTEMPT_TIMEOUT_MILLIS..=MAXIMUM_ATTEMPT_TIMEOUT_MILLIS)
        .contains(&attempt_timeout_millis)
    {
        return Err(format!(
            "{CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_ENV} must be between \
             {MINIMUM_ATTEMPT_TIMEOUT_MILLIS} and {MAXIMUM_ATTEMPT_TIMEOUT_MILLIS} milliseconds"
        ));
    }
    Ok(CertificateArchiveConfig {
        selection,
        retry,
        attempt_timeout_millis,
    })
}

/// Builds the store the selection names, or `None` when archiving is off.
///
/// `None` is not a failure: the service then records every version's archive row as
/// `SKIPPED`, which is the honest report for a deployment that does not archive.
///
/// # Errors
///
/// Never today — the in-process store needs no endpoint — but kept as a `Result`
/// so a future store that does validate its configuration cannot silently fall
/// back to `None` and report `SKIPPED` for a deployment that asked for archiving.
pub fn certificate_archive_store_from_selection(
    pool: &PgPool,
    selection: &CertificateArchiveSelection,
) -> Result<Option<Arc<dyn CertificateArchiveStore>>, String> {
    match selection {
        CertificateArchiveSelection::Off => Ok(None),
        CertificateArchiveSelection::Drive { .. } => Ok(Some(Arc::new(
            InProcessCertificateArchiveStore::new(pool.clone()),
        ))),
        CertificateArchiveSelection::Memory { .. } => {
            Ok(Some(Arc::new(MemoryCertificateArchiveStore::configured())))
        }
    }
}

/// Reads the switch from the environment and builds the store in one step.
pub fn certificate_archive_store_from_env(
    pool: &PgPool,
) -> Result<Option<Arc<dyn CertificateArchiveStore>>, String> {
    let config = certificate_archive_config_from_env()?;
    certificate_archive_store_from_selection(pool, &config.selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_switch_means_off_not_a_quiet_default() {
        // Design §7: unset is `off`, and `off` is `SKIPPED` — never a success that
        // did not happen.
        assert_eq!(
            select_certificate_archive(None, None, None, true).expect("selection"),
            CertificateArchiveSelection::Off
        );
        assert!(!CertificateArchiveSelection::Off.archives());
        assert_eq!(
            select_certificate_archive(Some("  "), None, None, false).expect("selection"),
            CertificateArchiveSelection::Off
        );
    }

    #[test]
    fn drive_is_an_explicit_opt_in_with_public_by_default() {
        let selection =
            select_certificate_archive(Some("drive"), None, None, true).expect("selection");
        assert_eq!(
            selection,
            CertificateArchiveSelection::Drive {
                bundle: ArchiveBundleKind::Public,
                space_name: "证书库".to_owned(),
            }
        );
        assert_eq!(selection.space_name(), "证书库");
        assert!(selection.archives());
    }

    #[test]
    fn the_space_name_can_be_overridden() {
        let selection = select_certificate_archive(Some("drive"), None, Some("Certificates"), false)
            .expect("selection");
        assert_eq!(selection.space_name(), "Certificates");
    }

    #[test]
    fn full_is_parsed_but_never_implied() {
        let selection =
            select_certificate_archive(Some("drive"), Some("full"), None, true).expect("selection");
        assert_eq!(selection.bundle(), ArchiveBundleKind::Full);
        assert_eq!(
            select_certificate_archive(Some("drive"), Some("private"), None, true).is_err(),
            true
        );
    }

    #[test]
    fn unknown_modes_are_rejected_rather_than_defaulting() {
        for mode in ["s3", "drve", "enabled"] {
            assert!(
                select_certificate_archive(Some(mode), None, None, false).is_err(),
                "{mode} must be refused"
            );
        }
    }

    #[test]
    fn production_never_selects_the_memory_store() {
        assert!(select_certificate_archive(Some("memory"), None, None, true).is_err());
        assert!(select_certificate_archive(Some("memory"), None, None, false).is_ok());
    }

    #[test]
    fn the_attempt_timeout_bounds_are_sane_against_the_worker_lease() {
        // The issuance worker's lease floor is 30 s; an attempt ceiling that could
        // consume minutes of it would compete with the CA conversation.
        assert_eq!(DEFAULT_ATTEMPT_TIMEOUT_MILLIS, 30_000);
        assert!(MINIMUM_ATTEMPT_TIMEOUT_MILLIS < DEFAULT_ATTEMPT_TIMEOUT_MILLIS);
        assert!(MAXIMUM_ATTEMPT_TIMEOUT_MILLIS < 30 * 60 * 1_000);
    }

    #[tokio::test]
    async fn off_never_builds_a_store() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://deploy:deploy@127.0.0.1:1/deploy")
            .expect("lazy pool");
        let store = certificate_archive_store_from_selection(
            &pool,
            &CertificateArchiveSelection::Off,
        )
        .expect("selection");
        assert!(store.is_none());
        let store = certificate_archive_store_from_selection(
            &pool,
            &CertificateArchiveSelection::Memory {
                bundle: ArchiveBundleKind::Public,
                space_name: "证书库".to_owned(),
            },
        )
        .expect("selection")
        .expect("store");
        assert!(store.is_configured());
    }
}

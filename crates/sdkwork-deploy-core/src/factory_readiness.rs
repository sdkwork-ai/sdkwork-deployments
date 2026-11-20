//! Derived per-stage factory readiness for one deploy application.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3 forbids storing a `READY`
//! verdict: readiness is derived from the rows that exist (platform targets, the
//! source, the installation status, the domain plan). Section 11's runbook is
//! then a walk over these six checks instead of a list an operator ticks off by
//! hand.
//!
//! The module is deliberately pure. The repository loads the facts, this module
//! decides, and the service attaches the report to the response, so the rule can
//! be tested without a database.
//!
//! The hosting carrier is labelled `EXTERNAL`: it lives in `sdkwork-webserver`,
//! and deploy may not read that module's tables. Deploy can only say whether it
//! has published the default-domain plan the carrier is expected to follow; the
//! carrier's own anchor is checked by `check-application-factory-standard.mjs`.

pub const READINESS_STATUS_READY: &str = "READY";
pub const READINESS_STATUS_PENDING: &str = "PENDING";
pub const READINESS_STATUS_BLOCKED: &str = "BLOCKED";
pub const READINESS_STATUS_NOT_APPLICABLE: &str = "NOT_APPLICABLE";
pub const READINESS_STATUS_EXTERNAL: &str = "EXTERNAL";

/// The stages, in factory order. `as_str` is the value the contract serializes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactoryStage {
    Registration,
    PlatformTarget,
    Environment,
    Identity,
    Source,
    HostingCarrier,
}

impl FactoryStage {
    pub const ALL: [Self; 6] = [
        Self::Registration,
        Self::PlatformTarget,
        Self::Environment,
        Self::Identity,
        Self::Source,
        Self::HostingCarrier,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Registration => "REGISTRATION",
            Self::PlatformTarget => "PLATFORM_TARGET",
            Self::Environment => "ENVIRONMENT",
            Self::Identity => "IDENTITY",
            Self::Source => "SOURCE",
            Self::HostingCarrier => "HOSTING_CARRIER",
        }
    }
}

/// One derived stage verdict. Strings rather than the wire type keep this crate
/// independent of `sdkwork-deploy-contract`; the contract converts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoryStageVerdict {
    pub stage: FactoryStage,
    pub status: &'static str,
    pub reason: Option<String>,
    pub detail: String,
}

/// The whole derived report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoryReadinessVerdict {
    pub status: &'static str,
    pub blocking_stage: Option<FactoryStage>,
    pub stages: Vec<FactoryStageVerdict>,
}

/// The facts one readiness derivation needs. Every field is loaded from a row;
/// nothing here is probabilistic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactoryReadinessFacts {
    /// `deploy_app.app_status`.
    pub app_status: String,
    /// Live (non-archived) `deploy_app_platform_target` rows.
    pub platform_target_count: i64,
    /// `deploy_app.identity_status`: `pending_identity` | `active` | `failed` |
    /// `disabled`.
    pub identity_status: String,
    /// `deploy_app_source.remix_enabled`.
    pub remix_enabled: bool,
    /// `deploy_app_source` derived readiness: `READY` | `MISSING`.
    pub source_readiness: String,
    /// The source readiness reason code when it is `MISSING`.
    pub source_reason: Option<String>,
    /// Live `deploy_app_environment` statuses.
    pub environment_statuses: Vec<String>,
    /// Whether the deploy-side default-domain plan exists (the host the carrier
    /// follows).
    pub domain_plan_published: bool,
}

/// Derives the report. `status` of the whole application is `READY` only when
/// every required step is `READY` or `EXTERNAL`; the first non-ready step in
/// factory order is reported as `blockingStep`.
#[must_use]
pub fn derive_factory_readiness(facts: &FactoryReadinessFacts) -> FactoryReadinessVerdict {
    let app_status = facts.app_status.trim().to_ascii_uppercase();
    let registration = if matches!(app_status.as_str(), "DRAFT" | "ACTIVE" | "PAUSED") {
        FactoryStageVerdict {
            stage: FactoryStage::Registration,
            status: READINESS_STATUS_READY,
            reason: None,
            detail: format!("application is registered as {app_status}"),
        }
    } else if app_status == "ARCHIVED" {
        FactoryStageVerdict {
            stage: FactoryStage::Registration,
            status: READINESS_STATUS_BLOCKED,
            reason: Some("APP_ARCHIVED".to_owned()),
            detail: "application is ARCHIVED; readiness of a retired application is not meaningful"
                .to_owned(),
        }
    } else {
        FactoryStageVerdict {
            stage: FactoryStage::Registration,
            status: READINESS_STATUS_BLOCKED,
            reason: Some("APP_STATUS_UNKNOWN".to_owned()),
            detail: format!("application status {app_status:?} is not a lifecycle status"),
        }
    };

    let platform_target = if facts.platform_target_count > 0 {
        FactoryStageVerdict {
            stage: FactoryStage::PlatformTarget,
            status: READINESS_STATUS_READY,
            reason: None,
            detail: format!(
                "{} platform target(s) registered",
                facts.platform_target_count
            ),
        }
    } else {
        FactoryStageVerdict {
            stage: FactoryStage::PlatformTarget,
            status: READINESS_STATUS_PENDING,
            reason: Some("NO_PLATFORM_TARGET".to_owned()),
            detail:
                "register at least one platform target (the kind/platform matrix decides which)"
                    .to_owned(),
        }
    };

    let active_environments = facts
        .environment_statuses
        .iter()
        .filter(|status| status.eq_ignore_ascii_case("ACTIVE"))
        .count();
    let environment = if facts.environment_statuses.is_empty() {
        FactoryStageVerdict {
            stage: FactoryStage::Environment,
            status: READINESS_STATUS_PENDING,
            reason: Some("NO_ENVIRONMENT".to_owned()),
            detail: "declare at least one lifecycle environment before promoting a release"
                .to_owned(),
        }
    } else if active_environments == 0 {
        FactoryStageVerdict {
            stage: FactoryStage::Environment,
            status: READINESS_STATUS_PENDING,
            reason: Some("NO_ACTIVE_ENVIRONMENT".to_owned()),
            detail: format!(
                "{} environment row(s) exist but none is ACTIVE",
                facts.environment_statuses.len()
            ),
        }
    } else {
        FactoryStageVerdict {
            stage: FactoryStage::Environment,
            status: READINESS_STATUS_READY,
            reason: None,
            detail: format!("{active_environments} active environment(s)"),
        }
    };

    let identity_status = facts.identity_status.trim().to_ascii_lowercase();
    let identity = match identity_status.as_str() {
        "active" => FactoryStageVerdict {
            stage: FactoryStage::Identity,
            status: READINESS_STATUS_READY,
            reason: None,
            detail: "IAM installation is active".to_owned(),
        },
        "pending_identity" => FactoryStageVerdict {
            stage: FactoryStage::Identity,
            status: READINESS_STATUS_PENDING,
            reason: Some("IDENTITY_PENDING".to_owned()),
            detail: "IAM installation provisioning has not completed; retry apps.identityProvision"
                .to_owned(),
        },
        "failed" => FactoryStageVerdict {
            stage: FactoryStage::Identity,
            status: READINESS_STATUS_BLOCKED,
            reason: Some("IDENTITY_FAILED".to_owned()),
            detail: "IAM installation provisioning failed; retry apps.identityProvision and read \
                     the logged cause"
                .to_owned(),
        },
        "disabled" => FactoryStageVerdict {
            stage: FactoryStage::Identity,
            status: READINESS_STATUS_BLOCKED,
            reason: Some("IDENTITY_DISABLED".to_owned()),
            detail: "IAM installation is disabled; uninstall is in progress or complete".to_owned(),
        },
        other => FactoryStageVerdict {
            stage: FactoryStage::Identity,
            status: READINESS_STATUS_BLOCKED,
            reason: Some("IDENTITY_STATUS_UNKNOWN".to_owned()),
            detail: format!("identity status {other:?} is not a known installation state"),
        },
    };

    let source = if !facts.remix_enabled {
        FactoryStageVerdict {
            stage: FactoryStage::Source,
            status: READINESS_STATUS_NOT_APPLICABLE,
            reason: None,
            detail: "remix is disabled, so no source is required".to_owned(),
        }
    } else if facts.source_readiness.eq_ignore_ascii_case("READY") {
        FactoryStageVerdict {
            stage: FactoryStage::Source,
            status: READINESS_STATUS_READY,
            reason: None,
            detail: "remix is enabled and the source is ready".to_owned(),
        }
    } else {
        FactoryStageVerdict {
            stage: FactoryStage::Source,
            status: READINESS_STATUS_BLOCKED,
            reason: Some(
                facts
                    .source_reason
                    .clone()
                    .unwrap_or_else(|| "SOURCE_NOT_READY".to_owned()),
            ),
            detail: format!(
                "remix is enabled but the source is not ready ({}); fix the reported reason \
                 before any download",
                facts
                    .source_reason
                    .as_deref()
                    .unwrap_or("SOURCE_NOT_CONFIGURED")
            ),
        }
    };

    let hosting_carrier = if facts.domain_plan_published {
        FactoryStageVerdict {
            stage: FactoryStage::HostingCarrier,
            status: READINESS_STATUS_EXTERNAL,
            reason: Some("CARRIER_ANCHOR_IS_WEBSERVER_OWNED".to_owned()),
            detail: "the deploy-side default-domain plan exists; the carrier row and TLS are \
                     verified in sdkwork-webserver"
                .to_owned(),
        }
    } else {
        FactoryStageVerdict {
            stage: FactoryStage::HostingCarrier,
            status: READINESS_STATUS_PENDING,
            reason: Some("DOMAIN_PLAN_MISSING".to_owned()),
            detail: "publish the default-domain plan before asking sdkwork-webserver to anchor a \
                     carrier"
                .to_owned(),
        }
    };

    let stages = vec![
        registration,
        platform_target,
        environment,
        identity,
        source,
        hosting_carrier,
    ];
    let blocking = stages.iter().find(|verdict| {
        !matches!(
            verdict.status,
            READINESS_STATUS_READY | READINESS_STATUS_NOT_APPLICABLE | READINESS_STATUS_EXTERNAL
        )
    });
    FactoryReadinessVerdict {
        status: if blocking.is_some() {
            READINESS_STATUS_PENDING
        } else {
            READINESS_STATUS_READY
        },
        blocking_stage: blocking.map(|verdict| verdict.stage),
        stages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_facts() -> FactoryReadinessFacts {
        FactoryReadinessFacts {
            app_status: "ACTIVE".to_owned(),
            platform_target_count: 1,
            identity_status: "active".to_owned(),
            remix_enabled: false,
            source_readiness: "MISSING".to_owned(),
            source_reason: Some("SOURCE_NOT_CONFIGURED".to_owned()),
            environment_statuses: vec!["ACTIVE".to_owned()],
            domain_plan_published: true,
        }
    }

    #[test]
    fn a_fully_provisioned_application_is_ready() {
        let readiness = derive_factory_readiness(&ready_facts());
        assert_eq!(readiness.status, READINESS_STATUS_READY);
        assert!(readiness.blocking_stage.is_none());
        assert_eq!(readiness.stages.len(), 6);
    }

    fn stage<'a>(readiness: &'a FactoryReadinessVerdict, name: &str) -> &'a FactoryStageVerdict {
        readiness
            .stages
            .iter()
            .find(|verdict| verdict.stage.as_str() == name)
            .expect("stage present")
    }

    #[test]
    fn remix_disabled_is_not_a_blocker_but_a_broken_enabled_source_is() {
        let mut facts = ready_facts();
        facts.remix_enabled = true;
        facts.source_readiness = "MISSING".to_owned();
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(readiness.status, READINESS_STATUS_PENDING);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("SOURCE")
        );
        assert_eq!(
            stage(&readiness, "SOURCE").reason.as_deref(),
            Some("SOURCE_NOT_CONFIGURED")
        );
    }

    #[test]
    fn a_pending_identity_is_reported_in_factory_order() {
        let mut facts = ready_facts();
        facts.identity_status = "pending_identity".to_owned();
        facts.platform_target_count = 0;
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("PLATFORM_TARGET")
        );
        facts.platform_target_count = 1;
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("IDENTITY")
        );
        assert_eq!(
            stage(&readiness, "IDENTITY").reason.as_deref(),
            Some("IDENTITY_PENDING")
        );
    }

    #[test]
    fn an_archived_application_can_never_be_ready() {
        let mut facts = ready_facts();
        facts.app_status = "ARCHIVED".to_owned();
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("REGISTRATION")
        );
        assert_eq!(
            stage(&readiness, "REGISTRATION").reason.as_deref(),
            Some("APP_ARCHIVED")
        );
    }

    #[test]
    fn an_environment_row_that_is_not_active_pends() {
        let mut facts = ready_facts();
        facts.environment_statuses = vec!["DRAFT".to_owned()];
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("ENVIRONMENT")
        );
    }

    #[test]
    fn hosting_carrier_is_external_and_never_blocks() {
        let readiness = derive_factory_readiness(&ready_facts());
        assert_eq!(
            stage(&readiness, "HOSTING_CARRIER").status,
            READINESS_STATUS_EXTERNAL
        );
        assert_eq!(readiness.status, READINESS_STATUS_READY);
    }

    #[test]
    fn an_unknown_identity_status_blocks_instead_of_passing() {
        let mut facts = ready_facts();
        facts.identity_status = "actve".to_owned();
        let readiness = derive_factory_readiness(&facts);
        assert_eq!(
            readiness.blocking_stage.map(FactoryStage::as_str),
            Some("IDENTITY")
        );
        assert_eq!(
            stage(&readiness, "IDENTITY").reason.as_deref(),
            Some("IDENTITY_STATUS_UNKNOWN")
        );
    }
}

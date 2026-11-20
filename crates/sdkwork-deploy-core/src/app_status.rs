//! The `deploy_app.app_status` state machine.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3 fixes the vocabulary and
//! the direction of movement: `DRAFT` → `ACTIVE` → `PAUSED` → `ARCHIVED` through
//! the deploy commands, and uninstall is **forward-only**. Before this rule lived
//! in Rust, the status was writable from two independent places
//! (`apps.activate`/`apps.pause` and the generic `apps.update` PATCH), neither
//! validated a transition, and `ARCHIVED` could be reversed by re-activating.
//!
//! The rule is a pure function so every writer shares it and it can be tested
//! without a database. The repository calls [`validate_app_status_transition`]
//! before any `UPDATE`, and `apps.update` routes a caller-supplied `appStatus`
//! through the same function rather than writing the column directly.
//!
//! Terminality: `ARCHIVED` is terminal for this surface. Uninstall is a one-way
//! door; re-publishing an archived application is a new deploy application with
//! its own installation and token audience, not an `ACTIVE` write on the old
//! row. The only escape is a governed data repair, which the database still
//! permits because the CHECK constraint is deliberately a vocabulary check, not
//! a transition table.

/// The stored statuses, in the order the factory moves through them.
pub const APP_STATUS_DRAFT: &str = "DRAFT";
pub const APP_STATUS_ACTIVE: &str = "ACTIVE";
pub const APP_STATUS_PAUSED: &str = "PAUSED";
pub const APP_STATUS_ARCHIVED: &str = "ARCHIVED";

/// Every status `deploy_app.app_status` may hold.
///
/// `READY` and `FAILED` are deliberately absent. They exist in the wire enum for
/// historical callers, but `chk_deploy_app_status` rejects both:
/// readiness is derived (section 3) and a failure is reported through
/// `identity_status` or a release/deployment status, never invented as an
/// application state.
pub const STORED_APP_STATUSES: &[&str] = &[
    APP_STATUS_DRAFT,
    APP_STATUS_ACTIVE,
    APP_STATUS_PAUSED,
    APP_STATUS_ARCHIVED,
];

/// Why a requested status change is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStatusTransitionError {
    /// The stored value is not one of [`STORED_APP_STATUSES`]. A row in this
    /// state was written by a bypassed writer; the API refuses to build on it
    /// rather than guess.
    UnknownCurrent,
    /// The requested value is not one of [`STORED_APP_STATUSES`].
    UnknownTarget,
    /// `ARCHIVED` is terminal for this surface.
    ArchivedIsTerminal,
    /// The transition is not in the documented direction.
    NotAllowed,
}

impl AppStatusTransitionError {
    /// The problem detail an operator reads. Names both statuses so a caller
    /// never has to infer which rule fired.
    #[must_use]
    pub fn message(self, from: &str, to: &str) -> String {
        match self {
            Self::UnknownCurrent => format!(
                "application status {from:?} is not a lifecycle status; expected one of {}",
                STORED_APP_STATUSES.join(", ")
            ),
            Self::UnknownTarget => format!(
                "application status {to:?} is not a lifecycle status; expected one of {}",
                STORED_APP_STATUSES.join(", ")
            ),
            Self::ArchivedIsTerminal => format!(
                "application is ARCHIVED and cannot become {to}; uninstall is forward-only and a \
                 return to service is a new deploy application"
            ),
            Self::NotAllowed => format!(
                "application status cannot move from {from} to {to}; allowed moves are \
                 DRAFT->ACTIVE, DRAFT->PAUSED, DRAFT->ARCHIVED, ACTIVE->PAUSED, ACTIVE->ARCHIVED, \
                 PAUSED->ACTIVE, PAUSED->ARCHIVED"
            ),
        }
    }
}

/// Decides whether `from` may become `to`.
///
/// A no-op request (`from == to`) is allowed: replaying a command must not fail
/// because the first attempt already applied it, which is what makes an
/// idempotent retry land on one row.
///
/// # Errors
/// Returns the typed reason when the move is not allowed.
pub fn validate_app_status_transition(
    from: &str,
    to: &str,
) -> Result<(), AppStatusTransitionError> {
    let from = normalize(from);
    let to = normalize(to);
    if !STORED_APP_STATUSES.contains(&from.as_str()) {
        return Err(AppStatusTransitionError::UnknownCurrent);
    }
    if !STORED_APP_STATUSES.contains(&to.as_str()) {
        return Err(AppStatusTransitionError::UnknownTarget);
    }
    if from == to {
        return Ok(());
    }
    if from == APP_STATUS_ARCHIVED {
        return Err(AppStatusTransitionError::ArchivedIsTerminal);
    }
    let allowed = matches!(
        (from.as_str(), to.as_str()),
        (APP_STATUS_DRAFT, APP_STATUS_ACTIVE)
            | (APP_STATUS_DRAFT, APP_STATUS_PAUSED)
            | (APP_STATUS_DRAFT, APP_STATUS_ARCHIVED)
            | (APP_STATUS_ACTIVE, APP_STATUS_PAUSED)
            | (APP_STATUS_ACTIVE, APP_STATUS_ARCHIVED)
            | (APP_STATUS_PAUSED, APP_STATUS_ACTIVE)
            | (APP_STATUS_PAUSED, APP_STATUS_ARCHIVED)
    );
    if allowed {
        Ok(())
    } else {
        Err(AppStatusTransitionError::NotAllowed)
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_path_is_allowed() {
        for (from, to) in [
            (APP_STATUS_DRAFT, APP_STATUS_ACTIVE),
            (APP_STATUS_DRAFT, APP_STATUS_PAUSED),
            (APP_STATUS_DRAFT, APP_STATUS_ARCHIVED),
            (APP_STATUS_ACTIVE, APP_STATUS_PAUSED),
            (APP_STATUS_ACTIVE, APP_STATUS_ARCHIVED),
            (APP_STATUS_PAUSED, APP_STATUS_ACTIVE),
            (APP_STATUS_PAUSED, APP_STATUS_ARCHIVED),
        ] {
            assert!(
                validate_app_status_transition(from, to).is_ok(),
                "{from} -> {to} must be allowed"
            );
        }
    }

    #[test]
    fn archive_is_terminal() {
        for to in [APP_STATUS_DRAFT, APP_STATUS_ACTIVE, APP_STATUS_PAUSED] {
            assert_eq!(
                validate_app_status_transition(APP_STATUS_ARCHIVED, to),
                Err(AppStatusTransitionError::ArchivedIsTerminal),
                "ARCHIVED -> {to} must be refused"
            );
        }
    }

    #[test]
    fn skipping_backwards_is_refused() {
        assert_eq!(
            validate_app_status_transition(APP_STATUS_ACTIVE, APP_STATUS_DRAFT),
            Err(AppStatusTransitionError::NotAllowed)
        );
        assert_eq!(
            validate_app_status_transition(APP_STATUS_PAUSED, APP_STATUS_DRAFT),
            Err(AppStatusTransitionError::NotAllowed)
        );
    }

    #[test]
    fn a_replay_of_the_same_status_is_a_no_op_not_an_error() {
        for status in STORED_APP_STATUSES {
            assert!(
                validate_app_status_transition(status, status).is_ok(),
                "re-applying {status} must stay idempotent"
            );
        }
    }

    #[test]
    fn states_the_database_rejects_are_not_transitions() {
        // `READY`/`FAILED` exist on the wire but `chk_deploy_app_status` rejects
        // both, so neither may be treated as a reachable state.
        assert_eq!(
            validate_app_status_transition(APP_STATUS_DRAFT, "READY"),
            Err(AppStatusTransitionError::UnknownTarget)
        );
        assert_eq!(
            validate_app_status_transition("FAILED", APP_STATUS_ACTIVE),
            Err(AppStatusTransitionError::UnknownCurrent)
        );
    }

    #[test]
    fn the_rule_is_case_and_padding_insensitive() {
        assert!(validate_app_status_transition(" active ", "paused").is_ok());
    }

    #[test]
    fn messages_name_both_statuses() {
        let message = AppStatusTransitionError::NotAllowed.message("ACTIVE", "DRAFT");
        assert!(message.contains("ACTIVE"), "{message}");
        assert!(message.contains("DRAFT"), "{message}");
    }
}

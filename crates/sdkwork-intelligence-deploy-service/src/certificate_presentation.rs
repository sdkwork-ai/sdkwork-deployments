//! Inspecting whether a manual challenge response is actually published.
//!
//! The console lets an operator add the DNS-01 TXT record themselves. Asking the
//! CA to validate before that record is visible burns one of the CA's
//! failed-validation allowances (Let's Encrypt: five per account per hostname per
//! hour), so the control plane checks first and only then releases the order.
//!
//! The decision is deliberately split from the lookup: [`dns_txt_matches`] is a
//! pure function over the observed values, and the lookup is a port. That keeps
//! the comparison table-testable without DNS, and lets a deployment choose which
//! resolvers it trusts.

use async_trait::async_trait;

use crate::DeployServiceResult;

/// One resolver's answer, kept so a failure can name who saw what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationResolverReport {
    /// Resolver identity (address or configured name), for the operator report.
    pub resolver: String,
    pub observed: Vec<String>,
    pub error: Option<String>,
}

/// What a presentation check established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationReport {
    pub visible: bool,
    pub reports: Vec<PresentationResolverReport>,
}

impl PresentationReport {
    /// A check that never ran: reported as "not visible" because the caller must
    /// not be told a record is present when nobody looked.
    pub fn unverifiable(reason: impl Into<String>) -> Self {
        Self {
            visible: false,
            reports: vec![PresentationResolverReport {
                resolver: "inspector".to_owned(),
                observed: Vec::new(),
                error: Some(reason.into()),
            }],
        }
    }
}

/// Looks up the TXT records an operator was told to publish.
#[async_trait]
pub trait CertificatePresentationInspectorPort: Send + Sync {
    /// Returns what the configured resolvers currently see at `record_name`.
    ///
    /// An implementation that cannot check must answer with
    /// [`PresentationReport::unverifiable`] rather than pretending the record is
    /// there: a false "visible" sends the CA after a record that does not exist.
    async fn inspect_dns_txt(
        &self,
        record_name: &str,
        expected: &[String],
    ) -> DeployServiceResult<PresentationReport> {
        let _ = (record_name, expected);
        Ok(PresentationReport::unverifiable(
            "this deployment has no DNS inspection resolvers configured",
        ))
    }
}

/// Whether a presentation check permits releasing the order to the CA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationReleaseDecision {
    /// At least one resolver answered and none contradicted the expected value, or
    /// nobody could check. The operator's assertion stands.
    Allowed,
    /// A resolver actually checked and did not see the value. Releasing now would
    /// spend one of the CA's failed-validation allowances.
    Blocked,
}

/// Turns a presentation report into the release decision.
///
/// The distinction that matters is **checked-and-absent** versus
/// **could-not-check**. Collapsing them would either block manual issuance on a
/// deployment with no trusted resolver (unusable) or hand the CA a record nobody
/// can see (a wasted failed validation). Extracted as a pure function so the rule
/// is table-testable rather than living inline in a database path.
pub fn presentation_release_decision(report: &PresentationReport) -> PresentationReleaseDecision {
    let answered = report.reports.iter().filter(|entry| entry.error.is_none());
    if answered.count() == 0 {
        // Nobody answered: an assertion, not a contradiction.
        return PresentationReleaseDecision::Allowed;
    }
    if report.visible {
        PresentationReleaseDecision::Allowed
    } else {
        PresentationReleaseDecision::Blocked
    }
}

#[cfg(test)]
mod release_decision_tests {
    use super::*;

    fn report(visible: bool, entries: &[(&str, Option<&str>)]) -> PresentationReport {
        PresentationReport {
            visible,
            reports: entries
                .iter()
                .map(|(resolver, error)| PresentationResolverReport {
                    resolver: (*resolver).to_owned(),
                    observed: Vec::new(),
                    error: error.map(str::to_owned),
                })
                .collect(),
        }
    }

    #[test]
    fn a_visible_record_is_releasable() {
        assert_eq!(
            presentation_release_decision(&report(true, &[("1.1.1.1", None)])),
            PresentationReleaseDecision::Allowed
        );
    }

    #[test]
    fn a_checked_but_absent_record_blocks_the_release() {
        assert_eq!(
            presentation_release_decision(&report(false, &[("1.1.1.1", None)])),
            PresentationReleaseDecision::Blocked
        );
    }

    #[test]
    fn an_unverifiable_check_does_not_block_the_operator() {
        // No resolver answered: the operator asserted the record exists, and
        // blocking here would make manual issuance unusable.
        assert_eq!(
            presentation_release_decision(&report(false, &[("1.1.1.1", Some("timeout"))])),
            PresentationReleaseDecision::Allowed
        );
    }

    #[test]
    fn one_authoritative_absence_outweighs_a_failed_lookup() {
        // A resolver that checked and saw nothing is evidence; another resolver
        // failing to answer is not a contradiction of it.
        assert_eq!(
            presentation_release_decision(&report(
                false,
                &[("1.1.1.1", None), ("8.8.8.8", Some("SERVFAIL"))]
            )),
            PresentationReleaseDecision::Blocked
        );
    }
}

///
/// The comparison is exact after normalization, and deliberately **not**
/// constant-time: an ACME challenge value is public by design, so timing leaks
/// nothing an attacker could not read from DNS. What matters is that it is
/// *strict*: a trimmed, lower-cased or partially concatenated value is a different
/// record to the CA, and accepting it would spend a failed validation.
/// Whether any observed TXT value equals one of the expected proofs.
///
/// The comparison is exact after normalization (see [`normalize_txt_value`]): a
/// trimmed, lower-cased or partially concatenated value is a different record to
/// the CA, and accepting it would spend a failed validation.
pub fn dns_txt_matches(observed: &[String], expected: &[String]) -> bool {
    let expected = expected
        .iter()
        .map(|value| normalize_txt_value(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if expected.is_empty() {
        return false;
    }
    observed
        .iter()
        .map(|value| normalize_txt_value(value))
        .any(|value| expected.iter().any(|candidate| candidate == &value))
}

/// Normalizes one DNS TXT representation to its wire value.
///
/// Resolvers return TXT data in two shapes: a plain string, or RFC 1035
/// character-strings (`"chunk" "chunk"`) whose chunks are concatenated by the
/// resolver. Both forms describe the same record, so both are reduced to the
/// concatenated content. Nothing else is normalized: padding, case and interior
/// spacing are part of the value.
fn normalize_txt_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') {
        return trimmed.to_owned();
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut in_quotes = false;
    let mut current = String::new();
    let mut chunks: Vec<String> = Vec::new();
    for ch in trimmed.chars() {
        match ch {
            '"' if !in_quotes => {
                in_quotes = true;
                current.clear();
            }
            '"' if in_quotes => {
                in_quotes = false;
                chunks.push(current.clone());
                current.clear();
            }
            _ if in_quotes => current.push(ch),
            _ => {}
        }
    }
    for chunk in chunks {
        out.push_str(&chunk);
    }
    if out.is_empty() {
        trimmed.to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn an_exact_value_matches() {
        assert!(dns_txt_matches(
            &values(&["abc-DEF_123"]),
            &values(&["abc-DEF_123"])
        ));
    }

    #[test]
    fn a_quoted_value_matches_its_unquoted_form() {
        assert!(dns_txt_matches(
            &values(&["\"abc-DEF_123\""]),
            &values(&["abc-DEF_123"])
        ));
    }

    #[test]
    fn character_string_chunks_are_concatenated() {
        assert!(dns_txt_matches(
            &values(&["\"abc\" \"def\""]),
            &values(&["abcdef"])
        ));
    }

    #[test]
    fn a_near_miss_does_not_match() {
        // Case and truncation are different records to the CA. (Surrounding
        // whitespace is tolerated below: resolvers pad, and a pad is not part of
        // the value the CA compares.)
        for wrong in ["abc-def_123", "abc-DEF_12", "abc-DEF_1234"] {
            assert!(
                !dns_txt_matches(&values(&["abc-DEF_123"]), &values(&[wrong])),
                "{wrong} must not match"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_value() {
        // A resolver that pads its answer still describes the same record.
        assert!(dns_txt_matches(
            &values(&["  abc-DEF_123 "]),
            &values(&["abc-DEF_123"])
        ));
    }

    #[test]
    fn one_matching_value_among_several_is_enough() {
        // A wildcard order publishes two TXT values at one name; either matching
        // authorization is progress.
        assert!(dns_txt_matches(
            &values(&["other-value", "abc", "\"abc\""]),
            &values(&["abc"])
        ));
    }

    #[test]
    fn nothing_expected_or_nothing_observed_never_matches() {
        assert!(!dns_txt_matches(&values(&["abc"]), &[]));
        assert!(!dns_txt_matches(&[], &values(&["abc"])));
        assert!(!dns_txt_matches(&values(&["abc"]), &values(&[""])));
        assert!(!dns_txt_matches(&values(&["abc"]), &values(&["   "])));
    }

    #[test]
    fn an_unverifiable_report_is_never_reported_as_visible() {
        let report = PresentationReport::unverifiable("no resolvers");
        assert!(!report.visible);
        assert_eq!(report.reports.len(), 1);
    }
}

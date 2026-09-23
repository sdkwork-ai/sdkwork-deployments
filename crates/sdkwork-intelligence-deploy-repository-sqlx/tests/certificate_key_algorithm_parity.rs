//! The Rust key-algorithm vocabulary and the DDL must name the same set.
//!
//! The set used to live in seven places: one literal per validation site
//! (`certificates.rs`, `backend.rs`, and the ACME engine in the webserver repo), the
//! default in the contract crate, and four `CHECK (… IN (…))` constraints in the
//! PostgreSQL baseline. Rust now derives from
//! [`CERTIFICATE_KEY_ALGORITHMS`]; the constraints cannot, because a baseline is
//! applied DDL and not a build input.
//!
//! So the remaining risk is a *one-sided* change: adding an algorithm to the
//! vocabulary passes every Rust check and then fails the constraint at INSERT, as a
//! 500 with `new row violates check constraint` instead of a field error — and
//! removing one is worse, because it silently narrows what the API accepts while the
//! DDL keeps accepting the wider set. These tests read the baseline and fail on
//! either direction.
//!
//! Reading the file is enough here: this is a pure test, no database. The
//! integration tests that apply the baseline exercise it as SQL separately.

use std::collections::BTreeSet;

use sdkwork_deploy_core::CERTIFICATE_KEY_ALGORITHMS;

const POSTGRES_BASELINE: &str =
    include_str!("../../../database/ddl/baseline/postgres/0001_deploy_baseline.sql");

/// Every constraint in the baseline that guards a stored algorithm.
///
/// Hand-maintained on purpose: the point is to fail when the DDL grows a fifth one,
/// and to do that the test has to hold a name to compare against.
const ALGORITHM_CONSTRAINTS: &[&str] = &[
    "chk_deploy_certificate_key_algorithm",
    "chk_deploy_certificate_version_key_algorithm",
    "chk_deploy_listener_certificate_binding_algorithm",
    "chk_deploy_tls_runtime_assignment_algorithm",
];

/// The constraint names in the baseline whose `IN (…)` list mentions a vocabulary
/// value — discovered, not listed, so a new table storing an algorithm shows up.
fn discovered_constraints(baseline: &str) -> BTreeSet<String> {
    baseline
        .lines()
        .filter(|line| {
            line.contains("CONSTRAINT")
                && CERTIFICATE_KEY_ALGORITHMS
                    .iter()
                    .any(|value| line.contains(&format!("'{value}'")))
        })
        .filter_map(|line| {
            let start = line.find("CONSTRAINT ")? + "CONSTRAINT ".len();
            let rest = &line[start..];
            let end = rest.find(char::is_whitespace)?;
            Some(rest[..end].to_owned())
        })
        .collect()
}

/// The values one constraint's `IN (…)` list admits.
fn admitted_by(baseline: &str, constraint: &str) -> BTreeSet<String> {
    let line = baseline
        .lines()
        .find(|line| line.contains(constraint))
        .unwrap_or_else(|| panic!("{constraint} is not in the baseline"));
    let open = line
        .find("IN (")
        .unwrap_or_else(|| panic!("{constraint} does not guard its column with IN (…): {line}"));
    let close = open
        + line[open..]
            .find(')')
            .unwrap_or_else(|| panic!("{constraint} has an unterminated IN list: {line}"));
    line[open + "IN (".len()..close]
        .split(',')
        .map(|value| value.trim().trim_matches('\'').to_owned())
        .collect()
}

#[test]
fn the_baseline_guards_an_algorithm_in_exactly_the_tables_we_know_about() {
    let expected: BTreeSet<String> = ALGORITHM_CONSTRAINTS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        discovered_constraints(POSTGRES_BASELINE),
        expected,
        "the set of algorithm-guarding constraints changed; update ALGORITHM_CONSTRAINTS \
         and the vocabulary together, or the two halves of the platform will disagree"
    );
}

#[test]
fn every_algorithm_constraint_admits_exactly_the_shared_vocabulary() {
    let expected: BTreeSet<String> = CERTIFICATE_KEY_ALGORITHMS
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
    for constraint in ALGORITHM_CONSTRAINTS {
        assert_eq!(
            admitted_by(POSTGRES_BASELINE, constraint),
            expected,
            "{constraint} and CERTIFICATE_KEY_ALGORITHMS disagree; a certificate the API \
             accepts would be refused by the database, or the other way round"
        );
    }
}

#[test]
fn the_parser_reads_a_list_rather_than_returning_an_empty_set() {
    // Guards the two assertions above from passing vacuously: a parser that matched
    // nothing would make `discovered_constraints` empty and `admitted_by` panic-free
    // only by accident, and an empty expected set would compare equal on both sides.
    assert!(
        !CERTIFICATE_KEY_ALGORITHMS.is_empty(),
        "the vocabulary is empty, so every comparison above is vacuous"
    );
    let discovered = discovered_constraints(POSTGRES_BASELINE);
    assert_eq!(discovered.len(), ALGORITHM_CONSTRAINTS.len());
    let first = admitted_by(POSTGRES_BASELINE, ALGORITHM_CONSTRAINTS[0]);
    assert!(
        first.len() >= 2,
        "the first constraint parsed to {first:?}, which is not a list"
    );
}

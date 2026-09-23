//! The DNS-provider vocabulary and every surface that repeats it must name one set.
//!
//! One provider family is spelled in three *live* places, and each falls over
//! differently when it lags:
//!
//! * [`dns_provider::ALL`] — the account center's answer to "may this family be
//!   bound to an account", and the validator for a zone's declared provider.
//!   Behind: the console cannot register an account for a family the engine can
//!   already drive, so a supported provider looks unsupported.
//! * `DnsProviderKind` in the ACME crate — the engine's own list. Behind: the row is
//!   refused at the first order with "unsupported DNS provider kind", after the
//!   operator has already stored a credential that looks fine.
//! * the app-api contract's closed `dnsProvider` enums — the authority the SDK is
//!   generated from, and therefore the console's provider union. Behind: the console
//!   offers a family the server cannot drive, or cannot offer one it can.
//!
//! Every direction above is a *one-sided* change: it compiles, every Rust check
//! passes, and the failure lands on an operator. These tests read all three surfaces
//! and fail on either side of a comparison, so they can only move together.
//!
//! ## Why this gate does not track the DDL
//!
//! The one `CHECK (provider_kind IN (…))` in the baseline sits on
//! `deploy_dns_provider_credential`, which the baseline itself declares
//! **DEPRECATED** — "no code path reads or writes it". A live DNS family is not
//! stored anywhere: accounts live in the IAM account center under a free-form
//! `vendor_code`, and the family is *derived* from it
//! ([`dns_provider::family_for_vendor_code`]). So there is no applied-DDL surface to
//! keep in step, and demanding one would ask for a migration that changes nothing.
//!
//! What the gate does instead is check the deprecation is still declared and pin the
//! frozen list: the day that note goes away, the frozen list stops being history and
//! becomes the live constraint, and the failing test says so.
//!
//! They are pure: no database, no network. The integration suites apply the baseline
//! as SQL separately.

use std::collections::BTreeSet;

use sdkwork_deploy_cloud_account_port::dns_provider;
use sdkwork_deploy_contract::DeployServiceErrorKind;
use sdkwork_intelligence_deploy_service::DeployDnsProviderCredential;
use sdkwork_webserver_acme_service::{DnsProviderKind, MAX_HTTP_REQUEST_CONFIG_BYTES};

const POSTGRES_BASELINE: &str =
    include_str!("../../../database/ddl/baseline/postgres/0001_deploy_baseline.sql");

/// The materialised app-api contract.
///
/// Read as the authority rather than the authored `openapi.yaml` because a Rust gate
/// has no YAML parser, and the two are already compared by
/// `tests/contract/openapi-materialization.contract.test.mjs` — which also proves the
/// SDK, its generation input, the component spec and the route manifest follow this
/// file. So aligning here is aligning everything downstream of it.
const APP_API_CONTRACT: &str =
    include_str!("../../../apis/app-api/deploy/deploy-app-api.openapi.json");

/// The deprecated table whose `CHECK` is the platform's only stored-provider list.
const DEPRECATED_PROVIDER_TABLE: &str = "deploy_dns_provider_credential";
const DEPRECATED_PROVIDER_CONSTRAINT: &str = "chk_deploy_dns_provider_credential_kind";

/// The families that constraint admits, frozen at the table's deprecation.
///
/// Deliberately *not* [`dns_provider::ALL`]: widening it would be a DDL edit to a
/// table nothing writes, and keeping it equal is not the invariant anyone needs.
/// What matters is that it is not quietly treated as live (see
/// `the_deprecated_table_is_still_declared_deprecated`).
const DEPRECATED_FAMILIES: &[&str] = &["ALIYUN_DNS", "DNSPOD", "CLOUDFLARE"];

/// Contract sites whose provider list is **closed**: they are the vocabulary.
///
/// Hand-maintained, and compared against the discovered set, so a *new* closed list
/// fails here rather than quietly becoming one more place to remember. Array indices
/// are normalized away (see [`discovered_contract_sites`]), so inserting an unrelated
/// query parameter does not read as a provider change.
const CLOSED_PROVIDER_SITES: &[&str] = &[
    "/components/schemas/CloudAccountResponse/properties/dnsProvider",
    "/components/schemas/CreateCloudAccountRequest/properties/dnsProvider",
    "/paths//app/v3/api/cloud_accounts/get/parameters[]",
];

/// Contract sites where `dnsProvider` is **open** on purpose.
///
/// A zone's declaration is free text because `manual` is a real answer — "these
/// records are published by hand" — and because an operator may name a provider this
/// build cannot drive. Closing these lists would make both unexpressible, and such a
/// zone would become indistinguishable from one nobody has decided yet.
const OPEN_PROVIDER_SITES: &[&str] = &[
    "/components/schemas/CreateDomainZoneRequest/properties/dnsProvider",
    "/components/schemas/UpdateDomainZoneRequest/properties/dnsProvider",
    "/components/schemas/DomainZoneResponse/properties/dnsProvider",
    // The zone-verify response echoes the zone's own declaration back, so it
    // inherits the openness: a `manual` or undecided zone verifies through the
    // manual path, and the response must not pretend a family was chosen.
    "/components/schemas/DomainZoneVerifyResponse/properties/dnsProvider",
];

fn vocabulary() -> BTreeSet<String> {
    dns_provider::ALL
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

/// Every `CONSTRAINT <name>` in the baseline, with the text of its statement.
///
/// The body is read as a **statement region, not a line**. The provider constraint
/// wraps its `IN (…)` list onto the next line, so a line-scoped reader found no
/// values for it at all — and an empty discovery set compares equal to an empty
/// expectation, which is exactly the "no findings" that must never read as success.
fn constraint_bodies(baseline: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = baseline.lines().collect();
    let mut bodies = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("CONSTRAINT ") {
            continue;
        }
        let rest = &line.trim_start()["CONSTRAINT ".len()..];
        let Some(end) = rest.find(char::is_whitespace) else {
            continue;
        };
        let name = rest[..end].to_owned();
        // A constraint ends at the first `)`, which closes either its `IN (…)` list
        // or whatever single expression it guards.
        let mut body = String::new();
        for follow in &lines[index..] {
            body.push_str(follow);
            body.push(' ');
            if body.contains(')') {
                break;
            }
        }
        bodies.push((name, body));
    }
    bodies
}

/// The values one constraint body admits through `IN (…)`; empty when it has none.
fn admitted_from(body: &str) -> BTreeSet<String> {
    let Some(open) = body.find("IN (") else {
        return BTreeSet::new();
    };
    let from = open + "IN (".len();
    let Some(close) = body[from..].find(')') else {
        return BTreeSet::new();
    };
    body[from..from + close]
        .split(',')
        .map(|value| value.trim().trim_matches('\'').to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

fn admitted_by(baseline: &str, constraint: &str) -> BTreeSet<String> {
    let (_, body) = constraint_bodies(baseline)
        .into_iter()
        .find(|(name, _)| name == constraint)
        .unwrap_or_else(|| panic!("{constraint} is not in the baseline"));
    let admitted = admitted_from(&body);
    assert!(
        !admitted.is_empty(),
        "{constraint} guards no list, so every comparison against it would be vacuous: {body}"
    );
    admitted
}

/// Constraints whose body admits one of the vocabulary's families.
fn discovered_constraints(baseline: &str) -> BTreeSet<String> {
    let families = vocabulary();
    constraint_bodies(baseline)
        .into_iter()
        .filter(|(_, body)| {
            families
                .iter()
                .any(|family| body.contains(&format!("'{family}'")))
        })
        .map(|(name, _)| name)
        .collect()
}

fn contract() -> serde_json::Value {
    serde_json::from_str(APP_API_CONTRACT).expect("the app-api contract must be JSON")
}

/// Every site in the contract that declares a provider, with its closed list.
///
/// Two shapes count, because OpenAPI spells them differently: a *schema property*
/// named `dnsProvider`, and a *parameter* whose `name` is `dnsProvider` (whose list
/// sits one level down, under `schema`). A walk that only looked for the first shape
/// would miss the picker's own filter parameter — the one the console calls with a
/// family, and whose narrowing is what makes the account list a picker rather than an
/// inventory.
fn discovered_contract_sites() -> Vec<(String, Option<BTreeSet<String>>)> {
    fn enum_of(node: &serde_json::Value) -> Option<BTreeSet<String>> {
        Some(
            node.get("enum")?
                .as_array()?
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect(),
        )
    }

    fn walk(
        node: &serde_json::Value,
        path: &str,
        out: &mut Vec<(String, Option<BTreeSet<String>>)>,
    ) {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "dnsProvider" {
                        out.push((format!("{path}/{key}"), enum_of(value)));
                    }
                    if key == "parameters" {
                        if let Some(items) = value.as_array() {
                            for parameter in items {
                                if parameter.get("name").and_then(|name| name.as_str())
                                    == Some("dnsProvider")
                                {
                                    // `[]` rather than the index: the parameter's
                                    // position is not the fact under test.
                                    out.push((
                                        format!("{path}/parameters[]"),
                                        parameter.get("schema").and_then(enum_of),
                                    ));
                                }
                            }
                        }
                    }
                    walk(value, &format!("{path}/{key}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, path, out);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    walk(&contract(), "", &mut out);
    out
}

#[test]
fn the_vocabulary_is_a_non_empty_set_of_distinct_families() {
    // Guards every comparison below from passing vacuously.
    let families = vocabulary();
    assert!(
        !families.is_empty(),
        "the provider vocabulary is empty, so every comparison in this file is vacuous"
    );
    assert_eq!(
        families.len(),
        dns_provider::ALL.len(),
        "dns_provider::ALL repeats a family, which makes the set comparisons lenient"
    );
    for family in &families {
        assert!(
            dns_provider::is_supported(family),
            "{family} is in ALL but is_supported says no"
        );
        assert_eq!(
            dns_provider::normalize(&family.to_lowercase()),
            Some(family.as_str()),
            "{family} must normalize from any casing"
        );
    }
}

#[test]
fn the_acme_engine_drives_exactly_the_same_families() {
    let engine: BTreeSet<String> = DnsProviderKind::ALL
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    assert_eq!(
        engine.len(),
        DnsProviderKind::ALL.len(),
        "DnsProviderKind::ALL repeats a family"
    );
    assert_eq!(
        engine,
        vocabulary(),
        "the engine and the account center disagree; the difference is a credential an operator \
         can store and never use, or a family the console cannot offer"
    );
    for family in &vocabulary() {
        let kind = DnsProviderKind::parse(family)
            .unwrap_or_else(|| panic!("the engine cannot parse {family}"));
        assert_eq!(
            kind.as_str(),
            family,
            "{family} does not round-trip through the engine"
        );
    }
}

#[test]
fn the_contract_declares_a_provider_in_exactly_the_sites_we_know_about() {
    let discovered: BTreeSet<String> = discovered_contract_sites()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    let known: BTreeSet<String> = CLOSED_PROVIDER_SITES
        .iter()
        .chain(OPEN_PROVIDER_SITES)
        .map(|path| (*path).to_owned())
        .collect();
    assert_eq!(
        discovered, known,
        "the contract grew or lost a provider site; decide whether it is closed (part of the \
         vocabulary) or open (free text, like a zone declaration) and list it here"
    );
}

#[test]
fn every_closed_contract_site_lists_exactly_the_shared_vocabulary() {
    let families = vocabulary();
    let closed: BTreeSet<String> = CLOSED_PROVIDER_SITES
        .iter()
        .map(|p| (*p).to_owned())
        .collect();
    let mut checked = 0;
    for (path, listed) in discovered_contract_sites() {
        if !closed.contains(&path) {
            continue;
        }
        checked += 1;
        assert_eq!(
            listed,
            Some(families.clone()),
            "{path} and the provider vocabulary disagree; the console's provider union is \
             generated from this contract, so this is also what the picker offers"
        );
    }
    assert_eq!(
        checked,
        CLOSED_PROVIDER_SITES.len(),
        "a closed site was not found in the contract at all, so its assertion never ran"
    );
}

#[test]
fn the_zone_declaration_stays_open_text() {
    // The counterpart of the test above: these three must NOT become closed lists.
    let open: BTreeSet<String> = OPEN_PROVIDER_SITES
        .iter()
        .map(|p| (*p).to_owned())
        .collect();
    let mut checked = 0;
    for (path, listed) in discovered_contract_sites() {
        if !open.contains(&path) {
            continue;
        }
        checked += 1;
        assert_eq!(
            listed, None,
            "{path} became a closed list, which makes `manual` and any not-yet-integrated \
             provider unexpressible"
        );
    }
    assert_eq!(
        checked,
        OPEN_PROVIDER_SITES.len(),
        "an open site was not found in the contract at all, so its assertion never ran"
    );
}

#[test]
fn the_deprecated_table_is_still_declared_deprecated() {
    // This is the test that makes the DDL exemption above safe. If the deprecation
    // note is removed, the frozen constraint below becomes the live one, and this
    // gate must stop excusing it.
    let needle = format!("COMMENT ON TABLE {DEPRECATED_PROVIDER_TABLE} IS 'DEPRECATED");
    assert!(
        POSTGRES_BASELINE.contains(&needle),
        "{DEPRECATED_PROVIDER_TABLE} is no longer declared deprecated, so \
         {DEPRECATED_PROVIDER_CONSTRAINT} is now the live provider list: make it equal \
         dns_provider::ALL and delete the frozen-list exemption in this file"
    );
    assert_eq!(
        admitted_by(POSTGRES_BASELINE, DEPRECATED_PROVIDER_CONSTRAINT),
        DEPRECATED_FAMILIES
            .iter()
            .map(|family| (*family).to_owned())
            .collect::<BTreeSet<String>>(),
        "{DEPRECATED_PROVIDER_CONSTRAINT} admits a different set than the one it was frozen \
         with; a stored-provider list changed on a table nothing writes"
    );
}

#[test]
fn the_baseline_guards_a_stored_provider_in_exactly_the_table_we_know_about() {
    // The discovery half of the exemption: a *second* table storing a provider would
    // show up here rather than slipping past, and finding only the deprecated one is
    // what proves the parser reads wrapped lists (it once found nothing at all).
    let expected: BTreeSet<String> = [DEPRECATED_PROVIDER_CONSTRAINT.to_owned()]
        .into_iter()
        .collect();
    assert_eq!(
        discovered_constraints(POSTGRES_BASELINE),
        expected,
        "the set of stored-provider constraints changed; a new table must be gated against \
         dns_provider::ALL, and a removed one means this exemption can shrink"
    );
}

#[test]
fn every_family_declares_a_vendor_and_a_credential_shape() {
    for family in &vocabulary() {
        let vendor = dns_provider::vendor_code_for(family)
            .unwrap_or_else(|| panic!("{family} names no vendor code"));
        assert!(
            dns_provider::credential_kind_for(family).is_some(),
            "{family} has no credential shape, so the console cannot render its form"
        );
        assert_eq!(
            dns_provider::family_for_vendor_code(vendor),
            Some(family.as_str()),
            "vendor code {vendor} does not map back to {family}"
        );
    }
}

#[test]
fn every_family_can_be_built_into_a_presenter_credential() {
    // The account-center → presenter hop is where a family added to the vocabulary but
    // forgotten in the credential layer fails — and it fails at issuance rather than
    // at registration. One identifier plus one secret covers every shape the account
    // center stores: a family with no public half ignores the first.
    for family in &vocabulary() {
        let credential = DeployDnsProviderCredential::from_account_credential(
            family,
            "probe-identifier",
            "probe-secret",
        )
        .unwrap_or_else(|error| {
            panic!(
                "{family} is offered for binding but the credential layer refuses it: {:?}",
                error.kind()
            )
        });
        assert_eq!(
            credential.kind().as_str(),
            family.as_str(),
            "{family} builds a credential for a different family"
        );
    }
}

#[test]
fn an_unknown_family_is_refused_by_both_halves() {
    // The other direction of the same invariant: neither half may accept a family the
    // other has never heard of, or a typo in a zone's declaration would reach a
    // provider as a signed request nobody can answer.
    let unknown = "NOT_A_DNS_FAMILY";
    assert!(!dns_provider::is_supported(unknown));
    assert_eq!(dns_provider::normalize(unknown), None);
    assert!(DnsProviderKind::parse(unknown).is_none());
    let error = DeployDnsProviderCredential::from_account_credential(unknown, "id", "secret")
        .expect_err("an unknown family must be refused");
    assert_eq!(error.kind(), DeployServiceErrorKind::Validation);
}

#[test]
fn the_generic_family_is_what_keeps_the_provider_list_open() {
    // Every other family is one vendor. This one carries the requests as data, so
    // the answer to "do you support provider X" stops depending on the release you
    // are running. Pinned explicitly because a set-equality test would keep passing
    // if it were quietly dropped — and dropping it is the change that would make the
    // list closed again.
    assert!(
        dns_provider::is_supported(dns_provider::HTTP_REQUEST),
        "the generic family is not in the vocabulary, so a provider without a dedicated \
         adapter cannot be registered at all"
    );
    assert!(
        DnsProviderKind::ALL.contains(&DnsProviderKind::HttpRequest),
        "the account center offers the generic family but the engine cannot drive it"
    );
    assert_eq!(
        dns_provider::family_for_vendor_code("custom"),
        Some(dns_provider::HTTP_REQUEST),
        "the account center's `custom` vendor must resolve to the generic family: that vendor is \
         how an operator registers a provider this build ships no adapter for"
    );
}

#[test]
fn the_engine_dedicated_list_is_the_frozen_ddl_list() {
    // The deprecated constraint is the platform's only stored-provider list, frozen at
    // the table's deprecation. The engine now carries that same history as a named
    // constant, so the exemption below can point at something instead of at a comment.
    let dedicated: BTreeSet<String> = DnsProviderKind::DEDICATED
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    assert_eq!(
        dedicated,
        admitted_by(POSTGRES_BASELINE, DEPRECATED_PROVIDER_CONSTRAINT),
        "the engine's dedicated families and the frozen DDL list disagree; the exemption below \
         assumes the DDL list is exactly the families that have their own adapter"
    );
    let families = vocabulary();
    assert!(
        dedicated.is_subset(&families),
        "DEDICATED names a family that is not in ALL"
    );
    assert!(
        families.len() > dedicated.len(),
        "ALL has no family beyond the dedicated ones, so nothing here is the generic fallback"
    );
}

#[test]
fn the_contract_admits_a_request_configuration_the_engine_can_parse() {
    // The generic family's configuration travels as the account's secret, so the two
    // bounds have to be the same number: a document the console accepts but the
    // presenter refuses is an account that cannot present a challenge, and the
    // operator finds out at the first order rather than at registration.
    let max = contract()
        .pointer(
            "/components/schemas/CreateCloudAccountRequest/properties/secretAccessKey/maxLength",
        )
        .and_then(serde_json::Value::as_u64)
        .expect("the register request must bound the secret it accepts");
    assert_eq!(
        max as usize, MAX_HTTP_REQUEST_CONFIG_BYTES,
        "the contract's secretAccessKey.maxLength and the engine's configuration ceiling differ"
    );
}

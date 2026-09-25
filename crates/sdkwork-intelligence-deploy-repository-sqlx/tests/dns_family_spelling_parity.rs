//! The console reads a declared DNS provider with the same spellings the engine
//! resolves, or an operator is offered a family nothing can bind.
//!
//! `deploy_dns_zone.dns_provider` is free text, so the delivery console carries its own
//! reader for the values a tenant actually types (`dnsFamilyFromDeclared`). That reader
//! is a *second* copy of a vocabulary the engine already owns
//! ([`dns_provider::family_for_vendor_code`]), and the copy is what the account picker
//! filters by. When the two disagree the failure is invisible on the Rust side and
//! loud on the operator's: the console shows the family, offers the accounts, and then
//! the bind fails with "does not map to a cloud vendor" — or, the other way, the console
//! finds no family and silently lists every account unfiltered.
//!
//! `alidns` was exactly that: the console had always read Alibaba Cloud's own spelling
//! of its DNS product, and the engine did not resolve it.
//!
//! Reading the file is enough: this is a pure test, no database. It is a `tests/*.rs`
//! target on purpose — the `tests/pending/` directory next to it is invisible to cargo
//! (see its README), so a gate parked there would never run.

use std::collections::{BTreeMap, BTreeSet};

use sdkwork_deploy_cloud_account_port::dns_provider;

const DELIVERY_CONSOLE: &str = include_str!(
    "../../../apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx"
);

/// The first double-quoted literal on a line, if any.
fn quoted(text: &str) -> Option<String> {
    let rest = &text[text.find('"')? + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// The spellings the console reads, keyed by the family it answers with.
///
/// Parsed rather than listed: a hand-written copy of the console's list would be a third
/// copy of the vocabulary, and it would agree with the console right up until the console
/// changed. The reader is deliberately small — the function it reads is four short
/// `case` runs ending in one `return "<FAMILY>"`.
fn console_spellings(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    let start = source
        .find("export function dnsFamilyFromDeclared")
        .expect("the delivery console no longer declares `dnsFamilyFromDeclared`");
    let body = &source[start..];
    // The function's own closing brace is the first one in column 0; every brace inside
    // it is indented, so this cuts the body and nothing beyond it.
    let end = body
        .find("\n}")
        .expect("`dnsFamilyFromDeclared` has no closing brace");
    let body = &body[..end];

    let mut families: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut pending: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("case ") {
            if let Some(spelling) = quoted(rest) {
                pending.push(spelling);
            }
            continue;
        }
        // `return undefined;` (the `default:` arm) names no family, so the pending run
        // must already be empty there — a `case` that never reaches a family would be a
        // spelling the console cannot answer with.
        if let Some(rest) = line.strip_prefix("return ") {
            if let Some(family) = quoted(rest) {
                let entry = families.entry(family).or_default();
                for spelling in pending.drain(..) {
                    entry.insert(spelling);
                }
            }
        }
    }
    assert!(
        !families.is_empty(),
        "no family could be read out of `dnsFamilyFromDeclared`; the parser no longer \
         matches the function, so this gate would pass on anything"
    );
    families
}

/// The engine's own answer, in the same shape.
fn engine_spellings() -> BTreeMap<String, BTreeSet<String>> {
    dns_provider::ALL
        .iter()
        .map(|family| {
            let codes = dns_provider::vendor_codes_for(family);
            assert!(
                !codes.is_empty(),
                "{family} drives no vendor code, so no account can supply it"
            );
            (
                (*family).to_owned(),
                codes.iter().map(|code| (*code).to_owned()).collect(),
            )
        })
        .collect()
}

#[test]
fn the_console_reads_a_declared_provider_by_the_spellings_the_engine_resolves() {
    let console = console_spellings(DELIVERY_CONSOLE);
    let engine = engine_spellings();

    // Both directions, and per family rather than in aggregate: a spelling moved from
    // one family to another keeps the totals equal while changing what an operator gets.
    for (family, expected) in &engine {
        let found = console.get(family).unwrap_or_else(|| {
            panic!("the console reads no spelling for {family}, so the picker can never be filtered by it")
        });
        assert_eq!(
            found, expected,
            "{family}: the console reads {found:?} but the engine resolves {expected:?}"
        );
    }
    let extra: Vec<&String> = console
        .keys()
        .filter(|family| !engine.contains_key(*family))
        .collect();
    assert!(
        extra.is_empty(),
        "the console answers with families the engine cannot drive: {extra:?}"
    );
}

#[test]
fn every_spelling_the_console_accepts_is_one_the_engine_resolves() {
    // The `alidns` failure, stated directly: the console reads the value, so the value
    // has to reach a family. A spelling only the console knows produces an account
    // picker filtered to a provider that `ensure_bindable` then refuses.
    for (family, spellings) in console_spellings(DELIVERY_CONSOLE) {
        for spelling in spellings {
            assert_eq!(
                dns_provider::family_for_vendor_code(&spelling).map(str::to_owned),
                Some(family.clone()),
                "the console reads `{spelling}` as {family}, but the engine does not \
                 resolve that spelling"
            );
        }
    }
}

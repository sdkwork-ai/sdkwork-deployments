//! Every DNS provider family must be reachable from the console to the vendor.
//!
//! A DNS family is spelled in four places, and each of them can be *separately*
//! correct while the path as a whole is broken:
//!
//! 1. the account center's vocabulary ([`dns_provider::ALL`]) — what an operator
//!    is allowed to register;
//! 2. the engine's [`DnsProviderKind`] — what can actually be driven;
//! 3. the **console's own lists** — the families the account form offers and the
//!    credential fields it asks for;
//! 4. the **issuance path** — the code that turns a resolved credential into a
//!    presenter and talks to the vendor.
//!
//! `dns_provider_parity.rs` closes 1↔2 and the contract between them. This file
//! closes the remaining two, because they are the ones that fail *silently*:
//!
//! * A family missing from the console's list still satisfies every Rust check.
//!   The server accepts it, the engine drives it, and no operator can ever pick
//!   it — the capability exists and is unreachable.
//! * A credential field asked for on the wrong families produces a form an
//!   operator fills in correctly and the server refuses, or one that omits a
//!   field the server requires. The console's credential table is typed as
//!   `Record<CloudAccountDnsProvider, …>`, so TypeScript forces every family to
//!   be *present* — but nothing forces the fields to match what the server wants,
//!   and nothing forces the *list* to stay complete.
//! * An adapter with a perfect `verify_account` that no host ever calls is dead
//!   code wearing the shape of a feature. That is exactly what happened to the
//!   provider probe: it was implemented, tested per adapter, and unreachable from
//!   the real issuance host, so a wrong DNS credential was still discovered
//!   several round trips into an order.
//!
//! The console is read as source rather than through a renderer because these are
//! *declarations*, not behaviour: the question is what the file offers, and a
//! shallow typed table is the whole answer. The issuance path is read as source
//! for the same reason — the question is whether the call is there and whether it
//! comes first, which no runtime assertion over this crate can see.

use std::collections::BTreeSet;

use sdkwork_deploy_cloud_account_port::dns_provider;
use sdkwork_intelligence_deploy_service::{build_dns01_presenter, DeployDnsProviderCredential};
use sdkwork_webserver_acme_service::DnsProviderKind;

/// The delivery console's provider surfaces.
const DELIVERY_CONSOLE: &str = include_str!(
    "../../../apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-delivery/src/DeliveryManagement.tsx"
);

/// The host's issuance path, which is what makes a presenter reachable.
const ISSUANCE_SERVICE: &str =
    include_str!("../../sdkwork-intelligence-deploy-service/src/certificate_issuance.rs");

/// The smallest `HTTP_REQUEST` configuration the generic family accepts.
const HTTP_REQUEST_CONFIG: &str = r#"{
  "publish": { "url": "https://api.example.com/records", "body": "{{recordValue}}" },
  "withdraw": { "url": "https://api.example.com/records" }
}"#;

/// The `(identifier, secret)` pair that is well formed for one family.
///
/// The two shapes the account center stores, spelled the way each family needs
/// them: a credential the family cannot read is not a family that is unreachable.
fn well_formed_credential(family: &str) -> (&'static str, &'static str) {
    match family {
        dns_provider::HTTP_REQUEST => ("", HTTP_REQUEST_CONFIG),
        // Cloudflare has no public half; the two key-pair families do.
        dns_provider::CLOUDFLARE => ("", "probe-secret"),
        _ => ("probe-identifier", "probe-secret"),
    }
}

fn vocabulary() -> BTreeSet<String> {
    dns_provider::ALL
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

/// The families the console's account form offers, in source order.
///
/// Anchors on `= [` rather than on the first `[`: the declaration's type is
/// `readonly CloudAccountDnsProvider[]`, whose empty `[]` sits earlier in the
/// line. Slicing from there produced a list containing `] = ["ALIYUN_DNS`, which
/// is how this parser first read a four-family list as four *wrong* families —
/// and a parser whose output is obviously wrong is the lucky case.
fn console_offered_families() -> Vec<String> {
    let declaration = DELIVERY_CONSOLE
        .lines()
        .find(|line| line.trim_start().starts_with("const DNS_FAMILIES"))
        .expect("the console must declare the providers it offers");
    let open = declaration
        .find("= [")
        .expect("the console's provider list must be a list literal")
        + "= [".len();
    let close = declaration
        .rfind(']')
        .expect("the console's provider list must be closed");
    let listed = declaration
        .get(open..close)
        .expect("the list literal must be well formed");
    listed
        .split(',')
        .map(|value| value.trim().trim_matches('"').to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

/// The body of one `impl` member: its signature up to the next member.
///
/// Reading a *member's* body rather than the whole file is what makes an
/// ordering assertion mean anything here. The file defines
/// `fulfil_certificate_order` before `build_issuance_request`, while the runtime
/// order is the opposite — the former calls the latter — so a positional
/// comparison over the file text asserts the layout of the file, not the order of
/// the calls.
fn member_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("{signature} is gone from the issuance path"));
    let rest = &source[start..];
    let end = [
        "\n    async fn ",
        "\n    fn ",
        "\n    pub async fn ",
        "\n    pub fn ",
    ]
    .iter()
    .filter_map(|marker| rest[1..].find(marker).map(|offset| offset + 1))
    .min()
    .unwrap_or(rest.len());
    &rest[..end]
}

/// Each family's block in the console's credential-field projection.
///
/// Read as a *region per family* rather than by searching the whole file for a
/// field name: `identifierLabel` appears once per family, and a whole-file search
/// would let one family's answer stand in for all four.
fn console_credential_field_blocks() -> Vec<(String, String)> {
    let lines: Vec<&str> = DELIVERY_CONSOLE.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with("const CLOUD_ACCOUNT_CREDENTIAL_FIELDS"))
        .expect("the console must project each family's credential shape");
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_end() == "};")
        .map(|offset| start + offset)
        .expect("the credential table must be closed");
    let body = &lines[start..=end];

    // A top-level entry is two-space indented and opens an object.
    let keys: Vec<(usize, String)> = body
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let rest = line.strip_prefix("  ")?;
            if rest.starts_with(' ') {
                return None;
            }
            let (name, tail) = rest.split_once(':')?;
            tail.trim_start()
                .starts_with('{')
                .then(|| (index, name.trim().to_owned()))
        })
        .collect();
    assert!(
        !keys.is_empty(),
        "the console projection lists no family, so every comparison here is vacuous"
    );
    keys.iter()
        .enumerate()
        .map(|(position, (index, name))| {
            let until = keys
                .get(position + 1)
                .map(|(next, _)| *next)
                .unwrap_or(body.len());
            (name.clone(), body[*index..until].join("\n"))
        })
        .collect()
}

/// The issuance source with its own test module removed.
///
/// `include_str!` carries the test block too, and the block names the very
/// function under test — so an assertion over the whole file could be satisfied
/// by this file's own literals. Slicing first is what makes the assertion about
/// production code.
fn issuance_production_source() -> String {
    ISSUANCE_SERVICE
        .split("#[cfg(test)]")
        .next()
        .expect("split always yields a head")
        .to_owned()
}

/// The end-to-end hop for every family: the account center's vocabulary entry
/// becomes a credential, and that credential becomes a presenter that says which
/// provider it speaks to.
///
/// This is the assertion that would have caught a family offered for binding but
/// never wired into the adapter layer — the failure mode where an operator stores
/// a credential that looks fine and is refused at the first order.
#[test]
fn every_family_becomes_a_presenter_that_speaks_its_own_family() {
    for family in &vocabulary() {
        let (identifier, secret) = well_formed_credential(family);
        let credential =
            DeployDnsProviderCredential::from_account_credential(family, identifier, secret)
                .unwrap_or_else(|error| {
                    panic!("{family} is offered for binding but cannot be built: {error}")
                });
        let presenter =
            build_dns01_presenter(&credential, Some("zone-ref")).unwrap_or_else(|error| {
                panic!("{family} builds a credential no presenter accepts: {error}")
            });
        let expected = DnsProviderKind::parse(family)
            .unwrap_or_else(|| panic!("the engine cannot parse {family}"));
        assert_eq!(
            presenter.provider_kind(),
            Some(expected),
            "{family} builds a presenter that speaks to a different provider"
        );
    }
}

/// The console must offer exactly the families the engine can drive.
///
/// Kept as set equality in both directions: a family the console omits is a
/// capability nobody can reach, and a family it offers that the engine cannot
/// parse is a form that produces a credential refused at the first order.
#[test]
fn the_console_offers_exactly_the_families_the_engine_can_drive() {
    let offered = console_offered_families();
    let distinct: BTreeSet<String> = offered.iter().cloned().collect();
    assert_eq!(
        distinct.len(),
        offered.len(),
        "the console's provider list repeats a family, which makes a set comparison lenient"
    );
    assert_eq!(
        distinct,
        vocabulary(),
        "the console and the account center disagree about which providers exist; the console's \
         list is hand-written, so a new family has to be added here too"
    );
}

/// The console has to ask for exactly the fields the server requires.
///
/// The property under test is *which families have a public half*, taken from the
/// credential layer rather than restated: a form that asks Cloudflare for an
/// identifier asks for something it cannot supply, and one that omits Aliyun's
/// AccessKeyId produces a credential the server refuses after the operator has
/// already pasted the secret.
#[test]
fn the_console_asks_for_the_credential_fields_each_family_needs() {
    let blocks = console_credential_field_blocks();
    let listed: BTreeSet<String> = blocks.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(
        listed,
        vocabulary(),
        "the console's credential projection and the provider vocabulary disagree"
    );

    for (family, body) in &blocks {
        // The server's own answer, so the two cannot drift.
        let empty_identifier_refused =
            DeployDnsProviderCredential::from_account_credential(family, "", "probe-secret")
                .is_err();
        assert!(
            DeployDnsProviderCredential::from_account_credential(
                family,
                "probe-id",
                HTTP_REQUEST_CONFIG
            )
            .is_ok(),
            "{family} refuses a well-formed credential, so the comparison below is vacuous"
        );
        let console_asks_for_one = !body.contains("identifierLabel: undefined");
        assert_eq!(
            console_asks_for_one,
            empty_identifier_refused,
            "{family}: the console {} ask for an identifier while the server {} one",
            if console_asks_for_one {
                "does"
            } else {
                "does not"
            },
            if empty_identifier_refused {
                "requires"
            } else {
                "ignores"
            }
        );
    }
}

/// The provider probe must be **called**, and called before the order is
/// presented.
///
/// The adapters' `verify_account` was implemented and unit-tested per family, and
/// no host called it: a wrong DNS credential was still discovered by the CA
/// several round trips into an order. Nothing in a type signature can see "this
/// function is never invoked", so the call, and its position, are asserted here.
#[test]
fn the_issuance_path_probes_the_account_before_the_order_is_presented() {
    let source = issuance_production_source();
    assert!(
        source.contains("async fn probe_dns_account("),
        "the issuance path no longer defines a provider probe"
    );

    // The request is built — and therefore probed — before the engine is called.
    let fulfilment = member_body(&source, "async fn fulfil_certificate_order(");
    let built = fulfilment
        .find(".build_issuance_request(")
        .expect("the order must be built before it is presented");
    let presented = fulfilment
        .find("issuer.issue(request)")
        .expect("the order must be presented to the engine");
    assert!(
        built < presented,
        "the probe must run before the order is presented, or its refusal arrives after the CA has \
         already been contacted"
    );

    // ...and the building step is where the probe is called, so a future edit
    // cannot keep the ordering while dropping the call.
    let building = member_body(&source, "async fn build_issuance_request(");
    assert!(
        building.contains("probe_dns_account(context)"),
        "the issuance path never probes the resolved account, so the adapters' verify_account is \
         unreachable from the host and a wrong credential is only found mid-order"
    );
}

/// The probe's verdict must reach the operator through the order, not just the
/// log.
///
/// A refusal the probe classifies as a configuration mistake is reported as a
/// [`sdkwork_deploy_contract::DeployServiceError::Conflict`], and that is the
/// variant the order's failure-detail extractor reads. If either half moves, the
/// vendor's sentence stops reaching the console and the operator is back to the
/// generic conflict code — which is the defect this whole path exists to remove.
#[test]
fn a_probe_refusal_is_the_variant_that_reaches_the_console() {
    let source = issuance_production_source();
    let branch = source
        .find("Err(AcmeServiceError::Config(message))")
        .expect("the probe must single out the refusals that prove the account is wrong");
    let window = &source[branch..source.len().min(branch + 400)];
    assert!(
        window.contains("DeployServiceError::Conflict"),
        "a configuration refusal must be reported as a Conflict, which is what carries a detail \
         onto the order: {window}"
    );
    assert!(
        window.contains("message"),
        "the provider's own sentence must be carried into the conflict, not replaced: {window}"
    );

    // The other half: the detail extractor reads exactly that variant.
    let extractor = source
        .find("fn issuance_failure_detail(")
        .expect("the order failure detail must be extracted somewhere");
    let extractor_body = &source[extractor..source.len().min(extractor + 400)];
    assert!(
        extractor_body.contains("DeployServiceError::Conflict(message)"),
        "the order's detail extractor no longer reads a Conflict, so nothing a probe reports can \
         reach the console: {extractor_body}"
    );
}

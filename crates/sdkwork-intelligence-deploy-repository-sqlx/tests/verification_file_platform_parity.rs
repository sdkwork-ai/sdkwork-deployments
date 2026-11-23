//! The verification-file vocabulary is one set, spelled in four places.
//!
//! A platform is renamed or added in the baseline DDL, in the service's
//! validator, in the contract the SDKs are generated from, and in the console
//! pickers. Every one of those is a place where a one-sided edit compiles and
//! ships: the console offers a platform the server refuses, or the server stores
//! a platform the contract cannot name. This test reads the machine-readable
//! halves of that set and fails on either side of the comparison.
//!
//! It reads only the authored baseline and the materialised contract, so it
//! needs no database and no network; `tests/domain_zones_postgres.rs` applies the
//! same baseline as SQL against a real server.

use std::collections::BTreeSet;

use sdkwork_deploy_contract::{
    content_type_for_extension, platform_accepts_extension, verification_platform,
    verification_platform_codes, VERIFICATION_FILE_EXTENSIONS,
};

const POSTGRES_BASELINE: &str =
    include_str!("../../../database/ddl/baseline/postgres/0001_deploy_baseline.sql");

/// The materialised app-api contract, which is what the SDKs are generated from.
const APP_API_CONTRACT: &str =
    include_str!("../../../apis/app-api/deploy/deploy-app-api.openapi.json");

const PLATFORM_CONSTRAINT: &str = "chk_deploy_dns_zone_verification_file_platform";
const FILE_NAME_CONSTRAINT: &str = "chk_deploy_dns_zone_verification_file_name";

/// The value list of `CONSTRAINT <name> CHECK (<column> IN ( … ))`.
fn admitted_values(baseline: &str, constraint: &str) -> BTreeSet<String> {
    let start = baseline
        .find(constraint)
        .unwrap_or_else(|| panic!("{constraint} is not declared in the baseline"));
    let rest = &baseline[start..];
    let open = rest
        .find("IN (")
        .unwrap_or_else(|| panic!("{constraint} carries no IN (…) list"))
        + "IN (".len();
    let close = rest[open..]
        .find(')')
        .unwrap_or_else(|| panic!("{constraint}'s IN (…) list is unterminated"));
    rest[open..open + close]
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn contract() -> serde_json::Value {
    serde_json::from_str(APP_API_CONTRACT).expect("the materialised app-api contract is valid JSON")
}

fn contract_platforms(schema: &str) -> BTreeSet<String> {
    contract()["components"]["schemas"][schema]["properties"]["platform"]["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("{schema}.platform is not a closed enum"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{schema}.platform holds a non-string entry"))
                .to_owned()
        })
        .collect()
}

#[test]
fn the_ddl_and_the_service_agree_on_the_platform_set() {
    let ddl: BTreeSet<String> = admitted_values(POSTGRES_BASELINE, PLATFORM_CONSTRAINT);
    let service: BTreeSet<String> = verification_platform_codes().map(str::to_owned).collect();
    assert_eq!(
        ddl, service,
        "the baseline's platform CHECK and the verification-file catalog disagree; a stored value \
         must be one the service validates, and a validated value must be one the schema admits"
    );
}

/// The catalog's per-platform rules are the reason it exists: each platform's
/// issued file has a fixed shape, and every extension it names is one the edge
/// answers.
#[test]
fn every_platform_declares_a_shape_the_edge_can_serve() {
    for code in verification_platform_codes() {
        let spec = verification_platform(code).expect("the code comes from the catalog");
        assert!(
            VERIFICATION_FILE_EXTENSIONS.contains(&spec.extension),
            "{code} declares extension {} outside the servable set",
            spec.extension
        );
        assert!(
            spec.file_name_hint
                .to_ascii_lowercase()
                .contains(&format!(".{}", spec.extension)),
            "{code}'s hint {} does not name its own extension",
            spec.file_name_hint
        );
        assert!(
            spec.label_key.starts_with("domain.verification.platform."),
            "{code}'s label key must stay in the shared namespace"
        );
        assert!(
            platform_accepts_extension(code, spec.extension),
            "{code} must accept its own extension"
        );
        assert_eq!(
            content_type_for_extension(spec.extension),
            if spec.extension == "txt" {
                "text/plain; charset=utf-8"
            } else if spec.extension == "html" {
                "text/html; charset=utf-8"
            } else {
                "application/xml; charset=utf-8"
            },
            "{code}'s content type drifted from the extension"
        );
    }
    // A fixed-name platform pins the name regardless of the token.
    assert_eq!(
        sdkwork_deploy_contract::suggested_verification_file_name("APPLE", "ignored").as_deref(),
        Some("apple-developer-domain-association.txt")
    );
    // The escape hatch keeps every servable extension.
    for extension in VERIFICATION_FILE_EXTENSIONS {
        assert!(platform_accepts_extension("CUSTOM", extension));
    }
}

#[test]
fn the_contract_admits_exactly_the_platforms_the_schema_does() {
    let ddl: BTreeSet<String> = admitted_values(POSTGRES_BASELINE, PLATFORM_CONSTRAINT);
    // Both directions: the response enum is what a console renders, and the
    // create enum is what a caller may send.
    assert_eq!(
        contract_platforms("DomainVerificationFileResponse"),
        ddl,
        "DomainVerificationFileResponse.platform does not match the baseline"
    );
    assert_eq!(
        contract_platforms("CreateDomainVerificationFileRequest"),
        ddl,
        "CreateDomainVerificationFileRequest.platform does not match the baseline"
    );
}

#[test]
fn the_ddl_regex_is_the_contract_pattern_verbatim() {
    // One regex, two homes. Keeping them byte-identical is what makes the
    // client-side pre-check and the schema's refusal agree; a client that
    // accepts what the database rejects is an operator-visible 500.
    const EXPECTED_PATTERN: &str = r"'^[A-Za-z0-9][A-Za-z0-9._-]*\.(txt|html|xml)$'";
    let start = POSTGRES_BASELINE
        .find(FILE_NAME_CONSTRAINT)
        .expect("the file-name CHECK is declared");
    let region = &POSTGRES_BASELINE[start..];
    let region = &region[..region.find(';').expect("the CHECK statement terminates")];
    assert!(
        region.contains(EXPECTED_PATTERN),
        "{FILE_NAME_CONSTRAINT} no longer carries the expected pattern"
    );
    assert_eq!(
        contract()["components"]["schemas"]["CreateDomainVerificationFileRequest"]["properties"]
            ["fileName"]["pattern"]
            .as_str()
            .expect("the create request declares a pattern"),
        "^[A-Za-z0-9][A-Za-z0-9._-]*\\.(txt|html|xml)$",
        "the contract's fileName pattern drifted from the schema's"
    );
    // Every admitted extension is one the service validates.
    let admitted: BTreeSet<String> = ["txt", "html", "xml"]
        .iter()
        .map(|extension| (*extension).to_owned())
        .collect();
    assert_eq!(
        VERIFICATION_FILE_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_owned())
            .collect::<BTreeSet<String>>(),
        admitted,
        "the service validates a different extension set than the schema and contract admit"
    );
}

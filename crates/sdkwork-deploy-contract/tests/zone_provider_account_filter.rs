//! The zone cloud-account pin filter: its wire shape and its three states.
//!
//! The root-domain list's cloud-account facet depends on two readings that nothing
//! else exercises: what the query member means when it is absent, blank, or the
//! reserved literal. Both fail silently when they are wrong — a filter that reads
//! `ALL` as an account id answers an empty list, and one that reads the reserved
//! word as an id hides exactly the zones the operator asked to see.

use sdkwork_deploy_contract::{
    ListDomainZonesQuery, ZoneProviderAccountFilter, ZONE_PROVIDER_ACCOUNT_UNASSIGNED,
};

fn filter(query: &str) -> ZoneProviderAccountFilter {
    let parsed: ListDomainZonesQuery = serde_json::from_str(query).expect("query member parses");
    parsed.provider_account_filter()
}

#[test]
fn the_filter_reads_absent_blank_reserved_and_named_members_apart() {
    // An absent member filters nothing.
    assert_eq!(filter("{}"), ZoneProviderAccountFilter::Any);
    // A blank member is the same omission: that is what a cleared form field
    // submits, and reading it as an account named "" would answer an empty list.
    assert_eq!(
        filter(r#"{"provider_account_id":""}"#),
        ZoneProviderAccountFilter::Any,
    );
    assert_eq!(
        filter(r#"{"provider_account_id":"   "}"#),
        ZoneProviderAccountFilter::Any,
    );
    // The reserved literal is the "pinned to nothing" arm.
    assert_eq!(
        filter(r#"{"provider_account_id":"UNASSIGNED"}"#),
        ZoneProviderAccountFilter::Unassigned,
    );
    assert_eq!(
        filter(r#"{"provider_account_id":"UNASSIGNED "}"#),
        ZoneProviderAccountFilter::Unassigned,
        "surrounding whitespace is trimmed before the reserved word is read",
    );
    // Anything else names an account, kept verbatim: it is a reference into the
    // account center's table, so this surface does not re-case it.
    assert_eq!(
        filter(r#"{"provider_account_id":"aliyun-prod"}"#),
        ZoneProviderAccountFilter::Assigned("aliyun-prod".to_owned()),
    );
    // Case matters. Folding it would make `unassigned` mean something the caller's
    // own account id `unassigned` does not.
    assert_eq!(
        filter(r#"{"provider_account_id":"unassigned"}"#),
        ZoneProviderAccountFilter::Assigned("unassigned".to_owned()),
    );
}

#[test]
fn the_facet_composes_with_the_other_listing_members() {
    // The pin is a facet on the same listing as `scope` and `keyword`, not a
    // replacement for them: a request that sends all three must keep all three, or
    // the page would silently widen back to every zone in the tenant.
    let parsed: ListDomainZonesQuery = serde_json::from_str(
        r#"{"page":2,"page_size":20,"keyword":"example","scope":"USER","provider_account_id":"aliyun-prod"}"#,
    )
    .expect("multi-facet query parses");
    assert_eq!(parsed.page, 2);
    assert_eq!(parsed.page_size, 20);
    assert_eq!(parsed.keyword.as_deref(), Some("example"));
    assert_eq!(parsed.scope.map(|scope| scope.as_str()), Some("USER"));
    assert_eq!(
        parsed.provider_account_filter(),
        ZoneProviderAccountFilter::Assigned("aliyun-prod".to_owned()),
    );
}

#[test]
fn the_reserved_literal_is_a_fixed_word() {
    // Pinned because it is a wire contract shared with the console: renaming it
    // here without the client would make the "no cloud account" option send an
    // account id that does not exist, which answers an empty list rather than an
    // error.
    assert_eq!(ZONE_PROVIDER_ACCOUNT_UNASSIGNED, "UNASSIGNED");
}

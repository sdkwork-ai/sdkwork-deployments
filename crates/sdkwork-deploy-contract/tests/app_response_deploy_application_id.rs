//! `AppResponse.deployApplicationId`: the decimal application id that IAM anchors on.
//!
//! `AppResponse.id` is `deploy_app.uuid`, the opaque address every app endpoint
//! resolves against; the IAM installation anchor is the row's `BIGINT`
//! `deploy_app.id`. `identity_provision` sends that decimal id to IAM, and the
//! minted token's `app_id` / `aud` are the same number — so a caller that creates
//! an application and then has to prove which installation it owns can only read
//! it back from this field.
//!
//! Two properties are load-bearing and both are asserted here:
//!
//! 1. the field is on the wire under the contract's name, and
//! 2. it is a **string**, never a JSON number. `API_SPEC.md` §13.6: a number
//!    silently rounds ids past `Number.MAX_SAFE_INTEGER` (2^53), and the rounded
//!    value then addresses a different application in IAM.

use sdkwork_deploy_contract::AppResponse;

/// Ids this service issues are snowflakes (>= 2^63-ish in the general case), so
/// the fixture deliberately uses one that a browser would round and one that it
/// would not.
const SAFE_ID: i64 = 5_001;
const ROUNDING_ID: i64 = 9_007_199_254_740_993; // 2^53 + 1

fn response(deploy_application_id: i64) -> AppResponse {
    AppResponse {
        id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
        deploy_application_id,
        name: "Release App".to_owned(),
        slug: "release-app".to_owned(),
        app_kind: "MOBILE".to_owned(),
        app_status: "ACTIVE".to_owned(),
        default_environment: "production".to_owned(),
        app_domain_label: "release-app".to_owned(),
        app_domain_suffixes: vec!["example.com".to_owned()],
        owner_type: sdkwork_deploy_contract::AppOwnerType::Tenant,
        tenant_id: "7".to_owned(),
        created_at: "2026-01-01T00:00:00.000Z".to_owned(),
        updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
        version: "1".to_owned(),
        ..AppResponse::default()
    }
}

#[test]
fn the_response_carries_the_decimal_application_id_beside_the_uuid() {
    let json = serde_json::to_value(response(SAFE_ID)).expect("AppResponse serializes");

    // The two identities are distinct facts and both are on the wire.
    assert_eq!(
        json["id"], "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "`id` stays the uuid every app endpoint resolves against",
    );
    assert_eq!(
        json["deployApplicationId"], "5001",
        "`deployApplicationId` is the decimal deploy_app.id, not the uuid",
    );
}

#[test]
fn the_decimal_application_id_is_a_string_not_a_number() {
    let json = serde_json::to_value(response(SAFE_ID)).expect("AppResponse serializes");
    assert!(
        json["deployApplicationId"].is_string(),
        "int64 wire fields MUST be strings (API_SPEC §13.6); a JSON number lets browsers \
         round the id past 2^53 and then replay a wrong application id",
    );
    assert!(
        !json["deployApplicationId"].is_number(),
        "`type: integer, format: int64` is a contract violation",
    );
    assert!(
        json["deployApplicationId"].as_str().is_some_and(|value| {
            value
                .strip_prefix('-')
                .unwrap_or(value)
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                && !value.trim_matches('-').is_empty()
        }),
        "the string form matches the contract pattern ^-?[0-9]+$",
    );
}

#[test]
fn an_id_past_two_to_the_53_survives_serialization_exactly() {
    // 2^53 + 1 is the first integer a IEEE-754 double cannot represent; reading it
    // through a JS `number` would yield 2^53 and address the neighbouring
    // application. The string form keeps every digit.
    let json = serde_json::to_value(response(ROUNDING_ID)).expect("AppResponse serializes");
    assert_eq!(
        json["deployApplicationId"], "9007199254740993",
        "the decimal id is serialized digit-for-digit, never through an f64",
    );
}

#[test]
fn the_decimal_application_id_round_trips_from_its_string_form() {
    let original = response(ROUNDING_ID);
    let encoded = serde_json::to_string(&original).expect("AppResponse serializes");
    let decoded: AppResponse = serde_json::from_str(&encoded).expect("AppResponse deserializes");
    assert_eq!(
        decoded.deploy_application_id, ROUNDING_ID,
        "the request boundary parses the same decimal string form it emits",
    );
}

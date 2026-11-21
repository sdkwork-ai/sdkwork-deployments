//! `AppIdentityProvisionResponse.deployApplicationId`: the register-command twin
//! of `AppResponse.deployApplicationId`.
//!
//! `apps.identityProvision` is the replay entry that (re)registers an
//! application's IAM installation. `appId` on that response is
//! `deploy_app.uuid`; the installation anchor IAM stored is the row's `BIGINT`
//! `deploy_app.id`. A caller that just re-registered must read that same anchor
//! back from the command that performed it, exactly as it does after
//! `apps.create` / `apps.update`, instead of deriving it from the uuid.
//!
//! Two properties are load-bearing:
//!
//! 1. the field is on the wire under the contract's name, and
//! 2. it is a **string**, never a JSON number (`API_SPEC.md` §13.6): a snowflake
//!    is past `Number.MAX_SAFE_INTEGER`, so a browser reading it as a number
//!    would round it onto a neighbouring application.

use sdkwork_deploy_contract::AppIdentityProvisionResponse;

/// 2^53 + 1 is the first integer an IEEE-754 double cannot represent.
const ROUNDING_ID: i64 = 9_007_199_254_740_993;

fn response(deploy_application_id: i64) -> AppIdentityProvisionResponse {
    AppIdentityProvisionResponse {
        app_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
        deploy_application_id,
        identity_status: "active".to_owned(),
    }
}

#[test]
fn the_register_response_carries_the_decimal_application_id_beside_the_uuid() {
    let json =
        serde_json::to_value(response(5_001)).expect("AppIdentityProvisionResponse serializes");

    assert_eq!(
        json["appId"], "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "`appId` stays the uuid the app endpoints resolve against",
    );
    assert_eq!(
        json["deployApplicationId"], "5001",
        "`deployApplicationId` is the decimal deploy_app.id, not the uuid",
    );
}

#[test]
fn the_register_response_id_is_a_string_not_a_number() {
    let json = serde_json::to_value(response(ROUNDING_ID))
        .expect("AppIdentityProvisionResponse serializes");

    assert!(
        json["deployApplicationId"].is_string(),
        "int64 wire fields MUST be strings (API_SPEC §13.6)",
    );
    assert_eq!(
        json["deployApplicationId"], "9007199254740993",
        "the decimal id is serialized digit-for-digit, never through an f64",
    );
}

#[test]
fn the_register_response_id_round_trips_from_its_string_form() {
    let original = response(ROUNDING_ID);
    let encoded =
        serde_json::to_string(&original).expect("AppIdentityProvisionResponse serializes");
    let decoded: AppIdentityProvisionResponse =
        serde_json::from_str(&encoded).expect("AppIdentityProvisionResponse deserializes");

    assert_eq!(
        decoded.deploy_application_id, ROUNDING_ID,
        "the request boundary parses the same decimal string form it emits",
    );
}

//! Application **resource link** DTOs: the account bindings an application
//! environment publishes through.
//!
//! An application is not only a bundle. A production app serves through a WeChat
//! official account, a mini program, and a payment provider account, and each of
//! those is issued *per environment*: a production app must never inherit a
//! development official account or a sandbox merchant account. `deploy_application_resource_link`
//! stores exactly that fact — one row per (app, environment, kind, provider-side
//! reference).
//!
//! # Why this table is the authority
//!
//! The bindings used to exist only inside `deploy_app.metadata.integrations`
//! (`sdkwork.application.integrations/v1`): a nested JSON document with no
//! uniqueness, no referential shape and no way to ask "which applications use
//! this official account". The document survives as a *legacy input* that the
//! ordered migration `0013_deploy_application_resource_link_backfill` normalizes
//! once; every read and write after that goes through this contract.
//!
//! `deploy_app_platform_target.app_id_value` is **not** a second authority for a
//! mini program: the platform target records the deployment unit and the
//! platform-side identifier a build/package/release hangs off, while the resource
//! link records *which account the published environment acts as*. Both name a
//! WeChat AppID, but they answer different questions, and the resource link is
//! the one the runtime resolves. See
//! `docs/architecture/tech/TECH-application-resource-bindings.md`.
//!
//! # One vocabulary, from the CHECK constraint up
//!
//! [`AppResourceKind`] serializes to exactly the values the column CHECK accepts
//! (`official_account`, `mini_program`, `payment_account`), so a value never has
//! to be translated between the DTO, the SQL parameter and the SDK. The console's
//! legacy document spells its account kinds differently (`official-account`); the
//! translation lives in the backfill, once, instead of at every call site.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_composition::AppPublishEnvironment;
use crate::problem::{DeployServiceError, DeployServiceResult};

/// Longest provider-side reference the column stores (`resource_ref VARCHAR(128)`).
pub const APP_RESOURCE_LINK_REF_MAX_LEN: usize = 128;

/// Longest provider code the column stores (`provider_code VARCHAR(32)`).
pub const APP_RESOURCE_LINK_PROVIDER_CODE_MAX_LEN: usize = 32;

/// Which kind of account a link binds.
///
/// Spelled exactly like `chk_deploy_application_resource_link_kind`, so the enum
/// value *is* the column value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppResourceKind {
    /// A WeChat (or compatible) official account.
    OfficialAccount,
    /// A WeChat (or compatible) mini program.
    MiniProgram,
    /// A payment provider merchant account.
    PaymentAccount,
}

impl AppResourceKind {
    pub const ALL: [Self; 3] = [
        Self::OfficialAccount,
        Self::MiniProgram,
        Self::PaymentAccount,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OfficialAccount => "official_account",
            Self::MiniProgram => "mini_program",
            Self::PaymentAccount => "payment_account",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    /// Whether an environment may bind at most one row of this kind.
    ///
    /// The single-instance rule is a database fact
    /// (`uk_deploy_application_resource_link_single`) because "the environment's
    /// official account" is one account, not a collection: two ACTIVE rows would
    /// leave the runtime picking arbitrarily. Payment accounts are a collection —
    /// one per provider, plus a sandbox/production pair — so they are exempt, and
    /// their cardinality is governed by `uk_deploy_application_resource_link_default`
    /// (one default per provider).
    pub const fn is_single_instance(self) -> bool {
        matches!(self, Self::OfficialAccount | Self::MiniProgram)
    }

    /// The values the column CHECK accepts, for documentation and tests.
    pub fn all_values() -> Vec<&'static str> {
        Self::ALL.into_iter().map(Self::as_str).collect()
    }
}

/// Whether the binding is live.
///
/// `DISABLED` keeps the row (and therefore the audit trail and the "this account
/// is the one that used to serve production" answer) while taking it out of
/// service, which is why it is a status and not a delete.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppResourceLinkStatus {
    #[default]
    Active,
    Disabled,
}

impl AppResourceLinkStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Disabled => "DISABLED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ACTIVE" => Some(Self::Active),
            "DISABLED" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// One account binding as the API returns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppResourceLinkResponse {
    /// The link's public uuid (the table's `id` column).
    pub id: String,
    /// The owning application's public uuid.
    #[serde(rename = "appId")]
    pub app_id: String,
    /// The lifecycle environment the binding is issued for.
    pub environment: AppPublishEnvironment,
    /// Which kind of account is bound.
    #[serde(rename = "resourceKind")]
    pub resource_kind: AppResourceKind,
    /// The provider-side identifier: the WeChat AppID for a channel account, the
    /// merchant account number for a payment account.
    #[serde(rename = "resourceRef")]
    pub resource_ref: String,
    /// The provider the account belongs to (`wechat`, `alipay`, `stripe`, …).
    #[serde(
        rename = "providerCode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub provider_code: Option<String>,
    /// Whether this is the environment's default account *for its provider*.
    /// Always `false` for the single-instance kinds: there is only one.
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    /// `ACTIVE` | `DISABLED`.
    #[serde(rename = "bindingStatus")]
    pub binding_status: AppResourceLinkStatus,
    /// Display snapshot and provider facts the binding was approved with.
    #[serde(default)]
    pub metadata: Value,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    /// Optimistic-concurrency token (int64 on the wire per API_SPEC §13.6).
    pub version: String,
}

/// A bounded collection: an environment binds at most a handful of accounts, so
/// the page envelope carries the v3 list shape without meaning to paginate deeply.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppResourceLinkPage {
    pub items: Vec<AppResourceLinkResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// `apps.resourceLinks.list` filters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListAppResourceLinksQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    /// Narrow the listing to one lifecycle environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<AppPublishEnvironment>,
    /// Narrow the listing to one account kind.
    ///
    /// Deliberately *not* renamed to `resourceKind`: this is the wire query
    /// parameter (`?resource_kind=`), and API_SPEC §13 requires multi-word query
    /// parameters to be `lower_snake_case`. A `rename` here would leave the
    /// documented filter silently ignored, because the deserializer only matches
    /// the declared name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<AppResourceKind>,
}

/// Bind (or re-bind) one account.
///
/// The write is an **explicit upsert**, which is why it is a `PUT` documented as
/// replacing the addressed binding rather than a hidden create (API_SPEC §15.4):
///
/// * a single-instance kind ([`AppResourceKind::is_single_instance`]) is addressed
///   by `(app, environment, kind)`, so `resourceRef` is a *replacable* attribute —
///   switching an environment from one official account to another is one call,
///   not a delete plus a create that a failure could leave half-done;
/// * a payment account is addressed by `(app, environment, kind, resourceRef)`,
///   so binding a second merchant account adds a row.
///
/// `isDefault` is only meaningful for a payment account, where at most one row per
/// `(environment, providerCode)` may carry it; setting it on another row moves the
/// default in the same transaction instead of failing on the unique index.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAppResourceLinkRequest {
    pub environment: AppPublishEnvironment,
    #[serde(rename = "resourceKind")]
    pub resource_kind: AppResourceKind,
    #[serde(rename = "resourceRef")]
    pub resource_ref: String,
    /// Required for a payment account: the default-uniqueness key is
    /// `(environment, providerCode)` and a NULL provider would make every row its
    /// own default.
    #[serde(
        rename = "providerCode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub provider_code: Option<String>,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
    /// Absent means `ACTIVE`: a `PUT` replaces the whole binding, so an omitted
    /// status is the live state, not "leave whatever was there".
    #[serde(rename = "bindingStatus", default)]
    pub binding_status: AppResourceLinkStatus,
    /// Display snapshot and provider facts; absent means `{}`.
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A validated binding, ready for the repository.
///
/// Produced by [`UpsertAppResourceLinkRequest::normalize`] and used unchanged by
/// the repository, so the checks below run exactly once and cannot drift between
/// the service and the SQL layer.
// Not `Eq`: `metadata` is a `serde_json::Value`, and a JSON number may be a
// float. `PartialEq` is what the tests compare.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedAppResourceLink {
    pub environment: &'static str,
    pub resource_kind: AppResourceKind,
    pub resource_ref: String,
    pub provider_code: Option<String>,
    pub is_default: bool,
    pub binding_status: AppResourceLinkStatus,
    pub metadata: Value,
}

impl NormalizedAppResourceLink {
    /// The natural key the write addresses.
    ///
    /// Exposed so the repository and the tests agree on which rows are "the same
    /// binding": for a single-instance kind the reference is not part of it.
    pub fn natural_key(&self) -> String {
        if self.resource_kind.is_single_instance() {
            format!("{}|{}", self.environment, self.resource_kind.as_str())
        } else {
            format!(
                "{}|{}|{}",
                self.environment,
                self.resource_kind.as_str(),
                self.resource_ref
            )
        }
    }
}

impl UpsertAppResourceLinkRequest {
    /// Validates and normalizes the request.
    ///
    /// Every rule here is also a database constraint, so the database stays the
    /// last word; this function exists so the operator sees a `40003` with a
    /// sentence instead of a `23514` the HTTP layer can only mask.
    pub fn normalize(&self) -> DeployServiceResult<NormalizedAppResourceLink> {
        let resource_ref = self.resource_ref.trim();
        if resource_ref.is_empty() {
            return Err(DeployServiceError::validation(
                "resourceRef is required: an account binding without the provider-side identifier cannot be resolved",
            ));
        }
        if resource_ref.chars().count() > APP_RESOURCE_LINK_REF_MAX_LEN {
            return Err(DeployServiceError::validation(format!(
                "resourceRef must be at most {APP_RESOURCE_LINK_REF_MAX_LEN} characters"
            )));
        }
        if resource_ref.chars().any(char::is_control) {
            return Err(DeployServiceError::validation(
                "resourceRef must not contain control characters",
            ));
        }

        let provider_code = self
            .provider_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(provider_code) = provider_code.as_deref() {
            if provider_code.chars().count() > APP_RESOURCE_LINK_PROVIDER_CODE_MAX_LEN {
                return Err(DeployServiceError::validation(format!(
                    "providerCode must be at most {APP_RESOURCE_LINK_PROVIDER_CODE_MAX_LEN} characters"
                )));
            }
            if provider_code.chars().any(char::is_control) {
                return Err(DeployServiceError::validation(
                    "providerCode must not contain control characters",
                ));
            }
        }
        if self.resource_kind == AppResourceKind::PaymentAccount && provider_code.is_none() {
            return Err(DeployServiceError::validation(
                "providerCode is required for a payment account: the default account is unique per provider",
            ));
        }
        if self.is_default && self.resource_kind != AppResourceKind::PaymentAccount {
            return Err(DeployServiceError::validation(
                "isDefault applies only to a payment account; an official account or mini program is single-instance by definition",
            ));
        }

        // `metadata` is a JSON object by contract (the column is `JSONB NOT NULL
        // DEFAULT '{}'`). A scalar or array is rejected rather than stored,
        // because every reader treats it as a bag of named facts.
        let metadata = if self.metadata.is_null() {
            empty_object()
        } else {
            self.metadata.clone()
        };
        if !metadata.is_object() {
            return Err(DeployServiceError::validation(
                "metadata must be a JSON object",
            ));
        }

        Ok(NormalizedAppResourceLink {
            environment: self.environment.as_str(),
            resource_kind: self.resource_kind,
            resource_ref: resource_ref.to_owned(),
            provider_code,
            is_default: self.is_default,
            binding_status: self.binding_status,
            metadata,
        })
    }
}

#[cfg(test)]
mod normalize_tests {
    use super::*;
    use serde_json::json;

    fn request(
        kind: AppResourceKind,
        reference: &str,
        provider: Option<&str>,
        is_default: bool,
    ) -> UpsertAppResourceLinkRequest {
        UpsertAppResourceLinkRequest {
            environment: AppPublishEnvironment::Production,
            resource_kind: kind,
            resource_ref: reference.to_owned(),
            provider_code: provider.map(str::to_owned),
            is_default,
            binding_status: AppResourceLinkStatus::Active,
            metadata: json!({ "displayName": "SDKWork" }),
        }
    }

    #[test]
    fn kind_values_are_the_column_values() {
        assert_eq!(
            AppResourceKind::all_values(),
            vec!["official_account", "mini_program", "payment_account"]
        );
        for kind in AppResourceKind::ALL {
            assert_eq!(AppResourceKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(AppResourceKind::parse("official-account"), None);
    }

    #[test]
    fn the_reference_is_trimmed_and_required() {
        let normalized = request(
            AppResourceKind::MiniProgram,
            "  wx1234567890  ",
            Some("wechat"),
            false,
        )
        .normalize()
        .expect("a non-empty reference normalizes");
        assert_eq!(normalized.resource_ref, "wx1234567890");

        let error = request(AppResourceKind::MiniProgram, "   ", None, false)
            .normalize()
            .expect_err("a blank reference must be refused");
        assert_eq!(error.kind(), crate::DeployServiceErrorKind::Validation);
    }

    #[test]
    fn an_over_long_reference_is_refused_before_the_database_sees_it() {
        let too_long = "a".repeat(APP_RESOURCE_LINK_REF_MAX_LEN + 1);
        assert!(
            request(AppResourceKind::OfficialAccount, &too_long, None, false)
                .normalize()
                .is_err()
        );
        let at_limit = "a".repeat(APP_RESOURCE_LINK_REF_MAX_LEN);
        assert!(
            request(AppResourceKind::OfficialAccount, &at_limit, None, false)
                .normalize()
                .is_ok()
        );
    }

    #[test]
    fn a_payment_account_requires_a_provider() {
        let error = request(
            AppResourceKind::PaymentAccount,
            "6222000000000000",
            None,
            false,
        )
        .normalize()
        .expect_err("the default-uniqueness key needs a provider");
        assert_eq!(error.kind(), crate::DeployServiceErrorKind::Validation);
        assert!(request(
            AppResourceKind::PaymentAccount,
            "6222000000000000",
            Some("stripe"),
            true
        )
        .normalize()
        .is_ok());
    }

    #[test]
    fn a_channel_account_does_not_need_a_provider() {
        // The provider is derivable for a WeChat channel account, and legacy rows
        // carry none, so requiring it here would refuse a document the backfill
        // accepts.
        assert!(request(
            AppResourceKind::OfficialAccount,
            "wx1234567890",
            None,
            false
        )
        .normalize()
        .is_ok());
    }

    #[test]
    fn only_a_payment_account_may_be_the_default() {
        let error = request(
            AppResourceKind::MiniProgram,
            "wx1234567890",
            Some("wechat"),
            true,
        )
        .normalize()
        .expect_err("a single-instance kind is its own default");
        assert_eq!(error.kind(), crate::DeployServiceErrorKind::Validation);
    }

    #[test]
    fn metadata_must_be_an_object_and_null_becomes_empty() {
        let mut null_metadata = request(AppResourceKind::OfficialAccount, "wx1", None, false);
        null_metadata.metadata = Value::Null;
        assert_eq!(
            null_metadata
                .normalize()
                .expect("null metadata normalizes")
                .metadata,
            json!({})
        );

        let mut scalar = request(AppResourceKind::OfficialAccount, "wx1", None, false);
        scalar.metadata = json!("not-an-object");
        assert_eq!(
            scalar.normalize().expect_err("a scalar is refused").kind(),
            crate::DeployServiceErrorKind::Validation
        );
    }

    #[test]
    fn a_blank_provider_is_absent_rather_than_empty() {
        let normalized = request(
            AppResourceKind::PaymentAccount,
            "6222000000000000",
            Some("   "),
            false,
        )
        .normalize()
        .expect_err("a blank provider on a payment account is a missing provider");
        assert_eq!(normalized.kind(), crate::DeployServiceErrorKind::Validation);

        let normalized = request(AppResourceKind::MiniProgram, "wx1", Some("   "), false)
            .normalize()
            .expect("a blank provider on a channel account is simply absent");
        assert_eq!(normalized.provider_code, None);
    }

    #[test]
    fn the_natural_key_ignores_the_reference_for_single_instance_kinds() {
        let official = request(AppResourceKind::OfficialAccount, "wx1", None, false)
            .normalize()
            .expect("normalizes");
        let same_kind = request(AppResourceKind::OfficialAccount, "wx2", None, false)
            .normalize()
            .expect("normalizes");
        assert_eq!(official.natural_key(), same_kind.natural_key());

        let payment = request(
            AppResourceKind::PaymentAccount,
            "6222",
            Some("stripe"),
            false,
        )
        .normalize()
        .expect("normalizes");
        let other_payment = request(
            AppResourceKind::PaymentAccount,
            "6223",
            Some("stripe"),
            false,
        )
        .normalize()
        .expect("normalizes");
        assert_ne!(payment.natural_key(), other_payment.natural_key());
    }

    /// The declared query parameters are part of the contract: the OpenAPI
    /// document declares `page`, `page_size`, `environment` and
    /// `resource_kind`, and the handler deserializes this struct. A field
    /// renamed to camelCase here would make the documented `resource_kind`
    /// filter silently ignored (API_SPEC §13).
    #[test]
    fn the_list_query_binds_the_documented_wire_parameters() {
        let query: ListAppResourceLinksQuery = serde_json::from_value(json!({
            "page": 2,
            "page_size": 50,
            "environment": "staging",
            "resource_kind": "mini_program",
        }))
        .expect("the documented query parameters deserialize");

        assert_eq!(query.page, 2);
        assert_eq!(query.page_size, 50);
        assert_eq!(query.environment, Some(AppPublishEnvironment::Staging));
        assert_eq!(query.resource_kind, Some(AppResourceKind::MiniProgram));
    }

    /// A binding payload serializes with the camelCase names the SDK exposes, so
    /// the generated TypeScript models match the Rust DTO field by field.
    #[test]
    fn the_binding_serializes_with_the_wire_names() {
        let response = AppResourceLinkResponse {
            id: "00000000-0000-4000-8000-000000000001".to_owned(),
            app_id: "00000000-0000-4000-8000-000000000002".to_owned(),
            environment: AppPublishEnvironment::Production,
            resource_kind: AppResourceKind::PaymentAccount,
            resource_ref: "6222000000000001".to_owned(),
            provider_code: Some("stripe".to_owned()),
            is_default: true,
            binding_status: AppResourceLinkStatus::Disabled,
            metadata: json!({ "accountName": "primary" }),
            created_at: "2026-11-17T00:00:00.000Z".to_owned(),
            updated_at: "2026-11-17T00:00:00.000Z".to_owned(),
            version: "3".to_owned(),
        };
        let value = serde_json::to_value(&response).expect("serializes");
        assert_eq!(value["resourceKind"], json!("payment_account"));
        assert_eq!(value["resourceRef"], json!("6222000000000001"));
        assert_eq!(value["providerCode"], json!("stripe"));
        assert_eq!(value["bindingStatus"], json!("DISABLED"));
        assert_eq!(value["isDefault"], json!(true));
        assert_eq!(value["version"], json!("3"));
        assert_eq!(value["environment"], json!("production"));
    }
}

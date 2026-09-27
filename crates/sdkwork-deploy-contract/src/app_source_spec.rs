//! Application **source spec** DTOs.
//!
//! A spec is the *source* dimension of an app: one uploaded bundle per
//! (app, environment, `spec_key`) — a PC build and an H5 build are two sources,
//! not two variants of one source. The published app keeps exactly one binding
//! (one hostname); the edge picks the spec from the request's client
//! classification, which is why a spec participates in routing through
//! [`AppSourceSpecRouteDefinition`] rather than through a flag of its own.
//!
//! The runtime descriptor has no `spec` concept and does not need one: the
//! repository projects each active spec into a Variant (`variant_key` =
//! `spec_key`) plus one `CLIENT_CLASS` VariantRule per declared route, and the
//! compiler validates the result exactly like a hand-authored composition. The
//! spec table is therefore the authoring surface and the descriptor the derived
//! one — there is never a second authored model to reconcile.
//!
//! # Two vocabularies, neither invented here
//!
//! [`SdkworkRuntimeTarget`] is the canonical runtime-target list owned by
//! `CONFIG_SPEC.md` §2.1, which states plainly that other specs "may reference
//! this list but must not invent alternate deployment-mode values". A repo-local
//! `WEB`/`NATIVE`/`SERVICE` spelling would silently disagree with the manifests,
//! workflow targets and release evidence that validate against the canonical
//! fifteen.
//!
//! [`AppClientArchitecture`] exists because the runtime target alone cannot
//! describe this feature's central pair: `CONFIG_SPEC.md` §2.1 records that "H5
//! web and PC web both use `runtimeTarget = 'browser'`", distinguished by the
//! app root and framework. Without a second column a PC bundle and an H5 bundle
//! are the same kind of thing, and "serve the H5 source to a phone" is not
//! expressible.
//!
//! [`AppClientClass`] is reused rather than re-spelled for the same reason: the
//! edge already classifies clients into exactly that closed set, and a second
//! spelling would be a translation table nobody owns.

use serde::{Deserialize, Serialize};

use crate::app_composition::{AppClientClass, AppMountHandler, AppPublishEnvironment};
use crate::app_delivery::deserialize_double_option;
use crate::ContentProviderResourceSource;

/// Where the executable config for a spec is consumed.
///
/// The canonical fifteen values of `CONFIG_SPEC.md` §2.1, spelled exactly as
/// that spec spells them (lower kebab case) so a value never needs translating
/// between this DTO, the manifest, and the release matrix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SdkworkRuntimeTarget {
    #[default]
    Browser,
    Desktop,
    TabletIpados,
    TabletAndroid,
    CapacitorIos,
    CapacitorAndroid,
    FlutterIos,
    FlutterAndroid,
    AndroidNative,
    IosNative,
    HarmonyNative,
    MiniProgram,
    Server,
    Container,
    TestRunner,
}

impl SdkworkRuntimeTarget {
    pub const ALL: [Self; 15] = [
        Self::Browser,
        Self::Desktop,
        Self::TabletIpados,
        Self::TabletAndroid,
        Self::CapacitorIos,
        Self::CapacitorAndroid,
        Self::FlutterIos,
        Self::FlutterAndroid,
        Self::AndroidNative,
        Self::IosNative,
        Self::HarmonyNative,
        Self::MiniProgram,
        Self::Server,
        Self::Container,
        Self::TestRunner,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Desktop => "desktop",
            Self::TabletIpados => "tablet-ipados",
            Self::TabletAndroid => "tablet-android",
            Self::CapacitorIos => "capacitor-ios",
            Self::CapacitorAndroid => "capacitor-android",
            Self::FlutterIos => "flutter-ios",
            Self::FlutterAndroid => "flutter-android",
            Self::AndroidNative => "android-native",
            Self::IosNative => "ios-native",
            Self::HarmonyNative => "harmony-native",
            Self::MiniProgram => "mini-program",
            Self::Server => "server",
            Self::Container => "container",
            Self::TestRunner => "test-runner",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// The architecture *within* a runtime target.
///
/// Values are the package segments of
/// `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2, so a spec can be matched
/// against a client root without an intermediate mapping. The browser pair
/// (`react` for PC, `react-h5` for H5) is the reason the column exists: both
/// carry `runtimeTarget = "browser"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppClientArchitecture {
    /// `apps/sdkwork-<code>-pc` — the PC browser/large-screen root.
    #[default]
    React,
    /// `apps/sdkwork-<code>-h5` — the H5 mobile root.
    ReactH5,
    /// `apps/sdkwork-<code>-static-web` — the plain-HTML fallback root.
    StaticWeb,
    FlutterMobile,
    /// `apps/sdkwork-<code>-mini-program`.
    MiniProgram,
    AndroidMobile,
    IosMobile,
    HarmonyMobile,
    Unity,
    Uniapp,
    /// `apps/sdkwork-<code>-pad`.
    Pad,
}

impl AppClientArchitecture {
    pub const ALL: [Self; 11] = [
        Self::React,
        Self::ReactH5,
        Self::StaticWeb,
        Self::FlutterMobile,
        Self::MiniProgram,
        Self::AndroidMobile,
        Self::IosMobile,
        Self::HarmonyMobile,
        Self::Unity,
        Self::Uniapp,
        Self::Pad,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::React => "react",
            Self::ReactH5 => "react-h5",
            Self::StaticWeb => "static-web",
            Self::FlutterMobile => "flutter-mobile",
            Self::MiniProgram => "mini-program",
            Self::AndroidMobile => "android-mobile",
            Self::IosMobile => "ios-mobile",
            Self::HarmonyMobile => "harmony-mobile",
            Self::Unity => "unity",
            Self::Uniapp => "uniapp",
            Self::Pad => "pad",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// Whether a spec takes part in routing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppSourceSpecStatus {
    #[default]
    Active,
    Disabled,
}

impl AppSourceSpecStatus {
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

/// Whether a spec already has an uploaded source behind it.
///
/// Distinct from [`AppSourceSpecStatus`] on purpose: a spec is normally created
/// first (so the routing shape can be reviewed) and filled by the upload that
/// follows. Collapsing the two would make "declared but not yet uploaded" and
/// "deliberately switched off" indistinguishable in the console — and the
/// difference decides whether the fallback chain stops at this spec or skips
/// past it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppSourceBindingStatus {
    #[default]
    Empty,
    Bound,
    Invalid,
    Revoked,
}

impl AppSourceBindingStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "EMPTY",
            Self::Bound => "BOUND",
            Self::Invalid => "INVALID",
            Self::Revoked => "REVOKED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "EMPTY" => Some(Self::Empty),
            "BOUND" => Some(Self::Bound),
            "INVALID" => Some(Self::Invalid),
            "REVOKED" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Which content provider backs a bound source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppSourceProviderType {
    Drive,
    Knowledgebase,
}

impl AppSourceProviderType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drive => "DRIVE",
            Self::Knowledgebase => "KNOWLEDGEBASE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DRIVE" => Some(Self::Drive),
            "KNOWLEDGEBASE" => Some(Self::Knowledgebase),
            _ => None,
        }
    }
}

/// One client class this spec serves, and where it sits in that class's
/// preference order.
///
/// The class's **default spec** is the lowest `preference` declared for it, and
/// `preference = 0` is the conventional way to say so. Lower ranks are tried
/// first, so the chain is what makes a phone still reach a page while the H5
/// bundle is being uploaded: the projected rule for `MOBILE` at rank 0 is not
/// emitted at all when that spec has no source yet, and rank 1 then wins.
///
/// Ranks may be sparse — a class can be declared at `1` before anything claims
/// `0` — because a per-spec write cannot see the rest of the environment, and
/// forcing completeness there would make the natural authoring order illegal.
/// Uniqueness of each rank within a class is enforced by the schema, so the
/// minimum — and therefore the default — is always unambiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceSpecRouteDefinition {
    #[serde(rename = "clientClass")]
    pub client_class: AppClientClass,
    /// `0` is the conventional rank for a client class's default; lower ranks
    /// win, so a class is served by whichever spec holds its lowest rank.
    #[serde(default)]
    pub preference: u16,
}

/// The provider triple a bound spec carries.
///
/// Emitted only when a source exists; a spec without an upload reports no
/// `source` object at all rather than an object full of nulls, so the console
/// can branch on presence instead of on emptiness.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceBindingResponse {
    #[serde(rename = "providerType")]
    pub provider_type: String,
    #[serde(rename = "providerResourceUuid")]
    pub provider_resource_uuid: String,
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    pub status: String,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Not `Default`: `handler` has no meaningful zero value, and a response built
/// by accident would silently claim `STATIC`. Rows are always mapped from the
/// database instead.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceSpecResponse {
    pub id: String,
    #[serde(rename = "specKey")]
    pub spec_key: String,
    pub label: String,
    /// Canonical `SdkworkRuntimeTarget` value.
    #[serde(rename = "runtimeTarget")]
    pub runtime_target: SdkworkRuntimeTarget,
    /// Distinguishes the PC root from the H5 root, which share `runtimeTarget`.
    #[serde(rename = "clientArchitecture")]
    pub client_architecture: AppClientArchitecture,
    /// The client classes this spec serves with their preference rank; each is
    /// one projected routing rule.
    #[serde(rename = "clientClassRoutes")]
    pub client_class_routes: Vec<AppSourceSpecRouteDefinition>,
    #[serde(rename = "pathPrefix")]
    pub path_prefix: String,
    pub handler: AppMountHandler,
    #[serde(rename = "indexFiles")]
    pub index_files: Vec<String>,
    #[serde(rename = "spaFallback", skip_serializing_if = "Option::is_none")]
    pub spa_fallback: Option<String>,
    /// The app-level default variant for a client matching *no* class route.
    /// Per-client-class defaults are the lowest-ranked entries of
    /// [`Self::client_class_routes`].
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub priority: u16,
    pub status: String,
    /// `EMPTY` | `BOUND` | `INVALID` | `REVOKED`.
    #[serde(rename = "sourceStatus")]
    pub source_status: String,
    /// Present only once a source has been bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AppSourceBindingResponse>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

/// An app has at most a handful of specs, so the page envelope carries
/// `page`/`pageSize` only to satisfy the v3 list shape consumers parse.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppSourceSpecPage {
    pub items: Vec<AppSourceSpecResponse>,
    pub total: i64,
}

/// A spec declared inline by `apps.create` / `apps.composition.update`.
///
/// Same fields as [`CreateAppSourceSpecRequest`] minus `environment`, which is
/// the enclosing request's: the two entry points exist because a spec is
/// publishable both as part of building an app and on its own afterwards, and
/// they must not drift into two vocabularies.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceSpecDefinition {
    #[serde(rename = "specKey")]
    pub spec_key: String,
    pub label: String,
    #[serde(rename = "runtimeTarget", default)]
    pub runtime_target: SdkworkRuntimeTarget,
    #[serde(rename = "clientArchitecture", default)]
    pub client_architecture: AppClientArchitecture,
    #[serde(rename = "clientClassRoutes", default)]
    pub client_class_routes: Vec<AppSourceSpecRouteDefinition>,
    #[serde(rename = "pathPrefix", default = "root_path")]
    pub path_prefix: String,
    #[serde(default = "default_handler")]
    pub handler: AppMountHandler,
    #[serde(rename = "indexFiles", default)]
    pub index_files: Vec<String>,
    #[serde(rename = "spaFallback", default)]
    pub spa_fallback: Option<String>,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
    #[serde(default)]
    pub priority: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAppSourceSpecRequest {
    pub environment: AppPublishEnvironment,
    #[serde(rename = "specKey")]
    pub spec_key: String,
    pub label: String,
    #[serde(rename = "runtimeTarget", default)]
    pub runtime_target: SdkworkRuntimeTarget,
    #[serde(rename = "clientArchitecture", default)]
    pub client_architecture: AppClientArchitecture,
    #[serde(rename = "clientClassRoutes", default)]
    pub client_class_routes: Vec<AppSourceSpecRouteDefinition>,
    #[serde(rename = "pathPrefix", default = "root_path")]
    pub path_prefix: String,
    #[serde(default = "default_handler")]
    pub handler: AppMountHandler,
    #[serde(rename = "indexFiles", default)]
    pub index_files: Vec<String>,
    #[serde(rename = "spaFallback", default)]
    pub spa_fallback: Option<String>,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
    #[serde(default)]
    pub priority: u16,
}

/// Partial update. `spec_key` is immutable: it is the projected variant key, and
/// renaming it would silently re-point every `CLIENT_CLASS` rule that names it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSourceSpecRequest {
    pub label: Option<String>,
    #[serde(rename = "runtimeTarget")]
    pub runtime_target: Option<SdkworkRuntimeTarget>,
    #[serde(rename = "clientArchitecture")]
    pub client_architecture: Option<AppClientArchitecture>,
    /// Whole-set replacement of this spec's routes, absent leaves them alone.
    /// Not additive: a class removed from the list loses its routing rule, which
    /// is the only way "stop serving phones from this spec" is expressible.
    #[serde(rename = "clientClassRoutes")]
    pub client_class_routes: Option<Vec<AppSourceSpecRouteDefinition>>,
    #[serde(rename = "pathPrefix")]
    pub path_prefix: Option<String>,
    pub handler: Option<AppMountHandler>,
    #[serde(rename = "indexFiles")]
    pub index_files: Option<Vec<String>>,
    /// Same double-`Option` shape as `apps.update.app_domain_label`: `null`
    /// clears the SPA fallback, an absent field leaves it untouched.
    #[serde(
        rename = "spaFallback",
        default,
        deserialize_with = "deserialize_double_option"
    )]
    pub spa_fallback: Option<Option<String>>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
    pub priority: Option<u16>,
    pub status: Option<AppSourceSpecStatus>,
}

/// Register (or replace) the uploaded source behind a spec.
///
/// The source is described the same way `apps.composition.update` describes a
/// resource — a Drive directory or a knowledgebase wiki — and is resolved
/// through the content provider before it is stored, so the row can only ever
/// carry a provider reference the delivery plane has already accepted.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindAppSourceSpecSourceRequest {
    pub source: ContentProviderResourceSource,
}

fn root_path() -> String {
    "/".to_owned()
}

fn default_handler() -> AppMountHandler {
    AppMountHandler::Static
}

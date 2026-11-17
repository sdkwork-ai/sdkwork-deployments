pub const PREFIX: &str = "/app/v3/api";

pub const DOMAIN_ZONES: &str = "/app/v3/api/domain_zones";
pub const DOMAIN_ZONE: &str = "/app/v3/api/domain_zones/{zoneId}";
pub const DOMAIN_ZONE_HOSTNAMES: &str = "/app/v3/api/domain_zones/{zoneId}/hostnames";
pub const DOMAIN_ZONE_HOSTNAME: &str = "/app/v3/api/domain_zones/{zoneId}/hostnames/{hostnameId}";
pub const DOMAIN_ZONE_HOSTNAME_VERIFY: &str =
    "/app/v3/api/domain_zones/{zoneId}/hostnames/{hostnameId}/verify";
pub const DOMAIN_ZONE_HOSTNAME_CLAIMS: &str = "/app/v3/api/domain_zones/{zoneId}/hostname_claims";
pub const DOMAIN_ZONE_DNS_RECORDS: &str = "/app/v3/api/domain_zones/{zoneId}/dns_records";
pub const DOMAIN_ZONE_DNS_RECORD: &str = "/app/v3/api/domain_zones/{zoneId}/dns_records/{recordId}";
pub const DOMAIN_ZONE_DNS_RECORD_STATUS: &str =
    "/app/v3/api/domain_zones/{zoneId}/dns_records/{recordId}/status";

/// Domain-ownership verification files registered on a zone.
///
/// A collection rather than a singleton: a zone publishes one file per platform
/// that asks for a root-path proof (WeChat official account, WeChat mini program,
/// Alipay, ByteDance, Google, Apple, custom), and a reissued file is a new
/// resource until the old one is deleted.
pub const DOMAIN_ZONE_VERIFICATION_FILES: &str =
    "/app/v3/api/domain_zones/{zoneId}/verification_files";
pub const DOMAIN_ZONE_VERIFICATION_FILE: &str =
    "/app/v3/api/domain_zones/{zoneId}/verification_files/{fileId}";
pub const DOMAIN_ZONE_VERIFICATION_FILE_CHECK: &str =
    "/app/v3/api/domain_zones/{zoneId}/verification_files/{fileId}/check";
pub const DOMAIN_ZONE_DNS_RECORDS_SYNC: &str = "/app/v3/api/domain_zones/{zoneId}/dns_records/sync";
/// Cloud accounts a zone or certificate can be bound to.
///
/// A top-level collection rather than a sub-resource of a zone: the same account is
/// what a zone pins and what a certificate overrides, and an account outlives both.
pub const CLOUD_ACCOUNTS: &str = "/app/v3/api/cloud_accounts";

pub const APP_COMPOSITION: &str = "/app/v3/api/apps/{appId}/composition";
pub const APP_ACTIVATE: &str = "/app/v3/api/apps/{appId}/activate";
pub const APP_PAUSE: &str = "/app/v3/api/apps/{appId}/pause";
pub const APP_ENV_VARIABLES: &str = "/app/v3/api/apps/{appId}/env_variables";
/// The account bindings an application environment publishes through.
///
/// One collection, because the three kinds share one authority: an official
/// account, a mini program and a payment account are all "the account this
/// environment acts as", and the kind is a field rather than a route segment. The
/// write is a `PUT` on the collection (an explicit upsert addressed by
/// `(environment, kind[, resourceRef])` — API_SPEC §15.4 forbids hiding an upsert
/// behind a create), and one binding is retired through its own id.
pub const APP_RESOURCE_LINKS: &str = "/app/v3/api/apps/{appId}/resource_links";
pub const APP_RESOURCE_LINK: &str = "/app/v3/api/apps/{appId}/resource_links/{resourceLinkId}";
/// The app's publishing domains: every default hostname
/// (`<appDomainLabel>.app[-<env>].<suffix>`) plus the custom hostnames bound
/// to the app, with each one's binding/verification state.
pub const APP_DOMAINS: &str = "/app/v3/api/apps/{appId}/domains";
pub const APP_HEALTH_CHECKS: &str = "/app/v3/api/apps/{appId}/health_checks";
pub const CERTIFICATES: &str = "/app/v3/api/certificates";
pub const CERTIFICATE: &str = "/app/v3/api/certificates/{certificateId}";
pub const CERTIFICATE_RENEW: &str = "/app/v3/api/certificates/{certificateId}/renew";
pub const CERTIFICATE_RENEWALS: &str = "/app/v3/api/certificates/{certificateId}/renewals";
pub const UPLOAD_SESSIONS: &str = "/app/v3/api/upload_sessions";
pub const UPLOAD_SESSION: &str = "/app/v3/api/upload_sessions/{uploadSessionId}";
pub const UPLOAD_SESSION_COMPLETE: &str = "/app/v3/api/upload_sessions/{uploadSessionId}/complete";
pub const UPLOAD_SESSION_CANCEL: &str = "/app/v3/api/upload_sessions/{uploadSessionId}/cancel";
pub const ARTIFACTS: &str = "/app/v3/api/artifacts";
pub const ARTIFACT: &str = "/app/v3/api/artifacts/{artifactId}";

pub const APPS: &str = "/app/v3/api/apps";
pub const APP: &str = "/app/v3/api/apps/{appId}";
pub const APP_PLATFORM_TARGETS: &str = "/app/v3/api/apps/{appId}/platform_targets";
pub const APP_PLATFORM_TARGET: &str =
    "/app/v3/api/apps/{appId}/platform_targets/{platformTargetId}";
pub const APP_SOURCE_REPOSITORIES: &str = "/app/v3/api/apps/{appId}/source_repositories";
pub const APP_SOURCE_REPOSITORY: &str =
    "/app/v3/api/apps/{appId}/source_repositories/{sourceRepositoryId}";
/// An app's source specs: one row per *source* the app serves.
///
/// A spec is a source dimension, not a delivery variant — a PC bundle and an H5
/// bundle are two rows here, each uploaded on its own, and the published app
/// keeps one hostname while the edge picks the spec from the request's client
/// classification.
pub const APP_SOURCE_SPECS: &str = "/app/v3/api/apps/{appId}/source_specs";
pub const APP_SOURCE_SPEC: &str = "/app/v3/api/apps/{appId}/source_specs/{specId}";
pub const APP_SOURCE_SPEC_SOURCE: &str = "/app/v3/api/apps/{appId}/source_specs/{specId}/source";
/// The application's remix source: what a remix downloads, and whether it can.
///
/// A singleton under the application (one source per app, a database fact) rather
/// than a collection: `GET` answers `200` with `MISSING` when nothing is
/// configured, because "no source" is a state of the resource, not an absent one.
pub const APP_SOURCE: &str = "/app/v3/api/apps/{appId}/source";
/// The validation command for that source.
///
/// A sub-resource rather than a field on the PUT: validating is an *action*
/// against the current configuration, it can be repeated, and it is the only
/// operation that can turn a Git source `READY`. Folding it into the write would
/// make every PUT depend on the reachability of a third-party host.
pub const APP_SOURCE_VALIDATE: &str = "/app/v3/api/apps/{appId}/source/validate";
/// The remix state: the enable switch plus the download instructions.
pub const APP_REMIX: &str = "/app/v3/api/apps/{appId}/remix";
pub const BUILD_TEMPLATES: &str = "/app/v3/api/build_templates";
pub const BUILD_TEMPLATE: &str = "/app/v3/api/build_templates/{buildTemplateId}";
pub const APP_BUILDS: &str = "/app/v3/api/apps/{appId}/builds";
pub const APP_BUILD: &str = "/app/v3/api/apps/{appId}/builds/{buildId}";
pub const APP_BUILD_STATE: &str = "/app/v3/api/apps/{appId}/builds/{buildId}/state";
pub const APP_PACKAGES: &str = "/app/v3/api/apps/{appId}/packages";
pub const APP_PACKAGE: &str = "/app/v3/api/apps/{appId}/packages/{packageId}";
pub const APP_RELEASES: &str = "/app/v3/api/apps/{appId}/releases";
pub const APP_RELEASE: &str = "/app/v3/api/apps/{appId}/releases/{releaseId}";
pub const APP_CHANNELS: &str = "/app/v3/api/apps/{appId}/channels";
pub const APP_CHANNEL: &str = "/app/v3/api/apps/{appId}/channels/{channelId}";
pub const APP_CHANNEL_PROMOTIONS: &str = "/app/v3/api/apps/{appId}/channels/{channelId}/promotions";
pub const APP_CHANNEL_ROLLOUTS: &str = "/app/v3/api/apps/{appId}/channels/{channelId}/rollouts";
pub const APP_DEPLOYMENTS: &str = "/app/v3/api/apps/{appId}/deployments";
pub const APP_DEPLOYMENT: &str = "/app/v3/api/apps/{appId}/deployments/{deploymentId}";
pub const SIGNING_IDENTITIES: &str = "/app/v3/api/signing_identities";
pub const SIGNING_IDENTITY: &str = "/app/v3/api/signing_identities/{signingIdentityId}";
pub const USAGE_EVENTS: &str = "/app/v3/api/usage_events";
pub const APP_DATABASE_PROFILES: &str = "/app/v3/api/apps/{appId}/database_profiles";
pub const APP_DATABASE_PROFILE: &str = "/app/v3/api/apps/{appId}/database_profiles/{profileId}";
pub const APP_DATABASE_MIGRATIONS: &str =
    "/app/v3/api/apps/{appId}/database_profiles/{profileId}/migrations";
pub const APP_DATABASE_MIGRATION: &str =
    "/app/v3/api/apps/{appId}/database_profiles/{profileId}/migrations/{migrationId}";
pub const APP_ENVIRONMENTS: &str = "/app/v3/api/apps/{appId}/environments";
pub const APP_ENVIRONMENT: &str = "/app/v3/api/apps/{appId}/environments/{environmentId}";
pub const APP_ENVIRONMENT_PROMOTIONS: &str =
    "/app/v3/api/apps/{appId}/environments/{environmentId}/promotions";

// -- app template marketplace ------------------------------------------------

pub const TEMPLATE_CATEGORIES: &str = "/app/v3/api/template_categories";
pub const MARKETPLACE_TEMPLATES: &str = "/app/v3/api/marketplace/templates";
pub const MARKETPLACE_TEMPLATE: &str = "/app/v3/api/marketplace/templates/{templateUuid}";
pub const APP_TEMPLATES: &str = "/app/v3/api/app_templates";
pub const APP_TEMPLATE: &str = "/app/v3/api/app_templates/{templateUuid}";
pub const APP_TEMPLATE_SUBMIT: &str = "/app/v3/api/app_templates/{templateUuid}/submit";
pub const APP_TEMPLATE_VERSIONS: &str = "/app/v3/api/app_templates/{templateUuid}/versions";

// -- download catalog (public) -------------------------------------------------

pub const DOWNLOAD_CATALOG: &str = "/app/v3/api/downloads/catalog/{catalogKey}";
pub const DOWNLOAD_CONFIG: &str = "/app/v3/api/downloads/config";
pub const DOWNLOAD_FEED: &str =
    "/app/v3/api/downloads/feed/{productKey}/{platform}/{architecture}/{channelFile}";
pub const DOWNLOAD_ARTIFACT: &str = "/app/v3/api/downloads/artifact/{itemUuid}";

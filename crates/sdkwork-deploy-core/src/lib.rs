//! Deploy core runtime helpers.

pub mod app_domains;
pub mod app_kind_rules;
pub mod certificate_key_algorithm;
pub mod certificate_validity;
pub mod database_profile;
pub mod package_manifest;
pub mod runtime_env;
pub mod util;
pub mod versioning;

pub use app_domains::{
    app_domain_label, default_app_domain_pattern, default_app_hostname,
    default_app_hostname_with_label, effective_app_domain_label, effective_app_domain_suffixes,
    environment_for_app_domain_label, normalize_app_domain_label, normalize_app_domain_suffixes,
    parse_default_app_hostname, parse_platform_app_hostname, platform_app_domain_suffixes,
    DefaultAppHostname, PLATFORM_APP_DOMAIN_LABELS, PLATFORM_APP_DOMAIN_SUFFIXES,
};
pub use app_kind_rules::{
    package_size_ceiling, required_identity_field, validate_app_kind_platform,
    validate_package_format_for_platform, validate_package_size, validate_platform_identity,
    RequiredIdentityField, DESKTOP_INSTALLER_MAXIMUM_BYTES, DOUYIN_MINIPROGRAM_MAIN_PACKAGE_BYTES,
    DOUYIN_MINIPROGRAM_TOTAL_PACKAGE_BYTES, JVM_ARTIFACT_MAXIMUM_BYTES,
    PROCESS_BUNDLE_MAXIMUM_BYTES, WEB_BUNDLE_MAXIMUM_BYTES, WECHAT_MINIPROGRAM_MAIN_PACKAGE_BYTES,
    WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES,
};
pub use certificate_key_algorithm::{
    validate_certificate_key_algorithm, CERTIFICATE_DEFAULT_KEY_ALGORITHM,
    CERTIFICATE_KEY_ALGORITHMS, CERTIFICATE_KEY_ALGORITHM_ECDSA, CERTIFICATE_KEY_ALGORITHM_RSA,
};
pub use certificate_validity::{
    CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS, CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS,
    CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS,
};
pub use database_profile::{
    validate_catalog_name, validate_database_engine, validate_migration_name,
    validate_migration_strategy, validate_migration_version, validate_profile_key,
    validate_profile_status, DATABASE_ENGINES, MIGRATION_STATUSES, MIGRATION_STRATEGIES,
    PROFILE_STATUSES,
};
pub use package_manifest::{
    canonical_manifest_sha256, validate_package_manifest, validate_sha256_hex,
    PackageManifestValidation, PACKAGE_MANIFEST_KIND, PACKAGE_MANIFEST_SCHEMA_VERSION,
};
pub use runtime_env::{
    deploy_dev_auth_bypass_enabled, deploy_entitlement_enforcement_enabled,
    deploy_environment_name, deploy_is_production_like_environment,
    deploy_use_dev_inline_auth_resolver, env_test_lock,
};
pub use util::{normalize_pagination, pagination_offset};
pub use versioning::{SemanticVersion, MAXIMUM_IDENTIFIER_LENGTH, MAXIMUM_VERSION_LENGTH};

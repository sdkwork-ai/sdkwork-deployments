//! Platform app publishing domain catalog.
//!
//! Every App is automatically published on `<slug>.app.<suffix>` for each
//! platform suffix (the same 14-domain catalog the IM/drive/knowledgebase
//! modules use, `SDKWORK_WEBSERVER_SPEC.md` domain inventory). Users can
//! additionally bind custom domains to their site; the Web Server resolves
//! unmatched hosts against the Deploy control plane
//! (`sdkwork-webserver` `appDomainFallback` section).

/// Platform app-domain suffixes (lowercase, no leading dot). Keep in sync
/// with the sdkwork-im `deployments/webserver/server.*.toml` serverName
/// catalog: sdkwork.com, sdkwork.cn, birdcoder.com, birdcoder.cn, dtupay.com,
/// dtupay.cn, skubc.com, skubc.cn, zowalk.com, zowalk.cn, offer86.com,
/// offer86.cn, 86offer.com, 86offer.cn.
pub const PLATFORM_APP_DOMAIN_SUFFIXES: [&str; 14] = [
    "sdkwork.com",
    "sdkwork.cn",
    "birdcoder.com",
    "birdcoder.cn",
    "dtupay.com",
    "dtupay.cn",
    "skubc.com",
    "skubc.cn",
    "zowalk.com",
    "zowalk.cn",
    "offer86.com",
    "offer86.cn",
    "86offer.com",
    "86offer.cn",
];

/// Subdomain label placed between the app label and the platform suffix.
/// Production uses `app`; every non-production environment uses
/// `app-<env>` so all five lifecycle environments get their own publishable
/// hostname (`myapp.app.sdkwork.com` / `myapp.app-dev.sdkwork.com` /
/// `myapp.app-demo.sdkwork.com`).
///
/// The label set must stay aligned with
/// `AppPublishEnvironment` / `RuntimeEnvironment` in this repository and with
/// `WebsiteRuntimeEnvironment` in sdkwork-webserver.
pub fn app_domain_label(environment: &str) -> &'static str {
    match environment {
        "development" => "app-dev",
        "test" => "app-test",
        "staging" => "app-staging",
        "demo" => "app-demo",
        "production" => "app",
        // Unknown keys must never silently share the production label: doing
        // so aliases two environments onto one hostname (the pre-`demo`
        // behaviour). Callers validate the key first; this keeps the
        // function total without producing a false default.
        _ => "app",
    }
}

/// The platform labels in `app` / `app-<env>` form, longest first so prefix
/// matching never mis-splits `app-dev` as `app`.
pub const PLATFORM_APP_DOMAIN_LABELS: [&str; 5] =
    ["app-staging", "app-demo", "app-dev", "app-test", "app"];

/// The lifecycle environment a platform label encodes, if any.
pub fn environment_for_app_domain_label(label: &str) -> Option<&'static str> {
    match label {
        "app" => Some("production"),
        "app-dev" => Some("development"),
        "app-test" => Some("test"),
        "app-staging" => Some("staging"),
        "app-demo" => Some("demo"),
        _ => None,
    }
}

/// The single-label prefix an app uses in its default publishing hostnames.
///
/// `<appId>` semantics: when the app carries an explicit
/// `deploy_app.app_domain_label`, that value replaces the slug, which is how
/// an app publishes on `<custom-prefix>.app.<suffix>` — including the app's
/// own public id (`deploy_app.uuid`) if the owner chooses it. Otherwise the
/// globally unique slug is used, preserving the historical
/// `<slug>.app.<suffix>` catalog.
pub fn effective_app_domain_label<'a>(app_domain_label: Option<&'a str>, slug: &'a str) -> &'a str {
    match app_domain_label {
        Some(label) if !label.trim().is_empty() => label.trim(),
        _ => slug,
    }
}

/// Validate a custom app-domain prefix: one DNS label (1-63 bytes, lowercase
/// alphanumeric plus interior hyphens). Returns the normalized value.
pub fn normalize_app_domain_label(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err("appDomainLabel must not be blank".into());
    }
    if value.len() > 63 {
        return Err(format!("appDomainLabel is longer than 63 bytes: {value}"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!(
            "appDomainLabel must be a lowercase DNS label (alphanumeric and hyphen): {value}"
        ));
    }
    if value.starts_with('-') || value.ends_with('-') {
        return Err(format!("appDomainLabel must not start or end with a hyphen: {value}"));
    }
    Ok(value)
}

/// Validate a per-app platform-suffix override. Each entry is a lowercase
/// dotted domain without a leading dot; an empty list is rejected so an app
/// can never be left unpublished.
pub fn normalize_app_domain_suffixes(raw: &[String]) -> Result<Vec<String>, String> {
    if raw.is_empty() {
        return Err("appDomainSuffixes must contain at least one suffix".into());
    }
    let mut normalized = Vec::with_capacity(raw.len());
    for entry in raw {
        let value = entry.trim().trim_start_matches('.').to_ascii_lowercase();
        if value.is_empty() || value.ends_with('.') || value.len() > 253 {
            return Err(format!("appDomainSuffixes entry is not a domain: {entry}"));
        }
        if !value.split('.').all(label_is_sane) {
            return Err(format!("appDomainSuffixes entry has an invalid label: {entry}"));
        }
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    normalized.sort();
    Ok(normalized)
}

/// The platform-owned suffix catalog, owned by `IM_SPEC` module domain
/// inventory. Every reader must go through this function so the catalog has
/// exactly one definition (`PLATFORM_APP_DOMAIN_SUFFIXES`).
pub fn platform_app_domain_suffixes() -> Vec<String> {
    PLATFORM_APP_DOMAIN_SUFFIXES
        .iter()
        .map(|suffix| (*suffix).to_owned())
        .collect()
}

/// The catalog an app publishes on: its explicit override when present,
/// otherwise the platform catalog.
///
/// The override is re-normalized on read (lowercase, de-duplicated, sorted) so
/// a hand-edited or legacy row can never widen the catalog with a mixed-case or
/// duplicated entry. A row that fails validation falls back to the platform
/// catalog instead of publishing on a malformed suffix; `appDomainSuffixes` is
/// validated on write (`normalize_app_domain_suffixes`) so this is a corruption
/// guard, not the primary gate.
pub fn effective_app_domain_suffixes(override_list: Option<&[String]>) -> Vec<String> {
    match override_list {
        Some(list) if !list.is_empty() => {
            normalize_app_domain_suffixes(list).unwrap_or_else(|_| platform_app_domain_suffixes())
        }
        _ => platform_app_domain_suffixes(),
    }
}

/// The default publishable hostname for an app in one environment, for one
/// platform suffix: `<appLabel>.app[-<env>].<suffix>`.
pub fn default_app_hostname(app_label: &str, suffix: &str, environment: &str) -> String {
    format!("{app_label}.{}.{suffix}", app_domain_label(environment))
}

/// Default publishing hostname using an explicit label and suffix pair.
pub fn default_app_hostname_with_label(app_label: &str, suffix: &str, environment: &str) -> String {
    default_app_hostname(app_label, suffix, environment)
}

/// Parsed default app hostname: the app label and the platform suffix behind
/// `<appLabel>.app[-<env>].<suffix>`. `None` when the hostname is not a
/// platform default app hostname (custom domains do not parse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultAppHostname {
    /// The label in front of the platform label: the app slug, or the app's
    /// custom `appDomainLabel`.
    pub app_label: String,
    pub environment: String,
    pub suffix: String,
}

impl DefaultAppHostname {
    /// Historical alias: the label in front of the platform label.
    pub fn slug(&self) -> &str {
        &self.app_label
    }
}

/// Parse a hostname into its default-app parts. The environment label must
/// be one of the platform labels (`app`, `app-dev`, `app-test`,
/// `app-staging`, `app-demo`), the suffix must be in the supplied catalog,
/// and the leading app label must be a well-formed DNS label. Hostnames are
/// compared case-insensitively and must be ASCII.
pub fn parse_default_app_hostname(
    hostname: &str,
    suffixes: &[String],
) -> Option<DefaultAppHostname> {
    let hostname = hostname.trim().to_ascii_lowercase();
    if hostname.is_empty()
        || hostname.len() > 253
        || hostname.ends_with('.')
        || !hostname.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        return None;
    }
    let mut labels = hostname.split('.');
    let app_label = labels.next()?;
    let label = labels.next()?;
    let suffix = labels.collect::<Vec<_>>().join(".");
    if !label_is_sane(app_label) || suffix.is_empty() || !labels_are_sane(&suffix) {
        return None;
    }
    let environment = environment_for_app_domain_label(label)?;
    if !suffixes.iter().any(|item| item == &suffix) {
        return None;
    }
    Some(DefaultAppHostname {
        app_label: app_label.to_owned(),
        environment: environment.to_owned(),
        suffix,
    })
}

/// Parse using the platform catalog (convenience for callers that do not
/// carry a per-app override).
pub fn parse_platform_app_hostname(hostname: &str) -> Option<DefaultAppHostname> {
    parse_default_app_hostname(hostname, &effective_app_domain_suffixes(None))
}

/// Wildcard form used for DNS and TLS planning: `*.app[-<env>].<suffix>`.
pub fn default_app_domain_pattern(environment: &str, suffix: &str) -> String {
    format!("*.{}.{suffix}", app_domain_label(environment))
}

fn labels_are_sane(suffix: &str) -> bool {
    let labels = suffix.split('.');
    let mut count = 0;
    for label in labels {
        count += 1;
        if !label_is_sane(label) {
            return false;
        }
    }
    count >= 2
}

fn label_is_sane(label: &str) -> bool {
    !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_the_im_module_domain_inventory() {
        assert_eq!(PLATFORM_APP_DOMAIN_SUFFIXES.len(), 14);
        assert!(PLATFORM_APP_DOMAIN_SUFFIXES.contains(&"sdkwork.com"));
        assert!(PLATFORM_APP_DOMAIN_SUFFIXES.contains(&"sdkwork.cn"));
        assert!(PLATFORM_APP_DOMAIN_SUFFIXES.contains(&"86offer.cn"));
        for suffix in PLATFORM_APP_DOMAIN_SUFFIXES {
            assert!(suffix.is_ascii() && !suffix.starts_with('.') && !suffix.ends_with('.'));
            assert_eq!(suffix, suffix.to_ascii_lowercase());
        }
    }

    #[test]
    fn production_and_environment_hostnames_are_distinct() {
        assert_eq!(
            default_app_hostname("myapp", "sdkwork.com", "production"),
            "myapp.app.sdkwork.com"
        );
        assert_eq!(
            default_app_hostname("myapp", "sdkwork.com", "development"),
            "myapp.app-dev.sdkwork.com"
        );
        assert_eq!(
            default_app_hostname("myapp", "sdkwork.com", "test"),
            "myapp.app-test.sdkwork.com"
        );
        assert_eq!(
            default_app_hostname("myapp", "sdkwork.com", "staging"),
            "myapp.app-staging.sdkwork.com"
        );
        assert_eq!(
            default_app_hostname("myapp", "sdkwork.com", "demo"),
            "myapp.app-demo.sdkwork.com"
        );
        assert_eq!(
            default_app_hostname("myapp", "86offer.cn", "production"),
            "myapp.app.86offer.cn"
        );
    }

    #[test]
    fn every_environment_gets_a_unique_label() {
        let mut labels = Vec::new();
        for environment in ["development", "test", "staging", "demo", "production"] {
            let label = app_domain_label(environment);
            assert!(
                !labels.contains(&label),
                "environment {environment} reuses label {label}"
            );
            labels.push(label);
        }
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn custom_prefix_replaces_the_app_id_label() {
        // Default: the slug is the `<appId>` label.
        assert_eq!(
            effective_app_domain_label(None, "myapp"),
            "myapp"
        );
        // Explicit prefix replaces it.
        assert_eq!(
            effective_app_domain_label(Some("shop"), "myapp"),
            "shop"
        );
        // Blank is treated as absent, never as an empty label.
        assert_eq!(effective_app_domain_label(Some("  "), "myapp"), "myapp");
        assert_eq!(
            default_app_hostname(effective_app_domain_label(Some("shop"), "myapp"), "sdkwork.com", "production"),
            "shop.app.sdkwork.com"
        );
        // A UUID app id is a legal prefix (the "appId" reading of the spec).
        let app_id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            default_app_hostname(app_id, "sdkwork.com", "production"),
            format!("{app_id}.app.sdkwork.com")
        );
    }

    #[test]
    fn custom_prefix_validation_rejects_malformed_labels() {
        assert_eq!(normalize_app_domain_label("Shop").unwrap(), "shop");
        assert_eq!(normalize_app_domain_label(" my-app ").unwrap(), "my-app");
        for invalid in ["", "   ", "-shop", "shop-", "a.b", "sh_op", "日本語"] {
            assert!(
                normalize_app_domain_label(invalid).is_err(),
                "must reject {invalid}"
            );
        }
        assert!(normalize_app_domain_label(&"a".repeat(64)).is_err());
    }

    #[test]
    fn per_app_suffix_override_replaces_the_platform_catalog() {
        let platform = effective_app_domain_suffixes(None);
        assert_eq!(platform.len(), 14);
        let overridden =
            effective_app_domain_suffixes(Some(&["Example.COM".to_owned(), "example.com".to_owned()]));
        assert_eq!(overridden, vec!["example.com".to_owned()]);
        assert_eq!(normalize_app_domain_suffixes(&[]).is_err(), true);
        assert_eq!(
            normalize_app_domain_suffixes(&["example.com".to_owned()]).unwrap(),
            vec!["example.com".to_owned()]
        );
    }

    #[test]
    fn parse_round_trips_default_hostnames() {
        let suffixes = effective_app_domain_suffixes(None);
        for (hostname, app_label, environment, suffix) in [
            (
                "myapp.app.sdkwork.com",
                "myapp",
                "production",
                "sdkwork.com",
            ),
            (
                "myapp.app-dev.sdkwork.cn",
                "myapp",
                "development",
                "sdkwork.cn",
            ),
            (
                "shop.app-test.birdcoder.com",
                "shop",
                "test",
                "birdcoder.com",
            ),
            (
                "wiki.app-staging.86offer.cn",
                "wiki",
                "staging",
                "86offer.cn",
            ),
            (
                "demoapp.app-demo.sdkwork.com",
                "demoapp",
                "demo",
                "sdkwork.com",
            ),
        ] {
            let parsed = parse_default_app_hostname(hostname, &suffixes).expect("must parse");
            assert_eq!(
                parsed,
                DefaultAppHostname {
                    app_label: app_label.to_owned(),
                    environment: environment.to_owned(),
                    suffix: suffix.to_owned(),
                }
            );
        }
        // Case-insensitive.
        assert_eq!(
            parse_default_app_hostname("MyApp.APP.Sdkwork.COM", &suffixes)
                .map(|value| value.app_label),
            Some("myapp".to_owned())
        );
    }

    #[test]
    fn custom_and_malformed_hostnames_do_not_parse() {
        let suffixes = effective_app_domain_suffixes(None);
        for hostname in [
            "mysite.example.com",
            "myapp.example.com",
            "myapp.app.example.com",
            "myapp.app.",
            ".app.sdkwork.com",
            "myapp.app",
            "myapp.other.sdkwork.com",
            "myapp.app.sdkwork.com.evil.com",
            "myapp.app.sdkwork.com.",
            "-myapp.app.sdkwork.com",
            "myapp.app.sdkwork.com/path",
            "",
        ] {
            assert_eq!(
                parse_default_app_hostname(hostname, &suffixes),
                None,
                "hostname must not parse: {hostname}"
            );
        }
    }

    #[test]
    fn wildcard_patterns_cover_the_whole_catalog() {
        assert_eq!(
            default_app_domain_pattern("production", "sdkwork.com"),
            "*.app.sdkwork.com"
        );
        assert_eq!(
            default_app_domain_pattern("development", "sdkwork.cn"),
            "*.app-dev.sdkwork.cn"
        );
    }
}

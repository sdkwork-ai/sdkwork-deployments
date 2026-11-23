//! The closed platform catalog for domain-ownership verification files.
//!
//! Every platform that asks an operator to prove domain control by publishing a
//! file at the domain root issues that file itself, and the shapes differ: WeChat
//! hands out `MP_verify_<token>.txt` for both the official account and the mini
//! program, Alipay and ByteDance hand out `<token>.txt`, Google hands out a
//! `<token>.html`, Apple always wants `apple-developer-domain-association.txt`.
//! The control plane already stored one row per platform, but the rule that says
//! *which* name a platform will fetch lived in three places (the DDL `CHECK`, the
//! service constants, and each console's option list) and drifted between them.
//!
//! This module is the one place the rules live. The DDL `CHECK`, the service
//! validation, the edge's `Content-Type` table, and the consoles' platform
//! pickers are all expected to agree with it; a new platform is added here and
//! the parity tests name every site that still has to follow.

/// One platform's verification-file contract.
///
/// `label_key` is the i18n key every console translates, so the three surfaces
/// cannot spell the same platform differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationFilePlatformSpec {
    /// Wire code stored on the row and sent by every client.
    pub platform: &'static str,
    /// i18n key for the platform's display name.
    pub label_key: &'static str,
    /// Human hint for the file name the platform issues, e.g.
    /// `MP_verify_<token>.txt`.
    pub file_name_hint: &'static str,
    /// Fixed file name the platform always issues, when it has one.
    pub fixed_file_name: Option<&'static str>,
    /// Prefix the platform puts before its token (`MP_verify_`), when it does.
    pub token_prefix: Option<&'static str>,
    /// Extension the platform's file always has.
    pub extension: &'static str,
    /// Where the operator obtains the file, for the console's empty state.
    pub issued_by: &'static str,
}

/// Every platform the control plane, the edge, and the consoles accept.
pub const VERIFICATION_FILE_PLATFORMS: &[VerificationFilePlatformSpec] = &[
    VerificationFilePlatformSpec {
        platform: "WECHAT_OFFICIAL_ACCOUNT",
        label_key: "domain.verification.platform.WECHAT_OFFICIAL_ACCOUNT",
        file_name_hint: "MP_verify_<token>.txt",
        fixed_file_name: None,
        token_prefix: Some("MP_verify_"),
        extension: "txt",
        issued_by: "WeChat MP admin console: 设置与开发 → 公众号设置 → 功能设置 → JS接口安全域名/网页授权域名",
    },
    VerificationFilePlatformSpec {
        platform: "WECHAT_MINI_PROGRAM",
        label_key: "domain.verification.platform.WECHAT_MINI_PROGRAM",
        file_name_hint: "MP_verify_<token>.txt",
        fixed_file_name: None,
        token_prefix: Some("MP_verify_"),
        extension: "txt",
        issued_by: "WeChat mini program admin console: 开发管理 → 开发设置 → 服务器域名",
    },
    VerificationFilePlatformSpec {
        platform: "ALIPAY",
        label_key: "domain.verification.platform.ALIPAY",
        file_name_hint: "<token>.txt",
        fixed_file_name: None,
        token_prefix: None,
        extension: "txt",
        issued_by: "Alipay mini program console: 开发设置 → 服务器域名白名单",
    },
    VerificationFilePlatformSpec {
        platform: "BYTEDANCE",
        label_key: "domain.verification.platform.BYTEDANCE",
        file_name_hint: "<token>.txt",
        fixed_file_name: None,
        token_prefix: None,
        extension: "txt",
        issued_by: "Douyin / ByteDance mini program console: 开发设置 → 服务器域名",
    },
    VerificationFilePlatformSpec {
        platform: "APPLE",
        label_key: "domain.verification.platform.APPLE",
        file_name_hint: "apple-developer-domain-association.txt",
        fixed_file_name: Some("apple-developer-domain-association.txt"),
        token_prefix: None,
        extension: "txt",
        issued_by: "Apple Developer: Certificates, Identifiers & Profiles → Associated Domains",
    },
    VerificationFilePlatformSpec {
        platform: "GOOGLE",
        label_key: "domain.verification.platform.GOOGLE",
        file_name_hint: "<token>.html",
        fixed_file_name: None,
        token_prefix: None,
        extension: "html",
        issued_by: "Google Search Console: 设置 → 所有权验证 → HTML 文件",
    },
    VerificationFilePlatformSpec {
        platform: "CUSTOM",
        label_key: "domain.verification.platform.CUSTOM",
        file_name_hint: "<file>.txt | <file>.html | <file>.xml",
        fixed_file_name: None,
        token_prefix: None,
        extension: "txt",
        issued_by: "Other platform (Bing, Baidu, 微信开放平台…): use the file it issued",
    },
];

/// Extensions the edge knows how to answer, and therefore the only ones a row
/// may carry. Kept in step with [`crate::verification_file_catalog::content_type_for`].
pub const VERIFICATION_FILE_EXTENSIONS: [&str; 3] = ["txt", "html", "xml"];

/// The platform's spec, or `None` when the code is outside the closed set.
pub fn verification_platform(code: &str) -> Option<&'static VerificationFilePlatformSpec> {
    VERIFICATION_FILE_PLATFORMS
        .iter()
        .find(|spec| spec.platform == code)
}

/// Every accepted wire code, in catalog order.
pub fn verification_platform_codes() -> impl Iterator<Item = &'static str> {
    VERIFICATION_FILE_PLATFORMS.iter().map(|spec| spec.platform)
}

/// Whether `file_name` is one the edge will serve: a single ASCII segment with
/// one of the known extensions. Mirrors the DDL `CHECK` and the edge's
/// `requested_verification_file_name`.
pub fn is_servable_verification_file_name(file_name: &str) -> bool {
    if file_name.is_empty() || file_name.len() > 128 || file_name.contains('/') {
        return false;
    }
    if !file_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return false;
    }
    VERIFICATION_FILE_EXTENSIONS
        .iter()
        .any(|extension| file_name.ends_with(&format!(".{extension}")))
}

/// The platform's expected file name for the token it issued.
///
/// Answers `None` when the platform's shape needs a token the caller did not
/// supply, or when the code is unknown. Returning a suggestion rather than
/// forcing it is deliberate: an operator who already has the file keeps it, and
/// the console only prefills the field.
pub fn suggested_verification_file_name(platform: &str, token: &str) -> Option<String> {
    let spec = verification_platform(platform)?;
    if let Some(fixed) = spec.fixed_file_name {
        return Some(fixed.to_owned());
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    // A token that already carries the accepted shape (the operator pasted the
    // platform's file name) is accepted verbatim rather than double-prefixed.
    if is_servable_verification_file_name(token) && token.ends_with(&format!(".{}", spec.extension))
    {
        return Some(token.to_owned());
    }
    let stem = match spec.token_prefix {
        Some(prefix) => format!("{prefix}{token}"),
        None => token.to_owned(),
    };
    Some(format!("{stem}.{}", spec.extension))
}

/// `Content-Type` the edge answers with, by extension.
pub fn content_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "html" => "text/html; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

/// Whether the platform's own file always uses `extension`.
///
/// `CUSTOM` accepts every extension the edge serves; a platform that issues a
/// fixed shape is held to it, so an operator cannot register `MP_verify_x.html`
/// under WeChat and then watch the platform fetch nothing.
pub fn platform_accepts_extension(platform: &str, extension: &str) -> bool {
    match verification_platform(platform) {
        Some(spec) if spec.platform == "CUSTOM" => {
            VERIFICATION_FILE_EXTENSIONS.contains(&extension)
        }
        Some(spec) => spec.extension == extension,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_closed_set_the_ddl_admits() {
        let codes: Vec<&str> = verification_platform_codes().collect();
        assert_eq!(
            codes,
            vec![
                "WECHAT_OFFICIAL_ACCOUNT",
                "WECHAT_MINI_PROGRAM",
                "ALIPAY",
                "BYTEDANCE",
                "APPLE",
                "GOOGLE",
                "CUSTOM",
            ]
        );
        for code in &codes {
            assert!(!verification_platform(code)
                .expect("spec")
                .label_key
                .is_empty());
        }
        assert!(verification_platform("DOUYIN_NA").is_none());
    }

    #[test]
    fn each_platform_derives_the_name_it_will_fetch() {
        assert_eq!(
            suggested_verification_file_name("WECHAT_OFFICIAL_ACCOUNT", "abc123").as_deref(),
            Some("MP_verify_abc123.txt")
        );
        assert_eq!(
            suggested_verification_file_name("WECHAT_MINI_PROGRAM", "abc123").as_deref(),
            Some("MP_verify_abc123.txt")
        );
        assert_eq!(
            suggested_verification_file_name("ALIPAY", "tok").as_deref(),
            Some("tok.txt")
        );
        assert_eq!(
            suggested_verification_file_name("BYTEDANCE", "tok").as_deref(),
            Some("tok.txt")
        );
        assert_eq!(
            suggested_verification_file_name("APPLE", "ignored").as_deref(),
            Some("apple-developer-domain-association.txt")
        );
        assert_eq!(
            suggested_verification_file_name("GOOGLE", "g-tok").as_deref(),
            Some("g-tok.html")
        );
        // A token that is already a servable file name is not double-suffixed.
        assert_eq!(
            suggested_verification_file_name("GOOGLE", "google123.html").as_deref(),
            Some("google123.html")
        );
        assert!(suggested_verification_file_name("WECHAT_OFFICIAL_ACCOUNT", " ").is_none());
    }

    #[test]
    fn the_extension_rule_keeps_a_platform_to_its_own_shape() {
        assert!(platform_accepts_extension("WECHAT_OFFICIAL_ACCOUNT", "txt"));
        assert!(!platform_accepts_extension(
            "WECHAT_OFFICIAL_ACCOUNT",
            "html"
        ));
        assert!(platform_accepts_extension("GOOGLE", "html"));
        assert!(!platform_accepts_extension("GOOGLE", "txt"));
        assert!(platform_accepts_extension("APPLE", "txt"));
        // CUSTOM is the escape hatch for platforms without a catalog entry.
        assert!(platform_accepts_extension("CUSTOM", "txt"));
        assert!(platform_accepts_extension("CUSTOM", "html"));
        assert!(platform_accepts_extension("CUSTOM", "xml"));
        assert!(!platform_accepts_extension("CUSTOM", "js"));
    }

    #[test]
    fn servable_names_match_the_edge_rules() {
        assert!(is_servable_verification_file_name("MP_verify_abc.txt"));
        assert!(is_servable_verification_file_name("google123.html"));
        assert!(is_servable_verification_file_name("BingSiteAuth.xml"));
        assert!(!is_servable_verification_file_name("MP_verify_abc.txt/"));
        assert!(!is_servable_verification_file_name("dir/MP_verify_abc.txt"));
        assert!(!is_servable_verification_file_name("app.js"));
        assert!(!is_servable_verification_file_name(""));
    }

    #[test]
    fn content_types_match_the_edge_table() {
        assert_eq!(
            content_type_for_extension("txt"),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            content_type_for_extension("html"),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type_for_extension("xml"),
            "application/xml; charset=utf-8"
        );
    }
}

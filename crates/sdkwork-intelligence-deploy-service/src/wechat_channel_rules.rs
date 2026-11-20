//! WeChat channel rules that must be enforced before a provider call.
//!
//! Domain edits, privacy declarations, category applications, and review
//! packages all have documented limits, and every one of them is quota-bound or
//! review-blocking. The rules live here as pure validators so a command is
//! rejected locally with a precise reason instead of consuming a monthly quota
//! or failing a review with `44002`/`61040`.
//!
//! These are Deploy-side guards. The provider remains the authority on whether a
//! domain is备案 or a category is accepted.

/// Maximum configured domains per type (request/socket/upload/download/udp).
pub const MAX_WECHAT_DOMAINS_PER_KIND: usize = 200;
/// Monthly domain-modification quota per account.
pub const WECHAT_DOMAIN_MONTHLY_CHANGE_LIMIT: u32 = 50;
/// Maximum categories in one audit submission.
pub const MAX_AUDIT_CATEGORIES: usize = 5;
/// Maximum characters in the review feedback field.
pub const MAX_AUDIT_FEEDBACK_CHARS: usize = 200;

/// The domain edit action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WechatDomainAction {
    /// Append domains.
    Add,
    /// Replace the whole set.
    Set,
    /// Remove domains.
    Delete,
    /// Read the current set.
    Get,
}

impl WechatDomainAction {
    /// Stable wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Set => "set",
            Self::Delete => "delete",
            Self::Get => "get",
        }
    }

    /// Parses the wire value; unknown actions stay unknown instead of guessing.
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "add" => Self::Add,
            "set" => Self::Set,
            "delete" => Self::Delete,
            "get" => Self::Get,
            _ => return None,
        })
    }
}

/// One domain edit request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WechatDomainChange {
    /// Edit action.
    pub action: WechatDomainAction,
    /// Domain entries, `https://host` form.
    pub domains: Vec<String>,
}

/// Validates one domain edit before it consumes the monthly quota.
pub fn validate_wechat_domain_change(change: &WechatDomainChange) -> Result<(), String> {
    if change.action == WechatDomainAction::Get {
        if !change.domains.is_empty() {
            return Err("a get action must not carry domains".to_string());
        }
        return Ok(());
    }
    if change.domains.is_empty() {
        return Err(format!(
            "{} requires at least one domain",
            change.action.as_str()
        ));
    }
    if change.domains.len() > MAX_WECHAT_DOMAINS_PER_KIND {
        return Err(format!(
            "at most {MAX_WECHAT_DOMAINS_PER_KIND} domains may be configured per type"
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for domain in &change.domains {
        let normalized = normalize_domain(domain)?;
        if !seen.insert(normalized) {
            return Err(format!("domain {domain} is duplicated"));
        }
    }
    Ok(())
}

/// Normalizes and validates one provider domain entry.
///
/// WeChat accepts `https://host` (no port, no path) for request domains; this
/// guard rejects an entry the provider would reject instead of spending quota.
pub fn normalize_domain(domain: &str) -> Result<String, String> {
    let trimmed = domain.trim();
    let Some(rest) = trimmed.strip_prefix("https://") else {
        return Err(format!("domain {domain} must start with https://"));
    };
    let host = rest.trim_end_matches('/');
    if host.is_empty() || host.contains('/') || host.contains(':') || host.contains('?') {
        return Err(format!(
            "domain {domain} must be a bare https host without port or path"
        ));
    }
    Ok(host.to_ascii_lowercase())
}

/// True when the monthly domain quota is exhausted.
pub fn domain_quota_exhausted(changes_this_month: u32) -> bool {
    changes_this_month >= WECHAT_DOMAIN_MONTHLY_CHANGE_LIMIT
}

/// Privacy declaration version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WechatPrivacyVersion {
    /// Development version (`privacy_ver = 2`).
    Development,
    /// Live version (`privacy_ver = 1`).
    Live,
}

impl WechatPrivacyVersion {
    /// Wire value used by `setprivacysetting`.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Live => 1,
            Self::Development => 2,
        }
    }

    /// Parses the wire value; anything else is not a privacy version.
    pub const fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::Live),
            2 => Some(Self::Development),
            _ => None,
        }
    }
}

/// One privacy declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WechatPrivacySetting {
    /// Target version.
    pub privacy_version: WechatPrivacyVersion,
    /// Contact email, phone, QQ, or WeChat id; at least one is required.
    pub contact_email: Option<String>,
    /// Phone contact.
    pub contact_phone: Option<String>,
    /// QQ contact.
    pub contact_qq: Option<String>,
    /// WeChat id contact.
    pub contact_weixin: Option<String>,
    /// How the user is notified.
    pub notice_method: String,
    /// Store region for an overseas subject.
    pub store_region: Option<String>,
    /// True when the subject is registered outside mainland China.
    pub overseas_subject: bool,
}

/// Validates a privacy declaration before submission.
pub fn validate_wechat_privacy_setting(setting: &WechatPrivacySetting) -> Result<(), String> {
    if setting.notice_method.trim().is_empty() {
        return Err("noticeMethod is required".to_string());
    }
    let contacts = [
        setting.contact_email.as_deref(),
        setting.contact_phone.as_deref(),
        setting.contact_qq.as_deref(),
        setting.contact_weixin.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .count();
    if contacts != 1 {
        return Err("exactly one privacy contact channel is required".to_string());
    }
    if setting.overseas_subject
        && setting
            .store_region
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err("storeRegion is required for an overseas subject".to_string());
    }
    Ok(())
}

/// One category entry in a review submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WechatAuditCategory {
    /// Primary category id.
    pub first_id: String,
    /// Secondary category id.
    pub second_id: String,
}

/// One review submission package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WechatAuditSubmission {
    /// Categories the version is submitted under.
    pub categories: Vec<WechatAuditCategory>,
    /// Reviewer feedback, at most 200 characters.
    pub feedback: Option<String>,
}

/// Validates the review package shape before `submit_audit`.
pub fn validate_wechat_audit_submission(submission: &WechatAuditSubmission) -> Result<(), String> {
    if submission.categories.is_empty() {
        return Err("at least one category is required".to_string());
    }
    if submission.categories.len() > MAX_AUDIT_CATEGORIES {
        return Err(format!(
            "at most {MAX_AUDIT_CATEGORIES} categories may be submitted"
        ));
    }
    for category in &submission.categories {
        if category.first_id.trim().is_empty() || category.second_id.trim().is_empty() {
            return Err("category first and second ids are required".to_string());
        }
    }
    if let Some(feedback) = submission.feedback.as_deref() {
        if feedback.chars().count() > MAX_AUDIT_FEEDBACK_CHARS {
            return Err(format!(
                "feedback must be at most {MAX_AUDIT_FEEDBACK_CHARS} characters"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(action: WechatDomainAction, domains: &[&str]) -> WechatDomainChange {
        WechatDomainChange {
            action,
            domains: domains.iter().map(|item| item.to_string()).collect(),
        }
    }

    #[test]
    fn accepts_a_bare_https_domain() {
        assert!(validate_wechat_domain_change(&change(
            WechatDomainAction::Add,
            &["https://api.example.com"]
        ))
        .is_ok());
        assert_eq!(
            normalize_domain("https://API.Example.com/").expect("normalize"),
            "api.example.com"
        );
    }

    #[test]
    fn rejects_paths_ports_and_plain_hosts() {
        for domain in [
            "http://api.example.com",
            "api.example.com",
            "https://api.example.com/v1",
            "https://api.example.com:443",
        ] {
            assert!(
                validate_wechat_domain_change(&change(WechatDomainAction::Add, &[domain])).is_err(),
                "{domain} must be rejected"
            );
        }
    }

    #[test]
    fn get_requires_an_empty_set_and_edits_require_domains() {
        assert!(validate_wechat_domain_change(&change(WechatDomainAction::Get, &[])).is_ok());
        assert!(validate_wechat_domain_change(&change(
            WechatDomainAction::Get,
            &["https://api.example.com"]
        ))
        .is_err());
        assert!(validate_wechat_domain_change(&change(WechatDomainAction::Set, &[])).is_err());
    }

    #[test]
    fn rejects_duplicates_and_the_domain_cap() {
        assert!(validate_wechat_domain_change(&change(
            WechatDomainAction::Add,
            &["https://a.example.com", "https://A.example.com"]
        ))
        .is_err());
        let domains: Vec<String> = (0..=MAX_WECHAT_DOMAINS_PER_KIND)
            .map(|index| format!("https://d{index}.example.com"))
            .collect();
        let over = WechatDomainChange {
            action: WechatDomainAction::Add,
            domains,
        };
        assert!(validate_wechat_domain_change(&over).is_err());
    }

    #[test]
    fn domain_quota_is_fifty_per_month() {
        assert!(!domain_quota_exhausted(49));
        assert!(domain_quota_exhausted(50));
    }

    #[test]
    fn domain_actions_round_trip_and_reject_unknown_values() {
        for action in [
            WechatDomainAction::Add,
            WechatDomainAction::Set,
            WechatDomainAction::Delete,
            WechatDomainAction::Get,
        ] {
            assert_eq!(WechatDomainAction::from_wire(action.as_str()), Some(action));
        }
        assert_eq!(WechatDomainAction::from_wire("replace"), None);
    }

    fn privacy() -> WechatPrivacySetting {
        WechatPrivacySetting {
            privacy_version: WechatPrivacyVersion::Live,
            contact_email: Some("ops@example.com".to_string()),
            contact_phone: None,
            contact_qq: None,
            contact_weixin: None,
            notice_method: "in-app notice".to_string(),
            store_region: None,
            overseas_subject: false,
        }
    }

    #[test]
    fn privacy_requires_exactly_one_contact_and_notice_method() {
        assert!(validate_wechat_privacy_setting(&privacy()).is_ok());
        let mut two = privacy();
        two.contact_phone = Some("+8610000000000".to_string());
        assert!(validate_wechat_privacy_setting(&two).is_err());
        let mut none = privacy();
        none.contact_email = None;
        assert!(validate_wechat_privacy_setting(&none).is_err());
        let mut no_notice = privacy();
        no_notice.notice_method = " ".to_string();
        assert!(validate_wechat_privacy_setting(&no_notice).is_err());
    }

    #[test]
    fn overseas_privacy_requires_a_store_region() {
        let mut overseas = privacy();
        overseas.overseas_subject = true;
        assert!(validate_wechat_privacy_setting(&overseas).is_err());
        overseas.store_region = Some("US".to_string());
        assert!(validate_wechat_privacy_setting(&overseas).is_ok());
    }

    #[test]
    fn privacy_versions_map_to_the_documented_codes() {
        assert_eq!(WechatPrivacyVersion::Live.as_i64(), 1);
        assert_eq!(WechatPrivacyVersion::Development.as_i64(), 2);
        assert_eq!(
            WechatPrivacyVersion::from_i64(1),
            Some(WechatPrivacyVersion::Live)
        );
        assert_eq!(
            WechatPrivacyVersion::from_i64(2),
            Some(WechatPrivacyVersion::Development)
        );
        assert_eq!(WechatPrivacyVersion::from_i64(3), None);
    }

    fn submission(count: usize) -> WechatAuditSubmission {
        WechatAuditSubmission {
            categories: (0..count)
                .map(|index| WechatAuditCategory {
                    first_id: format!("1{index}"),
                    second_id: format!("2{index}"),
                })
                .collect(),
            feedback: None,
        }
    }

    #[test]
    fn audit_submission_is_bounded() {
        assert!(validate_wechat_audit_submission(&submission(1)).is_ok());
        assert!(validate_wechat_audit_submission(&submission(0)).is_err());
        assert!(validate_wechat_audit_submission(&submission(MAX_AUDIT_CATEGORIES + 1)).is_err());
        let mut long_feedback = submission(1);
        long_feedback.feedback = Some("f".repeat(MAX_AUDIT_FEEDBACK_CHARS + 1));
        assert!(validate_wechat_audit_submission(&long_feedback).is_err());
        let mut empty_category = submission(1);
        empty_category.categories[0].second_id = String::new();
        assert!(validate_wechat_audit_submission(&empty_category).is_err());
    }
}

//! Builds ACME DNS-01 presenters from control-plane DNS provider credentials.
//!
//! The control plane stores only a `secret://` reference plus a provider kind
//! and an optional provider zone identity; the secret itself is resolved by the
//! caller and handed to this module as an in-memory payload. Nothing here reads
//! a secret store, and no credential field is ever rendered into an error, a
//! log line, or a `Debug` representation.

use std::fmt;
use std::sync::Arc;

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_webserver_acme_service::{
    AcmeServiceResult, AliyunDns01Presenter, CloudflareDns01Presenter, Dns01Presenter,
    DnsApiClient, DnsProviderKind, DnspodDns01Presenter,
};

/// A credential value that never renders its contents.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The only way to read the secret. Deliberately explicit so a call site is
    /// visible in review.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// Resolved credential material for one DNS provider family.
#[derive(Clone, PartialEq, Eq)]
pub enum DeployDnsProviderCredential {
    AliyunDns {
        access_key_id: String,
        access_key_secret: SecretString,
    },
    Dnspod {
        login_id: String,
        api_token: SecretString,
    },
    Cloudflare {
        api_token: SecretString,
    },
}

impl fmt::Debug for DeployDnsProviderCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The provider kind and the non-secret identifiers are useful in logs;
        // every secret field is redacted by `SecretString`.
        match self {
            Self::AliyunDns { access_key_id, .. } => formatter
                .debug_struct("DeployDnsProviderCredential::AliyunDns")
                .field("access_key_id", access_key_id)
                .field("access_key_secret", &"<redacted>")
                .finish(),
            Self::Dnspod { login_id, .. } => formatter
                .debug_struct("DeployDnsProviderCredential::Dnspod")
                .field("login_id", login_id)
                .field("api_token", &"<redacted>")
                .finish(),
            Self::Cloudflare { .. } => formatter
                .debug_struct("DeployDnsProviderCredential::Cloudflare")
                .field("api_token", &"<redacted>")
                .finish(),
        }
    }
}

impl DeployDnsProviderCredential {
    pub const fn kind(&self) -> DnsProviderKind {
        match self {
            Self::AliyunDns { .. } => DnsProviderKind::AliyunDns,
            Self::Dnspod { .. } => DnsProviderKind::Dnspod,
            Self::Cloudflare { .. } => DnsProviderKind::Cloudflare,
        }
    }

    /// Builds the credential for a family from the two halves every supported
    /// family's secret reduces to.
    ///
    /// This is the account-center shape rather than a provider one: the account
    /// center stores a secret as an optional identifier plus a secret, because that
    /// is all any of these families needs, and it deliberately does not know that
    /// DNSPod calls its identifier a login id. The translation to each provider's
    /// own field names belongs here, next to the presenter that will use them.
    pub fn from_account_credential(
        dns_provider: &str,
        access_key_id: &str,
        secret_access_key: &str,
    ) -> DeployServiceResult<Self> {
        let kind = DnsProviderKind::parse(dns_provider).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "unsupported DNS provider kind {dns_provider}; supported: ALIYUN_DNS, DNSPOD, CLOUDFLARE"
            ))
        })?;
        let identifier = access_key_id.trim();
        let secret = secret_access_key.trim();
        if secret.is_empty() {
            return Err(DeployServiceError::validation(
                "the DNS provider credential has no secret half",
            ));
        }
        match kind {
            DnsProviderKind::AliyunDns => {
                if identifier.is_empty() {
                    return Err(DeployServiceError::validation(
                        "the ALIYUN_DNS credential has no AccessKeyId",
                    ));
                }
                Ok(Self::AliyunDns {
                    access_key_id: identifier.to_owned(),
                    access_key_secret: SecretString::new(secret),
                })
            }
            DnsProviderKind::Dnspod => {
                if identifier.is_empty() {
                    return Err(DeployServiceError::validation(
                        "the DNSPOD credential has no login id",
                    ));
                }
                Ok(Self::Dnspod {
                    login_id: identifier.to_owned(),
                    api_token: SecretString::new(secret),
                })
            }
            // Cloudflare authenticates with the token alone; an identifier sent
            // alongside it is ignored rather than rejected, because the console's
            // one-form design cannot know which family will need one.
            DnsProviderKind::Cloudflare => Ok(Self::Cloudflare {
                api_token: SecretString::new(secret),
            }),
        }
    }

    /// Parses the JSON payload a `deploy_dns_provider_credential` row's
    /// `credential_secret_ref` resolves to.
    ///
    /// Unknown fields are ignored so a provider can add metadata without a
    /// control-plane migration, but an unknown *provider kind* is an error: it
    /// means the row and this build disagree about the integration.
    pub fn from_secret_payload(provider_kind: &str, payload: &str) -> DeployServiceResult<Self> {
        let kind = DnsProviderKind::parse(provider_kind).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "unsupported DNS provider kind {provider_kind}; supported: ALIYUN_DNS, DNSPOD, CLOUDFLARE"
            ))
        })?;
        let document: serde_json::Value = serde_json::from_str(payload).map_err(|_| {
            // The payload is intentionally not echoed: a malformed secret
            // document is exactly the case where it would leak.
            DeployServiceError::validation("DNS provider credential is not a JSON object")
        })?;
        let object = document.as_object().ok_or_else(|| {
            DeployServiceError::validation("DNS provider credential is not a JSON object")
        })?;

        match kind {
            DnsProviderKind::AliyunDns => Ok(Self::AliyunDns {
                access_key_id: require_field(object, "accessKeyId")?,
                access_key_secret: SecretString::new(require_field(object, "accessKeySecret")?),
            }),
            DnsProviderKind::Dnspod => Ok(Self::Dnspod {
                login_id: require_field(object, "loginId")?,
                api_token: SecretString::new(require_field(object, "apiToken")?),
            }),
            DnsProviderKind::Cloudflare => Ok(Self::Cloudflare {
                api_token: SecretString::new(require_field(object, "apiToken")?),
            }),
        }
    }
}

fn require_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> DeployServiceResult<String> {
    let value = object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DeployServiceError::validation(format!(
                "DNS provider credential is missing a non-empty {field}"
            ))
        })?;
    Ok(value.to_string())
}

/// Builds the DNS-01 presenter for one issuance.
///
/// `zone_ref` is the provider-side zone identity cached in the credential row.
/// It is only meaningful to providers that address a zone by an opaque id; the
/// adapters that address a zone by name ignore it.
pub fn build_dns01_presenter(
    credential: &DeployDnsProviderCredential,
    zone_ref: Option<&str>,
) -> AcmeServiceResult<Arc<dyn Dns01Presenter>> {
    let client = DnsApiClient::new()?;
    let presenter: Arc<dyn Dns01Presenter> = match credential {
        DeployDnsProviderCredential::AliyunDns {
            access_key_id,
            access_key_secret,
        } => Arc::new(AliyunDns01Presenter::new(
            client,
            access_key_id,
            access_key_secret.expose(),
        )?),
        DeployDnsProviderCredential::Dnspod {
            login_id,
            api_token,
        } => Arc::new(DnspodDns01Presenter::new(
            client,
            login_id,
            api_token.expose(),
        )?),
        DeployDnsProviderCredential::Cloudflare { api_token } => {
            Arc::new(CloudflareDns01Presenter::new(
                client,
                api_token.expose(),
                zone_ref.map(str::to_string),
            )?)
        }
    };
    Ok(presenter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_provider_payload() {
        let aliyun = DeployDnsProviderCredential::from_secret_payload(
            "ALIYUN_DNS",
            r#"{"accessKeyId":"LTAI-test","accessKeySecret":"aliyun-secret"}"#,
        )
        .expect("aliyun payload");
        assert_eq!(aliyun.kind(), DnsProviderKind::AliyunDns);

        let dnspod = DeployDnsProviderCredential::from_secret_payload(
            "DNSPOD",
            r#"{"loginId":"12345","apiToken":"dnspod-secret"}"#,
        )
        .expect("dnspod payload");
        assert_eq!(dnspod.kind(), DnsProviderKind::Dnspod);

        let cloudflare = DeployDnsProviderCredential::from_secret_payload(
            "CLOUDFLARE",
            r#"{"apiToken":"cf-secret"}"#,
        )
        .expect("cloudflare payload");
        assert_eq!(cloudflare.kind(), DnsProviderKind::Cloudflare);
    }

    #[test]
    fn ignores_unknown_fields_but_requires_the_known_ones() {
        let credential = DeployDnsProviderCredential::from_secret_payload(
            "CLOUDFLARE",
            r#"{"apiToken":"cf-secret","futureField":"ignored"}"#,
        );
        assert!(credential.is_ok());

        let missing = DeployDnsProviderCredential::from_secret_payload(
            "CLOUDFLARE",
            r#"{"accountId":"only-an-id"}"#,
        );
        assert!(missing.is_err());

        let blank =
            DeployDnsProviderCredential::from_secret_payload("CLOUDFLARE", r#"{"apiToken":"   "}"#);
        assert!(blank.is_err());
    }

    #[test]
    fn an_unknown_provider_kind_is_rejected() {
        let error = DeployDnsProviderCredential::from_secret_payload("ROUTE53", "{}")
            .expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("unsupported DNS provider kind"));
        assert!(message.contains("ALIYUN_DNS"));
    }

    #[test]
    fn a_malformed_document_never_echoes_its_content() {
        for payload in ["not json", "", "[]", "\"a string\"", "42"] {
            let error = DeployDnsProviderCredential::from_secret_payload("CLOUDFLARE", payload)
                .expect_err("must fail");
            let message = error.to_string();
            assert!(!message.contains("a string"), "leaked payload: {message}");
            assert!(!message.contains("42"), "leaked payload: {message}");
        }
    }

    #[test]
    fn debug_redacts_every_secret_field() {
        let credential = DeployDnsProviderCredential::from_secret_payload(
            "ALIYUN_DNS",
            r#"{"accessKeyId":"LTAI-test","accessKeySecret":"aliyun-secret"}"#,
        )
        .expect("payload");
        let rendered = format!("{credential:?}");
        assert!(rendered.contains("LTAI-test"));
        assert!(!rendered.contains("aliyun-secret"));
        assert!(rendered.contains("<redacted>"));

        let secret = SecretString::new("super-secret");
        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(secret.expose(), "super-secret");
    }

    #[test]
    fn builds_a_presenter_for_every_supported_provider() {
        let zone_ref = Some("zone-abc");
        for (kind, payload) in [
            (
                "ALIYUN_DNS",
                r#"{"accessKeyId":"LTAI-test","accessKeySecret":"aliyun-secret"}"#,
            ),
            (
                "DNSPOD",
                r#"{"loginId":"12345","apiToken":"dnspod-secret"}"#,
            ),
            ("CLOUDFLARE", r#"{"apiToken":"cf-secret"}"#),
        ] {
            let credential =
                DeployDnsProviderCredential::from_secret_payload(kind, payload).expect("payload");
            let presenter = build_dns01_presenter(&credential, zone_ref).expect("presenter builds");
            assert_eq!(
                presenter.provider_kind(),
                Some(credential.kind()),
                "presenter must report the provider it speaks to"
            );
        }
    }

    #[test]
    fn presenter_construction_rejects_credentials_the_adapter_rejects() {
        // A blank secret survives `from_secret_payload` only if it is non-empty
        // after trimming, so this exercises the adapter's own guard instead.
        let credential = DeployDnsProviderCredential::Cloudflare {
            api_token: SecretString::new("   "),
        };
        assert!(build_dns01_presenter(&credential, None).is_err());
    }
}

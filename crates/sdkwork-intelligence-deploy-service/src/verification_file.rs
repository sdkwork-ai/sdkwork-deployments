//! 域名归属校验文件：登记、归一化、自检。
//!
//! 各平台都要求在域名根路径可公开取回一个自己签发的文件，然后凭这份文件确认
//! 操作员确实控制该域名：
//!
//! | 平台 | 文件 | 典型来源 |
//! | --- | --- | --- |
//! | 微信公众号 | `MP_verify_<token>.txt` | JS 接口安全域名 / 网页授权域名 / 业务域名 |
//! | 微信小程序 | `MP_verify_<token>.txt` | 小程序业务域名 |
//! | 支付宝小程序 | `<token>.txt` | 小程序域名白名单 |
//! | 抖音小程序 | `<token>.txt` | 小程序服务器域名 |
//! | Google Search Console | `google<token>.html` | HTML 文件验证 |
//! | Apple | `apple-developer-domain-association.txt` | Associated Domains |
//!
//! 它们的机制相同，只有文件形态不同，所以这里是一个通用能力而不是"微信专用
//! 上传口"：一个 Zone 可以同时登记多份，边缘按 `<hostname>/<fileName>` 精确
//! 应答。
//!
//! # 存储即承诺
//!
//! 内容按上传原文逐字节存储、逐字节返回——平台按精确内容比对，任何改写（去尾
//! 换行、编码归一）都会把一次本可通过的验证变成"内容不匹配"。唯一的归一化是
//! 输入校验：文件名限定为单段 ASCII 且以 `.txt` / `.html` / `.xml` 结尾，内容
//! 拒绝控制字符但不改动任何字节。
//!
//! # 自检从不抛错
//!
//! 探查端口把传输错误折叠进结果而不是作为异常抛出：操作员要的是"为什么现在还
//! 不能去平台点验证"的一句话，不是一次 500。

use async_trait::async_trait;
use chrono::Utc;
use sdkwork_deploy_contract::{
    CreateDomainVerificationFileRequest, DeployServiceError, DeployServiceResult,
    DomainVerificationFileCheckResponse, DomainVerificationFilePage,
    DomainVerificationFileResponse,
};

/// 平台封闭集合。
///
/// 与 DDL 的 `chk_deploy_dns_zone_verification_file_platform`、契约的
/// `DomainVerificationFilePlatform` 枚举、控制台的选择项同源；三处一起改，
/// 不允许单边新增。
pub const VERIFICATION_PLATFORMS: &[&str] = &[
    "WECHAT_OFFICIAL_ACCOUNT",
    "WECHAT_MINI_PROGRAM",
    "ALIPAY",
    "BYTEDANCE",
    "APPLE",
    "GOOGLE",
    "CUSTOM",
];

/// 文件名上限（含扩展名）。平台签发的名字远短于此；上限只为封住滥用。
pub const VERIFICATION_FILE_NAME_MAX: usize = 128;

/// 文件内容上限（字节）。微信/支付宝的校验文件是一个短 token，Google 的
/// HTML 文件与 Apple 的关联文件也只有几行；4096 字节覆盖任何已知形态，并与
/// 边缘服务端的响应体上限一致。
pub const VERIFICATION_FILE_CONTENT_MAX: usize = 4096;

/// 校验文件实际使用的扩展名。边缘按扩展名推断 `Content-Type`，所以这份
/// 列表同时是"哪些文件能被应答"的定义。
pub const VERIFICATION_FILE_EXTENSIONS: [&str; 3] = ["txt", "html", "xml"];

/// 一个 Zone 最多登记多少份校验文件。
///
/// 边缘把命中主机名的整组文件缓存在进程内，所以这份上限首先是内存保护：平台
/// 数量有界（七种），同一平台换发的新旧文件也只有两份，32 足够任何真实用法，
/// 而它能挡住"用一个 zone 塞满边缘缓存"的租户。
pub const MAX_VERIFICATION_FILES_PER_ZONE: usize = 32;

/// 自检外呼的一次结果。"从不失败"：传输错误进 `error`，让调用方拿到的永远
/// 是一个可呈报的 outcome，而不是一个需要再分类的异常。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationProbeOutcome {
    pub status_code: Option<i32>,
    pub body: Option<String>,
    pub error: Option<String>,
}

/// 按平台爬虫的视角取回一个 URL 的响应原文。
#[async_trait]
pub trait VerificationFileProbePort: Send + Sync {
    /// GET `url`，有界超时与响应体上限，返回状态码与响应原文。
    async fn fetch_text(&self, url: &str) -> VerificationProbeOutcome;
}

/// 未配置探查端口的兜底实现：自检回答"探测端未配置"而不是误报不可达——
/// 两者给操作员的修复动作完全不同。
pub struct UnconfiguredVerificationFileProbe;

#[async_trait]
impl VerificationFileProbePort for UnconfiguredVerificationFileProbe {
    async fn fetch_text(&self, _url: &str) -> VerificationProbeOutcome {
        VerificationProbeOutcome {
            status_code: None,
            body: None,
            error: Some("verification probe is not configured for this deployment".to_owned()),
        }
    }
}

/// 一份已登记的校验文件，按仓储层交回的形态。
#[derive(Clone, Debug)]
pub struct VerificationFileRecord {
    /// 文件自身的 uuid：删除与自检都按它寻址，文件名可以被同一 Zone 的新文件
    /// 复用。
    pub id: String,
    pub platform: String,
    pub file_name: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

fn normalize_platform(value: &str) -> DeployServiceResult<String> {
    let value = value.trim().to_ascii_uppercase();
    if !VERIFICATION_PLATFORMS.contains(&value.as_str()) {
        return Err(DeployServiceError::validation(format!(
            "platform must be one of {}",
            VERIFICATION_PLATFORMS.join(", ")
        )));
    }
    Ok(value)
}

/// 接受 `<名称>.<txt|html|xml>` 的单段 ASCII 文件名。
fn normalize_file_name(value: &str) -> DeployServiceResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > VERIFICATION_FILE_NAME_MAX {
        return Err(DeployServiceError::validation(
            "fileName must contain 1 to 128 characters",
        ));
    }
    // 扩展名之前的名称必须存在并以字母数字开头：裸 ".txt" 意味着名为空，
    // 任何平台都不签发这样的文件，而它能绕过"以字母数字开头"的名称校验。
    let (stem, extension) = match value.rsplit_once('.') {
        Some(parts) => parts,
        None => ("", ""),
    };
    let valid_extension = VERIFICATION_FILE_EXTENSIONS
        .iter()
        .any(|allowed| *allowed == extension);
    let valid = !stem.is_empty()
        && stem
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && valid_extension;
    if !valid {
        return Err(DeployServiceError::validation(
            "fileName must use ASCII letters, digits, dots, hyphens or underscores and end with .txt, .html or .xml",
        ));
    }
    Ok(value.to_owned())
}

fn normalize_content(value: &str) -> DeployServiceResult<String> {
    if value.is_empty() || value.len() > VERIFICATION_FILE_CONTENT_MAX {
        return Err(DeployServiceError::validation(
            "content must contain 1 to 4096 bytes",
        ));
    }
    // 控制字符（换行/回车/制表除外）会在响应体里产生不可预期字节；直接
    // 拒绝而不是悄悄剥离，剥离本身就是对"逐字节比对"承诺的破坏。
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(DeployServiceError::validation(
            "content must not contain control characters",
        ));
    }
    Ok(value.to_owned())
}

fn to_response(zone_id: &str, record: VerificationFileRecord) -> DomainVerificationFileResponse {
    DomainVerificationFileResponse {
        id: record.id,
        zone_id: zone_id.to_owned(),
        platform: record.platform,
        file_name: record.file_name,
        content: record.content,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Refuses a *new* file name once the zone holds
/// [`MAX_VERIFICATION_FILES_PER_ZONE`]; replacing an existing name is always
/// allowed, because a platform reissuing its file must not be blocked by the cap.
pub(crate) fn ensure_verification_file_capacity(
    existing_file_names: &[String],
    file_name: &str,
) -> DeployServiceResult<()> {
    if existing_file_names.len() >= MAX_VERIFICATION_FILES_PER_ZONE
        && !existing_file_names.iter().any(|name| name == file_name)
    {
        return Err(DeployServiceError::conflict(format!(
            "a zone may publish at most {MAX_VERIFICATION_FILES_PER_ZONE} verification files; \
             delete one before adding another"
        )));
    }
    Ok(())
}

impl crate::DeployService {
    /// Every verification file registered on the Zone, platform order first.
    pub(crate) async fn list_verification_files(
        &self,
        tenant_id: i64,
        owner: sdkwork_deploy_contract::OwnershipReach,
        zone_id: &str,
    ) -> DeployServiceResult<DomainVerificationFilePage> {
        let records = self
            .repository
            .list_dns_zone_verification_files(tenant_id, owner, zone_id)
            .await?;
        let items: Vec<DomainVerificationFileResponse> = records
            .into_iter()
            .map(|record| to_response(zone_id, record))
            .collect();
        let total = items.len() as i64;
        let page_size = items.len().max(1) as i32;
        Ok(DomainVerificationFilePage {
            items,
            total,
            page: 1,
            page_size,
        })
    }

    /// Registers a verification file, or replaces the one with the same name.
    ///
    /// Replacement is whole on purpose: when a platform reissues its file, the
    /// old name *and* content stop being valid together, and keeping either
    /// leaves the edge answering something the platform no longer accepts.
    pub(crate) async fn create_verification_file(
        &self,
        tenant_id: i64,
        owner: sdkwork_deploy_contract::OwnershipReach,
        zone_id: &str,
        request: &CreateDomainVerificationFileRequest,
    ) -> DeployServiceResult<DomainVerificationFileResponse> {
        let platform = normalize_platform(&request.platform)?;
        let file_name = normalize_file_name(&request.file_name)?;
        let content = normalize_content(&request.content)?;
        let existing: Vec<String> = self
            .repository
            .list_dns_zone_verification_files(tenant_id, owner, zone_id)
            .await?
            .into_iter()
            .map(|record| record.file_name)
            .collect();
        ensure_verification_file_capacity(&existing, &file_name)?;
        let record = self
            .repository
            .upsert_dns_zone_verification_file(
                tenant_id, owner, zone_id, &platform, &file_name, &content,
            )
            .await?;
        Ok(to_response(zone_id, record))
    }

    pub(crate) async fn delete_verification_file(
        &self,
        tenant_id: i64,
        owner: sdkwork_deploy_contract::OwnershipReach,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<()> {
        let deleted = self
            .repository
            .delete_dns_zone_verification_file(tenant_id, owner, zone_id, file_id)
            .await?;
        if !deleted {
            return Err(DeployServiceError::not_found(
                "no verification file with this id is registered on the zone",
            ));
        }
        Ok(())
    }

    /// 自检：按平台爬虫的视角取回 `https://<apex>/<fileName>`（失败再试
    /// http），与保存内容逐字节比对。
    ///
    /// https 命中即采信——各平台校验的都是 https 上的安全域名；https 不可达
    /// 时再试 http，把"域名解析了但没配证书"与"根本没解析"区分开。内容不匹配
    /// 时两个方案都试完，操作员拿到的是两种协议各自的事实。
    pub(crate) async fn check_verification_file(
        &self,
        tenant_id: i64,
        owner: sdkwork_deploy_contract::OwnershipReach,
        zone_id: &str,
        file_id: &str,
    ) -> DeployServiceResult<DomainVerificationFileCheckResponse> {
        let record = self
            .repository
            .retrieve_dns_zone_verification_file(tenant_id, owner, zone_id, file_id)
            .await?;
        let apex = self
            .repository
            .retrieve_domain_zone(tenant_id, owner, zone_id)
            .await?
            .apex_hostname;
        let checked_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let mut reachable: Option<(i32, &'static str)> = None;
        let mut details: Vec<String> = Vec::new();
        for scheme in ["https", "http"] {
            let url = format!("{scheme}://{apex}/{}", record.file_name);
            let outcome = self.verification_probe.fetch_text(&url).await;
            match (outcome.status_code, outcome.body) {
                (Some(status), Some(body)) if (200..300).contains(&status) => {
                    if body == record.content {
                        return Ok(DomainVerificationFileCheckResponse {
                            reachable: true,
                            matched: true,
                            status_code: Some(status),
                            scheme: Some(scheme.to_owned()),
                            detail: None,
                            checked_at,
                        });
                    }
                    if reachable.is_none() {
                        reachable = Some((status, scheme));
                    }
                    details.push(format!(
                        "{scheme}: reachable (HTTP {status}) but content differs from the stored file"
                    ));
                }
                (Some(status), _) => {
                    details.push(format!("{scheme}: HTTP {status}"));
                }
                (None, _) => {
                    let error = outcome.error.unwrap_or_else(|| "request failed".to_owned());
                    details.push(format!("{scheme}: {error}"));
                }
            }
        }
        let (status_code, scheme) = reachable
            .map(|(status, scheme)| (Some(status), Some(scheme.to_owned())))
            .unwrap_or((None, None));
        Ok(DomainVerificationFileCheckResponse {
            reachable: reachable.is_some(),
            matched: false,
            status_code,
            scheme,
            detail: Some(details.join("; ")),
            checked_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_verification_file_capacity, normalize_content, normalize_file_name,
        normalize_platform, MAX_VERIFICATION_FILES_PER_ZONE,
    };

    #[test]
    fn a_new_file_is_refused_once_the_zone_is_full() {
        let full: Vec<String> = (0..MAX_VERIFICATION_FILES_PER_ZONE)
            .map(|index| format!("MP_verify_{index}.txt"))
            .collect();
        assert!(
            ensure_verification_file_capacity(&full, "MP_verify_new.txt").is_err(),
            "a new name must be refused when the cap is reached"
        );
        // Replacing an existing name is the platform-reissue path and must work.
        assert!(ensure_verification_file_capacity(&full, "MP_verify_0.txt").is_ok());
        // One below the cap still accepts a new name.
        let roomy: Vec<String> = full
            .iter()
            .take(MAX_VERIFICATION_FILES_PER_ZONE - 1)
            .cloned()
            .collect();
        assert!(ensure_verification_file_capacity(&roomy, "MP_verify_new.txt").is_ok());
    }

    #[test]
    fn file_names_accept_the_shipped_platform_shapes() {
        // 微信各端（公众号 / 小程序）与 Apple 的 .txt
        assert_eq!(
            normalize_file_name(" MP_verify_aB123.txt ").unwrap(),
            "MP_verify_aB123.txt"
        );
        assert_eq!(
            normalize_file_name("apple-developer-domain-association.txt").unwrap(),
            "apple-developer-domain-association.txt"
        );
        // Google Search Console 的 HTML 文件、Bing 的 XML 文件
        assert_eq!(
            normalize_file_name("google1a2b3c4d5e6f7g8h.html").unwrap(),
            "google1a2b3c4d5e6f7g8h.html"
        );
        assert_eq!(
            normalize_file_name("BingSiteAuth.xml").unwrap(),
            "BingSiteAuth.xml"
        );
        for bad in [
            "",
            "no-extension",
            ".txt",
            "spaces in name.txt",
            "中文.txt",
            "/path/MP_verify.txt",
            "a.txt.exe",
            "a.php",
            "a.TXT",
            "a.txt/../b.txt",
        ] {
            assert!(normalize_file_name(bad).is_err(), "{bad} must be refused");
        }
        // 大写与内嵌点都在契约字符集内（schema 的 pattern 同此）。
        assert_eq!(
            normalize_file_name("UPPER.CASE.txt").unwrap(),
            "UPPER.CASE.txt"
        );
        let long = format!("{}.txt", "a".repeat(125));
        assert_eq!(long.len(), 129);
        assert!(normalize_file_name(&long).is_err());
        let at_limit = format!("{}.txt", "a".repeat(124));
        assert_eq!(at_limit.len(), 128);
        assert!(normalize_file_name(&at_limit).is_ok());
    }

    #[test]
    fn content_is_bounded_and_kept_verbatim() {
        assert_eq!(normalize_content("token-abc").unwrap(), "token-abc");
        // 换行被保留：逐字节存储意味着逐字节保留。
        assert_eq!(normalize_content("token\n").unwrap(), "token\n");
        assert!(normalize_content("").is_err());
        assert!(normalize_content(&"x".repeat(4097)).is_err());
        assert!(normalize_content(&"x".repeat(4096)).is_ok());
        assert!(normalize_content("ok\u{0}bad").is_err());
        // Google 的 HTML 校验文件是多行原文，必须整份存下来。
        let html = "<html>\n<body>\ngoogle-site-verification: google123.html\n</body>\n</html>\n";
        assert_eq!(normalize_content(html).unwrap(), html);
    }

    #[test]
    fn platforms_are_a_closed_set_normalized_to_wire_codes() {
        assert_eq!(
            normalize_platform("wechat_mini_program").unwrap(),
            "WECHAT_MINI_PROGRAM"
        );
        assert_eq!(
            normalize_platform(" WECHAT_OFFICIAL_ACCOUNT ").unwrap(),
            "WECHAT_OFFICIAL_ACCOUNT"
        );
        assert_eq!(normalize_platform("custom").unwrap(), "CUSTOM");
        assert!(normalize_platform("").is_err());
        assert!(normalize_platform("WECHAT").is_err());
    }
}

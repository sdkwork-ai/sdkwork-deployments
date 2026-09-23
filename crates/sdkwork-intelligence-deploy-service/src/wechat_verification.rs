//! ⚠️ 未落地：crate 内没有任何 `mod` 声明指向本文件，rustc 不会编译它。
//! 它随 `wip/deploy-certificate` 一并到来，是后续落地的规格——接线方式是补上
//! 模块声明，不是删除本文件。
//!
//! 微信公众号域名验证文件：存储、归一化、自检。
//!
//! 微信公众平台要求在域名根路径可访问一个形如 `MP_verify_xxxxx.txt` 的校验
//! 文件（JS 接口安全域名 / 网页授权域名 / 业务域名均如此）。控制台上传文件
//! 原文，边缘数据面按域名原样提供，自检端口按微信爬虫的视角取回比对。
//!
//! # 存储即承诺
//!
//! 内容按上传原文逐字节存储、逐字节返回——微信按精确内容比对，任何改写
//! （去尾换行、编码归一）都会把一次本可通过的验证变成"内容不匹配"。唯一
//! 的归一化是输入校验：文件名限定为微信形态的 `<名称>.txt`，内容拒绝控制
//! 字符但不改动任何字节。
//!
//! # 自检从不抛错
//!
//! 探查端口把传输错误折叠进结果而不是作为异常抛出：操作员要的是"为什么
//! 还不能去微信点验证"的一句话，不是一次 500。

use async_trait::async_trait;
use chrono::Utc;
use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, DomainWechatVerificationCheckResponse,
    DomainWechatVerificationResponse, OwnershipReach, UpsertDomainWechatVerificationRequest,
};

/// 文件名上限（含 `.txt` 后缀）。微信生成的文件名远短于此；上限只为封住滥用。
pub const WECHAT_VERIFICATION_FILE_NAME_MAX: usize = 64;

/// 文件内容上限（字节）。微信校验文件是一个短 token；2048 字节覆盖任何已知
/// 形态，并与边缘服务端的响应体上限一致。
pub const WECHAT_VERIFICATION_CONTENT_MAX: usize = 2048;

/// 自检外呼的一次结果。"从不失败"：传输错误进 `error`，让调用方拿到的永远
/// 是一个可呈报的 outcome，而不是一个需要再分类的异常。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WechatProbeOutcome {
    pub status_code: Option<i32>,
    pub body: Option<String>,
    pub error: Option<String>,
}

/// 按微信爬虫的视角取回一个 URL 的响应原文。
#[async_trait]
pub trait WechatVerificationProbePort: Send + Sync {
    /// GET `url`，有界超时与响应体上限，返回状态码与响应原文。
    async fn fetch_text(&self, url: &str) -> WechatProbeOutcome;
}

/// 未配置探查端口的兜底实现：自检回答"探测端未配置"而不是误报不可达——
/// 两者给操作员的修复动作完全不同。
pub struct UnconfiguredWechatVerificationProbe;

#[async_trait]
impl WechatVerificationProbePort for UnconfiguredWechatVerificationProbe {
    async fn fetch_text(&self, _url: &str) -> WechatProbeOutcome {
        WechatProbeOutcome {
            status_code: None,
            body: None,
            error: Some("verification probe is not configured for this deployment".to_owned()),
        }
    }
}

fn normalize_file_name(value: &str) -> DeployServiceResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > WECHAT_VERIFICATION_FILE_NAME_MAX {
        return Err(DeployServiceError::validation(
            "fileName must contain 1 to 64 characters",
        ));
    }
    // `.txt` 前必须有以字母数字开头的名称：裸 ".txt" 意味着名为空，微信从
    // 不生成这样的文件，而它能绕过"以字母数字开头"的名称校验。
    let body = value.strip_suffix(".txt").unwrap_or("");
    let valid = !body.is_empty()
        && body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !valid {
        return Err(DeployServiceError::validation(
            "fileName must use ASCII letters, digits, dots, hyphens or underscores and end with .txt",
        ));
    }
    Ok(value.to_owned())
}

fn normalize_content(value: &str) -> DeployServiceResult<String> {
    if value.is_empty() || value.len() > WECHAT_VERIFICATION_CONTENT_MAX {
        return Err(DeployServiceError::validation(
            "content must contain 1 to 2048 bytes",
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

impl crate::DeployService {
    /// 上传（或替换）微信验证文件。
    ///
    /// 替换即整体覆盖：微信重新生成校验文件时，旧的文件名与内容同时作废，
    /// 保留任何一个都会让边缘继续应答一个微信已不再认的记录。
    pub(crate) async fn upsert_wechat_verification(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
        request: &UpsertDomainWechatVerificationRequest,
    ) -> DeployServiceResult<DomainWechatVerificationResponse> {
        let file_name = normalize_file_name(&request.file_name)?;
        let content = normalize_content(&request.content)?;
        let updated_at = self
            .repository
            .upsert_dns_zone_wechat_verification(tenant_id, owner, zone_id, &file_name, &content)
            .await?;
        Ok(DomainWechatVerificationResponse {
            zone_id: zone_id.to_owned(),
            file_name: Some(file_name),
            content: Some(content),
            updated_at: Some(updated_at),
        })
    }

    pub(crate) async fn retrieve_wechat_verification(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
    ) -> DeployServiceResult<DomainWechatVerificationResponse> {
        let state = self
            .repository
            .dns_zone_wechat_verification(tenant_id, owner, zone_id)
            .await?;
        Ok(match state {
            Some((file_name, content, updated_at)) => DomainWechatVerificationResponse {
                zone_id: zone_id.to_owned(),
                file_name: Some(file_name),
                content: Some(content),
                updated_at: Some(updated_at),
            },
            None => DomainWechatVerificationResponse {
                zone_id: zone_id.to_owned(),
                file_name: None,
                content: None,
                updated_at: None,
            },
        })
    }

    pub(crate) async fn delete_wechat_verification(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
    ) -> DeployServiceResult<()> {
        let deleted = self
            .repository
            .delete_dns_zone_wechat_verification(tenant_id, owner, zone_id)
            .await?;
        if !deleted {
            return Err(DeployServiceError::not_found(
                "no WeChat verification file is configured for this zone",
            ));
        }
        Ok(())
    }

    /// 自检：按微信爬虫的视角取回 `https://<apex>/<fileName>`（失败再试
    /// http），与保存内容逐字节比对。
    ///
    /// https 命中即采信——微信公众号要求的安全域名都以 https 为准；https
    /// 不可达时再试 http，把"域名解析了但没配证书"与"根本没解析"区分开。
    /// 内容不匹配时两个方案都试完，操作员拿到的是两种协议各自的事实。
    pub(crate) async fn check_wechat_verification(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
    ) -> DeployServiceResult<DomainWechatVerificationCheckResponse> {
        let (file_name, content, _updated_at) = self
            .repository
            .dns_zone_wechat_verification(tenant_id, owner, zone_id)
            .await?
            .ok_or_else(|| {
                DeployServiceError::not_found(
                    "no WeChat verification file is configured for this zone",
                )
            })?;
        let apex = self
            .repository
            .retrieve_domain_zone(tenant_id, owner, zone_id)
            .await?
            .apex_hostname;
        let checked_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let mut reachable: Option<(i32, &'static str)> = None;
        let mut details: Vec<String> = Vec::new();
        for scheme in ["https", "http"] {
            let url = format!("{scheme}://{apex}/{file_name}");
            let outcome = self.wechat_probe.fetch_text(&url).await;
            match (outcome.status_code, outcome.body) {
                (Some(status), Some(body)) if (200..300).contains(&status) => {
                    if body == content {
                        return Ok(DomainWechatVerificationCheckResponse {
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
        Ok(DomainWechatVerificationCheckResponse {
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
    use super::{normalize_content, normalize_file_name};

    #[test]
    fn file_names_accept_the_wechat_shape_and_refuse_everything_else() {
        assert_eq!(
            normalize_file_name(" MP_verify_aB123.txt ").unwrap(),
            "MP_verify_aB123.txt"
        );
        assert_eq!(normalize_file_name("a.txt").unwrap(), "a.txt");
        for bad in [
            "",
            "no-extension",
            ".txt",
            "spaces in name.txt",
            "中文.txt",
            "/path/MP_verify.txt",
            "a.txt.exe",
        ] {
            assert!(normalize_file_name(bad).is_err(), "{bad} must be refused");
        }
        // 大写与内嵌点都在契约字符集内（schema 的 pattern 同此）。
        assert_eq!(
            normalize_file_name("UPPER.CASE.txt").unwrap(),
            "UPPER.CASE.txt"
        );
        let long = format!("{}.txt", "a".repeat(61));
        assert_eq!(long.len(), 65);
        assert!(normalize_file_name(&long).is_err());
        let at_limit = format!("{}.txt", "a".repeat(60));
        assert_eq!(at_limit.len(), 64);
        assert!(normalize_file_name(&at_limit).is_ok());
    }

    #[test]
    fn content_is_bounded_and_kept_verbatim() {
        assert_eq!(normalize_content("token-abc").unwrap(), "token-abc");
        // 换行被保留：逐字节存储意味着逐字节保留。
        assert_eq!(normalize_content("token\n").unwrap(), "token\n");
        assert!(normalize_content("").is_err());
        assert!(normalize_content(&"x".repeat(2049)).is_err());
        assert!(normalize_content("ok\u{0}bad").is_err());
    }
}

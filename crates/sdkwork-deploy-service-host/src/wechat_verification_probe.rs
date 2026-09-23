//! ⚠️ 未落地：crate 内没有任何 `mod` 声明指向本文件，rustc 不会编译它。
//! 它随 `wip/deploy-certificate` 一并到来，是后续落地的规格——接线方式是补上
//! 模块声明，不是删除本文件。
//!
//! 微信验证文件自检的生产探查端：复用 ACME 服务有界 HTTP 客户端。
//!
//! 探查与微信爬虫共享同一套约束——请求超时、响应体上限——这样"自检通过"
//! 才能代表爬虫也能取回：用不受限的客户端做自检，等于用一个爬虫不具备的
//! 能力替它背书。

use async_trait::async_trait;
use sdkwork_intelligence_deploy_service::{WechatProbeOutcome, WechatVerificationProbePort};
use sdkwork_webserver_acme_service::DnsApiClient;

/// 有界探查端：https 与 http 各持一个客户端，构造期一次定型。
pub struct BoundedWechatVerificationProbe {
    https: DnsApiClient,
    http: DnsApiClient,
}

impl BoundedWechatVerificationProbe {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            https: DnsApiClient::new().map_err(|error| error.to_string())?,
            http: DnsApiClient::new_permitting_plaintext().map_err(|error| error.to_string())?,
        })
    }
}

#[async_trait]
impl WechatVerificationProbePort for BoundedWechatVerificationProbe {
    async fn fetch_text(&self, url: &str) -> WechatProbeOutcome {
        let client = if url.starts_with("https://") {
            &self.https
        } else {
            &self.http
        };
        match client.get_text(url).await {
            Ok((status, body)) => WechatProbeOutcome {
                status_code: Some(i32::from(status)),
                body: Some(body),
                error: None,
            },
            Err(error) => WechatProbeOutcome {
                status_code: None,
                body: None,
                error: Some(error.to_string()),
            },
        }
    }
}

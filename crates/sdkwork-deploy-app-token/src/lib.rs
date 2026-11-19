//! Deploy-side application access token library.
//!
//! One responsibility per layer, so an application can integrate the exchange
//! without importing another module's internals:
//!
//! | Layer | Owns | Does not own |
//! | --- | --- | --- |
//! | [`DeployApplicationLookup`] | resolving a caller-facing selector (slug, uuid, deploy anchor) into a deploy application row of the caller's tenant | token issuance, IAM rows |
//! | [`AppTokenExchange`] | calling IAM `appAccessTokens.create` with the caller's session | which application is entitled to the caller |
//! | [`DeployAppTokenService`] | the composition: resolve, then exchange | storage, transport, signing |
//!
//! IAM stays the single signing authority: the deploy application anchor published
//! here is only an identifier, and the entitlement decision (owner / organization
//! membership / tenant membership) belongs to `sdkwork-iam`
//! (`APP_ACCESS_TOKEN_SPEC.md` section 8). Nothing in this crate signs, caches, or
//! widens a credential.
//!
//! ```no_run
//! use sdkwork_deploy_app_token::{
//!     AppTokenRequest, CallerSession, DeployAppTokenService, DeployApplicationSelector,
//!     HttpAppTokenExchange, PgDeployApplicationLookup,
//! };
//!
//! # async fn example(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let service = DeployAppTokenService::new(
//!     PgDeployApplicationLookup::new(pool),
//!     HttpAppTokenExchange::try_new("https://api.example.com")?,
//! );
//! let session = CallerSession::new("access-token", "auth-token");
//! let issued = service
//!     .issue(
//!         "100001",
//!         &session,
//!         &AppTokenRequest::new(DeployApplicationSelector::slug("sdkwork-cloudrouter-pc")),
//!     )
//!     .await?;
//! println!("{} expires at {}", issued.app_id, issued.expires_at);
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod exchange;
pub mod lookup;
pub mod model;
pub mod service;

pub use error::AppTokenError;
pub use exchange::{AppTokenExchange, HttpAppTokenExchange};
pub use lookup::{DeployApplicationLookup, PgDeployApplicationLookup};
pub use model::{
    AppAccessToken, AppTokenRequest, CallerSession, DeployApplicationRef, DeployApplicationSelector,
};
pub use service::DeployAppTokenService;

/// IAM app-api route that mints the credential (`appAccessTokens.create`).
pub const IAM_APP_ACCESS_TOKENS_PATH: &str = "/app/v3/api/iam/app_access_tokens";

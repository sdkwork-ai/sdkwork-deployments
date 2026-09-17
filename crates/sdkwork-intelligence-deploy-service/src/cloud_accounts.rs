//! The cloud-account surface: what the console picks from, and what a zone or
//! certificate pins.
//!
//! Two rules carry this module.
//!
//! * **A pin is proved at the write that supplied it.** Every path that stores a
//!   `provider_account_id` runs it past the account port first, so a wrong id
//!   fails the request the operator is looking at rather than the first
//!   certificate order, hours later, that needs the credential. A pin that is not
//!   visible to this caller answers "not found" rather than "forbidden", so the
//!   endpoint cannot be used to probe which ids exist.
//! * **A derived pin is never stored.** When the caller pins nothing, nothing is
//!   written: the certificate defers to its zone, the zone defers to the account
//!   center, and failing that to the deployment-level provider configuration — all
//!   decided at issuance time. Writing the account that *would* have been chosen
//!   would freeze a decision made from today's inventory onto a certificate that
//!   outlives it, and the first re-registration of that account would leave the
//!   certificate pointing at nothing.
//!
//! Environment is deliberately not part of either rule. Deploy's environment
//! vocabulary (`dev`, `test`, `staging`, `demo`, `production`) is not the account
//! center's (`development`, `sandbox`, `production`), so any translation would be
//! invented; and a tenant's DNS credential is normally the same one in every
//! environment of that tenant. The account center's own precedence — personal,
//! then tenant, then platform, default first — is the whole selection rule.

use sdkwork_deploy_cloud_account_port::CloudAccount;
use sdkwork_deploy_contract::{
    CloudAccountResponse, DeployAppRequestContext, DeployServiceError, DeployServiceResult,
};

use crate::DeployService;

/// Longest accepted account id. Matches `deploy_dns_zone.provider_account_id` and
/// `deploy_certificate.provider_account_id`, so an id that would not fit the
/// column is refused here rather than by a truncating insert.
pub(crate) const MAXIMUM_PROVIDER_ACCOUNT_ID_LENGTH: usize = 128;

/// Longest accepted account code, matching the account center's own column.
const MAXIMUM_ACCOUNT_CODE_LENGTH: usize = 32;
/// Longest accepted display name, matching the account center's own column.
pub(crate) const MAXIMUM_DISPLAY_NAME_LENGTH: usize = 200;

impl DeployService {
    /// Normalizes and authorises a pin supplied on a create request.
    ///
    /// `None` in, `None` out is the common case and not a gap: an unpinned zone or
    /// certificate resolves its account at issuance time, which is what keeps it
    /// correct after the inventory changes. A supplied id is proved visible and
    /// usable before it is stored.
    pub(crate) async fn pin_provider_account(
        &self,
        context: &DeployAppRequestContext,
        tenant_id: i64,
        requested: Option<&str>,
    ) -> DeployServiceResult<Option<String>> {
        let Some(raw) = requested else {
            return Ok(None);
        };
        let account_id = validate_provider_account_id(raw)?;
        self.cloud_accounts
            .ensure_bindable(tenant_id, context.actor_id, &account_id)
            .await?;
        Ok(Some(account_id))
    }

    /// Reads the update-time tri-state of a pin.
    ///
    /// `Ok(None)` = leave the current pin alone, `Ok(Some(None))` = clear it, and
    /// `Ok(Some(Some(id)))` = re-pin after proving the id. The three states exist
    /// because a certificate's fallback differs between them: clearing a zone's pin
    /// hands the decision back to the account center, which is not what an operator
    /// means by editing the display name.
    pub(crate) async fn resolve_provider_account_update(
        &self,
        context: &DeployAppRequestContext,
        tenant_id: i64,
        requested: Option<&str>,
    ) -> DeployServiceResult<Option<Option<String>>> {
        let Some(raw) = requested else {
            return Ok(None);
        };
        if raw.trim().is_empty() {
            return Ok(Some(None));
        }
        let account_id = validate_provider_account_id(raw)?;
        self.cloud_accounts
            .ensure_bindable(tenant_id, context.actor_id, &account_id)
            .await?;
        Ok(Some(Some(account_id)))
    }
}

/// A pin as the column stores it.
///
/// The shape is checked here as well as by the column's `CHECK`: a value that only
/// the database refuses surfaces as an unavailable store rather than as the
/// operator's typo, and the difference matters when the operator has to fix it.
fn validate_provider_account_id(raw: &str) -> DeployServiceResult<String> {
    let value = raw.trim();
    if value.len() < 2
        || value.len() > MAXIMUM_PROVIDER_ACCOUNT_ID_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-'))
        || !value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(DeployServiceError::validation(format!(
            "providerAccountId must be 2 to {MAXIMUM_PROVIDER_ACCOUNT_ID_LENGTH} characters of \
             letters, digits, '_', '.', ':' or '-', starting with a letter or digit"
        )));
    }
    Ok(value.to_owned())
}

/// Projects an account onto the wire shape.
pub(crate) fn cloud_account_response(account: &CloudAccount) -> CloudAccountResponse {
    CloudAccountResponse {
        id: account.id.clone(),
        account_code: account.account_code.clone(),
        display_name: account.display_name.clone(),
        vendor_code: account.vendor_code.clone(),
        dns_provider: account.dns_provider().map(str::to_owned),
        scope_type: account.scope_type.clone(),
        tenant_global: account.is_tenant_global(),
        owner_user_id: account.owner_user_id.clone(),
        is_default: account.is_default,
        status: account.status.clone(),
        credential_configured: account.credential_configured,
        capability_codes: account.capability_codes.clone(),
        environment: Some(account.environment.clone()).filter(|value| !value.is_empty()),
    }
}

pub(crate) fn required_text(
    value: &str,
    maximum_length: usize,
    field: &str,
) -> DeployServiceResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > maximum_length {
        return Err(DeployServiceError::validation(format!(
            "{field} must contain between 1 and {maximum_length} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

/// The account code to register under.
///
/// Derived from the family when the console sent none, rather than from a counter:
/// a code that already exists is the account center's `Conflict` to report, and
/// papering over it with an invented suffix would create the duplicate account the
/// operator was trying to avoid.
pub(crate) fn account_code(
    requested: &Option<String>,
    family: &str,
) -> DeployServiceResult<String> {
    let Some(value) = requested
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(family.to_ascii_lowercase());
    };
    if value.chars().count() > MAXIMUM_ACCOUNT_CODE_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
    {
        return Err(DeployServiceError::validation(format!(
            "accountCode must be 1 to {MAXIMUM_ACCOUNT_CODE_LENGTH} characters of lowercase \
             letters, digits or '_', starting with a letter"
        )));
    }
    Ok(value.to_owned())
}

//! Resolve a caller-facing selector into a deploy application.
//!
//! The lookup reads deploy-owned columns first (`deploy_app`). For an app
//! installed from a **paid template**, it also reads the recorded order's
//! payment status so a refund/chargeback stops issuing app tokens; the order
//! row itself is owned by the order center and is never written here.

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::error::AppTokenError;
use crate::model::{DeployApplicationRef, DeployApplicationSelector};

/// Resolves a selector inside one tenant.
#[async_trait]
pub trait DeployApplicationLookup: Send + Sync {
    /// Finds the application the selector names in `tenant_id`.
    ///
    /// @param tenant_id - the caller's tenant; a selector can never reach another
    ///   tenant's row.
    /// @param selector - slug, uuid, or deploy anchor.
    /// @returns the resolved application, or `None` when this tenant has no such
    ///   live application.
    /// @throws [`AppTokenError::Storage`] when the inventory cannot be read.
    async fn find(
        &self,
        tenant_id: &str,
        selector: &DeployApplicationSelector,
    ) -> Result<Option<DeployApplicationRef>, AppTokenError>;
}

/// SQLx-backed lookup over `deploy_app`.
#[derive(Clone, Debug)]
pub struct PgDeployApplicationLookup {
    pool: PgPool,
}

impl PgDeployApplicationLookup {
    /// @param pool - deploy PostgreSQL pool.
    /// @returns the lookup.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// `true` when the recorded paid order has been refunded/reversed.
    ///
    /// The order center owns `commerce_order`; a schema without it (a focused
    /// deploy test database) has no refund signal, so issuance stays unchanged.
    async fn order_is_refunded(
        &self,
        tenant_id: &str,
        order_id: &str,
    ) -> Result<bool, AppTokenError> {
        let present: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('commerce_order')::text")
                .fetch_one(&self.pool)
                .await
                .map_err(|error| AppTokenError::Storage(error.to_string()))?;
        if present.is_none() {
            return Ok(false);
        }
        let status: Option<String> = sqlx::query_scalar(
            "SELECT payment_status FROM commerce_order \
             WHERE tenant_id = $1 AND id = $2 LIMIT 1",
        )
        .bind(tenant_id)
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| AppTokenError::Storage(error.to_string()))?;
        Ok(status
            .as_deref()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "refunded" | "refund" | "chargeback" | "charged_back" | "reversed"
                )
            })
            .unwrap_or(false))
    }
}

/// Which column a selector addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectorColumn {
    Slug,
    Uuid,
    DeployApplicationId,
}

impl DeployApplicationSelector {
    fn column(&self) -> SelectorColumn {
        match self {
            Self::Slug(_) => SelectorColumn::Slug,
            Self::Uuid(_) => SelectorColumn::Uuid,
            Self::DeployApplicationId(_) => SelectorColumn::DeployApplicationId,
        }
    }
}

#[async_trait]
impl DeployApplicationLookup for PgDeployApplicationLookup {
    async fn find(
        &self,
        tenant_id: &str,
        selector: &DeployApplicationSelector,
    ) -> Result<Option<DeployApplicationRef>, AppTokenError> {
        // Archived and soft-deleted rows never issue: an archived application is a
        // retired deployment, and a deleted row is not addressable.
        //
        // One static statement with three NULL-guarded predicates: exactly one
        // parameter is bound per selector variant, so no caller input reaches the
        // SQL text and the three identity columns can never be confused.
        let (slug, uuid, deploy_application_id) = match selector.column() {
            SelectorColumn::Slug => (Some(selector.value()), None, None),
            SelectorColumn::Uuid => (None, Some(selector.value()), None),
            SelectorColumn::DeployApplicationId => (None, None, Some(selector.value())),
        };
        let row = sqlx::query(
            "SELECT a.uuid, a.id::text AS deploy_application_id, a.tenant_id::text AS tenant_id, a.slug, \
                    a.app_kind, a.owner_type, a.user_id, \
                    a.metadata #>> '{template,orderId}' AS template_order_id \
             FROM deploy_app a \
             WHERE a.tenant_id::text = $1 \
               AND a.deleted_at IS NULL AND a.archived_at IS NULL \
               AND (($2::text IS NOT NULL AND a.slug = $2) \
                 OR ($3::text IS NOT NULL AND a.uuid = $3) \
                 OR ($4::text IS NOT NULL AND a.id::text = $4)) \
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(slug)
        .bind(uuid)
        .bind(deploy_application_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| AppTokenError::Storage(error.to_string()))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let order_id: Option<String> = row
            .try_get("template_order_id")
            .map_err(|error| AppTokenError::Storage(error.to_string()))?;
        if let Some(order_id) = order_id.filter(|value| !value.trim().is_empty()) {
            if self.order_is_refunded(tenant_id, order_id.as_str()).await? {
                return Ok(None);
            }
        }
        Ok(Some(DeployApplicationRef {
            uuid: row
                .try_get::<String, _>("uuid")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            deploy_application_id: row
                .try_get::<String, _>("deploy_application_id")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            tenant_id: row
                .try_get::<String, _>("tenant_id")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            slug: row
                .try_get::<String, _>("slug")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            app_kind: row
                .try_get::<String, _>("app_kind")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            owner_type: row
                .try_get::<String, _>("owner_type")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?,
            owner_user_id: row
                .try_get::<Option<i64>, _>("user_id")
                .map_err(|error| AppTokenError::Storage(error.to_string()))?
                .map(|value| value.to_string()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_column_never_points_at_an_unlisted_column() {
        assert_eq!(
            DeployApplicationSelector::slug("s").column(),
            SelectorColumn::Slug
        );
        assert_eq!(
            DeployApplicationSelector::uuid("u").column(),
            SelectorColumn::Uuid
        );
        assert_eq!(
            DeployApplicationSelector::deploy_application_id("1").column(),
            SelectorColumn::DeployApplicationId
        );
    }
}

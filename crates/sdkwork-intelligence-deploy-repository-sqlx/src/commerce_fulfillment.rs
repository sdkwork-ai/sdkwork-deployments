//! Commerce fulfillment store for paid app-template purchases.
//!
//! This is the module-owned half of the order-system integration, mirroring
//! `sdkwork-membership`'s `PostgresCommerceMembershipStore`: the ORDER service
//! (sdkwork-order) owns checkout, payment (sdkwork-payment), and the
//! pending→paid state machine. When a paid template order settles, the order
//! service invokes its fulfillment port, and the integration adapter
//! (`sdkwork-order-integration-deploy`, hosted by the order repository) calls
//! into THIS store to grant the entitlement in the deployments module's own
//! tables. Cross-module integrity is by reference (`order_id`/`order_no`),
//! never by foreign key — another module's table lifecycle must not constrain
//! this baseline.
//!
//! Idempotency: fulfillment replays (payment webhooks retry) resolve to the
//! already-granted row and report `replayed` instead of double-granting.

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::DeployServiceError;
use sqlx::{PgPool, Row};

use crate::support::{new_uuid, next_id, store_error};

/// Command carried by the order service's fulfillment port when a paid
/// template purchase settles. Mirrors `FulfillPaidMembershipPurchaseCommand`.
#[derive(Clone, Debug)]
pub struct FulfillPaidTemplatePurchaseCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    /// commerce order reference (sdkwork-order TEXT ids).
    pub order_id: String,
    pub order_no: String,
    pub request_no: String,
    /// Idempotency key derived by the order service (per order).
    pub idempotency_key: String,
    /// The template being purchased (checkout line business reference).
    pub template_uuid: String,
}

#[derive(Clone, Debug)]
pub struct FulfillPaidTemplatePurchaseOutcome {
    /// The granted (or replayed) purchase uuid.
    pub purchase_uuid: String,
    /// True when this call replayed an already-granted fulfillment.
    pub replayed: bool,
}

/// Module-owned entitlement writer used by the order system's fulfillment
/// adapter. FREE templates never travel through here — they are granted
/// directly by the deploy acquire API without an order.
pub struct PostgresCommerceTemplatePurchaseStore {
    pool: PgPool,
    id_generator: SnowflakeIdGenerator,
}

impl PostgresCommerceTemplatePurchaseStore {
    pub fn new(pool: PgPool, id_generator: SnowflakeIdGenerator) -> Self {
        Self { pool, id_generator }
    }

    /// Grants the paid-template entitlement for a settled order. Idempotent:
    /// a replayed order id, or an existing live entitlement for the same
    /// (template, buyer), resolves to the already-granted row.
    pub async fn fulfill_paid_template_purchase(
        &self,
        command: &FulfillPaidTemplatePurchaseCommand,
    ) -> Result<FulfillPaidTemplatePurchaseOutcome, DeployServiceError> {
        let tenant_id: i64 = command.tenant_id.parse().map_err(|_| {
            DeployServiceError::validation("fulfillment tenant id is not an integer")
        })?;
        let buyer_user_id: i64 = command.owner_user_id.parse().map_err(|_| {
            DeployServiceError::validation("fulfillment buyer id is not an integer")
        })?;
        let organization_id: Option<i64> = match command.organization_id.as_deref() {
            None | Some("") | Some("0") => None,
            Some(raw) => Some(raw.parse().map_err(|_| {
                DeployServiceError::validation("fulfillment organization id is not an integer")
            })?),
        };

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin fulfill paid template purchase", error))?;

        // Order replay: this order already granted its entitlement.
        let replayed_by_order: Option<String> = sqlx::query_scalar(
            "SELECT uuid FROM deploy_app_template_purchase
             WHERE order_id = $1 AND deleted_at IS NULL",
        )
        .bind(&command.order_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("replay fulfill by order", error))?;
        if let Some(purchase_uuid) = replayed_by_order {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback order replay", error))?;
            return Ok(FulfillPaidTemplatePurchaseOutcome {
                purchase_uuid,
                replayed: true,
            });
        }

        // The template must exist, be tenant-scoped to the order's tenant, and
        // be live; a paid order for an unpublished/foreign template is a
        // commerce data error and fails the fulfillment loudly.
        let row = sqlx::query(
            "SELECT id, organization_id, pricing_model, price_minor, currency FROM deploy_app_template
             WHERE tenant_id = $1 AND uuid = $2 AND status = 'PUBLISHED' AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(&command.template_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_template for fulfillment", error))?;
        let row = row.ok_or_else(|| {
            DeployServiceError::not_found("purchased template not found or not published")
        })?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let template_organization_id: i64 = row
            .try_get("organization_id")
            .map_err(|error| DeployServiceError::Internal(format!("read organization: {error}")))?;
        let pricing_model: String = row
            .try_get("pricing_model")
            .map_err(|error| DeployServiceError::Internal(format!("read pricing: {error}")))?;
        let price_minor: i64 = row
            .try_get("price_minor")
            .map_err(|error| DeployServiceError::Internal(format!("read price: {error}")))?;
        let currency: String = row
            .try_get("currency")
            .map_err(|error| DeployServiceError::Internal(format!("read currency: {error}")))?;
        if pricing_model != "PAID" {
            return Err(DeployServiceError::conflict(
                "fulfillment targets a template that is not a paid listing",
            ));
        }

        // Entitlement replay: the buyer already holds this template (for
        // example granted through an earlier order). The new order is marked
        // as consumed by recording nothing — commerce stays authoritative for
        // the order; we report the existing entitlement instead of a double
        // grant (the partial unique index would reject it anyway).
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT uuid FROM deploy_app_template_purchase
             WHERE template_id = $1 AND buyer_user_id = $2 AND status = 'ACTIVE'
             AND deleted_at IS NULL",
        )
        .bind(template_id)
        .bind(buyer_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("find existing entitlement", error))?;
        if let Some(purchase_uuid) = existing {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback entitlement replay", error))?;
            return Ok(FulfillPaidTemplatePurchaseOutcome {
                purchase_uuid,
                replayed: true,
            });
        }

        // The purchased version is the template's newest published version at
        // fulfillment time (the listing may have moved on since checkout).
        let version_uuid: String = sqlx::query_scalar(
            "SELECT uuid FROM deploy_app_template_version
             WHERE template_id = $1 AND status = $2 AND deleted_at IS NULL
             ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(template_id)
        .bind("PUBLISHED")
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("resolve fulfilled template version", error))?
        .ok_or_else(|| {
            DeployServiceError::conflict("purchased template has no published version to grant")
        })?;

        let purchase_id = next_id(&self.id_generator)?;
        let purchase_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_app_template_purchase (
                id, uuid, tenant_id, organization_id, template_id, version_uuid, buyer_user_id,
                pricing_model, price_minor, currency, order_id, order_no, request_no, status,
                idempotency_key, created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, $9, $10, $11, $12, $13, 'ACTIVE',
                $14, $15, $15)",
        )
        .bind(purchase_id)
        .bind(&purchase_uuid)
        .bind(tenant_id)
        .bind(organization_id.or(Some(template_organization_id)))
        .bind(template_id)
        .bind(&version_uuid)
        .bind(buyer_user_id)
        .bind(&pricing_model)
        .bind(price_minor)
        .bind(&currency)
        .bind(&command.order_id)
        .bind(&command.order_no)
        .bind(&command.request_no)
        .bind(&command.idempotency_key)
        .bind(buyer_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert fulfilled deploy_app_template_purchase", error))?;
        sqlx::query(
            "UPDATE deploy_app_template SET install_count = install_count + 1 WHERE id = $1",
        )
        .bind(template_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("count install", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit fulfill paid template purchase", error))?;
        Ok(FulfillPaidTemplatePurchaseOutcome {
            purchase_uuid,
            replayed: false,
        })
    }
}

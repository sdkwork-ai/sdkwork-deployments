//! Archive applications whose paid template order was refunded or reversed.
//!
//! The worker is deliberately small and idempotent: it selects
//! `deploy_app.metadata.template.orderId` joined to a refunded `commerce_order`
//! and archives each candidate (`app_status = ARCHIVED`, `archived_at` set).
//! `apps.archived_at IS NULL` keeps a second pass from rewriting the same row.
//!
//! It never deletes data. App-token issuance already refuses a refunded order;
//! this worker performs the catalog cleanup that the token check cannot.

use sqlx::{PgPool, Row};
use tracing::{info, warn};

/// One pass of the revocation sweep.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefundRevocationReport {
    /// Application UUIDs that qualify (archived only when `applied`).
    pub candidates: Vec<String>,
    /// Rows actually archived in this pass.
    pub archived: usize,
}

/// Selects refunded-order applications and archives them when `apply`.
pub async fn revoke_once(
    pool: &PgPool,
    apply: bool,
) -> Result<RefundRevocationReport, sqlx::Error> {
    let present: Option<String> = sqlx::query_scalar("SELECT to_regclass('commerce_order')::text")
        .fetch_one(pool)
        .await?;
    if present.is_none() {
        warn!("commerce_order is absent; refund revocation skipped");
        return Ok(RefundRevocationReport::default());
    }

    let rows = sqlx::query(
        "SELECT a.tenant_id::text AS tenant_id, a.uuid
         FROM deploy_app a
         JOIN commerce_order o
           ON o.tenant_id = a.tenant_id::text
          AND o.id = a.metadata #>> '{template,orderId}'
         WHERE a.deleted_at IS NULL
           AND a.archived_at IS NULL
           AND LOWER(COALESCE(o.payment_status, '')) IN
               ('refunded', 'refund', 'chargeback', 'charged_back', 'reversed')
         ORDER BY a.tenant_id, a.id",
    )
    .fetch_all(pool)
    .await?;

    let candidates: Vec<(String, String)> = rows
        .into_iter()
        .map(|row| (row.get("tenant_id"), row.get("uuid")))
        .collect();
    let mut report = RefundRevocationReport {
        candidates: candidates.iter().map(|(_, uuid)| uuid.clone()).collect(),
        archived: 0,
    };
    if !apply {
        info!(
            candidates = report.candidates.len(),
            "refund revocation dry-run"
        );
        return Ok(report);
    }

    for (tenant_id, app_uuid) in candidates {
        let archived = sqlx::query(
            "UPDATE deploy_app
             SET app_status = 'ARCHIVED', archived_at = NOW(), updated_at = NOW(),
                 version = version + 1
             WHERE tenant_id = CAST($1 AS BIGINT) AND uuid = $2
               AND deleted_at IS NULL AND archived_at IS NULL",
        )
        .bind(&tenant_id)
        .bind(&app_uuid)
        .execute(pool)
        .await?
        .rows_affected();
        report.archived += archived as usize;
    }
    info!(archived = report.archived, "refund revocation applied");
    Ok(report)
}

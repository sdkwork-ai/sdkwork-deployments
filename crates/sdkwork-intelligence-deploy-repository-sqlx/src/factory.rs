//! The application-factory read model and uninstall bookkeeping.
//!
//! Two operations live here because they both read or write the factory's
//! cross-table view of one deploy application:
//!
//! - [`DeployRepository::derive_factory_readiness_repo`] loads the facts the
//!   readiness rule needs and hands them to
//!   `sdkwork_deploy_core::derive_factory_readiness`. Nothing is stored: a
//!   retired artifact or a disabled installation changes the answer on the next
//!   read (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3).
//! - [`DeployRepository::mark_uninstall_stage_repo`] and
//!   [`DeployRepository::archive_app_repo`] are the durable half of the uninstall
//!   sequence (`deploy_app.uninstall_status`). They write one stage marker per
//!   step so an interrupted uninstall can be replayed from the stored state
//!   instead of starting over.

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sqlx::Row;

use crate::support::store_error;
use crate::DeployRepository;

/// The stored uninstall stage vocabulary, mirroring
/// `chk_deploy_app_uninstall_status`.
pub const UNINSTALL_NOT_STARTED: &str = "not_started";
pub const UNINSTALL_PENDING_IDENTITY_DISABLE: &str = "pending_identity_disable";
pub const UNINSTALL_IDENTITY_DISABLED: &str = "identity_disabled";
pub const UNINSTALL_COMPLETE: &str = "complete";

impl DeployRepository {
    /// Loads the facts the readiness derivation needs for one application.
    ///
    /// The source facts are read through the same mapper as `apps.source.retrieve`
    /// so the readiness report and the source endpoint can never disagree about
    /// what "ready" means.
    pub(super) async fn factory_readiness_facts_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<sdkwork_deploy_core::FactoryReadinessFacts> {
        let app_row = sqlx::query(
            "SELECT app_status, identity_status, platform_target_count
             FROM deploy_app
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_app readiness facts", error))?;
        let Some(app_row) = app_row else {
            return Err(DeployServiceError::not_found("app not found"));
        };

        let environment_rows = sqlx::query(
            "SELECT env_status FROM deploy_app_environment
             WHERE tenant_id = $1 AND app_id = (SELECT id FROM deploy_app WHERE uuid = $2)
               AND deleted_at IS NULL
             ORDER BY id",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_app_environment readiness facts", error))?;
        let environment_statuses = environment_rows
            .iter()
            .map(|row| row.try_get::<String, _>("env_status").unwrap_or_default())
            .collect::<Vec<_>>();

        // The source is optional: an application that offers no remix source has
        // no row at all, and `apps.source.retrieve` reports that as
        // `SOURCE_NOT_CONFIGURED` rather than 404.
        let source = self.retrieve_app_source_repo(tenant_id, app_id).await.ok();
        let (remix_enabled, source_readiness, source_reason) = match source {
            Some(source) => (
                source.remix_enabled,
                format!("{:?}", source.readiness).to_ascii_uppercase(),
                source.readiness_reason.clone(),
            ),
            None => (false, "MISSING".to_owned(), None),
        };
        let domain_plan_published: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM deploy_app_binding d
                 JOIN deploy_app a ON a.id = d.app_id
                 WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.deleted_at IS NULL
                   AND d.deleted_at IS NULL
             )",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_app_domain readiness fact", error))?
        .unwrap_or(false);

        Ok(sdkwork_deploy_core::FactoryReadinessFacts {
            app_status: app_row.try_get("app_status").unwrap_or_default(),
            platform_target_count: app_row.try_get("platform_target_count").unwrap_or(0),
            identity_status: app_row.try_get("identity_status").unwrap_or_default(),
            remix_enabled,
            source_readiness,
            source_reason,
            environment_statuses,
            domain_plan_published,
        })
    }

    /// Writes one durable uninstall stage marker.
    ///
    /// The update is conditional on the row still being uninstalled-or-newer so
    /// a completed uninstall never regresses to `pending_identity_disable` when an
    /// old retry arrives after the sequence finished.
    pub(super) async fn mark_uninstall_stage_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        stage: &str,
    ) -> DeployServiceResult<()> {
        sqlx::query(
            "UPDATE deploy_app SET uninstall_status = $3, updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
               AND (uninstall_status <> 'complete' OR $3 = 'complete')",
        )
        .bind(tenant_id)
        .bind(app_id)
        .bind(stage)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("mark deploy_app uninstall stage", error))?;
        Ok(())
    }

    /// Archives the application after its identity was disabled: disables every
    /// live environment row, sets `app_status = 'ARCHIVED'`, stamps
    /// `archived_at`, and completes the uninstall marker, in one transaction.
    ///
    /// Forward-only: no row is deleted, and a replayed call on an already
    /// archived application is a no-op that returns success.
    pub(super) async fn archive_app_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<()> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin app archive transaction", error))?;
        let app_row = sqlx::query(
            "SELECT id, app_status FROM deploy_app
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app for archive", error))?;
        let Some(app_row) = app_row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        let app_internal_id: i64 = app_row.try_get("id").unwrap_or_default();
        let current_status: String = app_row.try_get("app_status").unwrap_or_default();
        if !current_status.eq_ignore_ascii_case("ARCHIVED") {
            sqlx::query(
                "UPDATE deploy_app SET app_status = 'ARCHIVED', archived_at = NOW(),
                    uninstall_status = $2, updated_at = NOW(), version = version + 1
                 WHERE id = $1",
            )
            .bind(app_internal_id)
            .bind(UNINSTALL_COMPLETE)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("archive deploy_app", error))?;
        } else {
            sqlx::query(
                "UPDATE deploy_app SET uninstall_status = $2, updated_at = NOW(),
                    version = version + 1
                 WHERE id = $1",
            )
            .bind(app_internal_id)
            .bind(UNINSTALL_COMPLETE)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("complete deploy_app uninstall", error))?;
        }
        // The environments stop accepting a release or a token before the
        // application is reported archived; the rows stay readable so the
        // operator keeps the history section 3 stage 6 requires.
        sqlx::query(
            "UPDATE deploy_app_environment SET env_status = 'ARCHIVED', updated_at = NOW(),
                version = version + 1
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
               AND env_status <> 'ARCHIVED'",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("archive deploy_app_environment rows", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit app archive transaction", error))?;
        Ok(())
    }

    /// Reads the stored uninstall stage and current app status for the response.
    pub(super) async fn retrieve_uninstall_state_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<(String, String)> {
        let row = sqlx::query(
            "SELECT uninstall_status, app_status FROM deploy_app
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_app uninstall state", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        Ok((
            row.try_get("uninstall_status").unwrap_or_default(),
            row.try_get("app_status").unwrap_or_default(),
        ))
    }
}

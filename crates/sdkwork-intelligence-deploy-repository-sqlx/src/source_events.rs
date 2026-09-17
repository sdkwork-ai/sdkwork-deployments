//! CI source event repository operations: webhook push events deduplicated
//! per (repository, commit) and matched to the bound source repository, plus
//! the management list surface (P0 product gap).

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, SourceEventPage, SourceEventResponse,
    SOURCE_EVENT_STATUS_PROCESSED, SOURCE_EVENT_STATUS_SKIPPED,
};
use sqlx::{AssertSqlSafe, Row};

use crate::support::{
    new_uuid, next_id, optional_datetime, pagination, required_datetime, store_error,
};
use sdkwork_intelligence_deploy_service::repository::TriggerTarget;

use crate::DeployRepository;

/// A matched source repository for an ingested webhook event.
pub(super) struct MatchedRepository {
    pub tenant_id: i64,
    pub app_id: String,
    pub repository_id: String,
    pub repository_internal_id: i64,
    pub app_internal_id: i64,
    pub default_branch: String,
}

impl DeployRepository {
    /// Matches a webhook payload repository reference (clone URL or html URL)
    /// against the bound source repositories. Exact normalized URL match only;
    /// no wildcards, no secret material in the comparison.
    pub(super) async fn match_repository_by_url_repo(
        &self,
        clone_url: &str,
    ) -> DeployServiceResult<Option<MatchedRepository>> {
        if clone_url.trim().is_empty() {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT r.tenant_id, a.uuid AS app_uuid, r.uuid AS repo_uuid, r.id AS repo_id,
                    a.id AS app_id, r.default_branch
             FROM deploy_source_repository r
             JOIN deploy_app a ON a.id = r.app_id
             WHERE r.deleted_at IS NULL AND r.repo_status <> 'ARCHIVED'
               AND lower(regexp_replace(regexp_replace(rtrim(r.repo_url, '/'), '\\.git$', ''), '^https?://', '')) =
                   lower(regexp_replace(regexp_replace(rtrim($1, '/'), '\\.git$', ''), '^https?://', ''))
             ORDER BY r.updated_at DESC LIMIT 1",
        )
        .bind(clone_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("match source repository by url", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(MatchedRepository {
            tenant_id: row.try_get("tenant_id").unwrap_or(0),
            app_id: row.try_get("app_uuid").unwrap_or_default(),
            repository_id: row.try_get("repo_uuid").unwrap_or_default(),
            repository_internal_id: row.try_get("repo_id").unwrap_or(0),
            app_internal_id: row.try_get("app_id").unwrap_or(0),
            default_branch: row.try_get("default_branch").unwrap_or_default(),
        }))
    }

    /// Records a push event, deduplicated per (repository, commit). Returns
    /// the event and whether it was newly inserted; a redelivered webhook
    /// yields the existing event with `fresh = false`.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn ingest_source_event_repo(
        &self,
        matched: &MatchedRepository,
        event_kind: &str,
        source_ref: &str,
        source_commit: &str,
        commit_message: Option<&str>,
        sender_ref: Option<&str>,
        payload_sha256: &str,
    ) -> DeployServiceResult<(SourceEventResponse, bool)> {
        let event_id = next_id(self.id_generator())?;
        let event_uuid = new_uuid();
        let now = crate::support::now_rfc3339();
        let inserted = sqlx::query(
            "INSERT INTO deploy_source_event
                (id, uuid, tenant_id, organization_id, app_id, source_repository_id,
                 event_kind, source_ref, source_commit, commit_message, sender_ref,
                 payload_sha256, event_status, builds_triggered, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'PENDING', 0,
                     CAST($13 AS TIMESTAMPTZ))
             ON CONFLICT (source_repository_id, source_commit) DO NOTHING",
        )
        .bind(event_id)
        .bind(&event_uuid)
        .bind(matched.tenant_id)
        .bind(0_i64)
        .bind(matched.app_internal_id)
        .bind(matched.repository_internal_id)
        .bind(event_kind)
        .bind(source_ref)
        .bind(source_commit)
        .bind(commit_message)
        .bind(sender_ref)
        .bind(payload_sha256)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_source_event", error))?;
        if inserted.rows_affected() == 0 {
            let existing = sqlx::query(
                "SELECT e.uuid, e.tenant_id, a.uuid AS app_uuid, r.uuid AS repo_uuid,
                        e.event_kind, e.source_ref, e.source_commit, e.commit_message,
                        e.payload_sha256, e.event_status, e.builds_triggered, e.error_code,
                        e.processed_at, e.created_at
                 FROM deploy_source_event e
                 JOIN deploy_app a ON a.id = e.app_id
                 JOIN deploy_source_repository r ON r.id = e.source_repository_id
                 WHERE e.source_repository_id = $1 AND e.source_commit = $2",
            )
            .bind(matched.repository_internal_id)
            .bind(source_commit)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("find existing source event", error))?;
            let Some(row) = existing else {
                return Err(DeployServiceError::Internal(
                    "source event disappeared after concurrent insert".into(),
                ));
            };
            return Ok((map_source_event_row(&row)?, false));
        }
        self.retrieve_source_event_internal_repo(matched.tenant_id, &event_uuid)
            .await
            .map(|event| (event, true))
    }

    /// Marks a processed/skipped event with the triggered build count.
    pub(super) async fn update_source_event_result_repo(
        &self,
        tenant_id: i64,
        event_id: &str,
        processed: bool,
        builds_triggered: i32,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()> {
        let status = if processed {
            SOURCE_EVENT_STATUS_PROCESSED
        } else {
            SOURCE_EVENT_STATUS_SKIPPED
        };
        sqlx::query(
            "UPDATE deploy_source_event
                SET event_status = $1, builds_triggered = $2, error_code = $3,
                    processed_at = NOW(), version = version + 1
             WHERE tenant_id = $4 AND uuid = $5 AND event_status = 'PENDING'",
        )
        .bind(status)
        .bind(builds_triggered)
        .bind(error_code)
        .bind(tenant_id)
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_source_event result", error))?;
        Ok(())
    }

    pub(super) async fn list_source_events_repo(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SourceEventPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        // See `list_entitlement_projections_repo`: the tenant predicate and the
        // page window must not share a placeholder number, or `LIMIT $1` reads
        // the tenant id and `OFFSET $2` reads the page size.
        let (filter, paging, bind) = match tenant_id {
            Some(_tenant_id) => ("WHERE e.tenant_id = $1", "LIMIT $2 OFFSET $3", true),
            None => ("", "LIMIT $1 OFFSET $2", false),
        };
        let count_query = format!("SELECT COUNT(*) AS total FROM deploy_source_event e {filter}");
        let mut count = sqlx::query(AssertSqlSafe(&*count_query));
        if bind {
            count = count.bind(tenant_id.unwrap_or(0));
        }
        let count_row = count
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count source events", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let list_query = format!(
            "SELECT e.uuid, e.tenant_id, a.uuid AS app_uuid, r.uuid AS repo_uuid,
                    e.event_kind, e.source_ref, e.source_commit, e.commit_message,
                    e.payload_sha256, e.event_status, e.builds_triggered, e.error_code,
                    e.processed_at, e.created_at
             FROM deploy_source_event e
             JOIN deploy_app a ON a.id = e.app_id
             JOIN deploy_source_repository r ON r.id = e.source_repository_id
             {filter}
             ORDER BY e.created_at DESC, e.id DESC {paging}"
        );
        let mut list = sqlx::query(AssertSqlSafe(&*list_query));
        if bind {
            list = list
                .bind(tenant_id.unwrap_or(0))
                .bind(page_size)
                .bind(offset);
        } else {
            list = list.bind(page_size).bind(offset);
        }
        let rows = list
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list source events", error))?;
        let items = rows
            .iter()
            .map(map_source_event_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SourceEventPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn retrieve_source_event_internal_repo(
        &self,
        tenant_id: i64,
        event_id: &str,
    ) -> DeployServiceResult<SourceEventResponse> {
        let row = sqlx::query(
            "SELECT e.uuid, e.tenant_id, a.uuid AS app_uuid, r.uuid AS repo_uuid,
                    e.event_kind, e.source_ref, e.source_commit, e.commit_message,
                    e.payload_sha256, e.event_status, e.builds_triggered, e.error_code,
                    e.processed_at, e.created_at
             FROM deploy_source_event e
             JOIN deploy_app a ON a.id = e.app_id
             JOIN deploy_source_repository r ON r.id = e.source_repository_id
             WHERE e.tenant_id = $1 AND e.uuid = $2",
        )
        .bind(tenant_id)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve source event", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("source event not found"));
        };
        map_source_event_row(&row)
    }
}

fn map_source_event_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SourceEventResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let processed_at = optional_datetime(row, "processed_at")?;
    Ok(SourceEventResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        source_repository_id: row.try_get("repo_uuid").unwrap_or_default(),
        event_kind: row.try_get("event_kind").unwrap_or_default(),
        source_ref: row.try_get("source_ref").unwrap_or_default(),
        source_commit: row.try_get("source_commit").unwrap_or_default(),
        commit_message: row.try_get("commit_message").ok(),
        payload_sha256: row.try_get("payload_sha256").unwrap_or_default(),
        event_status: row.try_get("event_status").unwrap_or_default(),
        builds_triggered: row.try_get("builds_triggered").unwrap_or(0),
        error_code: row.try_get("error_code").ok(),
        processed_at,
        created_at,
    })
}

impl DeployRepository {
    /// Active build trigger candidates for an app: platform targets with an
    /// ACTIVE status and a governed build template.
    pub(super) async fn list_trigger_targets_repo(
        &self,
        app_id: &str,
    ) -> DeployServiceResult<Vec<TriggerTarget>> {
        let rows = sqlx::query(
            "SELECT t.uuid AS target_uuid, bt.uuid AS template_uuid
             FROM deploy_app_platform_target t
             JOIN deploy_app a ON a.id = t.app_id
             JOIN deploy_build_template bt ON bt.id = t.build_template_id
             WHERE a.uuid = $1 AND t.deleted_at IS NULL
               AND t.target_status = 'ACTIVE' AND bt.template_status = 'ACTIVE'",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list trigger targets", error))?;
        Ok(rows
            .iter()
            .map(|row| TriggerTarget {
                platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
                template_id: row.try_get("template_uuid").unwrap_or_default(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    /// Normalizes a repository URL for matching: strip trailing `.git` and
    /// the scheme, drop userinfo credentials.
    fn normalize_repository_url(url: &str) -> String {
        let mut normalized = url.trim().to_owned();
        if let Some(rest) = normalized.strip_prefix("https://") {
            normalized = rest.to_owned();
        } else if let Some(rest) = normalized.strip_prefix("http://") {
            normalized = rest.to_owned();
        }
        if let Some(at) = normalized.find('@') {
            if let Some(slash) = normalized[at..].find('/') {
                normalized = normalized[slash + at..].to_owned();
            }
        }
        if let Some(stripped) = normalized.strip_suffix(".git") {
            normalized = stripped.to_owned();
        }
        normalized.trim_end_matches('/').to_owned()
    }

    #[test]
    fn repository_url_normalization_is_canonical() {
        assert_eq!(
            normalize_repository_url("https://github.com/sdkwork/deployments.git"),
            "github.com/sdkwork/deployments"
        );
        assert_eq!(
            normalize_repository_url("https://github.com/sdkwork/deployments/"),
            "github.com/sdkwork/deployments"
        );
        assert_eq!(
            normalize_repository_url("http://gitlab.example.com/team/app"),
            "gitlab.example.com/team/app"
        );
        assert_eq!(
            normalize_repository_url("  https://github.com/a/b.git  "),
            "github.com/a/b"
        );
    }
}

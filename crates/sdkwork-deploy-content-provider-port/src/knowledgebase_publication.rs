//! In-process read of an active Knowledgebase wiki publication.
//!
//! The knowledgebase internal API's `wiki_publications/{publicationUuid}` route
//! delegates to `KnowledgeWikiPublicProviderService::retrieve_publication`, whose
//! only stateful step is `WikiPublicProviderStore::get_active_publication_by_uuid`
//! (`sdkwork-intelligence-knowledgebase-repository-sqlx/src/wiki_persistence/public_provider.rs`).
//!
//! That store is implemented over a `sqlx::AnyPool`, which this host does not
//! own: the deploy process has one PostgreSQL pool and MODULE_SPEC.md §4.2.7
//! forbids opening a second pool for one call. This module therefore performs
//! the same read on the host pool, with the identical predicate, and maps it
//! into a two-field projection. When the knowledgebase module exposes a
//! `PgPool`-backed publication port, this query is deleted and replaced by a
//! call to it; until then the predicate below is the single source of drift.

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sqlx::PgPool;

/// The projection deploy needs from an active wiki publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnowledgebaseWikiPublication {
    pub publication_uuid: String,
    pub search_enabled: bool,
}

/// Reads the tenant's active wiki publication by UUID.
///
/// Predicate copied from `WikiPublicProviderStore::get_active_publication_by_uuid`:
/// `publication_type = 'wiki'`, `wiki_status = 'ACTIVE'`, a resolved source scope,
/// `status = 1`, scoped to `(tenant_id, organization_id)`.
pub(crate) async fn find_active_wiki_publication(
    pool: &PgPool,
    tenant_id: i64,
    organization_id: i64,
    publication_uuid: &str,
) -> DeployServiceResult<Option<KnowledgebaseWikiPublication>> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT uuid, search_enabled
         FROM kb_site_publication
         WHERE tenant_id = $1
           AND organization_id = $2
           AND uuid = $3
           AND publication_type = 'wiki'
           AND wiki_status = 'ACTIVE'
           AND source_scope_uuid IS NOT NULL
           AND status = 1
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(publication_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|_| DeployServiceError::Internal("Knowledgebase publication lookup failed".into()))?;
    Ok(row.map(
        |(publication_uuid, search_enabled)| KnowledgebaseWikiPublication {
            publication_uuid,
            search_enabled,
        },
    ))
}

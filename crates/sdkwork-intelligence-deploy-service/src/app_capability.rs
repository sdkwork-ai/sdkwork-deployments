//! Application **capability**: the six-dimension configuration document and its
//! review lifecycle.
//!
//! The domain is documented in
//! `docs/architecture/tech/TECH-application-capability-model.md`; this module is
//! the service half:
//!
//! * **Reach** — every route is addressed by `appId`, so every operation proves
//!   the credential's application scope and then the owner gate before touching a
//!   row (`APPLICATION_IDENTITY_SPEC.md` §4.1). The database only knows
//!   `tenant_id`.
//! * **Document validation** happens in
//!   [`sdkwork_deploy_contract::ReplaceAppCapabilityRequest::normalize`], once, so
//!   the repository never re-decides what a valid document is.
//! * **Review state machine** lives here: only `draft`/`published` may be
//!   submitted, only a `pending` review may be decided, and approving publishes
//!   the revision the reviewer actually read.
//! * **Snapshot identity** — the canonical JSON of the document and its SHA-256
//!   are produced before the write, so the revision row and the review row name
//!   exactly the bytes that were approved.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    AppCapabilityResponse, CapabilityReviewDecision, CapabilityReviewPage,
    CapabilityReviewResponse, CapabilityRevisionPage, CapabilityRevisionResponse, CapabilityStatus,
    DecideCapabilityReviewRequest, DeployAppRequestContext, DeployServiceError,
    DeployServiceResult, ListCapabilityReviewsQuery, ListCapabilityRevisionsQuery,
    NormalizedAppCapability, ReplaceAppCapabilityRequest, SubmitCapabilityReviewRequest,
};

/// Permission that authorizes deciding a capability review.
pub const DEPLOY_APP_CAPABILITY_APPROVE_PERMISSION: &str = "deploy.appCapabilities.approve";

/// Read the document of one application.
#[derive(Clone, Debug)]
pub struct LoadAppCapabilityCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
}

/// Replace the whole document.
#[derive(Clone, Debug)]
pub struct ReplaceAppCapabilityCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub document: NormalizedAppCapability,
    /// Canonical JSON of the document; the revision row stores exactly this.
    pub snapshot: serde_json::Value,
}

/// Submit the current revision for review.
#[derive(Clone, Debug)]
pub struct SubmitAppCapabilityReviewCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub note: Option<String>,
}

/// List the review ledger of one application.
#[derive(Clone, Debug)]
pub struct ListAppCapabilityReviewsCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
    pub status: Option<String>,
    pub page: i32,
    pub page_size: i32,
}

/// Decide one pending review.
#[derive(Clone, Debug)]
pub struct DecideAppCapabilityReviewCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    pub app_uuid: String,
    pub review_uuid: String,
    pub decision: CapabilityReviewDecision,
    pub decision_code: Option<&'static str>,
    pub decision_note: Option<String>,
}

/// List the revisions of one application.
#[derive(Clone, Debug)]
pub struct ListAppCapabilityRevisionsCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
    pub page: i32,
    pub page_size: i32,
}

/// Read one revision, including its snapshot.
#[derive(Clone, Debug)]
pub struct RetrieveAppCapabilityRevisionCommand {
    pub tenant_id: i64,
    pub app_uuid: String,
    pub revision_uuid: String,
}

#[async_trait]
pub trait AppCapabilityRepositoryPort: Send + Sync {
    async fn load_app_capability(
        &self,
        command: LoadAppCapabilityCommand,
    ) -> DeployServiceResult<AppCapabilityResponse>;

    async fn replace_app_capability(
        &self,
        command: ReplaceAppCapabilityCommand,
    ) -> DeployServiceResult<AppCapabilityResponse>;

    async fn submit_app_capability_review(
        &self,
        command: SubmitAppCapabilityReviewCommand,
    ) -> DeployServiceResult<CapabilityReviewResponse>;

    async fn list_app_capability_reviews(
        &self,
        command: ListAppCapabilityReviewsCommand,
    ) -> DeployServiceResult<CapabilityReviewPage>;

    async fn decide_app_capability_review(
        &self,
        command: DecideAppCapabilityReviewCommand,
    ) -> DeployServiceResult<CapabilityReviewResponse>;

    async fn list_app_capability_revisions(
        &self,
        command: ListAppCapabilityRevisionsCommand,
    ) -> DeployServiceResult<CapabilityRevisionPage>;

    async fn retrieve_app_capability_revision(
        &self,
        command: RetrieveAppCapabilityRevisionCommand,
    ) -> DeployServiceResult<CapabilityRevisionResponse>;
}

impl crate::DeployService {
    /// The capability document of one application.
    ///
    /// An application that has never been configured is not an error: the read
    /// returns an empty draft document, which is what the console shows before the
    /// first save.
    pub async fn get_app_capability(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppCapabilityResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_app_uuid(app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .load_app_capability(LoadAppCapabilityCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
            })
            .await
    }

    /// Replace the whole capability document.
    ///
    /// `submitForReview` folds the submission into the same transaction: an
    /// editor's "save and submit" must not leave a saved draft behind when the
    /// submission is refused.
    pub async fn put_app_capability(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &ReplaceAppCapabilityRequest,
    ) -> DeployServiceResult<AppCapabilityResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        let actor_id = Self::require_actor(context)?;
        validate_app_uuid(app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;

        let document = request.normalize()?;
        let snapshot = crate::app_capability_document::canonical_snapshot(request);
        let response = self
            .repository
            .replace_app_capability(ReplaceAppCapabilityCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0).max(0),
                actor_id,
                app_uuid: app_id.to_owned(),
                document,
                snapshot,
            })
            .await?;
        self.audit_app_action(context, "app.capability.update", app_id)
            .await?;
        Ok(response)
    }

    /// Submit the current document for review.
    pub async fn submit_app_capability_review(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &SubmitCapabilityReviewRequest,
    ) -> DeployServiceResult<CapabilityReviewResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        let actor_id = Self::require_actor(context)?;
        validate_app_uuid(app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;

        if let Some(note) = request.note.as_deref() {
            if note.chars().count() > 2000 {
                return Err(DeployServiceError::validation(
                    "note must be at most 2000 characters",
                ));
            }
        }
        let review = self
            .repository
            .submit_app_capability_review(SubmitAppCapabilityReviewCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0).max(0),
                actor_id,
                app_uuid: app_id.to_owned(),
                note: request
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            })
            .await?;
        self.audit_app_action(context, "app.capability.submit", app_id)
            .await?;
        Ok(review)
    }

    /// The review ledger of one application, newest first.
    pub async fn list_app_capability_reviews(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        query: &ListCapabilityReviewsQuery,
    ) -> DeployServiceResult<CapabilityReviewPage> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_app_uuid(app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .list_app_capability_reviews(ListAppCapabilityReviewsCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
                status: query.status.map(|value| value.as_str().to_owned()),
                page: query.page,
                page_size: query.page_size,
            })
            .await
    }

    /// Decide a pending review.
    ///
    /// Approval publishes the revision the reviewer read. The permission is
    /// checked here as well as on the route because it is a *different* right from
    /// the write permission that produced the document: an author may submit, only
    /// a reviewer may publish.
    pub async fn decide_app_capability_review(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        review_id: &str,
        request: &DecideCapabilityReviewRequest,
    ) -> DeployServiceResult<CapabilityReviewResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        let actor_id = Self::require_actor(context)?;
        validate_app_uuid(app_id)?;
        validate_uuid_like(review_id, "reviewId")?;
        self.assert_app_access(context, tenant_id, app_id).await?;

        if !context.has_permission(DEPLOY_APP_CAPABILITY_APPROVE_PERMISSION) {
            return Err(DeployServiceError::forbidden(
                "deploy.appCapabilities.approve permission is required to decide a capability review",
            ));
        }
        request.validate()?;

        let review = self
            .repository
            .decide_app_capability_review(DecideAppCapabilityReviewCommand {
                tenant_id,
                organization_id: context.organization_id.unwrap_or(0).max(0),
                actor_id,
                app_uuid: app_id.to_owned(),
                review_uuid: review_id.to_owned(),
                decision: request.decision,
                decision_code: request.decision_code.map(|value| value.as_str()),
                decision_note: request
                    .decision_note
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            })
            .await?;
        self.audit_app_action(context, "app.capability.review_decide", app_id)
            .await?;
        Ok(review)
    }

    /// The revision list of one application, newest first.
    pub async fn list_app_capability_revisions(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        query: &ListCapabilityRevisionsQuery,
    ) -> DeployServiceResult<CapabilityRevisionPage> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_app_uuid(app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .list_app_capability_revisions(ListAppCapabilityRevisionsCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
                page: query.page,
                page_size: query.page_size,
            })
            .await
    }

    /// One revision of one application, including the snapshot bytes.
    pub async fn get_app_capability_revision(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        revision_id: &str,
    ) -> DeployServiceResult<CapabilityRevisionResponse> {
        Self::assert_caller_app_scope(context, app_id)?;
        let tenant_id = Self::require_tenant(context)?;
        validate_app_uuid(app_id)?;
        validate_uuid_like(revision_id, "revisionId")?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .retrieve_app_capability_revision(RetrieveAppCapabilityRevisionCommand {
                tenant_id,
                app_uuid: app_id.to_owned(),
                revision_uuid: revision_id.to_owned(),
            })
            .await
    }
}

/// The statuses a submission may start from.
pub fn status_allows_submission(status: CapabilityStatus) -> bool {
    matches!(
        status,
        CapabilityStatus::Draft | CapabilityStatus::Approved | CapabilityStatus::Published
    )
}

fn validate_app_uuid(value: &str) -> DeployServiceResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 128 {
        return Err(DeployServiceError::validation(
            "appId must be 1..=128 characters",
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(DeployServiceError::validation(
            "appId must not contain control characters",
        ));
    }
    Ok(())
}

fn validate_uuid_like(value: &str, field: &str) -> DeployServiceResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 128 {
        return Err(DeployServiceError::validation(format!(
            "{field} must be 1..=128 characters"
        )));
    }
    Ok(())
}

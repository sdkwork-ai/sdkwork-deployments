//! App-template read access for the install flow.
//!
//! One listing has two legitimate readers, and they are not the same read:
//!
//! * the **author** keeps the author-scope read — every version of their own
//!   listing, drafts included;
//! * everyone else is a **marketplace consumer** and may read only what the
//!   marketplace itself advertises: a `PUBLIC`, approved (`PUBLISHED`), not
//!   soft-deleted listing and the versions that were published with it.
//!
//! The predicate lives here so the service decision and its tests cannot drift
//! from the SQL the repository applies: the marketplace branch is the same
//! `visibility = 'PUBLIC' AND status = 'PUBLISHED'` conjunction the browse and
//! retrieve queries use, never a loosened `author_user_id` filter. A draft,
//! hidden, rejected or disabled listing therefore falls through to
//! [`TemplateReadAccess::Denied`], which the service answers exactly like a
//! missing listing.

use sdkwork_deploy_contract::{TEMPLATE_STATUS_PUBLISHED, TEMPLATE_VISIBILITY_PUBLIC};

/// How one listing may be read by one caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateReadAccess {
    /// The caller is the author: the author-scope read, drafts included.
    Author,
    /// The caller is a consumer: only the marketplace-visible published view.
    Marketplace,
    /// Neither the author nor a legally visible consumer; answer "not found".
    Denied,
}

/// Classifies `actor_id` against one listing's ownership and marketplace state.
///
/// `visibility` and `status` are the row's stored values
/// (`deploy_application_scaffold.visibility` / `.status`). The marketplace
/// branch deliberately repeats the browse/retrieve gate instead of deriving a
/// second notion of "visible": a consumer read and the marketplace shelf must
/// agree, or the shelf advertises something the install cannot fetch.
#[must_use]
pub fn template_read_access(
    actor_id: i64,
    author_user_id: i64,
    visibility: &str,
    status: &str,
) -> TemplateReadAccess {
    if author_user_id == actor_id {
        TemplateReadAccess::Author
    } else if visibility == TEMPLATE_VISIBILITY_PUBLIC && status == TEMPLATE_STATUS_PUBLISHED {
        TemplateReadAccess::Marketplace
    } else {
        TemplateReadAccess::Denied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_keeps_the_author_scope_read_for_every_listing_state() {
        for (visibility, status) in [
            ("PUBLIC", "PUBLISHED"),
            ("PUBLIC", "DRAFT"),
            ("PUBLIC", "PENDING_REVIEW"),
            ("PUBLIC", "REJECTED"),
            ("PUBLIC", "DISABLED"),
            ("PRIVATE", "DRAFT"),
            ("PRIVATE", "PENDING_REVIEW"),
            ("PRIVATE", "PUBLISHED"),
        ] {
            assert_eq!(
                template_read_access(7, 7, visibility, status),
                TemplateReadAccess::Author,
                "the author of a {visibility}/{status} listing keeps the author read"
            );
        }
    }

    #[test]
    fn consumer_reads_a_published_public_listing_through_the_marketplace() {
        assert_eq!(
            template_read_access(12, 7, "PUBLIC", "PUBLISHED"),
            TemplateReadAccess::Marketplace
        );
    }

    #[test]
    fn consumer_cannot_read_anything_the_marketplace_does_not_show() {
        for (visibility, status) in [
            ("PUBLIC", "DRAFT"),
            ("PUBLIC", "PENDING_REVIEW"),
            ("PUBLIC", "REJECTED"),
            ("PUBLIC", "DISABLED"),
            ("PRIVATE", "DRAFT"),
            ("PRIVATE", "PENDING_REVIEW"),
            ("PRIVATE", "PUBLISHED"),
        ] {
            assert_eq!(
                template_read_access(12, 7, visibility, status),
                TemplateReadAccess::Denied,
                "a {visibility}/{status} listing is invisible to a consumer"
            );
        }
    }

    #[test]
    fn the_marketplace_branch_matches_the_browse_gate_constants() {
        // Guards the shared vocabulary rather than a literal: if the contract
        // renames the published/public tokens, the consumer branch must follow.
        assert_eq!(
            template_read_access(1, 2, TEMPLATE_VISIBILITY_PUBLIC, TEMPLATE_STATUS_PUBLISHED),
            TemplateReadAccess::Marketplace
        );
    }
}

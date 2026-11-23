//! IAM permission codes this module *reads* out of the caller's principal, and
//! the reach those codes grant.
//!
//! Only the codes the Deploy module itself evaluates live here. A permission is
//! declared and granted in the IAM catalog; what this module owns is the
//! decision it makes once the principal carries that code, and the string is
//! stated once so the read side and the doc cannot drift.

use crate::app_ports::DeployAppRequestContext;

/// Grant a caller tenant-wide reach over owned records.
///
/// Ordinary reachability is "my own zones, plus the tenant-level inventory" —
/// a zone belongs to the user who created it, and `user_id IS NULL` marks the
/// platform's `app.<suffix>` zones that every member may reach. This grant lifts
/// the per-user half of that predicate to "any ownership level inside my own
/// tenant", which is what the admin console needs in order to present the whole
/// domain and certificate inventory of the tenant it administers rather than
/// only the rows the operator happens to own personally.
///
/// Deliberately **one** code covering both inventories rather than one per
/// resource: the reachability predicate is a single rule, and two codes would
/// let a caller hold the domain half and the certificate half of one decision,
/// which is a state no surface has a use for and every test would have to cover.
///
/// It does **not** cross tenants. The predicate this code relaxes is always
/// conjoined with `tenant_id = $1`, so a holder still sees exactly one tenant —
/// the one the request is scoped to. There is no code in this module that widens
/// that, which is why "platform superadmin" is not expressible here.
///
/// Requires re-granting: an operator holding only the resource-level read codes
/// (`deploy.domainZones.read`, `deploy.certificates.read`) sees the narrower set
/// they always did. The platform super administrator's `*` grant matches this
/// code through [`crate::app_ports::DeployAppRequestContext::has_permission`],
/// which is how the console's own administrator reaches the whole inventory
/// without a separate catalog entry.
pub const PERM_MANAGE_ALL_OWNERSHIP: &str = "deploy.ownership.manage_all";

/// Every permission code this module reads, for catalog introspection and for a
/// host that needs to declare what mounting this module requires.
pub const DEPLOY_OWNERSHIP_PERMISSIONS: [&str; 1] = [PERM_MANAGE_ALL_OWNERSHIP];

/// Who is asking, and how far their reach extends over rows somebody else owns.
///
/// The domain inventory is user-private: `deploy_dns_zone.user_id` names the
/// user who created the zone, and a caller reaches its own rows plus the
/// tenant-level platform inventory (`user_id IS NULL`). This value carries that
/// actor *and*, separately, the one grant that lifts the per-user half of the
/// predicate — [`PERM_MANAGE_ALL_OWNERSHIP`] — because the two halves are used
/// differently: the actor is what an audit column records, and the grant is what
/// the reach predicate reads. Passing a single conflated id would make the admin
/// write its own subject into `created_by`/`updated_by` of a row it no longer
/// owns, or write the row's owner into the audit trail.
///
/// It is `Copy` and holds no strings, so every gated call takes it by value and
/// a query cannot be built from a partially populated reach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnershipReach {
    actor_id: Option<i64>,
    organization_id: Option<i64>,
    can_manage_all_ownership: bool,
}

impl OwnershipReach {
    /// A reach for `actor_id`, lifted to the whole tenant when
    /// `can_manage_all_ownership` is set.
    pub fn new(
        actor_id: Option<i64>,
        organization_id: Option<i64>,
        can_manage_all_ownership: bool,
    ) -> Self {
        Self {
            actor_id,
            organization_id,
            can_manage_all_ownership,
        }
    }

    /// The reach the verified principal behind `context` carries.
    ///
    /// The lift is read from the principal's own permission projection
    /// (`deploy.ownership.manage_all`, or a wildcard grant that covers it), never
    /// from a request body or query parameter, so a caller cannot widen its own
    /// reach.
    pub fn from_context(context: &DeployAppRequestContext) -> Self {
        Self::new(
            context.actor_id,
            context.organization_id,
            context.has_permission(PERM_MANAGE_ALL_OWNERSHIP),
        )
    }

    /// The caller's own subject: the actor an audit column records.
    pub fn actor_id(self) -> Option<i64> {
        self.actor_id
    }

    /// The caller's organization, for the ownership levels that key on one.
    pub fn organization_id(self) -> Option<i64> {
        self.organization_id
    }

    /// Whether the caller reaches every owner inside its tenant.
    pub fn can_manage_all_ownership(self) -> bool {
        self.can_manage_all_ownership
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(permissions: &[&str]) -> DeployAppRequestContext {
        DeployAppRequestContext {
            tenant_id: 7,
            actor_id: Some(11),
            organization_id: Some(9),
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_ownership_grant_is_the_only_thing_that_lifts_the_reach() {
        assert!(
            !OwnershipReach::from_context(&context(&["deploy.domainZones.read"]))
                .can_manage_all_ownership(),
            "a resource-level read code reaches only the caller's own rows"
        );
        assert!(
            OwnershipReach::from_context(&context(&[PERM_MANAGE_ALL_OWNERSHIP]))
                .can_manage_all_ownership(),
            "the ownership grant reaches the whole tenant"
        );
        assert!(
            OwnershipReach::from_context(&context(&["*"])).can_manage_all_ownership(),
            "the platform administrator's wildcard grant covers the ownership code"
        );
    }

    #[test]
    fn the_reach_keeps_the_actor_separate_from_the_lift() {
        let reach = OwnershipReach::from_context(&context(&[PERM_MANAGE_ALL_OWNERSHIP]));
        assert_eq!(reach.actor_id(), Some(11), "audit keeps the real actor");
        assert_eq!(reach.organization_id(), Some(9));
        assert!(reach.can_manage_all_ownership());
    }

    #[test]
    fn a_principal_without_a_subject_is_not_lifted_by_default() {
        let mut context = context(&[]);
        context.actor_id = None;
        let reach = OwnershipReach::from_context(&context);
        assert_eq!(reach.actor_id(), None);
        assert!(!reach.can_manage_all_ownership());
    }
}

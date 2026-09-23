//! IAM permission codes this module *reads* out of the caller's principal.
//!
//! Only the codes the Deploy module itself evaluates live here. A permission is
//! declared and granted in the IAM catalog; what this module owns is the
//! decision it makes once the principal carries that code, and the string is
//! stated once so the read side and the doc cannot drift.

/// Grant a caller tenant-wide reach over owned records.
///
/// Ordinary reachability is "my own, my organization's, or the tenant-level
/// inventory" — see [`crate::OwnershipReach`]. This grant lifts that to "any
/// ownership level inside my own tenant", which is what the Web Server admin
/// surface needs in order to present the whole domain and certificate inventory
/// of the tenant it administers rather than only the rows the operator happens
/// to own personally.
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
/// (`deploy.domains.read`, `deploy.certificates.read`) sees the narrower set they
/// always did.
pub const PERM_MANAGE_ALL_OWNERSHIP: &str = "deploy.ownership.manage_all";

/// Every permission code this module reads, for catalog introspection and for a
/// host that needs to declare what mounting this module requires.
pub const DEPLOY_OWNERSHIP_PERMISSIONS: [&str; 1] = [PERM_MANAGE_ALL_OWNERSHIP];

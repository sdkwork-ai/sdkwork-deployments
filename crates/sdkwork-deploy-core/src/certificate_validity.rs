//! Certificate validity and renewal constants shared across the deploy crates.
//!
//! The numbers live here rather than next to the renewal rule because two crates
//! that must never disagree need them: the contract crate uses the default when
//! deserializing `renewBeforeDays`, and the service crate uses all three to decide
//! when renewal starts. A duplicate default would let a certificate created
//! through the API and one created by an internal caller be scheduled against
//! different windows.
//!
//! The bounds are also mirrored by `chk_deploy_certificate_renew_before_days` on
//! `deploy_certificate.renew_before_days`. Validating here first means a bad value
//! is reported as a field error rather than as a constraint violation.

/// Lead time applied when a certificate does not choose one.
///
/// 30 days is the de-facto ACME client default. It leaves room, inside a 90-day
/// certificate, for a failed attempt, a manual DNS-01 fix, and a retry before the
/// certificate dies.
pub const CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS: i32 = 30;

/// Shortest allowed lead time.
///
/// Below a week, a CA outage or a DNS propagation delay leaves no room to recover
/// before expiry.
pub const CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS: i32 = 7;

/// Longest allowed lead time.
///
/// Larger values are safe but not more useful — the renewal window is floored at
/// one third of the certificate's lifetime, so this only bounds how wide the
/// scheduler's coarse expiry filter has to be.
pub const CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS: i32 = 90;

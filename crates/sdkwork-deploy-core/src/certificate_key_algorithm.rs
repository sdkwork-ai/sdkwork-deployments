//! The certificate key algorithms this platform will issue, and the one it picks
//! when the caller has no opinion.
//!
//! The vocabulary lives here because crates that must never disagree need it: the
//! contract crate returns the default while deserializing `preferredKeyAlgorithm`,
//! the repository and service crates refuse anything outside the set, and the ACME
//! engine maps a value to an actual curve or modulus. A duplicate list would let a
//! certificate be accepted at creation and refused at issuance — the caller then
//! sees a failure days later, in a worker, on a certificate that already exists.
//!
//! `RSA` is the default rather than `ECDSA` because a managed certificate is bought
//! to be accepted by every client that will ever reach the name, and compatibility
//! is the property an operator with no opinion needs:
//!
//! * RSA-2048 is the one leaf key every TLS client accepts, from old Java and
//!   Android stacks and embedded updaters to compliance scanners that still reject
//!   a P-256 certificate. ECDSA is universal in theory and not yet in the field:
//!   its failure mode is a handshake only the newest clients complete.
//! * Managed issuance is unattended, so the choice is made once by whoever fills in
//!   the form and then silently repeated at every renewal. Defaulting to the
//!   narrower option issues a certificate that an untested legacy client may reject
//!   months later, which is the worst time to learn it.
//! * RSA-2048 costs roughly 1.5 kB more per handshake — measurable in aggregate,
//!   immaterial next to one unexplained TLS alert from one old client.
//!
//! ECDSA stays in the set for whoever asks for it by name: P-256 is roughly
//! RSA-3072 in strength while signing an order of magnitude faster, and internet
//! scale operators recommend it — Google Cloud's certificate manager best practices
//! name ECDSA P-256 the recommended key type for most TLS certificates and certbot
//! has defaulted to `ecdsa` (`secp256r1`) since 2.0.0. This platform does not
//! inherit that posture as its *default*: an operator who wants ECDSA can choose it
//! per certificate, and an operator who does not think about it at all must land on
//! the key that works everywhere.
//!
//! The set is also mirrored by four DDL constraints in
//! `database/ddl/baseline/postgres/0001_deploy_baseline.sql` —
//! `chk_deploy_certificate_key_algorithm`,
//! `chk_deploy_certificate_version_key_algorithm`,
//! `chk_deploy_listener_certificate_binding_algorithm` and
//! `chk_deploy_tls_runtime_assignment_algorithm`. Validating here first means a bad
//! value is reported as a field error instead of as a constraint violation, and
//! `tests/certificate_key_algorithm_parity.rs` fails if the two sets ever diverge.

/// RSA, the default: the one leaf key every TLS client accepts, and therefore the
/// one an operator who never touches the field must end up with. See the module
/// docs for why compatibility outranks the industry posture here.
pub const CERTIFICATE_KEY_ALGORITHM_RSA: &str = "RSA";

/// ECDSA, for operators who ask for it: stronger per bit and an order of magnitude
/// faster to sign, at the cost of clients that predate it. See the module docs.
pub const CERTIFICATE_KEY_ALGORITHM_ECDSA: &str = "ECDSA";

/// Every key algorithm a certificate may ask for, spelled as the DDL stores it.
///
/// Upper case is not cosmetic: the columns are `CHECK (… IN ('RSA', 'ECDSA'))`, so
/// a lower-case value that slipped through a service-layer check would be rejected
/// by PostgreSQL with a constraint violation instead of a field error.
///
/// Built from the named constants above so that a match arm keyed by an algorithm
/// and this list cannot disagree; the order is the order the reason message lists
/// them in.
pub const CERTIFICATE_KEY_ALGORITHMS: &[&str] = &[
    CERTIFICATE_KEY_ALGORITHM_RSA,
    CERTIFICATE_KEY_ALGORITHM_ECDSA,
];

/// The algorithm a certificate gets when it does not choose one.
///
/// `RSA`, because a managed certificate is renewed unattended and must keep being
/// accepted by every client that reaches the name; see the module docs. The
/// narrower algorithm stays one explicit choice away, where operator intent exists.
pub const CERTIFICATE_DEFAULT_KEY_ALGORITHM: &str = CERTIFICATE_KEY_ALGORITHM_RSA;

/// Validates a declared certificate key algorithm.
///
/// The reason omits the field name so each caller can name its own: the field is
/// `preferredKeyAlgorithm` when a certificate is created and `keyAlgorithm` when
/// issued material is stored. The allowed set is still built from
/// [`CERTIFICATE_KEY_ALGORITHMS`], so adding an algorithm to the vocabulary updates
/// every message that reports it — which is the half a hand-written message per
/// call site would drift on.
pub fn validate_certificate_key_algorithm(algorithm: &str) -> Result<(), String> {
    if CERTIFICATE_KEY_ALGORITHMS.contains(&algorithm) {
        Ok(())
    } else {
        Err(format!(
            "must be one of {}",
            CERTIFICATE_KEY_ALGORITHMS.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_inside_the_vocabulary() {
        // A default outside the set would make every certificate that omits the
        // field fail validation at creation.
        assert!(CERTIFICATE_KEY_ALGORITHMS.contains(&CERTIFICATE_DEFAULT_KEY_ALGORITHM));
    }

    #[test]
    fn every_algorithm_in_the_vocabulary_validates() {
        for algorithm in CERTIFICATE_KEY_ALGORITHMS {
            assert!(
                validate_certificate_key_algorithm(algorithm).is_ok(),
                "{algorithm} is in the vocabulary but does not validate"
            );
        }
    }

    #[test]
    fn the_vocabulary_lists_exactly_the_named_algorithms() {
        // The set is reachable two ways — as a list to validate against, and as named
        // constants to key a match on. This is the assertion that keeps the two from
        // drifting apart, which is otherwise silent: a value missing from the list is
        // simply never offered, and one missing from the names is never mapped.
        assert_eq!(
            CERTIFICATE_KEY_ALGORITHMS,
            &[
                CERTIFICATE_KEY_ALGORITHM_RSA,
                CERTIFICATE_KEY_ALGORITHM_ECDSA
            ]
        );
    }

    #[test]
    fn the_default_is_rsa_because_it_reaches_every_client() {
        // Pins the decision, not just the mechanism: a change here is a change to
        // what every operator who never touches the field ends up with. The literal
        // is asserted too, because it is the wire value the DDL stores and the ACME
        // engine maps.
        assert_eq!(
            CERTIFICATE_DEFAULT_KEY_ALGORITHM,
            CERTIFICATE_KEY_ALGORITHM_RSA
        );
        assert_eq!(CERTIFICATE_DEFAULT_KEY_ALGORITHM, "RSA");
    }

    #[test]
    fn an_unknown_algorithm_is_refused_and_the_reason_lists_the_whole_set() {
        let reason = validate_certificate_key_algorithm("ED25519").expect_err("must be refused");
        assert!(
            !reason.contains("ED25519"),
            "the reason should not echo the input: {reason}"
        );
        for algorithm in CERTIFICATE_KEY_ALGORITHMS {
            assert!(
                reason.contains(algorithm),
                "the reason omits {algorithm}: {reason}"
            );
        }
    }

    #[test]
    fn the_check_is_case_sensitive_like_the_constraint_it_guards() {
        // `ecdsa` would pass a case-insensitive check here and then fail the DDL
        // `CHECK (… IN ('RSA','ECDSA'))`, turning a field error into a 500.
        assert!(validate_certificate_key_algorithm("ecdsa").is_err());
        assert!(validate_certificate_key_algorithm("rsa").is_err());
    }

    #[test]
    fn the_empty_string_is_refused() {
        assert!(validate_certificate_key_algorithm("").is_err());
    }
}

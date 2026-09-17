//! Certificate material custody for managed TLS.
//!
//! A managed certificate is not one artifact but a *set*: the leaf certificate,
//! its private key, the intermediate chain the CA returned, the root that
//! anchors that chain, and the concatenation a TLS terminator actually loads.
//! `ADR-20260723` §4 makes custody of that set an explicit reviewed contract;
//! this crate is the enforcement half of it.
//!
//! 1. [`bundle::assemble`] turns the material an issuance worker hands over into
//!    the canonical five-file bundle **after proving the set hangs together** —
//!    chain signatures, name chaining, and a private key that really does match
//!    the leaf — rather than trusting the caller's description of it. The input
//!    is the blob the CA returned (leaf first), because that is what a worker
//!    holds and what `chain_sha256` hashes. The digests it derives are
//!    byte-identical to `sdkwork-webserver-acme-service`'s `CertificateEvidence`,
//!    so the control plane can recompute them and reject a bundle whose declared
//!    metadata disagrees with the bytes it is being asked to store.
//! 2. [`anchors::TrustAnchorBundle`] supplies the root when the CA's chain stops
//!    at an intermediate, which is the normal case.
//! 3. [`envelope`] seals the secret file so material can live in the database
//!    without plaintext key material ever being persisted: a random
//!    data-encryption key (DEK) encrypts the file bytes, and the DEK is wrapped
//!    by a key-encryption key (KEK) held outside the database ([`keys`]).
//!
//! Public material (leaf, intermediates, root, full chain) is stored verbatim:
//! it is public by definition, and keeping it readable is what lets an operator
//! answer "which certificate covers this name" with a query.

pub mod anchors;
pub mod bundle;
pub mod envelope;
pub mod facts;
pub mod keys;

use thiserror::Error;

/// Failures the custody layer reports.
#[derive(Debug, Error)]
pub enum MaterialError {
    /// The material is structurally unusable: not PEM, not X.509, or using an
    /// algorithm this deployment cannot custody.
    #[error("certificate material is malformed: {0}")]
    Malformed(String),
    /// The set parses but does not hang together — a chain that fails to verify,
    /// a private key that does not match the leaf, a file that is missing.
    #[error("certificate material is inconsistent: {0}")]
    Inconsistent(String),
    /// The key-encryption layer refused the operation.
    #[error("certificate material custody failed: {0}")]
    Custody(String),
}

/// Result alias for this crate.
pub type MaterialResult<T> = Result<T, MaterialError>;

pub use anchors::TrustAnchorBundle;
pub use bundle::{
    assemble, assemble_with_root_source, CertificateFile, CertificateMaterialBundle, MaterialKind,
    RootSource, FILE_CHAIN, FILE_FULL_CHAIN, FILE_LEAF, FILE_PRIVATE_KEY, FILE_ROOT,
};
pub use envelope::{
    open, seal, Protection, SealedCertificateFile, SealedMaterial, MATERIAL_AAD_NAMESPACE,
};
pub use facts::{sha256_hex, CertificateFacts};
pub use keys::{FileKeyProvider, MaterialKeyProvider};

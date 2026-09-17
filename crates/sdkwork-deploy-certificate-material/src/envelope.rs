//! Envelope encryption for material at rest.
//!
//! The private key is the one file that must never be readable by whoever can
//! read the database. It is sealed with a fresh AES-256-GCM data key per file,
//! and that data key is wrapped by the custody master key. The result is a row
//! that carries everything needed to decrypt — ciphertext, nonce, wrapped key —
//! and still yields nothing without the master key, which never enters the
//! database.
//!
//! The additional authenticated data binds each ciphertext to the row it belongs
//! to. Moving a sealed blob to another version or another kind therefore fails
//! authentication instead of quietly serving the wrong certificate.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::Aes256Gcm;
use zeroize::Zeroizing;

use crate::bundle::MaterialKind;
use crate::facts::sha256_hex;
use crate::keys::{MaterialKeyProvider, DEK_WRAP_AAD};
use crate::{MaterialError, MaterialResult};

/// Namespace for the material AAD, versioned so a future layout change is
/// distinguishable rather than silently incompatible.
pub const MATERIAL_AAD_NAMESPACE: &str = "sdkwork.deploy.certificate-material.v1";

/// One file of a bundle paired with its sealed form.
///
/// The pair travels together because the sealing is bound to the kind: opening a
/// file under the wrong kind fails authentication, so a caller that unpacked
/// them into parallel lists could pair them up wrongly and only find out at read
/// time.
#[derive(Clone, Debug)]
pub struct SealedCertificateFile {
    pub kind: MaterialKind,
    pub sealed: SealedMaterial,
}

impl SealedCertificateFile {
    /// Seals one file of a bundle.
    pub fn seal(
        kind: MaterialKind,
        plaintext: &[u8],
        certificate_version_uuid: &str,
        provider: Option<&dyn MaterialKeyProvider>,
    ) -> MaterialResult<Self> {
        Ok(Self {
            kind,
            sealed: seal(kind, plaintext, certificate_version_uuid, provider)?,
        })
    }

    /// Opens the file, proving it is the one that was stored.
    pub fn open(
        &self,
        certificate_version_uuid: &str,
        provider: Option<&dyn MaterialKeyProvider>,
    ) -> MaterialResult<Zeroizing<Vec<u8>>> {
        open(self.kind, certificate_version_uuid, &self.sealed, provider)
    }
}

/// How a stored file is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    /// Public material, stored verbatim.
    None,
    /// AES-256-GCM under a per-file data key, itself wrapped by the custody key.
    EnvelopeAes256Gcm,
}

impl Protection {
    /// The stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Protection::None => "NONE",
            Protection::EnvelopeAes256Gcm => "ENVELOPE_AES_256_GCM",
        }
    }

    /// Reads the database value back.
    ///
    /// An unknown value is an error rather than a default: treating an
    /// unrecognised protection as "plaintext" would hand back ciphertext as if it
    /// were a certificate, and treating it as "sealed" would fail confusingly.
    pub fn from_db(value: &str) -> MaterialResult<Self> {
        match value {
            "NONE" => Ok(Protection::None),
            "ENVELOPE_AES_256_GCM" => Ok(Protection::EnvelopeAes256Gcm),
            other => Err(MaterialError::Malformed(format!(
                "stored certificate material declares unknown protection '{other}'"
            ))),
        }
    }
}

/// A file plus everything needed to read it back.
#[derive(Clone, Debug)]
pub struct SealedMaterial {
    pub protection: Protection,
    /// Ciphertext for protected files, the plaintext bytes otherwise.
    pub content: Vec<u8>,
    /// GCM nonce; empty when [`Protection::None`].
    pub nonce: Vec<u8>,
    /// Authenticated data binding the ciphertext to its row.
    pub aad: Vec<u8>,
    /// Data key sealed under the master key; empty when [`Protection::None`].
    pub wrapped_dek: Vec<u8>,
    /// Which master key wrapped the data key, so a rotation can find its rows.
    pub kek_ref: Option<String>,
    /// Digest of the **plaintext**, so integrity survives decryption.
    pub content_sha256: String,
    pub content_size_bytes: i64,
}

/// Binds a ciphertext to the version and kind it was sealed for.
pub fn material_aad(
    certificate_version_uuid: &str,
    kind: MaterialKind,
    content_sha256: &str,
) -> Vec<u8> {
    format!(
        "{MATERIAL_AAD_NAMESPACE}|{certificate_version_uuid}|{}|{content_sha256}",
        kind.as_str()
    )
    .into_bytes()
}

/// Seals one file of a bundle.
///
/// Public material is stored verbatim; only [`MaterialKind::is_secret`] kinds are
/// encrypted, and those refuse to be sealed without a key provider rather than
/// falling back to plaintext.
pub fn seal(
    kind: MaterialKind,
    plaintext: &[u8],
    certificate_version_uuid: &str,
    provider: Option<&dyn MaterialKeyProvider>,
) -> MaterialResult<SealedMaterial> {
    let content_sha256 = sha256_hex(plaintext);
    let aad = material_aad(certificate_version_uuid, kind, &content_sha256);
    let content_size_bytes = plaintext.len() as i64;

    if !kind.is_secret() {
        return Ok(SealedMaterial {
            protection: Protection::None,
            content: plaintext.to_vec(),
            nonce: Vec::new(),
            aad,
            wrapped_dek: Vec::new(),
            kek_ref: None,
            content_sha256,
            content_size_bytes,
        });
    }

    let provider = provider.ok_or_else(|| {
        MaterialError::Custody(format!(
            "{} is secret material and cannot be stored without a key provider",
            kind.as_str()
        ))
    })?;

    let dek = Zeroizing::new(Aes256Gcm::generate_key(&mut OsRng).to_vec());
    let cipher = Aes256Gcm::new_from_slice(&dek[..]).map_err(|_| {
        MaterialError::Custody("the generated data key was not 32 bytes".to_owned())
    })?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            MaterialError::Custody("certificate material could not be sealed".to_owned())
        })?;
    let wrapped_dek = provider.wrap_dek(&dek[..], DEK_WRAP_AAD)?;

    Ok(SealedMaterial {
        protection: Protection::EnvelopeAes256Gcm,
        content: ciphertext,
        nonce: nonce.as_slice().to_vec(),
        aad,
        wrapped_dek,
        kek_ref: Some(provider.kek_ref().to_owned()),
        content_sha256,
        content_size_bytes,
    })
}

/// Opens a sealed file, verifying it is the one that was stored.
///
/// Three things are checked before the plaintext is returned: the AAD still
/// describes this row, the size matches, and the plaintext still hashes to the
/// recorded digest. The last two catch a truncated column and a corrupted
/// backup, which GCM alone cannot see once the bytes are back in memory.
pub fn open(
    kind: MaterialKind,
    certificate_version_uuid: &str,
    sealed: &SealedMaterial,
    provider: Option<&dyn MaterialKeyProvider>,
) -> MaterialResult<Zeroizing<Vec<u8>>> {
    let expected_aad = material_aad(certificate_version_uuid, kind, &sealed.content_sha256);
    if expected_aad != sealed.aad {
        return Err(MaterialError::Inconsistent(
            "stored certificate material was sealed for a different version or kind".to_owned(),
        ));
    }

    let plaintext = match sealed.protection {
        Protection::None => Zeroizing::new(sealed.content.clone()),
        Protection::EnvelopeAes256Gcm => {
            let provider = provider.ok_or_else(|| {
                MaterialError::Custody(
                    "sealed certificate material needs a key provider to be opened".to_owned(),
                )
            })?;
            let dek = provider.unwrap_dek(&sealed.wrapped_dek, DEK_WRAP_AAD)?;
            let cipher = Aes256Gcm::new_from_slice(&dek[..]).map_err(|_| {
                MaterialError::Custody("the unwrapped data key was not 32 bytes".to_owned())
            })?;
            let plaintext = cipher
                .decrypt(
                    aes_gcm::aead::Nonce::<Aes256Gcm>::from_slice(&sealed.nonce),
                    Payload {
                        msg: &sealed.content,
                        aad: &expected_aad,
                    },
                )
                .map_err(|_| {
                    MaterialError::Inconsistent(
                        "stored certificate material failed authentication".to_owned(),
                    )
                })?;
            Zeroizing::new(plaintext)
        }
    };

    if plaintext.len() as i64 != sealed.content_size_bytes {
        return Err(MaterialError::Inconsistent(format!(
            "stored certificate material is {} bytes but {} were recorded",
            plaintext.len(),
            sealed.content_size_bytes
        )));
    }
    if sha256_hex(&plaintext) != sealed.content_sha256 {
        return Err(MaterialError::Inconsistent(
            "stored certificate material does not hash to its recorded digest".to_owned(),
        ));
    }
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::FileKeyProvider;

    const VERSION: &str = "0b6f1c62-6f1e-4a2f-9d0a-2c5b8f3a1e77";

    fn provider() -> FileKeyProvider {
        FileKeyProvider::from_material(&[42u8; 32], "file:test/master.key").expect("provider")
    }

    #[test]
    fn seals_a_private_key_and_never_stores_it_verbatim() {
        let secret = b"-----BEGIN PRIVATE KEY-----\nnot really\n-----END PRIVATE KEY-----\n";
        let sealed =
            seal(MaterialKind::PrivateKey, secret, VERSION, Some(&provider())).expect("seal");
        assert_eq!(sealed.protection, Protection::EnvelopeAes256Gcm);
        assert_ne!(sealed.content.as_slice(), secret.as_slice());
        assert!(!sealed.wrapped_dek.is_empty());
        assert_eq!(sealed.kek_ref.as_deref(), Some("file:test/master.key"));
        // The digest describes the plaintext, not the ciphertext.
        assert_eq!(sealed.content_sha256, sha256_hex(secret));

        let opened = open(
            MaterialKind::PrivateKey,
            VERSION,
            &sealed,
            Some(&provider()),
        )
        .expect("open");
        assert_eq!(opened.as_slice(), secret.as_slice());
    }

    #[test]
    fn stores_public_material_verbatim() {
        let leaf = b"-----BEGIN CERTIFICATE-----\nleaf\n-----END CERTIFICATE-----\n";
        let sealed = seal(MaterialKind::LeafCert, leaf, VERSION, None).expect("seal");
        assert_eq!(sealed.protection, Protection::None);
        assert_eq!(sealed.content.as_slice(), leaf.as_slice());
        assert!(sealed.wrapped_dek.is_empty());
        assert!(sealed.kek_ref.is_none());
        let opened = open(MaterialKind::LeafCert, VERSION, &sealed, None).expect("open");
        assert_eq!(opened.as_slice(), leaf.as_slice());
    }

    #[test]
    fn refuses_to_store_a_private_key_without_a_key_provider() {
        let error = seal(MaterialKind::PrivateKey, b"key", VERSION, None).expect_err("reject");
        assert!(
            format!("{error}").contains("cannot be stored without a key provider"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_tampered_ciphertext_fails_authentication() {
        let mut sealed = seal(
            MaterialKind::PrivateKey,
            b"-----BEGIN PRIVATE KEY-----\nsecret\n",
            VERSION,
            Some(&provider()),
        )
        .expect("seal");
        sealed.content[0] ^= 0x01;
        let error = open(
            MaterialKind::PrivateKey,
            VERSION,
            &sealed,
            Some(&provider()),
        )
        .expect_err("must refuse");
        assert!(
            format!("{error}").contains("failed authentication"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_sealed_file_cannot_be_moved_to_another_version_or_kind() {
        let sealed = seal(
            MaterialKind::PrivateKey,
            b"secret bytes",
            VERSION,
            Some(&provider()),
        )
        .expect("seal");

        let other_version = open(
            MaterialKind::PrivateKey,
            "11111111-2222-3333-4444-555555555555",
            &sealed,
            Some(&provider()),
        )
        .expect_err("must refuse another version");
        assert!(
            format!("{other_version}").contains("sealed for a different version"),
            "unexpected error: {other_version}"
        );

        let other_kind = open(
            MaterialKind::IntermediateChain,
            VERSION,
            &sealed,
            Some(&provider()),
        )
        .expect_err("must refuse another kind");
        assert!(format!("{other_kind}").contains("sealed for a different version"));
    }

    #[test]
    fn a_truncated_file_is_caught_even_though_it_decrypts() {
        // Simulates a column that lost its tail: the remaining bytes still
        // authenticate as a prefix only if the recorded metadata is also stale,
        // so the size and digest checks are what catch it.
        let sealed = seal(MaterialKind::LeafCert, b"abcdef", VERSION, None).expect("seal");
        let mut stale = sealed.clone();
        stale.content.truncate(3);
        let error = open(MaterialKind::LeafCert, VERSION, &stale, None).expect_err("must refuse");
        assert!(
            format!("{error}").contains("bytes but"),
            "unexpected error: {error}"
        );
    }
}

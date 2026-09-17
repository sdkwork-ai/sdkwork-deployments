//! Key-encryption keys for material at rest.
//!
//! `ADR-20260723` §4 requires the master key to come from a protected secret
//! file — never an environment value and never a database row. That is not
//! ceremony: a key stored in the database it protects protects nothing, and one
//! in the environment leaks through every process listing, crash dump, and
//! container inspection.
//!
//! Only the master key lives outside. The per-file data keys it wraps are stored
//! next to their ciphertext, which is what lets the control plane hold a full
//! bundle without being able to read the private key on its own.

use std::path::Path;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::Aes256Gcm;
use zeroize::Zeroizing;

use crate::{MaterialError, MaterialResult};

/// Additional authenticated data for the DEK wrap, distinct from the material's
/// own AAD so a wrapped key can never be replayed as file content.
pub const DEK_WRAP_AAD: &[u8] = b"sdkwork.deploy.certificate-material.dek.v1";

/// The custody root that data keys are wrapped by.
pub trait MaterialKeyProvider: Send + Sync {
    /// Stable identifier recorded on every sealed file, so a rotation can find
    /// the rows it still owns.
    fn kek_ref(&self) -> &str;

    /// Seals a data key. The returned blob is opaque to callers; its layout is
    /// this provider's business.
    fn wrap_dek(&self, dek: &[u8], aad: &[u8]) -> MaterialResult<Vec<u8>>;

    /// Unseals a data key previously returned by [`Self::wrap_dek`].
    fn unwrap_dek(&self, wrapped: &[u8], aad: &[u8]) -> MaterialResult<Zeroizing<Vec<u8>>>;
}

/// A master key read from a protected file.
///
/// The file may hold 32 raw bytes or 64 hex characters; operators generate
/// these with `openssl rand -out` and `openssl rand -hex` about equally often.
pub struct FileKeyProvider {
    kek_ref: String,
    kek: Zeroizing<[u8; 32]>,
}

impl FileKeyProvider {
    /// Loads the master key from disk.
    pub fn from_file(path: impl AsRef<Path>) -> MaterialResult<Self> {
        let path = path.as_ref();
        let raw = std::fs::read(path).map_err(|error| {
            MaterialError::Custody(format!(
                "the master key at {} could not be read: {error}",
                path.display()
            ))
        })?;
        Self::from_material(&raw, format!("file:{}", path.display()))
    }

    /// Builds a provider from key material already in memory.
    ///
    /// Tests use this; production reads [`Self::from_file`] so the key never
    /// passes through configuration.
    pub fn from_material(raw: &[u8], kek_ref: impl Into<String>) -> MaterialResult<Self> {
        let compact: Vec<u8> = raw
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        let kek = if compact.len() == 64 && compact.iter().all(u8::is_ascii_hexdigit) {
            let mut out = [0u8; 32];
            for (index, pair) in compact.chunks_exact(2).enumerate() {
                out[index] = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
            }
            out
        } else if raw.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(raw);
            out
        } else {
            return Err(MaterialError::Custody(format!(
                "the master key must be 32 raw bytes or 64 hex characters, found {} bytes",
                raw.len()
            )));
        };
        Ok(Self {
            kek_ref: kek_ref.into(),
            kek: Zeroizing::new(kek),
        })
    }

    fn cipher(&self) -> MaterialResult<Aes256Gcm> {
        Aes256Gcm::new_from_slice(&self.kek[..])
            .map_err(|_| MaterialError::Custody("the master key is not 32 bytes".to_owned()))
    }
}

impl std::fmt::Debug for FileKeyProvider {
    /// Prints the key's identity but never the key.
    ///
    /// A derived `Debug` would render the master key into any log line, panic
    /// message, or test failure that happens to format a provider — which is a
    /// one-accident route around the whole custody model.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileKeyProvider")
            .field("kek_ref", &self.kek_ref)
            .field("kek", &"<redacted>")
            .finish()
    }
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

impl MaterialKeyProvider for FileKeyProvider {
    fn kek_ref(&self) -> &str {
        &self.kek_ref
    }

    fn wrap_dek(&self, dek: &[u8], aad: &[u8]) -> MaterialResult<Vec<u8>> {
        let cipher = self.cipher()?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let sealed = cipher
            .encrypt(&nonce, Payload { msg: dek, aad })
            .map_err(|_| MaterialError::Custody("the data key could not be wrapped".to_owned()))?;
        let mut out = Vec::with_capacity(nonce.len() + sealed.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    fn unwrap_dek(&self, wrapped: &[u8], aad: &[u8]) -> MaterialResult<Zeroizing<Vec<u8>>> {
        const NONCE_LEN: usize = 12;
        if wrapped.len() <= NONCE_LEN {
            return Err(MaterialError::Custody(
                "the wrapped data key is truncated".to_owned(),
            ));
        }
        let (nonce, ciphertext) = wrapped.split_at(NONCE_LEN);
        let cipher = self.cipher()?;
        let dek = cipher
            .decrypt(
                aes_gcm::aead::Nonce::<Aes256Gcm>::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| {
                MaterialError::Custody(
                    "the wrapped data key did not authenticate under the configured master key"
                        .to_owned(),
                )
            })?;
        Ok(Zeroizing::new(dek))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(seed: u8) -> FileKeyProvider {
        FileKeyProvider::from_material(&[seed; 32], "file:test").expect("provider")
    }

    #[test]
    fn wraps_and_unwraps_a_data_key() {
        let provider = provider(11);
        let dek = [9u8; 32];
        let wrapped = provider.wrap_dek(&dek, DEK_WRAP_AAD).expect("wrap");
        assert_ne!(wrapped.as_slice(), dek.as_slice(), "the DEK must be sealed");
        let opened = provider.unwrap_dek(&wrapped, DEK_WRAP_AAD).expect("unwrap");
        assert_eq!(opened.as_slice(), dek.as_slice());
        assert_eq!(provider.kek_ref(), "file:test");
    }

    #[test]
    fn accepts_a_hex_master_key_of_the_same_length() {
        let hex_text = "0f".repeat(32);
        let from_hex =
            FileKeyProvider::from_material(hex_text.as_bytes(), "file:hex").expect("hex");
        let from_raw = FileKeyProvider::from_material(&[0x0fu8; 32], "file:raw").expect("raw");
        let dek = [3u8; 32];
        let wrapped = from_hex.wrap_dek(&dek, DEK_WRAP_AAD).expect("wrap");
        // Same key material, so the other spelling must open it.
        let opened = from_raw.unwrap_dek(&wrapped, DEK_WRAP_AAD).expect("unwrap");
        assert_eq!(opened.as_slice(), dek.as_slice());
    }

    #[test]
    fn rejects_a_master_key_of_the_wrong_size() {
        let error = FileKeyProvider::from_material(&[1u8; 16], "file:short").expect_err("reject");
        assert!(
            format!("{error}").contains("32 raw bytes or 64 hex"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_different_master_key_cannot_unwrap_the_data_key() {
        let wrapped = provider(1)
            .wrap_dek(&[5u8; 32], DEK_WRAP_AAD)
            .expect("wrap");
        let error = provider(2)
            .unwrap_dek(&wrapped, DEK_WRAP_AAD)
            .expect_err("must refuse");
        assert!(
            format!("{error}").contains("did not authenticate"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_wrapped_key_is_bound_to_its_aad() {
        let provider = provider(4);
        let wrapped = provider.wrap_dek(&[7u8; 32], DEK_WRAP_AAD).expect("wrap");
        let error = provider
            .unwrap_dek(&wrapped, b"some other context")
            .expect_err("must refuse");
        assert!(format!("{error}").contains("did not authenticate"));
    }
}

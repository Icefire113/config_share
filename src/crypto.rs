//! Key derivation and authenticated encryption for uploaded file contents.
//!
//! The encryption key is derived from a user supplied passphrase with Argon2id.
//! Every encrypted file carries the salt that was used for it, so a machine with
//! the same passphrase can decrypt files that were uploaded by another machine
//! without having to copy any other state across.

use std::fmt;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use thiserror::Error;
use zeroize::Zeroizing;

/// Length of the Argon2 salt, in bytes.
pub(crate) const SALT_LEN: usize = 16;
/// Length of the derived encryption key, in bytes.
pub(crate) const KEY_LEN: usize = 32;
/// Length of the XChaCha20-Poly1305 nonce, in bytes.
pub(crate) const NONCE_LEN: usize = 24;

/// Argon2id parameters. The defaults are the OWASP recommended minimum: 19 MiB
/// of memory, 2 passes and one lane.
const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;

/// An encryption key derived from the user's passphrase.
///
/// The key is zeroed when it is dropped and never appears in debug output.
#[derive(Clone)]
pub(crate) struct EncryptionKey(Zeroizing<[u8; KEY_LEN]>);

impl EncryptionKey {
    fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EncryptionKey([redacted])")
    }
}

#[derive(Debug, Error)]
pub(crate) enum CryptoError {
    #[error("failed to derive the encryption key: {0}")]
    KeyDerivation(#[from] argon2::Error),
    #[error("failed to generate random bytes: {0}")]
    Random(#[from] getrandom::Error),
    #[error("failed to encrypt the file contents")]
    Encrypt,
    #[error(
        "failed to decrypt the file contents, the passphrase is wrong or the file was modified"
    )]
    Decrypt,
}

/// Derives the encryption key for a passphrase and salt.
pub(crate) fn derive_key(
    passphrase: &str,
    salt: &[u8; SALT_LEN],
) -> Result<EncryptionKey, CryptoError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(KEY_LEN),
    )?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key[..])?;

    Ok(EncryptionKey(key))
}

/// Fills an array with cryptographically secure random bytes.
pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)?;
    Ok(bytes)
}

/// Encrypts `plaintext` with XChaCha20-Poly1305 under the given nonce.
///
/// The nonce must never be reused with the same key for different plaintexts;
/// callers are expected to generate it with [`random_bytes`].
///
/// `aad` is authenticated but not encrypted; it must contain every header field
/// that changes how the ciphertext is interpreted (format version, KDF, salt and
/// nonce), so that tampering with them makes decryption fail.
pub(crate) fn encrypt(
    key: &EncryptionKey,
    nonce: &[u8; NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .expect("the key length is always valid for XChaCha20-Poly1305");

    cipher
        .encrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encrypt)
}

/// Decrypts `ciphertext` and verifies its authentication tag.
pub(crate) fn decrypt(
    key: &EncryptionKey,
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .expect("the key length is always valid for XChaCha20-Poly1305");

    cipher
        .decrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(salt: &[u8; SALT_LEN]) -> EncryptionKey {
        derive_key("correct horse battery staple", salt).unwrap()
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let salt = [7u8; SALT_LEN];
        let key = test_key(&salt);
        let nonce = random_bytes::<NONCE_LEN>().unwrap();

        let ciphertext = encrypt(&key, &nonce, b"some config", b"header").unwrap();
        let plaintext = decrypt(&key, &nonce, &ciphertext, b"header").unwrap();

        assert_eq!(plaintext, b"some config");
    }

    #[test]
    fn same_plaintext_with_different_nonces_encrypts_differently() {
        let salt = [7u8; SALT_LEN];
        let key = test_key(&salt);

        let first = encrypt(
            &key,
            &random_bytes::<NONCE_LEN>().unwrap(),
            b"some config",
            b"header",
        )
        .unwrap();
        let second = encrypt(
            &key,
            &random_bytes::<NONCE_LEN>().unwrap(),
            b"some config",
            b"header",
        )
        .unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let salt = [7u8; SALT_LEN];
        let key = test_key(&salt);
        let nonce = random_bytes::<NONCE_LEN>().unwrap();
        let ciphertext = encrypt(&key, &nonce, b"some config", b"header").unwrap();

        let wrong = derive_key("not the passphrase", &salt).unwrap();

        assert!(decrypt(&wrong, &nonce, &ciphertext, b"header").is_err());
    }

    #[test]
    fn wrong_aad_fails() {
        let salt = [7u8; SALT_LEN];
        let key = test_key(&salt);
        let nonce = random_bytes::<NONCE_LEN>().unwrap();
        let ciphertext = encrypt(&key, &nonce, b"some config", b"header").unwrap();

        assert!(decrypt(&key, &nonce, &ciphertext, b"other header").is_err());
    }

    #[test]
    fn different_salts_derive_different_keys() {
        assert_ne!(
            test_key(&[1u8; SALT_LEN]).as_bytes(),
            test_key(&[2u8; SALT_LEN]).as_bytes()
        );
    }
}

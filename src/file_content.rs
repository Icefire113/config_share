//! The on-the-wire representation of a file that is uploaded to the server.
//!
//! Every payload starts with a small header so that the client can tell plaintext
//! and encrypted uploads apart. The encrypted variant carries the Argon2 salt and
//! the AEAD nonce, which are also authenticated as associated data.
//!
//! Layout:
//!
//! ```text
//! magic      8 bytes  "CSHARE01"
//! kind       1 byte   0 = plaintext, 1 = encrypted
//! plaintext: the raw file contents follow the header
//! encrypted: version (1), kdf (1), salt (16), nonce (24), ciphertext + tag
//! ```

use thiserror::Error;

use crate::crypto::{self, CryptoError, EncryptionKey, NONCE_LEN, SALT_LEN};

const MAGIC: &[u8; 8] = b"CSHARE01";
const KIND_PLAINTEXT: u8 = 0;
const KIND_ENCRYPTED: u8 = 1;

const ENCRYPTION_VERSION: u8 = 1;
const KDF_ARGON2ID: u8 = 1;

/// `magic` + `kind` + `version` + `kdf` + `salt` + `nonce`
const ENCRYPTED_HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 1 + SALT_LEN + NONCE_LEN;
/// `magic` + `kind`
const PLAINTEXT_HEADER_LEN: usize = MAGIC.len() + 1;
/// An XChaCha20-Poly1305 authentication tag
const TAG_LEN: usize = 16;

/// A file as it is stored on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileContent {
    /// The raw file contents, used when encryption is not enabled on this machine.
    Plaintext(Vec<u8>),
    /// The encrypted file contents plus everything needed to decrypt them.
    Encrypted(EncryptedFile),
}

/// An encrypted file payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncryptedFile {
    /// The Argon2 salt that the encryption key was derived with.
    pub(crate) salt: [u8; SALT_LEN],
    /// The XChaCha20-Poly1305 nonce.
    pub(crate) nonce: [u8; NONCE_LEN],
    /// The ciphertext, including the Poly1305 authentication tag.
    pub(crate) ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub(crate) enum FileContentError {
    #[error("the file is too small to be a config_share payload")]
    TooShort,
    #[error(
        "the file was not uploaded by this version of config_share (unrecognised header), upload it again with `config_share push`"
    )]
    BadMagic,
    #[error("unknown payload kind {0}")]
    UnknownKind(u8),
    #[error("unsupported encryption version {0}")]
    UnsupportedVersion(u8),
    #[error("unsupported key derivation function {0}")]
    UnsupportedKdf(u8),
}

impl EncryptedFile {
    /// Encrypts `plaintext` with a fresh random nonce.
    pub(crate) fn seal(
        plaintext: &[u8],
        salt: [u8; SALT_LEN],
        key: &EncryptionKey,
    ) -> Result<Self, CryptoError> {
        let nonce = crypto::random_bytes::<NONCE_LEN>()?;
        let aad = encrypted_header(&salt, &nonce);
        let ciphertext = crypto::encrypt(key, &nonce, plaintext, &aad)?;

        Ok(Self {
            salt,
            nonce,
            ciphertext,
        })
    }

    /// Decrypts the file contents and verifies the authentication tag.
    pub(crate) fn open(&self, key: &EncryptionKey) -> Result<Vec<u8>, CryptoError> {
        let aad = encrypted_header(&self.salt, &self.nonce);
        crypto::decrypt(key, &self.nonce, &self.ciphertext, &aad)
    }
}

impl FileContent {
    /// Serialises the payload into the bytes that are uploaded to the server.
    pub(crate) fn encode(&self) -> Vec<u8> {
        match self {
            Self::Plaintext(contents) => {
                let mut bytes = Vec::with_capacity(PLAINTEXT_HEADER_LEN + contents.len());
                bytes.extend_from_slice(MAGIC);
                bytes.push(KIND_PLAINTEXT);
                bytes.extend_from_slice(contents);
                bytes
            }
            Self::Encrypted(file) => {
                let mut bytes = encrypted_header(&file.salt, &file.nonce);
                bytes.extend_from_slice(&file.ciphertext);
                bytes
            }
        }
    }

    /// Parses a payload that was downloaded from the server.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, FileContentError> {
        if bytes.len() < PLAINTEXT_HEADER_LEN {
            return Err(FileContentError::TooShort);
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(FileContentError::BadMagic);
        }

        match bytes[MAGIC.len()] {
            KIND_PLAINTEXT => Ok(Self::Plaintext(bytes[PLAINTEXT_HEADER_LEN..].to_vec())),
            KIND_ENCRYPTED => {
                if bytes.len() < ENCRYPTED_HEADER_LEN {
                    return Err(FileContentError::TooShort);
                }

                let mut offset = PLAINTEXT_HEADER_LEN;
                let version = bytes[offset];
                offset += 1;
                let kdf = bytes[offset];
                offset += 1;

                if version != ENCRYPTION_VERSION {
                    return Err(FileContentError::UnsupportedVersion(version));
                }
                if kdf != KDF_ARGON2ID {
                    return Err(FileContentError::UnsupportedKdf(kdf));
                }

                let mut salt = [0u8; SALT_LEN];
                salt.copy_from_slice(&bytes[offset..offset + SALT_LEN]);
                offset += SALT_LEN;

                let mut nonce = [0u8; NONCE_LEN];
                nonce.copy_from_slice(&bytes[offset..offset + NONCE_LEN]);
                offset += NONCE_LEN;

                let ciphertext = &bytes[offset..];
                if ciphertext.len() < TAG_LEN {
                    return Err(FileContentError::TooShort);
                }

                Ok(Self::Encrypted(EncryptedFile {
                    salt,
                    nonce,
                    ciphertext: ciphertext.to_vec(),
                }))
            }
            kind => Err(FileContentError::UnknownKind(kind)),
        }
    }
}

/// The authenticated header of an encrypted payload.
fn encrypted_header(salt: &[u8; SALT_LEN], nonce: &[u8; NONCE_LEN]) -> Vec<u8> {
    let mut header = Vec::with_capacity(ENCRYPTED_HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(KIND_ENCRYPTED);
    header.push(ENCRYPTION_VERSION);
    header.push(KDF_ARGON2ID);
    header.extend_from_slice(salt);
    header.extend_from_slice(nonce);
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> EncryptionKey {
        crypto::derive_key("correct horse battery staple", &[9u8; SALT_LEN]).unwrap()
    }

    #[test]
    fn plaintext_roundtrip() {
        let content = FileContent::Plaintext(b"some config".to_vec());
        let encoded = content.encode();

        assert_eq!(&encoded[..MAGIC.len()], MAGIC);
        assert_eq!(encoded[MAGIC.len()], KIND_PLAINTEXT);
        assert_eq!(FileContent::decode(&encoded).unwrap(), content);
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let content = FileContent::Plaintext(Vec::new());
        let encoded = content.encode();

        assert_eq!(encoded.len(), PLAINTEXT_HEADER_LEN);
        assert_eq!(FileContent::decode(&encoded).unwrap(), content);
    }

    #[test]
    fn encrypted_roundtrip() {
        let content = FileContent::Encrypted(
            EncryptedFile::seal(b"some config", [3u8; SALT_LEN], &key()).unwrap(),
        );
        let encoded = content.encode();

        assert_eq!(
            encoded.len(),
            ENCRYPTED_HEADER_LEN + b"some config".len() + TAG_LEN
        );

        let decoded = FileContent::decode(&encoded).unwrap();
        let FileContent::Encrypted(file) = decoded else {
            panic!("expected an encrypted payload");
        };

        assert_eq!(file.salt, [3u8; SALT_LEN]);
        assert_eq!(file.open(&key()).unwrap(), b"some config");
    }

    #[test]
    fn encrypted_payload_does_not_contain_the_plaintext() {
        let content = FileContent::Encrypted(
            EncryptedFile::seal(b"super secret value", [3u8; SALT_LEN], &key()).unwrap(),
        );
        let encoded = content.encode();

        assert!(
            !encoded
                .windows(b"super secret value".len())
                .any(|w| w == b"super secret value")
        );
    }

    #[test]
    fn tampered_ciphertext_fails_to_open() {
        let content = FileContent::Encrypted(
            EncryptedFile::seal(b"some config", [3u8; SALT_LEN], &key()).unwrap(),
        );
        let mut encoded = content.encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;

        let FileContent::Encrypted(file) = FileContent::decode(&encoded).unwrap() else {
            panic!("expected an encrypted payload");
        };

        assert!(file.open(&key()).is_err());
    }

    #[test]
    fn tampered_salt_fails_to_open() {
        let content = FileContent::Encrypted(
            EncryptedFile::seal(b"some config", [3u8; SALT_LEN], &key()).unwrap(),
        );
        let mut encoded = content.encode();
        // The salt starts after the magic, kind, version and KDF bytes
        encoded[MAGIC.len() + 3] ^= 0xff;

        let FileContent::Encrypted(file) = FileContent::decode(&encoded).unwrap() else {
            panic!("expected an encrypted payload");
        };

        assert!(file.open(&key()).is_err());
    }

    #[test]
    fn legacy_payload_is_rejected() {
        assert!(matches!(
            FileContent::decode(b"a raw file without a header"),
            Err(FileContentError::BadMagic)
        ));
    }

    #[test]
    fn truncated_payloads_are_rejected() {
        assert!(matches!(
            FileContent::decode(b"CSHAR"),
            Err(FileContentError::TooShort)
        ));
        assert!(matches!(
            FileContent::decode(b"CSHARE01\x01"),
            Err(FileContentError::TooShort)
        ));
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        assert!(matches!(
            FileContent::decode(b"CSHARE01\x02"),
            Err(FileContentError::UnknownKind(2))
        ));
    }
}

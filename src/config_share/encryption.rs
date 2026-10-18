//! Encryption setup and the conversion between local plaintext and the payload
//! that is uploaded to the server.
//!
//! The passphrase is cached in the OS keychain, so a sync normally does not prompt.
//! Derived keys are cached in memory for the lifetime of the process, keyed by the
//! salt embedded in each payload; that keeps a sync down to at most one Argon2 run
//! per machine that uploaded an encrypted file.

use anyhow::{Context, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use console::style;
use dialoguer::{Confirm, Password};
use zeroize::Zeroizing;

use crate::{
    config_share::ConfigShare,
    crypto::{self, EncryptionKey, SALT_LEN},
    file_content::{EncryptedFile, FileContent},
    secret_store,
    share_settings::EncryptionSettings,
};

/// What `config_share encrypt` should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncryptionAction {
    /// Set or replace the passphrase and enable encryption for new uploads.
    Setup,
    /// Remove the passphrase from the OS keychain, keeping encryption enabled.
    Forget,
    /// Stop encrypting new uploads and remove the cached passphrase.
    Disable,
}

impl ConfigShare {
    /// Encrypts `plaintext` for upload, or wraps it as plaintext when encryption
    /// has not been enabled for this machine.
    pub(crate) fn encode_for_upload(&self, plaintext: &[u8]) -> anyhow::Result<FileContent> {
        let Some(encryption) = &self.settings.encryption else {
            return Ok(FileContent::Plaintext(plaintext.to_vec()));
        };

        let salt = decode_salt(&encryption.salt)?;
        let key = self.key_for_salt(&salt)?;
        let file = EncryptedFile::seal(plaintext, salt, &key)
            .context("Failed to encrypt the file contents")?;

        Ok(FileContent::Encrypted(file))
    }

    /// Turns a payload downloaded from the server back into the local plaintext,
    /// decrypting it if necessary.
    pub(crate) fn decode_download(&self, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        match FileContent::decode(bytes)? {
            FileContent::Plaintext(contents) => Ok(contents),
            FileContent::Encrypted(file) => {
                let key = self.key_for_salt(&file.salt)?;
                file.open(&key)
                    .context("Failed to decrypt the file contents")
            }
        }
    }

    /// Sets up, forgets or disables encryption for this machine.
    pub fn configure_encryption(&mut self, action: EncryptionAction) -> anyhow::Result<()> {
        let server_url = self.server_url()?;

        match action {
            EncryptionAction::Setup => {
                let has_cached_passphrase =
                    secret_store::load_encryption_passphrase(&server_url)?.is_some();
                if has_cached_passphrase {
                    let replace = Confirm::new()
                        .with_prompt(
                            "An encryption passphrase is already saved for this server. Replace it? Files uploaded with the old passphrase stay unreadable with the new one",
                        )
                        .default(false)
                        .interact()?;
                    if !replace {
                        println!("{}", style("Kept the existing passphrase.").yellow());
                        return Ok(());
                    }
                }

                let passphrase = prompt_new_passphrase()?;

                // Keep the existing salt so that files uploaded before re-entering
                // the same passphrase remain readable. Changing the passphrase
                // changes the key, so warn about files uploaded with the old one.
                if self.settings.encryption.is_none() {
                    let salt = crypto::random_bytes::<SALT_LEN>()
                        .context("Failed to generate an encryption salt")?;
                    self.settings.encryption = Some(EncryptionSettings {
                        salt: BASE64.encode(salt),
                    });
                }

                secret_store::save_encryption_passphrase(&server_url, &passphrase)?;

                // Don't leave a passphrase in the keychain that the config file
                // doesn't know about if the settings cannot be saved.
                if let Err(e) = self.save_settings() {
                    let _ = secret_store::delete_encryption_passphrase(&server_url);
                    return Err(e);
                }

                *self.passphrase.borrow_mut() = Some(passphrase);
                self.keys.borrow_mut().clear();

                println!(
                    "{} uploads from this machine will be encrypted",
                    style("Encryption enabled:").green().bold()
                );
                println!(
                    "Use the same passphrase on your other machines, otherwise they cannot read these files."
                );
                println!(
                    "Changing the passphrase later makes files that were uploaded with the old one unreadable."
                );
                println!(
                    "Files that are already on the server stay exactly as they are until they are uploaded again; run `config_share push --force` to encrypt them now."
                );
            }
            EncryptionAction::Forget => {
                let forget = Confirm::new()
                    .with_prompt(
                        "Forget the cached encryption passphrase? Encrypted files on the server still need it, and you will have to enter it again",
                    )
                    .default(false)
                    .interact()?;
                if !forget {
                    println!("{}", style("Kept the cached passphrase.").yellow());
                    return Ok(());
                }

                secret_store::delete_encryption_passphrase(&server_url)?;
                self.passphrase.borrow_mut().take();
                self.keys.borrow_mut().clear();

                println!(
                    "{}",
                    style("Forgot the cached encryption passphrase.").green()
                );
                if self.settings.encryption.is_some() {
                    println!(
                        "Uploads stay encrypted; you will be asked for the passphrase the next time it is needed."
                    );
                }
            }
            EncryptionAction::Disable => {
                let was_enabled = self.settings.encryption.take().is_some();
                let had_passphrase =
                    secret_store::load_encryption_passphrase(&server_url)?.is_some();

                if !was_enabled && !had_passphrase {
                    println!("{}", style("Encryption is not enabled.").yellow());
                    return Ok(());
                }

                self.save_settings()?;
                secret_store::delete_encryption_passphrase(&server_url)?;
                self.passphrase.borrow_mut().take();
                self.keys.borrow_mut().clear();

                println!("{}", style("Encryption disabled for new uploads.").green());
                if was_enabled {
                    println!(
                        "Files that are already encrypted on the server stay encrypted; you will still be asked for the passphrase to download them."
                    );
                }
            }
        }

        Ok(())
    }

    /// The passphrase for this run, loaded from the OS keychain or prompted for.
    ///
    /// It is cached so that a multi-file operation asks at most once.
    fn encryption_passphrase(&self, reason: &str) -> anyhow::Result<Zeroizing<String>> {
        if let Some(passphrase) = self.passphrase.borrow().as_ref() {
            return Ok(passphrase.clone());
        }

        let server_url = self.server_url()?;
        let passphrase = match secret_store::load_encryption_passphrase(&server_url)? {
            Some(passphrase) => passphrase,
            None => {
                println!("{reason}");

                let passphrase = Zeroizing::new(
                    Password::new()
                        .with_prompt("Encryption passphrase")
                        .interact()?,
                );
                if passphrase.trim().is_empty() {
                    bail!("The encryption passphrase is empty");
                }

                let save = Confirm::new()
                    .with_prompt("Save the passphrase in the OS keychain?")
                    .default(true)
                    .interact()?;
                if save {
                    secret_store::save_encryption_passphrase(&server_url, &passphrase)?;
                }

                passphrase
            }
        };

        *self.passphrase.borrow_mut() = Some(passphrase.clone());
        Ok(passphrase)
    }

    /// Derives (or returns the cached) key for a payload's salt.
    fn key_for_salt(&self, salt: &[u8; SALT_LEN]) -> anyhow::Result<EncryptionKey> {
        if let Some(key) = self.keys.borrow().get(salt) {
            return Ok(key.clone());
        }

        let passphrase = self.encryption_passphrase(
            "A file needs the encryption passphrase. Use the passphrase that was set with `config_share encrypt` on your other machines.",
        )?;
        let key =
            crypto::derive_key(&passphrase, salt).context("Failed to derive the encryption key")?;

        self.keys.borrow_mut().insert(*salt, key.clone());
        Ok(key)
    }
}

/// Prompts for a new passphrase twice and checks the minimum length.
fn prompt_new_passphrase() -> anyhow::Result<Zeroizing<String>> {
    loop {
        let passphrase = Zeroizing::new(
            Password::new()
                .with_prompt("Enter a new encryption passphrase")
                .with_confirmation(
                    "Confirm the encryption passphrase",
                    "The passphrases do not match",
                )
                .interact()?,
        );

        if passphrase.chars().count() < 8 {
            println!(
                "{}",
                style("The passphrase must be at least 8 characters long.").yellow()
            );
            continue;
        }

        return Ok(passphrase);
    }
}

/// Decodes the salt stored in the config file.
fn decode_salt(encoded: &str) -> anyhow::Result<[u8; SALT_LEN]> {
    let bytes = BASE64
        .decode(encoded)
        .context("The encryption salt in the config file is not valid base64")?;

    bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!(
            "The encryption salt in the config file has the wrong length (expected {} bytes, got {})",
            SALT_LEN,
            bytes.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{file_content::FileContent, share_settings::ShareSettings};

    fn app(passphrase: &str, encryption: Option<EncryptionSettings>) -> ConfigShare {
        let settings = ShareSettings {
            encryption,
            ..ShareSettings::default()
        };

        let app = ConfigShare {
            settings,
            ..ConfigShare::default()
        };

        *app.passphrase.borrow_mut() = Some(Zeroizing::new(passphrase.to_string()));
        app
    }

    fn encryption_settings(salt: [u8; SALT_LEN]) -> EncryptionSettings {
        EncryptionSettings {
            salt: BASE64.encode(salt),
        }
    }

    #[test]
    fn uploads_are_plaintext_without_encryption() {
        let app = app("", None);

        assert!(matches!(
            app.encode_for_upload(b"some config").unwrap(),
            FileContent::Plaintext(_)
        ));
    }

    #[test]
    fn encrypted_roundtrip() {
        let app = app(
            "correct horse battery staple",
            Some(encryption_settings([1u8; SALT_LEN])),
        );

        let encoded = app.encode_for_upload(b"some config").unwrap().encode();
        assert_eq!(app.decode_download(&encoded).unwrap(), b"some config");
    }

    #[test]
    fn another_machine_can_decrypt_without_knowing_the_salt() {
        let first = app(
            "correct horse battery staple",
            Some(encryption_settings([1u8; SALT_LEN])),
        );
        let encoded = first.encode_for_upload(b"some config").unwrap().encode();

        // The second machine has the same passphrase but its own salt (or none at
        // all); the salt needed to decrypt travels inside the payload.
        let second = app(
            "correct horse battery staple",
            Some(encryption_settings([9u8; SALT_LEN])),
        );

        assert_eq!(second.decode_download(&encoded).unwrap(), b"some config");
    }

    #[test]
    fn decrypting_with_the_wrong_passphrase_fails() {
        let first = app(
            "correct horse battery staple",
            Some(encryption_settings([1u8; SALT_LEN])),
        );
        let encoded = first.encode_for_upload(b"some config").unwrap().encode();

        let second = app("not the passphrase", None);

        assert!(second.decode_download(&encoded).is_err());
    }

    #[test]
    fn legacy_plaintext_payload_is_rejected() {
        let app = app("correct horse battery staple", None);

        assert!(
            app.decode_download(b"raw bytes from an old upload")
                .is_err()
        );
    }
}

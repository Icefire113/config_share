//! Secrets that live in the operating system keychain rather than the config file.
//!
//! Both the access token and the encryption passphrase are scoped to the server,
//! so that different servers do not overwrite each other's entries.

use anyhow::{Context, anyhow};
use keyring::{Entry, Error as KeyringError};
use url::Url;
use zeroize::Zeroizing;

/// The service name used for all keychain entries created by config_share.
const KEYRING_SERVICE: &str = "config_share";

/// The keychain account that a value is stored under, scoped to the server so
/// that different servers do not overwrite each other. `purpose` is empty for
/// the access token, which keeps existing token entries working.
fn account(server_url: &Url, purpose: &str) -> String {
    let server = match (server_url.host_str(), server_url.port()) {
        (Some(host), Some(port)) => format!("{}:{}", host, port),
        (Some(host), None) => host.to_string(),
        (None, _) => server_url.to_string(),
    };

    if purpose.is_empty() {
        server
    } else {
        format!("{}:{}", server, purpose)
    }
}

/// Opens the keychain entry for the given server and purpose.
fn entry(server_url: &Url, purpose: &str) -> anyhow::Result<Entry> {
    Entry::new(KEYRING_SERVICE, &account(server_url, purpose)).with_context(|| {
        format!(
            "Failed to open the OS keychain entry for {}",
            account(server_url, purpose)
        )
    })
}

/// Saves a value for the given server and purpose in the OS keychain.
fn save(server_url: &Url, purpose: &str, value: &str) -> anyhow::Result<()> {
    entry(server_url, purpose)?
        .set_password(value)
        .with_context(|| {
            format!(
                "Failed to save the value in the OS keychain for {}",
                account(server_url, purpose)
            )
        })
}

/// Loads a value for the given server and purpose from the OS keychain, if there is one.
fn load(server_url: &Url, purpose: &str) -> anyhow::Result<Option<Zeroizing<String>>> {
    match entry(server_url, purpose)?.get_password() {
        Ok(value) => Ok(Some(Zeroizing::new(value))),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!(
            "Failed to read the value from the OS keychain for {}: {}",
            account(server_url, purpose),
            e
        )),
    }
}

/// Deletes a value for the given server and purpose from the OS keychain.
///
/// Deleting a value that is not there is not an error.
fn delete(server_url: &Url, purpose: &str) -> anyhow::Result<()> {
    match entry(server_url, purpose)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!(
            "Failed to delete the value from the OS keychain for {}: {}",
            account(server_url, purpose),
            e
        )),
    }
}

/// Saves the access token for the given server in the OS keychain.
pub fn save_token(server_url: &Url, token: &str) -> anyhow::Result<()> {
    save(server_url, "", token)
}

/// Loads the access token for the given server from the OS keychain, if there is one.
pub fn load_token(server_url: &Url) -> anyhow::Result<Option<String>> {
    Ok(load(server_url, "")?.map(|token| token.to_string()))
}

/// Deletes the access token for the given server from the OS keychain.
pub fn delete_token(server_url: &Url) -> anyhow::Result<()> {
    delete(server_url, "")
}

/// Saves the encryption passphrase for the given server in the OS keychain.
pub fn save_encryption_passphrase(server_url: &Url, passphrase: &str) -> anyhow::Result<()> {
    save(server_url, "encryption", passphrase)
}

/// Loads the encryption passphrase for the given server from the OS keychain, if
/// there is one.
pub fn load_encryption_passphrase(server_url: &Url) -> anyhow::Result<Option<Zeroizing<String>>> {
    load(server_url, "encryption")
}

/// Deletes the encryption passphrase for the given server from the OS keychain.
pub fn delete_encryption_passphrase(server_url: &Url) -> anyhow::Result<()> {
    delete(server_url, "encryption")
}

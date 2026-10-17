use anyhow::{Context, anyhow};
use keyring::{Entry, Error as KeyringError};
use url::Url;

/// The service name used for all keychain entries created by config_share.
const KEYRING_SERVICE: &str = "config_share";

/// The keychain account that a token is stored under. It is scoped to the server so
/// that the tokens for different servers do not overwrite each other.
fn account(server_url: &Url) -> String {
    match (server_url.host_str(), server_url.port()) {
        (Some(host), Some(port)) => format!("{}:{}", host, port),
        (Some(host), None) => host.to_string(),
        (None, _) => server_url.to_string(),
    }
}

/// Opens the keychain entry for the given server.
fn entry(server_url: &Url) -> anyhow::Result<Entry> {
    Entry::new(KEYRING_SERVICE, &account(server_url)).with_context(|| {
        format!(
            "Failed to open the OS keychain entry for {}",
            account(server_url)
        )
    })
}

/// Saves the access token for the given server in the OS keychain.
pub fn save_token(server_url: &Url, token: &str) -> anyhow::Result<()> {
    entry(server_url)?.set_password(token).with_context(|| {
        format!(
            "Failed to save the access token in the OS keychain for {}",
            account(server_url)
        )
    })
}

/// Loads the access token for the given server from the OS keychain, if there is one.
pub fn load_token(server_url: &Url) -> anyhow::Result<Option<String>> {
    match entry(server_url)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!(
            "Failed to read the access token from the OS keychain for {}: {}",
            account(server_url),
            e
        )),
    }
}

/// Deletes the access token for the given server from the OS keychain.
///
/// Deleting a token that is not there is not an error.
pub fn delete_token(server_url: &Url) -> anyhow::Result<()> {
    match entry(server_url)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!(
            "Failed to delete the access token from the OS keychain for {}: {}",
            account(server_url),
            e
        )),
    }
}

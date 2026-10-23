use anyhow::{Context, anyhow};
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    fs::{File, create_dir_all},
    io::{Read, Write},
    path::Path,
};
use url::Url;
use zeroize::Zeroizing;

use crate::{
    api::ApiClient,
    crypto::{EncryptionKey, SALT_LEN},
    secret_store,
    share_settings::ShareSettings,
};

mod add_file;
mod auth;
mod check_files;
pub(crate) mod encryption;
pub(crate) mod init;
mod list_files;
mod ping;
mod pull;
mod push;
mod remove;
mod startup_checks;
mod status;
mod sync;

pub(crate) use encryption::EncryptionAction;

pub struct ConfigShare {
    settings: ShareSettings,
    /// The encryption passphrase for this run, so a multi-file operation prompts
    /// at most once. It is never written to the config file or debug output.
    passphrase: RefCell<Option<Zeroizing<String>>>,
    /// Derived encryption keys for this run, keyed by the salt in each payload.
    keys: RefCell<HashMap<[u8; SALT_LEN], EncryptionKey>>,
}

impl fmt::Debug for ConfigShare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigShare")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Default for ConfigShare {
    /// Creates a new ConfigShare with the default setting
    fn default() -> Self {
        Self {
            settings: ShareSettings::default(),
            passphrase: RefCell::new(None),
            keys: RefCell::new(HashMap::new()),
        }
    }
}

impl ConfigShare {
    /// The url of the server that is configured to be synced with.
    pub(crate) fn server_url(&self) -> anyhow::Result<Url> {
        self.settings
            .server_url
            .clone()
            .context("Server url is not set")
    }

    /// Creates an API client for the configured server without any authentication,
    /// used for the endpoints that don't need a token (e.g. logging in).
    pub fn get_unauthenticated_api_client(&self) -> anyhow::Result<ApiClient> {
        Ok(ApiClient::new(self.server_url()?))
    }

    /// Creates an API client for the configured server, authenticated with the token
    /// stored in the OS keychain if the user has logged in.
    pub fn get_api_client(&self) -> anyhow::Result<ApiClient> {
        let server_url = self.server_url()?;
        let client = ApiClient::new(server_url.clone());

        match &self.settings.auth {
            Some(_) => {
                let token = secret_store::load_token(&server_url)?.with_context(|| {
                    anyhow!(
                        "No access token found in the OS keychain for {}, run `config_share login` again",
                        server_url
                    )
                })?;
                Ok(client.with_token(token))
            }
            None => Ok(client),
        }
    }

    /// Loads the settings from the config file
    pub fn load_settings(&mut self) -> anyhow::Result<()> {
        if !init::CONFIG_FILE_PATH.try_exists()? {
            return Err(anyhow!(
                "Config file does not exist, please use `config_share init` to set one up"
            ));
        }

        let mut file = File::options()
            .read(true)
            .write(false)
            .open(init::CONFIG_FILE_PATH.as_path())?;

        let mut buff = String::new();
        file.read_to_string(&mut buff)?;
        let mut settings: ShareSettings = serde_json::from_str(&buff)
            .with_context(|| "Failed to parse the saved json settings into a struct of the current settings, is the saved settings from an older version?")?;

        // Older versions stored Windows extended-length paths (`\\?\C:\...`) from
        // canonicalization. Reduce them to plain absolute paths once so that they
        // compare and display like the paths written by the current version.
        settings.tracked_files = settings
            .tracked_files
            .into_iter()
            .map(|(name, mut tracked)| {
                let name = {
                    let cleaned = dunce::simplified(Path::new(&name))
                        .to_string_lossy()
                        .to_string();
                    if cleaned != name {
                        tracked.name = cleaned.clone();
                    }
                    cleaned
                };
                tracked.file_path = dunce::simplified(&tracked.file_path).to_path_buf();
                for path in tracked.path_per_machine.values_mut() {
                    *path = dunce::simplified(path).to_path_buf();
                }
                (name, tracked)
            })
            .collect();
        self.settings = settings;

        Ok(())
    }

    /// Saves the settings in `self.settings` to the config file
    pub fn save_settings(&self) -> anyhow::Result<()> {
        create_dir_all(init::CONFIG_FILE_PATH.as_path().parent().unwrap())
            .with_context(|| "Failed to create parent dir(s) for config file")?;
        let mut file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(init::CONFIG_FILE_PATH.as_path())
            .with_context(|| "Failed to open the config file to save the settings")?;

        file.write(serde_json::to_string(&self.settings)?.as_bytes())
            .with_context(|| "Failed to write the settings into the config file")?;

        file.flush()?;
        file.sync_data()?;
        Ok(())
    }
}

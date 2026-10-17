use anyhow::{Context, anyhow};
use std::{
    fs::File,
    io::{Read, Write},
};
use url::Url;

use crate::{api::ApiClient, share_settings::ShareSettings, token_store};

mod add_file;
mod auth;
mod check_files;
mod init;
mod list_files;
mod ping;
mod pull;
mod push;
mod remove;
mod startup_checks;
mod status;
mod sync;

#[derive(Debug)]
pub struct ConfigShare {
    settings: ShareSettings,
}

impl Default for ConfigShare {
    /// Creates a new ConfigShare with the default setting
    fn default() -> Self {
        Self {
            settings: ShareSettings::default(),
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
                let token = token_store::load_token(&server_url)?.with_context(|| {
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
        self.settings = serde_json::from_str(&buff)
            .with_context(|| "Failed to parse the saved json settings into a struct of the current settings, is the saved settings from an older version?")?;

        Ok(())
    }

    /// Saves the settings in `self.settings` to the config file
    pub fn save_settings(&self) -> anyhow::Result<()> {
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

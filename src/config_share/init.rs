use anyhow::anyhow;
use gethostname::gethostname;
use lazy_static::lazy_static;
use log::trace;
use std::path::PathBuf;
use url::Url;

use crate::{config_share::ConfigShare, secret_store, share_settings::ShareSettings};

lazy_static! {
    #[derive(Debug)]
    /// The path to the config file for the program, location is `~/.config/config_share/config.json`
    pub static ref CONFIG_FILE_PATH: PathBuf = std::env::home_dir()
        .expect("User should have a home directory")
        .join(".config")
        .join("config_share")
        .join("config.json");
}

impl ConfigShare {
    /// Initialize config_share, creates the config file, and prompts the user to configure the app.
    pub(crate) fn initialize(&mut self, overwrite: bool) -> anyhow::Result<()> {
        trace!("[init.initialize] overwrite={}", overwrite);

        // Overwriting the config forgets the login, so remove the token that was stored
        // for the old config instead of leaving it behind in the keychain
        if overwrite
            && CONFIG_FILE_PATH.try_exists()?
            && self.load_settings().is_ok()
            && let Ok(server_url) = self.server_url()
        {
            let _ = secret_store::delete_token(&server_url);
            let _ = secret_store::delete_encryption_passphrase(&server_url);
        }

        self.create_config_file(overwrite)?;
        self.populate_config()?;

        if self.settings.auth.is_none() {
            println!("Run `config_share login` to authenticate with the server");
        }
        println!("Run `config_share encrypt` to encrypt files before they are uploaded");

        Ok(())
    }

    fn populate_config(&mut self) -> anyhow::Result<()> {
        self.populate_config_server()?;
        self.populate_config_machine_id()?;

        self.save_settings()?;
        Ok(())
    }

    fn populate_config_machine_id(&mut self) -> anyhow::Result<()> {
        let machine_id = dialoguer::Input::<String>::new()
            .allow_empty(false)
            .with_prompt("Enter the friendly name of the pc that you are using")
            .with_initial_text(
                gethostname()
                    .into_string()
                    .unwrap_or("MachineID".to_string()),
            )
            .interact_text()?;
        self.settings.machine_id = machine_id;
        Ok(())
    }

    fn populate_config_server(&mut self) -> anyhow::Result<()> {
        loop {
            let input = dialoguer::Input::<String>::new()
                .allow_empty(false)
                .with_prompt("Enter the server that you want to sync your files to")
                .with_initial_text("http://localhost:3000")
                .interact_text()?;

            trace!("[init.populate_config] input={}", input);

            let url = match Url::parse(&input) {
                Ok(url) => url,
                Err(e) => {
                    println!("That is not a valid URL ({}), try again", e);
                    continue;
                }
            };

            if url.scheme() != "http" && url.scheme() != "https" {
                println!("The server URL must start with http:// or https://, try again");
                continue;
            }

            if url.host_str().is_none() {
                println!("The server URL needs a host, e.g. http://localhost:3000, try again");
                continue;
            }

            trace!("[init.populate_config] Setting server_url={}", url);
            println!("Using server URL: {}", url);
            self.settings.server_url = Some(url);
            return Ok(());
        }
    }

    fn create_config_file(&mut self, overwrite: bool) -> anyhow::Result<()> {
        trace!(
            "[init.create_config_file] Using CONFIG_FILE_PATH={}",
            CONFIG_FILE_PATH.to_str().unwrap()
        );

        if !overwrite && CONFIG_FILE_PATH.try_exists()? {
            return Err(anyhow!(
                "config_share has already been initialized, use config_share init --overwrite to overwrite the config file"
            ));
        }

        self.settings = ShareSettings::default();

        self.save_settings()?;
        Ok(())
    }
}

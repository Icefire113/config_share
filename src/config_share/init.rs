use anyhow::anyhow;
use gethostname::gethostname;
use lazy_static::lazy_static;
use log::trace;
use std::path::PathBuf;
use url::{ParseError, Url};

use crate::{config_share::ConfigShare, share_settings::ShareSettings};

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
    pub(crate) fn initialize(&mut self, overwrite: bool, skip_setup: bool) -> anyhow::Result<()> {
        trace!(
            "[init.initialize] overwrite={}, skip_setup={}",
            overwrite, skip_setup
        );

        self.create_config_file(overwrite)?;
        if !skip_setup {
            self.populate_config()?;
        }

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
        let server_url: Url;
        loop {
            let input = dialoguer::Input::<String>::new()
                .allow_empty(false)
                .with_prompt("Enter the server that you want to sync your files to")
                .with_initial_text("http://localhost:3000")
                .interact_text()?;

            trace!("[init.populate_config] input={}", input);

            match Url::parse(&input) {
                Ok(u) => {
                    server_url = u;
                    trace!("[init.populate_config] Setting server_url={}", server_url);
                    break;
                }
                Err(ParseError::RelativeUrlWithoutBase) => {
                    return Err(anyhow!(
                        "Err: Server URL must have a base (e.g. http:// or https://)"
                    ));
                }
                Err(e) => {
                    return Err(anyhow!("Err: {}", e));
                }
            }
        }
        println!("Using server URL: {}", server_url);
        self.settings.server_url = Some(server_url);

        Ok(())
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

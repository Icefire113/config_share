use lazy_static::lazy_static;
use std::path::PathBuf;
use url::{ParseError, Url};
use util::dprint;

use crate::{config_share::ConfigShare, share_settings};

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
    pub(crate) fn initialize(
        &mut self,
        force: bool,
        skip_setup: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dprint(format!(
            "[init.initialize] force={}, skip_setup={}",
            force, skip_setup
        ));
        self.create_config_file(force)?;
        if !skip_setup {
            self.populate_config()?;
        }

        Ok(())
    }

    fn populate_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let server_url: Url;

        loop {
            let input = dialoguer::Input::<String>::new()
                .allow_empty(false)
                .with_prompt("Enter the server that you want to sync your files to")
                .clone()
                .interact_text()
                .unwrap();

            dprint(format!("[init.populate_config] input={}", input));

            match Url::parse(&input) {
                Ok(u) => {
                    server_url = u;
                    break;
                }
                Err(ParseError::RelativeUrlWithoutBase) => {
                    eprintln!("Err: Server URL must have a base (e.g. http:// or https://)")
                }
                Err(e) => {
                    dprint(format!("[init.populate_config] e={}", e));
                    println!("Err: {}", e)
                }
            }
        }
        println!("Using server URL: {}", server_url);
        self.settings.server_url = Some(server_url);

        Ok(())
    }

    fn create_config_file(&mut self, force: bool) -> Result<(), Box<dyn std::error::Error>> {
        dprint(format!(
            "[init.create_config_file] Using CONFIG_FILE_PATH={}",
            CONFIG_FILE_PATH.to_str().unwrap()
        ));

        if !force && CONFIG_FILE_PATH.try_exists()? {
            return Err(Box::from(
                "config_share has already been initialized, use config_share init --overwrite to overwrite the config file",
            ));
        }

        self.settings = share_settings::ShareSettings::default();

        self.save_settings()?;
        Ok(())
    }
}

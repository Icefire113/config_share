use std::{
    fs::File,
    io::{Read, Write},
};

use crate::share_settings::ShareSettings;

mod add_file;
mod init;
mod sync;

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
    /// Loads the settings from the config file
    pub fn load_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !init::CONFIG_FILE_PATH.try_exists().unwrap() {
            return Err(
                "Config file does not exist, please use `config_share init` to set one up".into(),
            );
        }

        let mut file = File::options()
            .read(true)
            .write(false)
            .open(init::CONFIG_FILE_PATH.as_path())?;

        let mut buff = String::new();
        file.read_to_string(&mut buff)?;
        let s: Result<ShareSettings, serde_json::Error> = serde_json::from_str(&buff);

        self.settings = s.unwrap();

        Ok(())
    }

    /// Saves the settings in `self.settings` to the config file
    pub fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(init::CONFIG_FILE_PATH.as_path())?;

        file.write(serde_json::to_string(&self.settings)?.as_bytes())?;

        file.flush()?;
        file.sync_data()?;
        Ok(())
    }

    pub fn check_initialized(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !init::CONFIG_FILE_PATH.try_exists().unwrap() {
            return Err(
                "Config file does not exist, please use `config_share init` to set one up".into(),
            );
        }
        Ok(())
    }
}

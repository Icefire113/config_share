use std::{fs::File, io::Write};

use crate::share_settings::ShareSettings;

mod init;
mod add_file;

pub struct ConfigShare {
    settings: ShareSettings,
}

impl Default for ConfigShare {
    fn default() -> Self {
        Self {
            settings: ShareSettings::default(),
        }
    }
}

impl ConfigShare {
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
}

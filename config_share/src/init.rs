use lazy_static::lazy_static;
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
};
use util::dprint;

use crate::share_settings;

lazy_static! {
    #[derive(Debug)]
    /// The path to the config file for the program, location is `~/.config/config_share/config.json`
    static ref CONFIG_FILE_PATH: PathBuf = std::env::home_dir()
        .expect("User should have a home directory")
        .join(".config")
        .join("config_share")
        .join("config.json");
}

/// Initialize config_share, creates the config file
pub(crate) fn initialize(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    dprint(format!("[init.initialize] force={}", force));
    create_config_file(force)?;
    Ok(())
}

fn create_config_file(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    dprint(format!(
        "[init.create_config_file] Using CONFIG_FILE_PATH={}",
        CONFIG_FILE_PATH.to_str().unwrap()
    ));

    if CONFIG_FILE_PATH.try_exists()? && !force {
        return Err(Box::from(
            "config_share has already been initialized, use config_share init --overwrite to overwrite the config file",
        ));
    }

    let mut file = File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(CONFIG_FILE_PATH.as_path())?;

    file.write(serde_json::to_string(&share_settings::ShareSettings::default())?.as_bytes())?;

    file.flush()?;
    file.sync_data()?;
    Ok(())
}

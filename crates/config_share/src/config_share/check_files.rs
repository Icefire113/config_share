use std::path::Path;

use anyhow::{Context, anyhow};
use util::{display_option, dprint, get_path_as_string};

use crate::config_share::ConfigShare;

impl ConfigShare {
    pub fn check_files(&self, file: Option<&String>) -> anyhow::Result<()> {
        dprint(format!(
            "[config_share.check_files] Checking file {}",
            display_option(file)
        ));
        if file.is_some() {
            let file = Path::new(file
                .with_context(|| anyhow!("[config_share.check_files] Failed to unwrap a value that we know is Some, what the fuck?"))?);

            if !file.try_exists()? || !file.is_file() {
                return Err(anyhow!(
                    "[config_share.check_files] File does not exist or is not a file"
                ));
            }

            if !self.settings.tracked_files.iter().any(|tracked_file| {
                tracked_file
                    .1
                    .path_per_machine
                    .get(&self.settings.machine_id)
                    .unwrap()
                    == file.canonicalize().unwrap().as_path()
            }) {
                return Err(anyhow!(
                    "The file {} is not currently tracked",
                    get_path_as_string(file)?
                ));
            }
        } else {
            // no file specified check all of them
        }
        Ok(())
    }
}

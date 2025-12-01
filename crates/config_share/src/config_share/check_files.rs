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
            let f = Path::new(file
                .with_context(|| anyhow!("[config_share.check_files] Failed to unwrap a value that we know is Some, what the fuck?"))?);

            if !f.try_exists()? || !f.is_file() {
                return Err(anyhow!(
                    "[config_share.check_files] File does not exist or is not a file"
                ));
            }

            if !self.settings.tracked_files.iter().any(|e| {
                e.1.path_per_machine.get(&self.settings.machine_id).unwrap()
                    == f.canonicalize().unwrap().as_path()
            }) {
                return Err(anyhow!(
                    "The file {} is not currently tracked",
                    get_path_as_string(f)?
                ));
            }
        } else {
            // no file specified check all of them
        }
        Ok(())
    }
}

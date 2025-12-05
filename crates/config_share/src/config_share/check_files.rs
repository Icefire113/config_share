use std::path::Path;

use anyhow::{Context, Ok, anyhow};
use util::{display_option, dprint, get_path_as_string};

use crate::{
    config_share::ConfigShare,
    net::{self, sync::is_remote_hash_same},
    tracked_file_info::TrackedFile,
};

impl ConfigShare {
    pub fn check_files(&self, file: Option<&String>) -> anyhow::Result<()> {
        dprint(format!(
            "[config_share.check_files] Checking file {}",
            display_option(file)
        ));
        if file.is_some() {
            let file = Path::new(file
                .with_context(|| anyhow!("[config_share.check_files] Failed to unwrap a value that we know is Some, what the fuck?")).unwrap());

            if !file.try_exists()? || !file.is_file() {
                return Err(anyhow!(
                    "[config_share.check_files] File does not exist or is not a file"
                ));
            }

            // If the file is not tracked then cant check it
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

            return self.check_single_file(
                self.settings
                    .get_tracked_file_from_path(&file.canonicalize()?)?,
                false,
            );
        } else {
            // no file specified check all of them
            for (name, tracked_file) in self.settings.tracked_files.iter() {
                self.check_single_file(tracked_file, true)
                    .with_context(|| {
                        anyhow!(
                            "[config_share.checkfiles] Failed to check single file: {}",
                            name
                        )
                    })?;
            }

            return Ok(());
        }
    }

    fn check_single_file(&self, file: &TrackedFile, batch: bool) -> anyhow::Result<()> {
        let r = is_remote_hash_same(file, &self.settings).with_context(|| {
            anyhow!(
                "[config_share.checkfiles] Failed to check single file: {}",
                file.file_path.display().to_string()
            )
        })?;

        if !r {
            println!(
                "[config_share.checkfiles] File {} is out of date",
                file.file_path.display().to_string()
            );
        } else if !batch {
            println!(
                "[config_share.checkfiles] File {} is up to date",
                file.file_path.display().to_string()
            );
        }

        return Ok(());
    }
}

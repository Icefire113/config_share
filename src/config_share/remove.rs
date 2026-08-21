use std::path::Path;

use crate::config_share::ConfigShare;
use crate::util::{get_path_as_string, get_pathbuf_as_string};
use anyhow::{Context, Ok, anyhow};
use log::debug;

impl ConfigShare {
    pub fn remove_file(&mut self, file_path: &String) -> anyhow::Result<()> {
        debug!(
            "[config_share.remove] Attempting to remove file: {}",
            file_path
        );
        let file_path = Path::new(file_path);

        if !file_path.try_exists()? || !file_path.is_file() {
            return Err(anyhow!(
                "{} is not a file or does not exist",
                get_path_as_string(&file_path)?
            ));
        }
        let file_path = file_path.canonicalize().with_context(|| {
            anyhow!(
                "[config_share.remove] Failed to get canonical path for a path that we know exists"
            )
        })?;

        let mut to_remove: Option<String> = None;
        for tracked_file in self.settings.tracked_files.iter() {
            let f = tracked_file
                .1
                .path_per_machine
                .get(&self.settings.machine_id);
            if f.is_some() {
                if get_path_as_string(f.unwrap())? == *file_path {
                    to_remove = Some(tracked_file.0.clone());
                    break;
                }
            }
        }

        if let Some(n) = to_remove {
            self.settings
                .tracked_files
                .remove(&n)
                .with_context(||
                    anyhow!("[config_share.remove] Failed to remove the file that we know exists, this is a bug")
                )?;
            self.save_settings()?;
            return Ok(());
        } else {
            return Err(anyhow!(
                "File {} was not found or is not currently tracked",
                get_pathbuf_as_string(&file_path)?
            ));
        }
    }
}

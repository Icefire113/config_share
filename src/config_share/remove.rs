use anyhow::{Context, anyhow, bail};
use console::style;
use log::trace;

use crate::{
    api::{ApiClientError, ApiErrorCode},
    config_share::ConfigShare,
    util::{absolute_path, paths_equal},
};

impl ConfigShare {
    pub fn remove_file(&mut self, file_path: &String, remote: bool) -> anyhow::Result<()> {
        trace!(
            "[config_share.remove] Attempting to remove file: {}, remote={}",
            file_path, remote
        );
        let path = absolute_path(file_path)?;

        if !path.try_exists()? {
            return Err(anyhow!(
                "{} is not a file or does not exist",
                path.display()
            ));
        }

        let to_remove = self
            .settings
            .tracked_files
            .iter()
            .find_map(|(name, tracked)| {
                (paths_equal(&tracked.file_path, &path)
                    || tracked
                        .path_per_machine
                        .get(&self.settings.machine_id)
                        .is_some_and(|machine_path| paths_equal(machine_path, &path)))
                .then(|| (name.clone(), tracked.id))
            });

        let Some((name, id)) = to_remove else {
            return Err(anyhow!(
                "File {} was not found or is not currently tracked",
                path.display()
            ));
        };

        if remote {
            match id {
                None => println!(
                    "{}",
                    style("warning: this file has never been synced, there is nothing to delete on the server")
                        .yellow()
                ),
                Some(id) => {
                    if self.settings.auth.is_none() {
                        bail!("You are not logged in, run `config_share login` first");
                    }

                    let client = self.get_api_client()?;

                    match client.delete_file(&id) {
                        Ok(()) => println!("Deleted {} from the server", name),
                        // Already gone is not a failure
                        Err(ApiClientError::Api {
                            code: ApiErrorCode::FileNotFound | ApiErrorCode::FileDeleted,
                            ..
                        }) => println!(
                            "{}",
                            style("warning: the file was already deleted on the server")
                                .yellow()
                        ),
                        Err(e) => {
                            return Err(e)
                                .with_context(|| format!("Failed to delete {} on the server", name));
                        }
                    }
                }
            }
        }

        self.settings.tracked_files.remove(&name).with_context(|| {
            anyhow!(
                "[config_share.remove] Failed to remove the file that we know exists, this is a bug"
            )
        })?;
        self.save_settings()?;
        println!("Stopped tracking {}", name);

        Ok(())
    }
}

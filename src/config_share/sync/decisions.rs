use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use api_types::get_file::PartialFileInfo;
use chrono::Utc;
use console::style;
use dialoguer::{Confirm, Input, Select};
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::{
    api::ApiClient,
    config_share::ConfigShare,
    sync_analysis::SyncPlan,
    tracked_file_info::TrackedFile,
    util::{absolute_path, paths_equal, write_file_atomic},
};

use super::{Outcome, plan_name, read_local_file};

impl ConfigShare {
    pub(super) fn resolve_remote_deleted(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        if yes {
            return Ok(Outcome::Skipped);
        }

        let name = plan_name(plan)?;
        let tracked = self.tracked(&name)?;
        let local_exists = tracked.file_path.try_exists()?;

        let choices = [
            "Delete the local file and stop tracking it",
            "Stop tracking it but keep the local file",
            "Upload the local file as a new remote file",
            "Skip this file",
        ];
        let choice = Select::new()
            .with_prompt(format!("{} was deleted on the server", plan.display_name))
            .items(choices)
            .default(3)
            .interact()?;

        match choice {
            0 => {
                if local_exists {
                    fs::remove_file(&tracked.file_path).with_context(|| {
                        format!("Failed to delete {}", tracked.file_path.display())
                    })?;
                }
                self.untrack(plan)?;
                Ok(Outcome::Untracked)
            }
            1 => {
                self.untrack(plan)?;
                Ok(Outcome::Untracked)
            }
            2 => {
                if !local_exists {
                    bail!("The local file is missing, there is nothing to upload");
                }
                let (bytes, hash) = read_local_file(&tracked.file_path)?;
                self.create_remote(client, &name, &bytes, hash)?;
                self.save_settings()?;
                Ok(Outcome::Pushed)
            }
            _ => Ok(Outcome::Skipped),
        }
    }

    pub(super) fn resolve_local_missing(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        let name = plan_name(plan)?;

        // Tracked but never uploaded, there is nothing to pull back down
        if plan.remote_id.is_none() {
            if yes {
                return Ok(Outcome::Skipped);
            }

            let untrack = Confirm::new()
                .with_prompt(format!(
                    "{} is missing locally and was never synced, stop tracking it?",
                    plan.display_name
                ))
                .default(true)
                .interact()?;

            if untrack {
                self.untrack(plan)?;
                return Ok(Outcome::Untracked);
            }
            return Ok(Outcome::Skipped);
        }

        if yes {
            return Ok(Outcome::Skipped);
        }

        let choices = [
            "Download the server copy to the saved path",
            "Stop tracking it",
            "Skip this file",
        ];
        let choice = Select::new()
            .with_prompt(format!("{} is missing locally", plan.display_name))
            .items(choices)
            .default(2)
            .interact()?;

        match choice {
            0 => {
                self.pull_file(client, &name, plan.remote_hash, None, false)?;
                Ok(Outcome::Pulled)
            }
            1 => {
                self.untrack(plan)?;
                Ok(Outcome::Untracked)
            }
            _ => Ok(Outcome::Skipped),
        }
    }

    pub(super) fn resolve_new_local(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        remote: &HashMap<Uuid, PartialFileInfo>,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        if yes {
            return Ok(Outcome::Skipped);
        }

        let name = plan_name(plan)?;
        let tracked = self.tracked(&name)?;

        if !tracked.file_path.try_exists()? {
            bail!("The local file is missing, there is nothing to upload");
        }

        let same_name = remote
            .values()
            .filter(|file| file.deleted_at.is_none() && file.name == tracked.name)
            .count();

        let choices = [
            "Upload it as a new file on the server",
            "Link it to an existing file on the server",
            "Skip for now",
        ];
        let prompt = if same_name == 0 {
            format!("{} has not been synced yet", plan.display_name)
        } else {
            format!(
                "{} has not been synced yet (the server already has {} file(s) with this name)",
                plan.display_name, same_name
            )
        };
        let choice = Select::new()
            .with_prompt(prompt)
            .items(choices)
            .default(2)
            .interact()?;

        match choice {
            0 => {
                let (bytes, hash) = read_local_file(&tracked.file_path)?;
                self.create_remote(client, &name, &bytes, hash)?;
                self.save_settings()?;
                Ok(Outcome::Pushed)
            }
            1 => self.link_to_remote(client, &name, remote, yes),
            _ => Ok(Outcome::Skipped),
        }
    }

    fn link_to_remote(
        &mut self,
        client: &ApiClient,
        name: &str,
        remote: &HashMap<Uuid, PartialFileInfo>,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        let tracked = self.tracked(name)?;

        let mut candidates: Vec<&PartialFileInfo> = remote
            .values()
            .filter(|file| file.deleted_at.is_none())
            .filter(|file| {
                !self
                    .settings
                    .tracked_files
                    .values()
                    .any(|tracked| tracked.id == Some(file.id))
            })
            .collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name));

        if candidates.is_empty() {
            println!("There are no unlinked files on the server.");
            return Ok(Outcome::Skipped);
        }

        let items: Vec<String> = candidates
            .iter()
            .map(|file| format!("{} ({:016x})", file.name, file.hash))
            .collect();
        let choice = Select::new()
            .with_prompt("Link to which file on the server?")
            .items(&items)
            .default(0)
            .interact()?;
        let remote_file = candidates[choice];

        let (_, local_hash) = read_local_file(&tracked.file_path)?;

        if remote_file.hash == local_hash {
            let tracked = self
                .settings
                .tracked_files
                .get_mut(name)
                .with_context(|| format!("Tracked file {} disappeared", name))?;
            tracked.id = Some(remote_file.id);
            tracked.synced_hash = Some(local_hash);
            tracked.last_checked = Some(Utc::now());
            self.save_settings()?;
            println!("{} {}", style("linked").green().bold(), name);
            return Ok(Outcome::Unchanged);
        }

        if yes {
            return Ok(Outcome::Skipped);
        }

        let choices = [
            "Upload the local file, overwriting the server copy",
            "Download the server copy, overwriting the local file",
            "Cancel",
        ];
        let choice = Select::new()
            .with_prompt(format!("{} differs from {}", name, remote_file.name))
            .items(choices)
            .default(2)
            .interact()?;

        match choice {
            0 => {
                self.set_remote_id(name, remote_file.id)?;
                self.push_file(client, name, Some(local_hash), Some(remote_file.hash))?;
                Ok(Outcome::Pushed)
            }
            1 => {
                self.set_remote_id(name, remote_file.id)?;
                self.pull_file(client, name, Some(remote_file.hash), Some(local_hash), true)?;
                Ok(Outcome::Pulled)
            }
            _ => Ok(Outcome::Skipped),
        }
    }

    fn set_remote_id(&mut self, name: &str, id: Uuid) -> anyhow::Result<()> {
        let tracked = self
            .settings
            .tracked_files
            .get_mut(name)
            .with_context(|| format!("Tracked file {} disappeared", name))?;
        tracked.id = Some(id);
        self.save_settings()?;
        Ok(())
    }

    pub(super) fn resolve_new_remote(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        remote: &HashMap<Uuid, PartialFileInfo>,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        if yes {
            return Ok(Outcome::Skipped);
        }

        let id = plan
            .remote_id
            .context("A new remote file has no id, this is a bug")?;
        let remote_file = remote
            .get(&id)
            .with_context(|| format!("Remote file {} is not in the manifest", plan.display_name))?;

        let choices = [
            "Download it to a local file",
            "Ignore it on this machine from now on",
            "Skip for now",
        ];
        let choice = Select::new()
            .with_prompt(format!("{} is new on the server", plan.display_name))
            .items(choices)
            .default(0)
            .interact()?;

        match choice {
            0 => {
                let input = Input::<String>::new()
                    .with_prompt("Local path")
                    .with_initial_text(default_remote_path(&remote_file.name))
                    .interact_text()?;

                let path = absolute_path(input.trim())?;

                if path.exists() {
                    let overwrite = Confirm::new()
                        .with_prompt(format!("{} already exists, overwrite it?", path.display()))
                        .default(false)
                        .interact()?;
                    if !overwrite {
                        return Ok(Outcome::Skipped);
                    }
                }

                let missing_dirs = missing_parent_dirs(&path);
                if !missing_dirs.is_empty() {
                    let list = missing_dirs
                        .iter()
                        .map(|dir| dir.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n  ");
                    let create = Confirm::new()
                        .with_prompt(format!(
                            "The following directories do not exist and will be created:\n  {}\nCreate them?",
                            list
                        ))
                        .default(true)
                        .interact()?;
                    if !create {
                        return Ok(Outcome::Skipped);
                    }
                }

                if self
                    .settings
                    .tracked_files
                    .values()
                    .any(|tracked| paths_equal(&tracked.file_path, &path))
                {
                    bail!("{} is already tracked by another file", path.display());
                }

                let bytes = client
                    .get_file_contents(&id)
                    .with_context(|| format!("Failed to download {}", remote_file.name))?;
                let hash = {
                    let bytes: &[u8] = &bytes;
                    XxHash3_64::oneshot(bytes)
                };

                if hash != remote_file.hash {
                    bail!("The server copy changed while syncing, run the sync again");
                }

                write_file_atomic(&path, &bytes)
                    .with_context(|| format!("Failed to write {}", path.display()))?;

                let local_name = self.unique_local_name(&remote_file.name);
                let now = Utc::now();
                let mut path_per_machine = HashMap::new();
                path_per_machine.insert(self.settings.machine_id.clone(), path.clone());

                self.settings.tracked_files.insert(
                    local_name.clone(),
                    TrackedFile {
                        id: Some(id),
                        name: local_name.clone(),
                        file_path: path.clone(),
                        synced_hash: Some(hash),
                        file_last_modified: fs::metadata(&path)?.modified()?.into(),
                        last_checked: Some(now),
                        updated_at: None,
                        last_updated_from: None,
                        path_per_machine,
                    },
                );
                // If it was ignored before, pulling it means it is wanted now
                self.settings.ignored_remote_files.remove(&id);
                self.save_settings()?;

                println!("{} {}", style("pulled").green().bold(), local_name);
                Ok(Outcome::Pulled)
            }
            1 => {
                self.settings.ignored_remote_files.insert(id);
                self.save_settings()?;
                println!("{} {}", style("ignored").yellow().bold(), plan.display_name);
                Ok(Outcome::Ignored)
            }
            _ => Ok(Outcome::Skipped),
        }
    }
}

/// The directories of `path` that do not exist yet, ordered from the outermost one
/// that is missing down to the immediate parent of `path`.
fn missing_parent_dirs(path: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();

    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.exists() {
            break;
        }
        missing.push(dir.to_path_buf());
        current = dir.parent();
    }

    missing.reverse();
    missing
}

/// A sensible default path for a file that only exists on the server. File names
/// added by `config_share add` are often absolute paths, so those are used as is.
fn default_remote_path(name: &str) -> String {
    if Path::new(name).is_absolute() || name.starts_with('/') || name.starts_with('\\') {
        name.to_string()
    } else {
        format!("~/{}", name)
    }
}

use std::fs;

use anyhow::{Context, bail};
use api_types::{
    create_file::CreateFileReq,
    update_file::{UpdateContents, UpdateFileReq},
};
use chrono::Utc;
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::{
    api::{ApiClient, ApiClientError, ApiErrorCode},
    config_share::ConfigShare,
    util::write_file_atomic,
};

use super::read_local_file;

impl ConfigShare {
    /// Uploads the local file, creating it on the server if it has never been synced.
    pub(crate) fn push_file(
        &mut self,
        client: &ApiClient,
        name: &str,
        expected_local_hash: Option<u64>,
        expected_remote_hash: Option<u64>,
    ) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let (bytes, hash) = read_local_file(&tracked.file_path)?;

        if expected_local_hash.is_some_and(|expected| expected != hash) {
            bail!("The local file changed while syncing, run the sync again");
        }

        match tracked.id {
            None => self.create_remote(client, name, &bytes, hash)?,
            Some(id) => {
                let expected_hash = expected_remote_hash
                    .context("The server hash for this file is unknown, run the sync again")?;
                self.update_remote(client, name, &id, expected_hash, &bytes, hash)?;
            }
        }

        self.save_settings()?;
        Ok(())
    }

    /// Downloads the server copy over the local file.
    ///
    /// Unless `overwrite_local_changes` is set, refuses to write when the local file
    /// no longer matches the hash it had at the last successful sync, so unsynced
    /// edits are never silently thrown away.
    pub(crate) fn pull_file(
        &mut self,
        client: &ApiClient,
        name: &str,
        expected_remote_hash: Option<u64>,
        expected_local_hash: Option<u64>,
        overwrite_local_changes: bool,
    ) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let id = tracked
            .id
            .context("This file has not been uploaded to the server yet")?;
        let expected_hash = expected_remote_hash
            .context("The server hash for this file is unknown, run the sync again")?;

        let bytes = client
            .get_file_contents(&id)
            .with_context(|| format!("Failed to download {}", tracked.name))?;
        let hash = {
            let bytes: &[u8] = &bytes;
            XxHash3_64::oneshot(bytes)
        };

        if hash != expected_hash {
            bail!("The server copy changed while syncing, run the sync again");
        }

        let current_hash = fs::read(&tracked.file_path).ok().map(|current| {
            let bytes: &[u8] = &current;
            XxHash3_64::oneshot(bytes)
        });

        // Don't clobber edits that happened after the sync started
        if let Some(expected) = expected_local_hash
            && let Some(current) = current_hash
            && current != expected
        {
            bail!("The local file changed while syncing, run the sync again");
        }

        // Don't clobber edits that have never been synced either
        if !overwrite_local_changes
            && let Some(current) = current_hash
            && tracked.synced_hash != Some(current)
        {
            bail!(
                "The local file has changes that have not been synced, run `config_share sync` to resolve them"
            );
        }

        write_file_atomic(&tracked.file_path, &bytes)
            .with_context(|| format!("Failed to write {}", tracked.file_path.display()))?;

        let now = Utc::now();
        let modified: chrono::DateTime<Utc> = fs::metadata(&tracked.file_path)?.modified()?.into();
        let machine_id = self.settings.machine_id.clone();
        let tracked = self
            .settings
            .tracked_files
            .get_mut(name)
            .with_context(|| format!("Tracked file {} disappeared", name))?;

        tracked.synced_hash = Some(hash);
        tracked.last_checked = Some(now);
        tracked.file_last_modified = modified;
        tracked
            .path_per_machine
            .insert(machine_id, tracked.file_path.clone());

        self.save_settings()?;
        Ok(())
    }

    pub(super) fn create_remote(
        &mut self,
        client: &ApiClient,
        name: &str,
        bytes: &[u8],
        hash: u64,
    ) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let machine_id = self.settings.machine_id.clone();

        let res = client
            .create_file(&CreateFileReq {
                name: tracked.name.clone(),
                hash,
                file_bytes: bytes.to_vec(),
                machine_name: machine_id.clone(),
            })
            .with_context(|| format!("Failed to upload {}", tracked.name))?;

        let now = Utc::now();
        let tracked = self
            .settings
            .tracked_files
            .get_mut(name)
            .with_context(|| format!("Tracked file {} disappeared", name))?;

        tracked.id = Some(res.id);
        tracked.synced_hash = Some(hash);
        tracked.updated_at = Some(now);
        tracked.last_updated_from = Some(machine_id);
        tracked.last_checked = Some(now);
        tracked.file_last_modified = fs::metadata(&tracked.file_path)?.modified()?.into();

        Ok(())
    }

    fn update_remote(
        &mut self,
        client: &ApiClient,
        name: &str,
        id: &Uuid,
        expected_hash: u64,
        bytes: &[u8],
        hash: u64,
    ) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let machine_id = self.settings.machine_id.clone();

        let res = match client.update_file(
            id,
            &UpdateFileReq {
                name: None,
                machine_name: machine_id,
                update_contents: Some(UpdateContents {
                    file_bytes: bytes.to_vec(),
                    expected_hash,
                    hash,
                }),
            },
        ) {
            Ok(res) => res,
            Err(ApiClientError::Api {
                code: ApiErrorCode::MismatchedFileHash,
                ..
            }) => bail!("The server copy changed while syncing, run the sync again"),
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to upload {}", tracked.name));
            }
        };

        let tracked = self
            .settings
            .tracked_files
            .get_mut(name)
            .with_context(|| format!("Tracked file {} disappeared", name))?;

        tracked.synced_hash = Some(hash);
        tracked.updated_at = Some(res.updated_at);
        tracked.last_updated_from = Some(res.machine_name);
        tracked.last_checked = Some(Utc::now());
        tracked.file_last_modified = fs::metadata(&tracked.file_path)?.modified()?.into();

        Ok(())
    }
}

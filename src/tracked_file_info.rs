use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrackedFile {
    /// The id assigned by the server on first sync, None until then
    pub(crate) id: Option<Uuid>,
    /// The display name of the file, used as the key in `tracked_files`
    pub(crate) name: String,
    /// The path to where the file is stored locally
    pub(crate) file_path: PathBuf,
    /// The hash that both the local file and the server copy had at the last
    /// successful sync. None if the file has never been synced, or if the config
    /// was written before sync tracked this.
    #[serde(default)]
    pub(crate) synced_hash: Option<u64>,
    /// When the local file was last modified
    pub(crate) file_last_modified: DateTime<Utc>,
    /// The last time the file was checked to be up to date with the server
    pub(crate) last_checked: Option<DateTime<Utc>>,
    /// The last time a write was performed to the server for this file
    pub(crate) updated_at: Option<DateTime<Utc>>,
    /// The machine that last updated the file on the server
    pub(crate) last_updated_from: Option<String>,

    /// A map of machine ids to file paths
    pub(crate) path_per_machine: HashMap<String, PathBuf>,
}

impl TrackedFile {
    pub fn create_from_filepath(
        file_path: &Path,
        machine_id: &str,
        name: String,
    ) -> anyhow::Result<Self> {
        // Open the file so that a file that exists but cannot be read is rejected
        // while adding it, instead of failing later during a sync
        let file = File::open(file_path)
            .with_context(|| format!("Failed to open {} for reading", file_path.display()))?;
        let metadata = file.metadata()?;
        let file_path = crate::util::absolute_path(file_path)?;

        let mut path_per_machine: HashMap<String, PathBuf> = HashMap::new();
        path_per_machine.insert(machine_id.to_string(), file_path.clone());

        Ok(Self {
            id: None,
            name,
            file_path,
            synced_hash: None,
            file_last_modified: metadata.modified()?.into(),
            last_checked: None,
            updated_at: None,
            last_updated_from: None,
            path_per_machine,
        })
    }
}

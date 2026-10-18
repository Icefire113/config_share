use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use twox_hash::XxHash3_64;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackedFile {
    /// The id assigned by the server on first sync, None until then
    pub(crate) id: Option<Uuid>,
    /// The display name of the file, used as the key in `tracked_files`
    pub(crate) name: String,
    /// The path to where the file is stored locally
    pub(crate) file_path: PathBuf,
    // xxhash3 hash of the local file contents
    pub(crate) hash: u64,
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
        machine_id: &String,
        name: String,
    ) -> anyhow::Result<Self> {
        let file_path = file_path.canonicalize()?;
        let mut file = File::open(&file_path).with_context(|| {
            anyhow!(
                "Failed to open the file for reading, grabbing metadata and hashing the contents"
            )
        })?;
        let mut data: Vec<u8> = Vec::new();
        file.read_to_end(&mut data)
            .with_context(|| anyhow!("Failed to read the entire file into a buffer"))?;

        let mut map: HashMap<String, PathBuf> = HashMap::new();
        map.insert(machine_id.clone(), file_path.clone());

        Ok(Self {
            id: None,
            name,
            file_path,
            hash: XxHash3_64::oneshot(&data),
            file_last_modified: file.metadata()?.modified()?.into(),
            last_checked: None,
            updated_at: None,
            last_updated_from: None,
            path_per_machine: map,
        })
    }
}

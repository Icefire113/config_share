use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use twox_hash::XxHash3_64;

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackedFile {
    /// The path to where the file is stored locally
    pub(crate) file_path: PathBuf,
    // xxhash3 hash of the local file contents
    pub(crate) hash: u64,
    pub(crate) file_last_modified: DateTime<Utc>,
    /// The last time the file was checked to be up to date with the server
    pub(crate) last_checked: Option<DateTime<Utc>>,
    pub(crate) use_custom_name: Option<String>,

    /// A map of machine ids to file paths
    pub(crate) path_per_machine: HashMap<String, PathBuf>,
}

impl TrackedFile {
    pub fn create_from_filepath(
        file_path: &Path,
        machine_id: &String,
        use_custom_name: Option<String>,
    ) -> anyhow::Result<Self> {
        let mut file = File::open(file_path).with_context(
            || "Failed to open the file for reading, grabbing metadata and hashing the contents",
        )?;
        let mut data: Vec<u8> = Vec::new();
        file.read_to_end(&mut data)
            .with_context(|| "Failed to read the entire file into a buffer")?;

        let mut map = HashMap::new();
        map.insert(machine_id.clone(), file_path.canonicalize()?.to_path_buf());

        Ok(Self {
            file_path: file_path.canonicalize()?.to_path_buf(),
            hash: XxHash3_64::oneshot(&data),
            file_last_modified: file.metadata()?.modified()?.into(),
            last_checked: None,
            use_custom_name,
            path_per_machine: map,
        })
    }
}

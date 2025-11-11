use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use twox_hash::XxHash3_64;

use crate::file_differentation_method::FileDifferentationMethod;

#[derive(Serialize, Deserialize)]
pub struct TrackedFileInfo {
    /// The path to where the file is stored locally
    pub(crate) file_path: PathBuf,
    // xxhash3 hash of the local file contents
    pub(crate) hash: u64,
    pub(crate) file_last_modified: DateTime<Utc>,
    pub(crate) last_checked: Option<DateTime<Utc>>,
    pub(crate) differentation_method: FileDifferentationMethod,
}

impl TrackedFileInfo {
    pub fn create_from_filepath(
        file_path: &Path,
        differentation_method: FileDifferentationMethod,
    ) -> anyhow::Result<Self> {
        let mut file = File::open(file_path).with_context(
            || "Failed to open the file for reading, grabbing metadata and hashing the contents",
        )?;
        let mut data: Vec<u8> = Vec::new();
        file.read_to_end(&mut data)
            .with_context(|| "Failed to read the entire file into a buffer")?;

        Ok(Self {
            file_path: file_path.to_path_buf(),
            hash: XxHash3_64::oneshot(&data),
            file_last_modified: file.metadata()?.modified()?.into(),
            last_checked: None,
            differentation_method,
        })
    }
}

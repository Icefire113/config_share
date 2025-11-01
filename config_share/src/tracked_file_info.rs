use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TrackedFileInfo {
    /// The path to where the file is stored locally
    file_path: PathBuf,
    // xxhash3 hash of the local file contents
    hash: u64,
    file_last_modified: DateTime<Utc>,
    last_checked: DateTime<Utc>,
}

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize)]
#[allow(dead_code)]
// Struct for storing settings for the program that will be saved to disk
pub(crate) struct ShareSettings {
    pub(crate) tracked_files: Vec<TrackedFile>,
    pub(crate) last_sync: Option<DateTime<Utc>>,
    pub(crate) server_url: Option<Url>,
    /// Whether or not to run the worker in the background when the user logs in
    pub(crate) enable_worker: bool,
}

impl Default for ShareSettings {
    fn default() -> Self {
        Self {
            tracked_files: Vec::new(),
            last_sync: None,
            server_url: None,
            enable_worker: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct TrackedFile {
    path: PathBuf,
    file_last_modified: DateTime<Utc>,
    last_checked: DateTime<Utc>,
}

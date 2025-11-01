use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::tracked_file_info::TrackedFileInfo;

#[derive(Serialize, Deserialize)]
// Struct for storing settings for the program that will be saved to disk
pub(crate) struct ShareSettings {
    pub(crate) tracked_files: Vec<TrackedFileInfo>,
    pub(crate) last_sync: Option<DateTime<Utc>>,
    pub(crate) server_url: Option<Url>,
    /// Whether or not to run the worker in the background when the user logs in
    pub(crate) enable_worker: bool,
    /// The version of the program that the config was last updated with
    pub(crate) version: String,
}

impl Default for ShareSettings {
    /// Creates a new ShareSettings struct with the default values, and no tracked files
    fn default() -> Self {
        Self {
            tracked_files: Vec::new(),
            last_sync: None,
            server_url: None,
            enable_worker: false,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl ShareSettings {
    /// Migrates the old settings to the new settings, updates the version, and handles all translations between config versions
    pub fn migrate(old: ShareSettings) -> Self {
        ShareSettings::default()
    }
}

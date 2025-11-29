use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::tracked_file_info::TrackedFile;

#[derive(Serialize, Deserialize)]
// Struct for storing settings for the program that will be saved to disk
pub(crate) struct ShareSettings {
    /// A mapping from the custom name of the file to the file information
    pub(crate) tracked_files: HashMap<String, TrackedFile>,
    /// The last time the user synced with the server manually
    pub(crate) last_sync: Option<DateTime<Utc>>,
    pub(crate) server_url: Option<Url>,
    /// The version of the program that the config was last updated with
    pub(crate) version: String,
    /// The time in hours before the user is reminded to manually sync
    pub(crate) manual_sync_reminder_time: i64,
    pub(crate) machine_id: String,
}

impl Default for ShareSettings {
    /// Creates a new ShareSettings struct with the default values, and no tracked files
    fn default() -> Self {
        Self {
            tracked_files: HashMap::new(),
            last_sync: None,
            server_url: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
            manual_sync_reminder_time: 24,
            machine_id: "".to_string(),
        }
    }
}

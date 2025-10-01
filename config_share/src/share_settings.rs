use std::{
    fs::File,
    path::PathBuf,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize)]
#[allow(dead_code)]
// Struct for storing settings for the program that will be saved to disk
pub(crate) struct ShareSettings {
    tracked_files: Vec<TrackedFile>,
    last_sync: Option<DateTime<Utc>>,
    server_url: Option<Url>,
}

impl Default for ShareSettings {
    fn default() -> Self {
        Self {
            tracked_files: Vec::new(),
            last_sync: None,
            server_url: Some(match Url::from_str("http://127.0.0.1") {
                Ok(url) => url,
                Err(e) => panic!("Error parsing default server URL: {}", e),
            }),
        }
    }
}

impl ShareSettings {
    /// Add a file to the list of tracked files
    pub fn add_tracked_file(&mut self, file_path: PathBuf) -> Result<(), std::io::Error> {
        let file = File::options().read(true).open(&file_path)?;

        let last_modified = file.metadata()?.modified()?;
        let last_checked = Utc::now();

        self.tracked_files.push(TrackedFile {
            path: file_path,
            file_last_modified: DateTime::from(last_modified),
            last_checked,
        });

        todo!("Push the file to the storage service");

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TrackedFile {
    path: PathBuf,
    file_last_modified: DateTime<Utc>,
    last_checked: DateTime<Utc>,
}

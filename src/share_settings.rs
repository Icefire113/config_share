use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::tracked_file_info::TrackedFile;

#[derive(Serialize, Deserialize, Debug)]
// Struct for storing settings for the program that will be saved to disk
pub(crate) struct ShareSettings {
    /// A mapping from the name of the file to the file information
    pub(crate) tracked_files: HashMap<String, TrackedFile>,
    /// The last time the user synced with the server manually
    pub(crate) last_sync: Option<DateTime<Utc>>,
    pub(crate) server_url: Option<Url>,
    /// The version of the program that the config was last updated with
    pub(crate) version: String,
    /// The time in hours before the user is reminded to manually sync
    pub(crate) manual_sync_reminder_time: i64,
    pub(crate) machine_id: String,
    /// Metadata about the access token stored in the OS keychain, `None` until the
    /// user logs in. The token itself is never written to the config file.
    /// Defaulted so that configs created before login existed still load.
    #[serde(default)]
    pub(crate) auth: Option<AuthInfo>,
    /// Remote files that the user chose not to track on this machine
    #[serde(default)]
    pub(crate) ignored_remote_files: HashSet<Uuid>,
    /// Enables client-side encryption for files uploaded from this machine.
    /// `None` means uploads are wrapped as plaintext. The passphrase itself is
    /// kept in the OS keychain and never written to the config file.
    #[serde(default)]
    pub(crate) encryption: Option<EncryptionSettings>,
}

/// Non-secret settings needed to encrypt new uploads on this machine.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct EncryptionSettings {
    /// Base64-encoded Argon2 salt. It is embedded in every encrypted upload, so
    /// other machines with the same passphrase can decrypt files uploaded here.
    pub(crate) salt: String,
}

/// Non-secret metadata about the access token stored in the OS keychain.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct AuthInfo {
    /// The id the server assigned to the token, used to revoke it on logout
    pub(crate) token_id: i64,
    /// The name given to the token when it was created
    pub(crate) name: String,
    /// When the token expires on the server
    pub(crate) expires_at: Option<DateTime<Utc>>,
}

impl Default for ShareSettings {
    /// Creates a new ShareSettings struct with the default values, and no tracked files
    fn default() -> Self {
        Self {
            tracked_files: HashMap::new(),
            last_sync: None,
            server_url: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
            manual_sync_reminder_time: 24 * 7, // 24hrs * 7 = 1 week
            machine_id: "".to_string(),
            auth: None,
            ignored_remote_files: HashSet::new(),
            encryption: None,
        }
    }
}

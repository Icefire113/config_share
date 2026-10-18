use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use api_types::get_file::PartialFileInfo;
use chrono::{DateTime, Utc};
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::share_settings::ShareSettings;

/// The current state of a tracked file on this machine, with its contents hashed.
#[derive(Debug, Clone)]
pub(crate) struct LocalFileState {
    pub(crate) hash: u64,
    pub(crate) last_modified: DateTime<Utc>,
}

/// How a tracked file (or a file that only exists on one side) differs from the
/// state it was in after the last successful sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    /// Local and remote contents match the last synced state
    UpToDate,
    /// Only the local file changed since the last sync
    LocalChanged,
    /// Only the server copy changed since the last sync
    RemoteChanged,
    /// Both sides changed since the last sync
    Diverged,
    /// The local file is gone
    LocalMissing,
    /// The server copy was deleted
    RemoteDeleted,
    /// The local file changed and the server copy was deleted
    LocalChangedRemoteDeleted,
    /// Both the local file and the server copy are gone
    BothMissing,
    /// Tracked locally but never uploaded
    NewLocal,
    /// Exists on the server but is not tracked on this machine
    NewRemote,
    /// The file has an id but no record of the last synced state
    Unknown,
    /// Two tracked files point at the same server file
    DuplicateId,
}

impl SyncStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::UpToDate => "up to date",
            Self::LocalChanged => "local changes",
            Self::RemoteChanged => "remote changes",
            Self::Diverged => "changed on both sides",
            Self::LocalMissing => "missing locally",
            Self::RemoteDeleted => "deleted remotely",
            Self::LocalChangedRemoteDeleted => "local changes, deleted remotely",
            Self::BothMissing => "missing locally and remotely",
            Self::NewLocal => "not synced yet",
            Self::NewRemote => "new on the server",
            Self::Unknown => "unknown (no sync anchor)",
            Self::DuplicateId => "duplicate remote id",
        }
    }
}

/// A single tracked file, or a file that only exists on one side, along with the
/// action state needed to bring both sides back in sync.
#[derive(Debug, Clone)]
pub(crate) struct SyncPlan {
    /// The key in `settings.tracked_files`, if this is a tracked file
    pub(crate) local_name: Option<String>,
    /// The server id, if the file exists on the server
    pub(crate) remote_id: Option<Uuid>,
    pub(crate) display_name: String,
    /// The path of the local file, if it is tracked
    pub(crate) path: Option<PathBuf>,
    pub(crate) status: SyncStatus,
    pub(crate) local_hash: Option<u64>,
    pub(crate) remote_hash: Option<u64>,
}

/// Reads and hashes a local file, returning `None` if it does not exist.
pub(crate) fn scan_local_file(path: &Path) -> anyhow::Result<Option<LocalFileState>> {
    match std::fs::read(path) {
        Ok(contents) => {
            let metadata = std::fs::metadata(path)
                .with_context(|| format!("Failed to read the metadata of {}", path.display()))?;

            Ok(Some(LocalFileState {
                hash: {
                    let bytes: &[u8] = &contents;
                    XxHash3_64::oneshot(bytes)
                },
                last_modified: metadata.modified()?.into(),
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Failed to read {}", path.display())),
    }
}

/// Works out what needs to happen for every tracked file and every file that only
/// exists on the server.
pub(crate) fn analyze(
    settings: &ShareSettings,
    local: &HashMap<String, LocalFileState>,
    remote: &HashMap<Uuid, PartialFileInfo>,
) -> Vec<SyncPlan> {
    analyze_with_ignored(settings, local, remote, false)
}

/// Like [`analyze`], but optionally includes remote files that the user chose to
/// ignore on this machine.
pub(crate) fn analyze_with_ignored(
    settings: &ShareSettings,
    local: &HashMap<String, LocalFileState>,
    remote: &HashMap<Uuid, PartialFileInfo>,
    include_ignored: bool,
) -> Vec<SyncPlan> {
    let mut plans = Vec::with_capacity(settings.tracked_files.len() + remote.len());
    let mut linked_ids: HashMap<Uuid, String> = HashMap::new();

    for (name, tracked) in &settings.tracked_files {
        let local_state = local.get(name);
        let local_hash = local_state.map(|state| state.hash);
        let path = Some(tracked.file_path.clone());

        let (status, remote_hash, duplicate_of) = match tracked.id {
            None => (
                if local_state.is_some() {
                    SyncStatus::NewLocal
                } else {
                    SyncStatus::LocalMissing
                },
                None,
                None,
            ),
            Some(id) => {
                let duplicate_of = linked_ids.insert(id, name.clone());
                let remote_file = remote.get(&id);
                let remote_hash = remote_file.map(|file| file.hash);

                let status = match remote_file {
                    // The server no longer knows about the file at all
                    None => SyncStatus::RemoteDeleted,
                    Some(file) if file.deleted_at.is_some() => {
                        match (local_state, tracked.synced_hash) {
                            (None, _) => SyncStatus::BothMissing,
                            (Some(_), Some(anchor)) if local_hash == Some(anchor) => {
                                SyncStatus::RemoteDeleted
                            }
                            _ => SyncStatus::LocalChangedRemoteDeleted,
                        }
                    }
                    Some(file) => match (local_state, tracked.synced_hash) {
                        (None, _) => SyncStatus::LocalMissing,
                        (Some(state), Some(anchor)) => {
                            let local_changed = state.hash != anchor;
                            let remote_changed = file.hash != anchor;

                            match (local_changed, remote_changed) {
                                (false, false) => SyncStatus::UpToDate,
                                (true, false) => SyncStatus::LocalChanged,
                                (false, true) => SyncStatus::RemoteChanged,
                                (true, true) => SyncStatus::Diverged,
                            }
                        }
                        // Configs written before the sync anchor existed can only
                        // fast-forward when the contents are already identical
                        (Some(state), None) => {
                            if state.hash == file.hash {
                                SyncStatus::UpToDate
                            } else {
                                SyncStatus::Unknown
                            }
                        }
                    },
                };

                (status, remote_hash, duplicate_of)
            }
        };

        let status = if duplicate_of.is_some() {
            SyncStatus::DuplicateId
        } else {
            status
        };

        let display_name = match &duplicate_of {
            Some(other) => format!("{} (duplicate, already tracked by {})", name, other),
            None => name.clone(),
        };

        plans.push(SyncPlan {
            local_name: Some(name.clone()),
            remote_id: tracked.id,
            display_name,
            path,
            status,
            local_hash,
            remote_hash,
        });
    }

    for file in remote.values() {
        if file.deleted_at.is_some()
            || linked_ids.contains_key(&file.id)
            || (!include_ignored && settings.ignored_remote_files.contains(&file.id))
        {
            continue;
        }

        plans.push(SyncPlan {
            local_name: None,
            remote_id: Some(file.id),
            display_name: file.name.clone(),
            path: None,
            status: SyncStatus::NewRemote,
            local_hash: None,
            remote_hash: Some(file.hash),
        });
    }

    plans.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    plans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracked_file_info::TrackedFile;
    use chrono::Utc;
    use std::collections::HashSet;

    fn tracked_file(path: &str, id: Option<Uuid>, synced_hash: Option<u64>) -> TrackedFile {
        TrackedFile {
            id,
            name: path.to_string(),
            file_path: PathBuf::from(path),
            synced_hash,
            file_last_modified: Utc::now(),
            last_checked: None,
            updated_at: None,
            last_updated_from: None,
            path_per_machine: HashMap::new(),
        }
    }

    fn settings_with(name: &str, tracked: TrackedFile) -> ShareSettings {
        let mut settings = ShareSettings::default();
        settings.tracked_files.insert(name.to_string(), tracked);
        settings
    }

    fn local_state(hash: u64) -> HashMap<String, LocalFileState> {
        let mut local = HashMap::new();
        local.insert(
            "file".to_string(),
            LocalFileState {
                hash,
                last_modified: Utc::now(),
            },
        );
        local
    }

    fn remote_file(id: Uuid, hash: u64, deleted: bool) -> PartialFileInfo {
        PartialFileInfo {
            id,
            name: "file".to_string(),
            hash,
            deleted_at: deleted.then(Utc::now),
        }
    }

    fn remote_with(id: Uuid, hash: u64, deleted: bool) -> HashMap<Uuid, PartialFileInfo> {
        let mut remote = HashMap::new();
        remote.insert(id, remote_file(id, hash, deleted));
        remote
    }

    fn only_status(plans: Vec<SyncPlan>) -> SyncStatus {
        assert_eq!(plans.len(), 1);
        plans[0].status
    }

    #[test]
    fn up_to_date_when_nothing_changed() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(10), &remote_with(id, 10, false));

        assert_eq!(only_status(plans), SyncStatus::UpToDate);
    }

    #[test]
    fn local_change_is_pushed() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(11), &remote_with(id, 10, false));

        assert_eq!(only_status(plans), SyncStatus::LocalChanged);
    }

    #[test]
    fn remote_change_is_pulled() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(10), &remote_with(id, 11, false));

        assert_eq!(only_status(plans), SyncStatus::RemoteChanged);
    }

    #[test]
    fn changes_on_both_sides_diverge() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(11), &remote_with(id, 12, false));

        assert_eq!(only_status(plans), SyncStatus::Diverged);
    }

    #[test]
    fn deleted_remotely_without_local_changes() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(10), &remote_with(id, 10, true));

        assert_eq!(only_status(plans), SyncStatus::RemoteDeleted);
    }

    #[test]
    fn deleted_remotely_with_local_changes() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(&settings, &local_state(11), &remote_with(id, 10, true));

        assert_eq!(only_status(plans), SyncStatus::LocalChangedRemoteDeleted);
    }

    #[test]
    fn missing_locally_and_remotely_is_both_missing() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), Some(10)));
        let plans = analyze(
            &settings,
            &HashMap::<String, LocalFileState>::new(),
            &remote_with(id, 10, true),
        );

        assert_eq!(only_status(plans), SyncStatus::BothMissing);
    }

    #[test]
    fn tracked_without_id_is_new_local() {
        let settings = settings_with("file", tracked_file("file", None, None));
        let plans = analyze(&settings, &local_state(10), &HashMap::new());

        assert_eq!(only_status(plans), SyncStatus::NewLocal);
    }

    #[test]
    fn untracked_remote_file_is_new_remote() {
        let id = Uuid::new_v4();
        let settings = ShareSettings::default();
        let plans = analyze(&settings, &HashMap::new(), &remote_with(id, 10, false));

        assert_eq!(only_status(plans), SyncStatus::NewRemote);
    }

    #[test]
    fn ignored_remote_file_is_skipped() {
        let id = Uuid::new_v4();
        let mut settings = ShareSettings::default();
        settings.ignored_remote_files = HashSet::from([id]);

        let plans = analyze(&settings, &HashMap::new(), &remote_with(id, 10, false));

        assert!(plans.is_empty());
    }

    #[test]
    fn include_ignored_keeps_ignored_remote_file() {
        let id = Uuid::new_v4();
        let mut settings = ShareSettings::default();
        settings.ignored_remote_files = HashSet::from([id]);

        let plans = analyze_with_ignored(
            &settings,
            &HashMap::new(),
            &remote_with(id, 10, false),
            true,
        );

        assert_eq!(only_status(plans), SyncStatus::NewRemote);
    }

    #[test]
    fn deleted_untracked_remote_file_is_skipped() {
        let id = Uuid::new_v4();
        let settings = ShareSettings::default();
        let plans = analyze(&settings, &HashMap::new(), &remote_with(id, 10, true));

        assert!(plans.is_empty());
    }

    #[test]
    fn missing_anchor_with_matching_contents_is_up_to_date() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), None));
        let plans = analyze(&settings, &local_state(10), &remote_with(id, 10, false));

        assert_eq!(only_status(plans), SyncStatus::UpToDate);
    }

    #[test]
    fn missing_anchor_with_different_contents_is_unknown() {
        let id = Uuid::new_v4();
        let settings = settings_with("file", tracked_file("file", Some(id), None));
        let plans = analyze(&settings, &local_state(10), &remote_with(id, 11, false));

        assert_eq!(only_status(plans), SyncStatus::Unknown);
    }

    #[test]
    fn duplicate_ids_are_reported() {
        let id = Uuid::new_v4();
        let mut settings = ShareSettings::default();
        settings
            .tracked_files
            .insert("file".to_string(), tracked_file("file", Some(id), Some(10)));
        settings.tracked_files.insert(
            "other".to_string(),
            TrackedFile {
                name: "other".to_string(),
                file_path: PathBuf::from("other"),
                ..tracked_file("other", Some(id), Some(10))
            },
        );

        let mut local = local_state(10);
        local.insert(
            "other".to_string(),
            LocalFileState {
                hash: 10,
                last_modified: Utc::now(),
            },
        );

        let plans = analyze(&settings, &local, &remote_with(id, 10, false));
        let duplicate = plans
            .iter()
            .find(|plan| plan.status == SyncStatus::DuplicateId);

        assert!(duplicate.is_some());
    }
}

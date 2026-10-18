use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, bail};
use api_types::get_file::{GetAllFilesReq, PartialFileInfo};
use chrono::Utc;
use console::style;
use log::{trace, warn};
use tabled::{Table, Tabled};
use twox_hash::XxHash3_64;
use uuid::Uuid;

use crate::{
    api::ApiClient,
    config_share::ConfigShare,
    share_settings::ShareSettings,
    sync_analysis::{self, LocalFileState, SyncPlan, SyncStatus},
    tracked_file_info::TrackedFile,
    util::{absolute_path, paths_equal},
};

mod conflict;
mod decisions;
mod transfer;

/// How many files to request per manifest page
const MANIFEST_PAGE_SIZE: u32 = 100;
/// Safety net so a misbehaving server can't make us page forever
const MAX_MANIFEST_PAGES: u32 = 1_000;

#[derive(Debug, Default)]
struct SyncSummary {
    pushed: usize,
    pulled: usize,
    unchanged: usize,
    untracked: usize,
    ignored: usize,
    skipped: usize,
    failed: usize,
}

impl SyncSummary {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Pushed => self.pushed += 1,
            Outcome::Pulled => self.pulled += 1,
            Outcome::Unchanged => self.unchanged += 1,
            Outcome::Untracked => self.untracked += 1,
            Outcome::Ignored => self.ignored += 1,
            Outcome::Skipped => self.skipped += 1,
        }
    }

    fn print(&self) {
        println!(
            "{} pushed {}, pulled {}, unchanged {}, untracked {}, ignored {}, skipped {}, failed {}",
            style("sync complete:").green().bold(),
            self.pushed,
            self.pulled,
            self.unchanged,
            self.untracked,
            self.ignored,
            self.skipped,
            self.failed,
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum Outcome {
    Pushed,
    Pulled,
    Unchanged,
    Untracked,
    Ignored,
    Skipped,
}

impl ConfigShare {
    /// Reconciles the local tracked files with the server.
    ///
    /// Safe changes (one side changed since the last sync) are applied without
    /// asking, everything that needs a decision is prompted for unless `yes` is set
    /// in which case those files are skipped.
    pub fn sync_files(
        &mut self,
        file: Option<&String>,
        dry_run: bool,
        yes: bool,
        include_ignored: bool,
    ) -> anyhow::Result<()> {
        trace!(
            "[sync.sync_files] file={:?}, dry_run={}, yes={}, include_ignored={}",
            file, dry_run, yes, include_ignored
        );

        self.require_sync_ready()?;

        let client = self.get_api_client()?;

        println!("Fetching the file list from the server...");
        let remote = fetch_manifest(&client)?;
        let local = scan_local(&self.settings)?;
        let mut plans =
            sync_analysis::analyze_with_ignored(&self.settings, &local, &remote, include_ignored);

        if let Some(file) = file {
            filter_plans_to_file(&mut plans, file)?;
        }

        if plans.is_empty() {
            println!("{}", style("Nothing to sync.").green());
            self.record_last_sync()?;
            return Ok(());
        }

        if dry_run {
            print_plan_table(&plans);
            return Ok(());
        }

        let mut summary = SyncSummary::default();
        for plan in &plans {
            match self.apply_plan(&client, plan, &local, &remote, yes) {
                Ok(outcome) => summary.add(outcome),
                Err(e) => {
                    summary.failed += 1;
                    println!(
                        "{} {}: {:#}",
                        style("failed").red().bold(),
                        style(&plan.display_name).bold(),
                        e
                    );
                }
            }
        }

        self.record_last_sync()?;
        summary.print();

        if summary.failed > 0 {
            bail!("{} file(s) failed to sync", summary.failed);
        }

        Ok(())
    }

    pub(crate) fn require_sync_ready(&self) -> anyhow::Result<()> {
        if self.settings.machine_id.trim().is_empty() {
            bail!("The machine id in the config file is blank, set it before syncing");
        }
        if self.settings.auth.is_none() {
            bail!("You are not logged in, run `config_share login` first");
        }
        Ok(())
    }

    fn record_last_sync(&mut self) -> anyhow::Result<()> {
        self.settings.last_sync = Some(Utc::now());
        self.save_settings()
    }

    fn apply_plan(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        local: &HashMap<String, LocalFileState>,
        remote: &HashMap<Uuid, PartialFileInfo>,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        match plan.status {
            SyncStatus::UpToDate => {
                self.mark_checked(plan, local, plan.remote_hash);
                Ok(Outcome::Unchanged)
            }
            SyncStatus::LocalChanged => {
                let name = plan_name(plan)?;
                self.push_file(client, &name, plan.local_hash, plan.remote_hash)?;
                println!("{} {}", style("pushed").green().bold(), plan.display_name);
                Ok(Outcome::Pushed)
            }
            SyncStatus::RemoteChanged => {
                let name = plan_name(plan)?;
                self.pull_file(client, &name, plan.remote_hash, plan.local_hash, false)?;
                println!("{} {}", style("pulled").green().bold(), plan.display_name);
                Ok(Outcome::Pulled)
            }
            SyncStatus::Diverged | SyncStatus::Unknown => {
                self.resolve_conflict(client, plan, local, yes)
            }
            SyncStatus::RemoteDeleted | SyncStatus::LocalChangedRemoteDeleted => {
                self.resolve_remote_deleted(client, plan, yes)
            }
            SyncStatus::LocalMissing => self.resolve_local_missing(client, plan, yes),
            SyncStatus::BothMissing => {
                self.untrack(plan)?;
                println!(
                    "{} {} (missing locally and remotely)",
                    style("untracked").yellow().bold(),
                    plan.display_name
                );
                Ok(Outcome::Untracked)
            }
            SyncStatus::NewLocal => self.resolve_new_local(client, plan, remote, yes),
            SyncStatus::NewRemote => self.resolve_new_remote(client, plan, remote, yes),
            SyncStatus::DuplicateId => {
                warn!("[sync] Skipping {}", plan.display_name);
                println!("{} {}", style("skipped").yellow().bold(), plan.display_name);
                Ok(Outcome::Skipped)
            }
        }
    }

    /// Updates the local bookkeeping for a file that did not need any content moved,
    /// adopting the current hash as the sync anchor if there wasn't one yet.
    fn mark_checked(
        &mut self,
        plan: &SyncPlan,
        local: &HashMap<String, LocalFileState>,
        remote_hash: Option<u64>,
    ) {
        let Some(name) = &plan.local_name else {
            return;
        };
        let Some(tracked) = self.settings.tracked_files.get_mut(name) else {
            return;
        };

        tracked.last_checked = Some(Utc::now());

        if let Some(state) = local.get(name) {
            tracked.file_last_modified = state.last_modified;

            if tracked.synced_hash.is_none() && remote_hash == Some(state.hash) {
                tracked.synced_hash = Some(state.hash);
            }
        }
    }

    fn tracked(&self, name: &str) -> anyhow::Result<TrackedFile> {
        self.settings
            .tracked_files
            .get(name)
            .cloned()
            .with_context(|| format!("Tracked file {} not found", name))
    }

    fn untrack(&mut self, plan: &SyncPlan) -> anyhow::Result<()> {
        let name = plan_name(plan)?;
        self.settings.tracked_files.remove(&name);
        self.save_settings()?;
        Ok(())
    }

    fn unique_local_name(&self, base: &str) -> String {
        if !self.settings.tracked_files.contains_key(base) {
            return base.to_string();
        }

        for suffix in 2.. {
            let candidate = format!("{} ({})", base, suffix);
            if !self.settings.tracked_files.contains_key(&candidate) {
                return candidate;
            }
        }

        unreachable!("the loop above always returns a name")
    }
}

fn plan_name(plan: &SyncPlan) -> anyhow::Result<String> {
    plan.local_name
        .clone()
        .with_context(|| format!("{} is not a tracked file", plan.display_name))
}

fn read_local_file(path: &Path) -> anyhow::Result<(Vec<u8>, u64)> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let hash = {
        let bytes: &[u8] = &bytes;
        XxHash3_64::oneshot(bytes)
    };
    Ok((bytes, hash))
}

/// Fetches every file the user has on the server, including deleted ones.
pub(crate) fn fetch_manifest(client: &ApiClient) -> anyhow::Result<HashMap<Uuid, PartialFileInfo>> {
    let mut files = HashMap::new();
    let mut offset = 0u32;

    for _ in 0..MAX_MANIFEST_PAGES {
        let res = client
            .get_all_files(&GetAllFilesReq {
                include_deleted: true,
                limit: Some(MANIFEST_PAGE_SIZE),
                offset: Some(offset),
            })
            .context("Failed to fetch the list of files from the server")?;

        let page_len = res.files.len() as u32;
        for file in res.files {
            files.insert(file.id, file);
        }

        if page_len < MANIFEST_PAGE_SIZE {
            return Ok(files);
        }

        offset += page_len;
    }

    bail!(
        "The server returned more than {} files, refusing to page any further",
        MANIFEST_PAGE_SIZE * MAX_MANIFEST_PAGES
    )
}

/// Hashes every tracked file that exists locally.
pub(crate) fn scan_local(
    settings: &ShareSettings,
) -> anyhow::Result<HashMap<String, LocalFileState>> {
    let mut states = HashMap::new();

    for (name, tracked) in &settings.tracked_files {
        if let Some(state) = sync_analysis::scan_local_file(&tracked.file_path)
            .with_context(|| format!("Failed to check the local file for {}", name))?
        {
            states.insert(name.clone(), state);
        }
    }

    Ok(states)
}

/// Restricts the plans to the tracked file at the given path.
pub(crate) fn filter_plans_to_file(plans: &mut Vec<SyncPlan>, file: &str) -> anyhow::Result<()> {
    let requested = absolute_path(file)?;

    plans.retain(|plan| {
        plan.path
            .as_ref()
            .is_some_and(|path| paths_equal(path, &requested))
    });

    if plans.is_empty() {
        bail!("The file {} is not currently tracked", requested.display());
    }

    Ok(())
}

/// Prints a read-only overview of what a sync would do.
pub(crate) fn print_plan_table(plans: &[SyncPlan]) {
    #[derive(Tabled)]
    struct Record<'a> {
        #[tabled(rename = "Status")]
        status: &'a str,
        #[tabled(rename = "Name")]
        name: &'a str,
        #[tabled(rename = "Local Path")]
        path: String,
    }

    let table = Table::new(plans.iter().map(|plan| {
        Record {
            status: plan.status.label(),
            name: &plan.display_name,
            path: plan
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string()),
        }
    }));

    println!("{}", table);
}

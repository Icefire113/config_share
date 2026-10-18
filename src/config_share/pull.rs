use anyhow::bail;
use console::style;

use crate::{
    config_share::{ConfigShare, sync},
    sync_analysis::{self, SyncStatus},
};

impl ConfigShare {
    /// Pulls the tracked files down from the server, overwriting the local copies.
    ///
    /// Unlike `sync`, this never asks what to do: every tracked file that exists on
    /// the server is downloaded, including files that have local changes.
    ///
    /// It can still ask for the encryption passphrase if it is not cached.
    pub fn pull_files(&mut self, file: Option<&String>) -> anyhow::Result<()> {
        self.require_sync_ready()?;

        let client = self.get_api_client()?;
        let remote = sync::fetch_manifest(&client)?;
        let local = sync::scan_local(&self.settings)?;
        let mut plans = sync_analysis::analyze(&self.settings, &local, &remote);

        if let Some(file) = file {
            sync::filter_plans_to_file(&mut plans, file)?;
        }

        let untracked_remote = plans
            .iter()
            .filter(|plan| plan.local_name.is_none() && plan.status == SyncStatus::NewRemote)
            .count();

        let mut pulled = 0;
        let mut unchanged = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for plan in &plans {
            // This command only pulls files that are tracked on this machine
            let Some(name) = &plan.local_name else {
                continue;
            };

            match plan.status {
                SyncStatus::UpToDate => {
                    unchanged += 1;
                    continue;
                }
                SyncStatus::NewLocal => {
                    println!(
                        "{} {} (not on the server yet, run `config_share push`)",
                        style("skipped").yellow().bold(),
                        plan.display_name
                    );
                    skipped += 1;
                    continue;
                }
                SyncStatus::RemoteDeleted
                | SyncStatus::LocalChangedRemoteDeleted
                | SyncStatus::BothMissing => {
                    println!(
                        "{} {} (deleted on the server)",
                        style("skipped").yellow().bold(),
                        plan.display_name
                    );
                    skipped += 1;
                    continue;
                }
                SyncStatus::LocalChanged | SyncStatus::Diverged | SyncStatus::Unknown => {
                    println!(
                        "{} {} (has local changes, run `config_share sync` to resolve them)",
                        style("skipped").yellow().bold(),
                        plan.display_name
                    );
                    skipped += 1;
                    continue;
                }
                SyncStatus::DuplicateId => {
                    skipped += 1;
                    continue;
                }
                _ => {}
            }

            match self.pull_file(&client, name, plan.remote_hash, plan.local_hash, false) {
                Ok(()) => {
                    pulled += 1;
                    println!("{} {}", style("pulled").green().bold(), plan.display_name);
                }
                Err(e) => {
                    failed += 1;
                    println!(
                        "{} {}: {:#}",
                        style("failed").red().bold(),
                        plan.display_name,
                        e
                    );
                }
            }
        }

        println!(
            "{} pulled {}, unchanged {}, skipped {}, failed {}",
            style("pull complete:").green().bold(),
            pulled,
            unchanged,
            skipped,
            failed
        );

        if untracked_remote > 0 {
            let names: Vec<&str> = plans
                .iter()
                .filter(|plan| plan.local_name.is_none() && plan.status == SyncStatus::NewRemote)
                .map(|plan| plan.display_name.as_str())
                .collect();

            println!(
                "{}: {} file(s) exist on the server but are not tracked on this machine: {}. Run `config_share sync` to pull them",
                style("warning").yellow().bold(),
                untracked_remote,
                names.join(", "),
            );
        }

        if failed > 0 {
            bail!("{} file(s) failed to pull", failed);
        }

        Ok(())
    }
}

use anyhow::bail;
use console::style;

use crate::{
    config_share::{ConfigShare, sync},
    sync_analysis::{self, SyncStatus},
};

impl ConfigShare {
    /// Pushes the tracked files up to the server, overwriting the remote copies.
    ///
    /// Unlike `sync`, this never asks what to do: every tracked file that exists
    /// locally is uploaded, including files that have remote changes. With `force`,
    /// files that are already up to date are uploaded as well, which is how
    /// existing files are moved over to encryption after `config_share encrypt`.
    ///
    /// It can still ask for the encryption passphrase if it is not cached.
    pub fn push_files(&mut self, file: Option<&String>, force: bool) -> anyhow::Result<()> {
        self.require_sync_ready()?;

        let client = self.get_api_client()?;
        let remote = sync::fetch_manifest(&client)?;
        let local = sync::scan_local(&self.settings)?;
        let mut plans = sync_analysis::analyze(&self.settings, &local, &remote);

        if let Some(file) = file {
            sync::filter_plans_to_file(&mut plans, file)?;
        }

        let mut pushed = 0;
        let mut unchanged = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for plan in &plans {
            // This command only pushes files that are tracked on this machine
            let Some(name) = &plan.local_name else {
                continue;
            };

            match plan.status {
                SyncStatus::UpToDate => {
                    // Re-uploading an up-to-date file is only useful when forcing,
                    // e.g. to encrypt a file that was uploaded as plaintext before
                    if !force {
                        unchanged += 1;
                        continue;
                    }
                }
                SyncStatus::LocalMissing | SyncStatus::BothMissing => {
                    println!(
                        "{} {} (missing locally)",
                        style("skipped").yellow().bold(),
                        plan.display_name
                    );
                    skipped += 1;
                    continue;
                }
                SyncStatus::RemoteDeleted | SyncStatus::LocalChangedRemoteDeleted => {
                    println!(
                        "{} {} (deleted on the server, run `config_share sync` to re-upload it)",
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

            match self.push_file(&client, name, plan.local_hash, plan.remote_hash) {
                Ok(()) => {
                    pushed += 1;
                    println!("{} {}", style("pushed").green().bold(), plan.display_name);
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
            "{} pushed {}, unchanged {}, skipped {}, failed {}",
            style("push complete:").green().bold(),
            pushed,
            unchanged,
            skipped,
            failed
        );

        if failed > 0 {
            bail!("{} file(s) failed to push", failed);
        }

        Ok(())
    }
}

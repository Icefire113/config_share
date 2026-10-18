use std::path::Path;

use anyhow::{Context, bail};
use console::style;
use log::trace;

use crate::{config_share::ConfigShare, sync_analysis, util::paths_equal};

impl ConfigShare {
    /// Checks the local files against the state saved in the config, without talking
    /// to the server. This answers "has anything changed locally since the last sync".
    pub fn check_files(&self, file: Option<&String>) -> anyhow::Result<()> {
        trace!("[config_share.check_files] Checking file {:?}", file);

        let names: Vec<String> = match file {
            Some(path) => {
                let requested = crate::util::absolute_path(Path::new(path))?;

                let names: Vec<String> = self
                    .settings
                    .tracked_files
                    .iter()
                    .filter(|(_, tracked)| paths_equal(&tracked.file_path, &requested))
                    .map(|(name, _)| name.clone())
                    .collect();

                if names.is_empty() {
                    bail!("The file {} is not currently tracked", requested.display());
                }

                names
            }
            None => self.settings.tracked_files.keys().cloned().collect(),
        };

        if names.is_empty() {
            println!("{}", style("No files are currently tracked.").yellow());
            return Ok(());
        }

        let mut up_to_date = 0;
        let mut modified = 0;
        let mut not_synced = 0;
        let mut no_anchor = 0;
        let mut missing = 0;

        for name in names {
            let tracked = self
                .settings
                .tracked_files
                .get(&name)
                .with_context(|| format!("Tracked file {} not found", name))?;
            let local = sync_analysis::scan_local_file(&tracked.file_path)?;

            let (label, color) = match (
                tracked.id,
                tracked.synced_hash,
                local.as_ref().map(|l| l.hash),
            ) {
                (None, _, Some(_)) => {
                    not_synced += 1;
                    ("not synced yet", console::Color::Yellow)
                }
                (None, _, None) => {
                    missing += 1;
                    ("never synced, missing locally", console::Color::Red)
                }
                (Some(_), None, Some(_)) => {
                    no_anchor += 1;
                    ("no sync anchor", console::Color::Yellow)
                }
                (Some(_), _, None) => {
                    missing += 1;
                    ("missing locally", console::Color::Red)
                }
                (Some(_), Some(anchor), Some(hash)) if anchor == hash => {
                    up_to_date += 1;
                    ("up to date locally", console::Color::Green)
                }
                (Some(_), Some(_), Some(_)) => {
                    modified += 1;
                    ("modified locally", console::Color::Red)
                }
            };

            let path = tracked.file_path.display().to_string();
            if name == path {
                println!("{} {}", style(format!("{:<30}", label)).fg(color), name);
            } else {
                println!(
                    "{} {} ({})",
                    style(format!("{:<30}", label)).fg(color),
                    name,
                    path
                );
            }
        }

        println!();
        println!(
            "{} up to date, {} modified locally, {} not synced, {} without a sync anchor, {} missing locally",
            up_to_date, modified, not_synced, no_anchor, missing
        );

        if modified > 0 || not_synced > 0 || no_anchor > 0 || missing > 0 {
            println!("Run `config_share sync` to reconcile these files with the server.");
        }

        Ok(())
    }
}

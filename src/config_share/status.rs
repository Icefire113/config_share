use anyhow::{Context, bail};

use super::sync;
use crate::{config_share::ConfigShare, sync_analysis};

impl ConfigShare {
    /// Shows how the tracked files differ from the server, without changing anything.
    pub fn status_files(&self, file: Option<&String>, include_ignored: bool) -> anyhow::Result<()> {
        if self.settings.auth.is_none() {
            bail!("You are not logged in, run `config_share login` first");
        }

        let client = self
            .get_api_client()
            .context("Failed to create the API client")?;
        let remote = sync::fetch_manifest(&client)?;
        let local = sync::scan_local(&self.settings)?;
        let mut plans =
            sync_analysis::analyze_with_ignored(&self.settings, &local, &remote, include_ignored);

        if let Some(file) = file {
            sync::filter_plans_to_file(&mut plans, file)?;
        }

        if plans.is_empty() {
            println!("Nothing to show.");
            return Ok(());
        }

        sync::print_plan_table(&plans);
        Ok(())
    }
}

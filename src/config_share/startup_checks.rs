use anyhow::{Context, anyhow};
use chrono::{TimeDelta, Utc};
use console::style;
use log::trace;

use crate::config_share::{ConfigShare, init};

impl ConfigShare {
    /// Startup checks for the app after all settings are loaded
    pub fn all_startup_checks(&self) -> anyhow::Result<()> {
        // self.check_last_manual_sync()?;
        self.check_machine_id_blank()?;
        self.check_unsynced_files()?;
        Ok(())
    }

    pub fn check_last_manual_sync(&self) -> anyhow::Result<()> {
        trace!("[startup_checks.check_last_manual_sync] Checking last manual sync.");

        if self.settings.last_sync.is_none() {
            return self.sync_files();
        }

        let time_since_last_sync = Utc::now()
            .checked_sub_signed(TimeDelta::hours(self.settings.manual_sync_reminder_time))
            .with_context(||
                anyhow!("[startup_checks.check_last_manual_sync] Failed to subtract {} hours from the current date", 
                self.settings.manual_sync_reminder_time)
            )?;

        // if the last sync time is older than current time - sync reminder time inform user
        if self.settings.last_sync.unwrap() < time_since_last_sync {
            println!(
                "{}: It's been {} hours since your last manual sync, please sync with the server, or change reminder time in the config",
                style("warning").yellow().bold(),
                time_since_last_sync
            );
        }

        Ok(())
    }

    pub fn check_machine_id_blank(&self) -> anyhow::Result<()> {
        trace!("[startup_checks.check_machine_id] Checking machine id.");
        if self.settings.machine_id.trim().is_empty() {
            println!(
                "{}: The machine id in the config file is currently blank, please change it to something unique",
                style("warning").yellow().bold()
            );
            return Ok(());
        }
        Ok(())
    }

    /// Checks if any of the tracked files have not yet been synced with the server.
    /// A file is considered unsynced if it has no server-assigned id.
    pub fn check_unsynced_files(&self) -> anyhow::Result<()> {
        trace!("[startup_checks.check_unsynced_files] Checking for unsynced files.");
        let mut unsynced: Vec<String> = Vec::new();
        for tracked_file in self.settings.tracked_files.iter() {
            if tracked_file.1.id.is_none() {
                unsynced.push(tracked_file.0.clone());
            }
        }

        if unsynced.is_empty() {
            return Ok(());
        }

        println!(
            "{}: The following tracked files have not yet been synced with the server, run `config_share sync` to push them: {}",
            style("warning").yellow().bold(),
            unsynced.join(", "),
        );

        Ok(())
    }

    /// Checks if the config file is present
    pub fn check_is_config_setup(&self) -> anyhow::Result<()> {
        if !init::CONFIG_FILE_PATH.try_exists()? {
            return Err(anyhow!(
                "Config file does not exist, please use `config_share init` to set one up"
            ));
        }
        Ok(())
    }
}

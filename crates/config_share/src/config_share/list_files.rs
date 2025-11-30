use crate::config_share::ConfigShare;
use anyhow::Ok;
use console::style;
use util::display_option;

impl ConfigShare {
    pub fn list_files(&self) -> anyhow::Result<()> {
        if self.settings.tracked_files.is_empty() {
            println!("{}", style("No files are currently tracked.").yellow());
            return Ok(());
        }

        println!(
            "{: <40} | {: <30} | {: <20}",
            style("File Path").bold(),
            style("Last Modified").bold(),
            style("Custom Name").bold(),
        );
        println!("{}", style("-".repeat(100)));

        for file_info in self.settings.tracked_files.iter() {
            println!(
                "{: <40} | {: <20} | {: <30}",
                file_info.1.file_path.to_string_lossy(),
                file_info.1.file_last_modified.to_rfc2822(),
                display_option(file_info.1.use_custom_name.clone()),
            );
        }

        Ok(())
    }
}

use crate::config_share::ConfigShare;
use anyhow::Ok;
use console::style;
use util::get_path_as_string;

impl ConfigShare {
    pub fn list_files(&self) -> anyhow::Result<()> {
        if self.settings.tracked_files.is_empty() {
            println!("{}", style("No files are currently tracked.").yellow());
            return Ok(());
        }

        println!(
            "{: <40} | {: <20} | {: <30}",
            style("File Path").bold(),
            style("Method").bold(),
            style("Last Modified").bold()
        );
        println!("{}", style("-".repeat(100)));

        for file_info in self.settings.tracked_files.iter() {
            println!(
                "{: <40} | {: <20} | {: <30}",
                file_info.1.file_path.to_string_lossy(),
                file_info
                    .1
                    .use_custom_name
                    .clone()
                    .unwrap_or(get_path_as_string(file_info.1.file_path.as_path())?),
                file_info.1.file_last_modified.to_rfc2822()
            );
        }

        Ok(())
    }
}

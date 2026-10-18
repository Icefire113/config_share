use std::borrow::Cow;

use crate::config_share::ConfigShare;
use console::style;
use tabled::{Table, Tabled};

impl ConfigShare {
    pub fn list_files(&self) -> anyhow::Result<()> {
        if self.settings.tracked_files.is_empty() {
            println!("{}", style("No files are currently tracked.").yellow());
            return Ok(());
        }

        #[derive(Tabled)]
        struct Record<'a> {
            #[tabled(rename = "Local Path")]
            path: Cow<'a, str>,
            #[tabled(rename = "Last Modified")]
            last_modified: String,
            #[tabled(rename = "Name")]
            name: &'a str,
        }

        let table = Table::new(self.settings.tracked_files.iter().map(|file| Record {
            path: file.1.file_path.to_string_lossy(),
            last_modified: file.1.file_last_modified.to_rfc2822(),
            name: &file.1.name,
        }));
        println!("{}", table);

        Ok(())
    }
}

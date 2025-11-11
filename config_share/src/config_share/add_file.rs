use std::path::Path;

use anyhow::{Context, anyhow};
use strum::IntoEnumIterator;
use util::dprint;

use crate::{
    config_share::ConfigShare, file_differentation_method::FileDifferentationMethod,
    tracked_file_info::TrackedFileInfo,
};

impl ConfigShare {
    pub fn add_file(
        &mut self,
        file_path: &String,
        _ignore_permissions: bool,
        prompt_for_method: bool,
    ) -> anyhow::Result<()> {
        dprint(format!("[add_file.add_file] Adding file: {}", file_path));
        let path = Path::new(file_path);

        if !path.is_file() {
            return Err(anyhow!(
                "The file {} could not be found, or is not a file",
                file_path
            ));
        }

        let mut method = FileDifferentationMethod::FileName;

        if prompt_for_method {
            method = FileDifferentationMethod::iter()
                .nth(
                    dialoguer::Select::new()
                        .with_prompt("Select a method to differentiate between files")
                        .items::<&_, &Vec<_>>(&FileDifferentationMethod::iter().collect())
                        .default(0)
                        .interact()?,
                )
                .with_context(|| "Failed to select a method, this is a bug, please report it")?;
            if method == FileDifferentationMethod::CustomName {
                todo!("Prompt for the custom name");
            }
        }

        self.settings
            .tracked_files
            .push(TrackedFileInfo::create_from_filepath(path, method)?);

        Ok(())
    }
}

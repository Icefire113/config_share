use std::path::Path;

use anyhow::{Context, anyhow};
use strum::IntoEnumIterator;
use util::{display_option, dprint, get_path_as_string};

use crate::{
    config_share::ConfigShare, file_differentation_method::FileDifferentationMethod,
    tracked_file_info::TrackedFile,
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

            if method == FileDifferentationMethod::CustomName("".to_string()) {
                let custom_name = dialoguer::Input::<String>::new()
                    .allow_empty(false)
                    .default(
                        path.canonicalize()?
                            .to_str()
                            .with_context(|| anyhow!("Failed to get the canonicalized path and convert it to a string, does the file path contain invalid UTF-8?"))?
                            .to_string(),
                    )
                    .interact()?;

                dprint(format!(
                    "[config_share.add_file] Using custom_name = {}",
                    custom_name
                ));

                method = FileDifferentationMethod::CustomName(custom_name);
            }
        }

        if is_already_tracked(self, path)? {
            return Err(anyhow!(
                "The file {} is already tracked, custom name: {}",
                file_path,
                display_option(
                    self.settings
                        .tracked_files
                        .get(&get_path_as_string(path)?)
                        .with_context(|| anyhow!("[config_share.add_file] Failed to get from the hashmap after we just checked if it contains the key, wtf"))
                        .unwrap()
                        .differentation_method
                        .get_custom_name()
                )
            ));
        }

        self.settings.tracked_files.insert(
            if method == FileDifferentationMethod::FileName {
                get_path_as_string(path)?
            } else {
                method
                    .get_custom_name()
                    .with_context(
                        || anyhow!("[config_share.add_file] Failed to get the custom name when the method is not FileName, this is a bug, please report it")
                    )?
                    .clone()
            },
            TrackedFile::create_from_filepath(path, method, &self.settings.machine_id)?,
        );
        self.save_settings()?;

        Ok(())
    }
}

// stupid fuck
fn is_already_tracked(s: &ConfigShare, path: &Path) -> anyhow::Result<bool> {
    Ok(s.settings.tracked_files.values().any(|v| {
        dbg!(v);
        dbg!(path);
        v.file_path == *path
    }))
}

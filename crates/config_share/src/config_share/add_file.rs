use std::path::Path;

use anyhow::{Context, anyhow};
use util::{display_option, dprint, get_path_as_string};

use crate::{config_share::ConfigShare, tracked_file_info::TrackedFile};

impl ConfigShare {
    pub fn add_file(&mut self, file_path: &String, prompt_for_method: bool) -> anyhow::Result<()> {
        dprint(format!("[add_file.add_file] Adding file: {}", file_path));
        let path = Path::new(file_path).canonicalize().with_context(
                || anyhow!("[config_share.add_file] Failed to get the canonicalized path and convert it to a string, does the file path contain invalid UTF-8?")
            )?;

        if !path.try_exists()? || !path.is_file() {
            return Err(anyhow!(
                "The file {} could not be found, or is not a file",
                file_path
            ));
        }

        let methods = vec!["File Path", "Custom Name"];

        let mut custom_name: Option<String> = None;

        if prompt_for_method {
            let m = dialoguer::Select::new()
                .with_prompt("Select a method to differentiate between files")
                .items(methods)
                .default(0)
                .interact()?;
            if m == 1 {
                let name = dialoguer::Input::<String>::new()
                    .allow_empty(false)
                    .default(
                        path.canonicalize()?
                            .to_str()
                            .with_context(
                                || anyhow!("[config_share.add_file] Failed to get the canonicalized path and convert it to a string, does the file path contain invalid UTF-8?"))?
                            .to_string(),
                    )
                    .interact()?;

                dprint(format!(
                    "[config_share.add_file] Using custom_name = {}",
                    name
                ));
                custom_name = Some(name);
            }
        }

        // If we have already added the file to be tracked, don't add it again and inform the user
        if self
            .settings
            .tracked_files
            .iter()
            .any(|e| e.1.file_path == path)
        {
            return Err(anyhow!(
                "The file {} is already tracked, custom name: {}",
                file_path,
                display_option(
                    self.settings
                        .tracked_files
                        .get(&get_path_as_string(path.as_path())?)
                        .with_context(|| anyhow!("[config_share.add_file] Failed to get from the hashmap after we just checked if it contains the key, wtf"))
                        .unwrap()
                        .use_custom_name
                        .clone()
                )
            ));
        }

        self.settings.tracked_files.insert(
            if custom_name.is_none() {
                get_path_as_string(path.as_path())?
            } else {
                custom_name.clone()
                    .with_context(
                        || anyhow!("[config_share.add_file] Failed to get the custom name when the method is not FileName, this is a bug, please report it")
                    ).unwrap()
            },
            TrackedFile::create_from_filepath(path.as_path(), &self.settings.machine_id, custom_name)?,
        );
        self.save_settings()?;

        println!(
            "File {} has been added, you can now run `config_share sync` to sync the file to the server",
            file_path
        );

        Ok(())
    }
}

use std::path::Path;

use anyhow::{Context, anyhow};
use log::trace;

use crate::{config_share::ConfigShare, tracked_file_info::TrackedFile, util::get_path_as_string};

impl ConfigShare {
    pub fn add_file(&mut self, file_path: &String, prompt_for_method: bool) -> anyhow::Result<()> {
        trace!("[add_file.add_file] Adding file: {}", file_path);
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

                trace!("[config_share.add_file] Using custom_name = {}", name);
                custom_name = Some(name);
            }
        }

        // The display name is the custom name if one was chosen, otherwise the file path
        let name = if custom_name.is_some() {
            custom_name.clone().unwrap()
        } else {
            get_path_as_string(path.as_path())?
        };

        // If we have already added the file to be tracked, don't add it again and inform the user
        if self
            .settings
            .tracked_files
            .iter()
            .any(|e| e.1.file_path == path)
        {
            return Err(anyhow!(
                "The file {} is already tracked, name: {}",
                file_path,
                name
            ));
        }

        self.settings.tracked_files.insert(
            name.clone(),
            TrackedFile::create_from_filepath(path.as_path(), &self.settings.machine_id, name)?,
        );
        self.save_settings()?;

        println!(
            "File {} has been added, you can now run `config_share sync` to sync the file to the server",
            file_path
        );

        Ok(())
    }
}

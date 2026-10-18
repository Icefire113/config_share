use anyhow::anyhow;
use log::trace;

use crate::{
    config_share::ConfigShare,
    tracked_file_info::TrackedFile,
    util::{absolute_path, paths_equal},
};

impl ConfigShare {
    pub fn add_file(&mut self, file_path: &String, use_path_name: bool) -> anyhow::Result<()> {
        trace!("[add_file.add_file] Adding file: {}", file_path);
        let path = absolute_path(file_path)?;

        if !path.try_exists()? {
            return Err(anyhow!("The file {} could not be found", path.display()));
        }

        let methods = ["File Path", "Custom Name"];

        let mut custom_name: Option<String> = None;

        if !use_path_name {
            let m = dialoguer::Select::new()
                .with_prompt("Select a method to differentiate between files")
                .items(methods)
                .default(0)
                .interact()?;
            if m == 1 {
                let name = dialoguer::Input::<String>::new()
                    .allow_empty(false)
                    .default(path.display().to_string())
                    .interact_text()?;

                trace!("[config_share.add_file] Using custom_name = {}", name);
                custom_name = Some(name);
            }
        }

        // The display name is the custom name if one was chosen, otherwise the file path
        let name = custom_name.unwrap_or_else(|| path.display().to_string());

        // If we have already added the file to be tracked, don't add it again and inform the user
        if self
            .settings
            .tracked_files
            .iter()
            .any(|e| paths_equal(&e.1.file_path, &path))
        {
            return Err(anyhow!(
                "The file {} is already tracked, name: {}",
                path.display(),
                name
            ));
        }

        self.settings.tracked_files.insert(
            name.clone(),
            TrackedFile::create_from_filepath(&path, &self.settings.machine_id, name)?,
        );
        self.save_settings()?;

        println!(
            "File {} has been added, you can now run `config_share sync` to sync the file to the server",
            path.display()
        );

        Ok(())
    }
}

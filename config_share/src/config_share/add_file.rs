use std::path::Path;

use util::dprint;

use crate::config_share::ConfigShare;

impl ConfigShare {
    pub fn add_file(
        &mut self,
        file_path: &String,
        _ignore_permissions: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dprint(format!("[add_file.add_file] Adding file: {}", file_path));
        let path = Path::new(file_path);

        if !path.is_file() {
            return Err(format!(
                "The file {} could not be found, or is not a file",
                file_path
            )
            .into());
        }

        Ok(())
    }
}

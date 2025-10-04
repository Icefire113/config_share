use util::dprint;

use crate::config_share::ConfigShare;

impl ConfigShare {
    pub fn add_file(&mut self, file: String) -> Result<(), Box<dyn std::error::Error>> {
        dprint(format!("[add_file.add_file] Adding file: {}", file));
        Ok(())
    }
}

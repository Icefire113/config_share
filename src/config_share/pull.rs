use crate::config_share::ConfigShare;

impl ConfigShare {
    /// Pulls the tracked files down from the server, overwriting the local copies.
    pub fn pull_files(&mut self, _file: Option<&String>) -> anyhow::Result<()> {
        todo!("`config_share pull` is not implemented yet")
    }
}

use crate::config_share::ConfigShare;

impl ConfigShare {
    /// Shows the sync status of the tracked files without syncing with the server.
    pub fn status_files(&self, _file: Option<&String>) -> anyhow::Result<()> {
        todo!("`config_share status` is not implemented yet")
    }
}

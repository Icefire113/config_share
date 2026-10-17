use crate::config_share::ConfigShare;

impl ConfigShare {
    /// Pushes the tracked files up to the server, overwriting the remote copies.
    pub fn push_files(&mut self, _file: Option<&String>) -> anyhow::Result<()> {
        todo!("`config_share push` is not implemented yet")
    }
}

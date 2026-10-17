use crate::config_share::ConfigShare;

impl ConfigShare {
    pub fn sync_files(&self, _file: Option<&String>) -> anyhow::Result<()> {
        todo!("`config_share sync` is not implemented yet")
    }
}

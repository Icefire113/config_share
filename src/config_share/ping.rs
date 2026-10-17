use anyhow::Context;

use crate::config_share::ConfigShare;

impl ConfigShare {
    pub fn ping_server(&self) -> anyhow::Result<()> {
        let ver = self
            .get_api_client()?
            .version()
            .context("Error getting version from server")?;
        println!("Ping OK, server version: {}", ver);
        Ok(())
    }
}

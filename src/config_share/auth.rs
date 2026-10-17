use crate::config_share::ConfigShare;

impl ConfigShare {
    /// Authenticates with the server and saves the access token for later requests.
    ///
    /// If a token is given it is used directly, otherwise the user is prompted for credentials.
    pub fn login(&mut self, _token: Option<&str>) -> anyhow::Result<()> {
        todo!("`config_share login` is not implemented yet")
    }

    /// Revokes the saved access token on the server and removes it locally.
    pub fn logout(&mut self) -> anyhow::Result<()> {
        todo!("`config_share logout` is not implemented yet")
    }
}

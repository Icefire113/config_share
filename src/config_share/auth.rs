use anyhow::{Context, bail};
use api_types::{
    get_token_info::{GetTokenInfoReq, GetTokenInfoRes},
    request_token::RequestTokenReq,
    revoke_token::RevokeTokenReq,
};
use chrono::{TimeDelta, Utc};
use console::style;
use log::trace;

use crate::{
    api::{ApiClient, ApiClientError, ApiErrorCode},
    config_share::ConfigShare,
    share_settings::AuthInfo,
    token_store,
};

impl ConfigShare {
    /// Authenticates with the server and saves the access token in the OS keychain.
    ///
    /// If a token is given it is verified and stored directly, otherwise the user is
    /// prompted for their username and password to request a new token.
    pub fn login(&mut self, token: Option<&str>) -> anyhow::Result<()> {
        trace!(
            "[auth.login] Logging in, token supplied: {}",
            token.is_some()
        );

        let server_url = self.server_url()?;
        let stored_token = token_store::load_token(&server_url)?;

        // An expired token is useless, so let the user log in again without having to
        // log out first (which would fail to revoke the expired token on the server).
        if let Some(existing) = &self.settings.auth
            && stored_token.is_some()
            && existing
                .expires_at
                .is_none_or(|expires_at| expires_at > Utc::now())
        {
            bail!(
                "Already logged in as token '{}', run `config_share logout` first to log in with a different token",
                existing.name
            );
        }

        let client = ApiClient::new(server_url.clone());

        let (auth, token) = match token {
            Some(token) => {
                let token = token.trim();
                if token.is_empty() {
                    bail!("The provided token is empty");
                }
                Self::auth_from_token(&client, token)?
            }
            None => Self::auth_from_credentials(&client, self.default_token_name())?,
        };

        let token_name = auth.name.clone();
        let expires_at = auth.expires_at;

        token_store::save_token(&server_url, &token)?;
        self.settings.auth = Some(auth);

        // Don't leave a token in the keychain that the config file doesn't know about
        if let Err(e) = self.save_settings() {
            let _ = token_store::delete_token(&server_url);
            return Err(e);
        }

        println!(
            "{} token {} (expires {})",
            style("Logged in with").green().bold(),
            style(token_name).bold(),
            expires_at
                .map(|expires_at| expires_at.to_rfc2822())
                .unwrap_or_else(|| "never".to_string()),
        );

        Ok(())
    }

    /// Revokes the saved access token on the server and removes it from the OS keychain.
    pub fn logout(&mut self) -> anyhow::Result<()> {
        trace!("[auth.logout] Logging out");

        let server_url = self.server_url()?;

        let Some(auth) = self.settings.auth.clone() else {
            // There is no login metadata, but a token may still be left in the keychain
            if token_store::load_token(&server_url)?.is_some() {
                token_store::delete_token(&server_url)?;
                println!(
                    "{}",
                    style("Removed a leftover token from the OS keychain.").green()
                );
            } else {
                println!("{}", style("Not logged in.").yellow());
            }
            return Ok(());
        };

        match token_store::load_token(&server_url)? {
            Some(token) => {
                let client = ApiClient::new(server_url.clone()).with_token(token);

                match client.revoke_token(&RevokeTokenReq { id: auth.token_id }) {
                    Ok(()) => {}
                    // The token can no longer be used, so there is nothing left to revoke
                    Err(ApiClientError::Api { code, .. }) if is_token_already_invalid(&code) => {
                        trace!(
                            "[auth.logout] Token was already invalid on the server, code: {}",
                            code
                        );
                    }
                    // The server refuses to revoke the last valid token so that the user
                    // cannot lock themselves out, ask before leaving a valid token behind
                    Err(ApiClientError::Api {
                        code: ApiErrorCode::CannotRevokeAllTokens,
                        detail,
                        ..
                    }) => {
                        println!(
                            "{}",
                            style(detail.as_deref().unwrap_or(
                                "The server does not allow revoking your last valid token."
                            ))
                            .yellow()
                        );
                        let remove = dialoguer::Confirm::new()
                            .with_prompt(
                                "Remove the token from this machine anyway? It will stay valid on the server",
                            )
                            .default(false)
                            .interact()?;
                        if !remove {
                            println!("{}", style("Kept the token, still logged in.").yellow());
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        return Err(e).context(
                            "Failed to revoke the access token on the server, run `config_share logout` again to retry",
                        );
                    }
                }
            }
            None => {
                println!(
                    "{}",
                    style(
                        "No access token found in the OS keychain, removing the local login only."
                    )
                    .yellow()
                );
            }
        }

        token_store::delete_token(&server_url)?;
        self.settings.auth = None;
        self.save_settings()?;
        println!("{}", style("Logged out.").green());
        Ok(())
    }

    /// Verifies an existing token with the server and returns the metadata and the
    /// token that should be stored.
    fn auth_from_token(client: &ApiClient, token: &str) -> anyhow::Result<(AuthInfo, String)> {
        // `/api/auth/tokens/info` is itself authenticated, so the token has to be
        // attached to the client to inspect it
        let info: GetTokenInfoRes = client
            .clone()
            .with_token(token.to_string())
            .token_info(&GetTokenInfoReq {
                token: token.to_string(),
            })
            .context("Failed to verify the access token")?;

        if info.revoked_at.is_some() {
            bail!("The provided access token has been revoked");
        }
        if info.expires_at <= Utc::now() {
            bail!(
                "The provided access token expired on {}",
                info.expires_at.to_rfc2822()
            );
        }

        Ok((
            AuthInfo {
                token_id: info.id,
                name: info.name,
                expires_at: Some(info.expires_at),
            },
            token.to_string(),
        ))
    }

    /// Prompts for credentials and requests a new token from the server.
    fn auth_from_credentials(
        client: &ApiClient,
        default_token_name: String,
    ) -> anyhow::Result<(AuthInfo, String)> {
        let username = dialoguer::Input::<String>::new()
            .allow_empty(false)
            .with_prompt("Username")
            .interact_text()?;

        let password = dialoguer::Password::new()
            .with_prompt("Password")
            .interact()?;

        let token_name = dialoguer::Input::<String>::new()
            .allow_empty(false)
            .with_prompt("Token name (used to tell tokens apart on the server)")
            .with_initial_text(default_token_name)
            .interact_text()?;

        let duration_days = dialoguer::Input::<u32>::new()
            .with_prompt("Token duration in days")
            .default(30)
            .interact_text()?;

        if duration_days == 0 {
            bail!("The token duration must be at least 1 day");
        }

        let res = client
            .request_token(&RequestTokenReq {
                username,
                password,
                token_name: token_name.clone(),
                duration_days,
            })
            .context("Failed to log in")?;

        Ok((
            AuthInfo {
                token_id: res.token_id,
                name: token_name,
                // The server sets the expiry, this is only used to warn the user when
                // the token is (about to be) expired, so a small clock difference is fine
                expires_at: Some(Utc::now() + TimeDelta::days(duration_days as i64)),
            },
            res.token,
        ))
    }

    /// The name used for new tokens, preferring the machine id over the hostname.
    fn default_token_name(&self) -> String {
        if self.settings.machine_id.trim().is_empty() {
            gethostname::gethostname()
                .into_string()
                .unwrap_or_else(|_| "config_share".to_string())
        } else {
            self.settings.machine_id.clone()
        }
    }
}

/// Whether the error means the token is already unusable, in which case there is
/// nothing left to revoke on the server.
fn is_token_already_invalid(code: &ApiErrorCode) -> bool {
    matches!(
        code,
        // The token is unknown to the server
        ApiErrorCode::Unauthorized
            // Token expired
            | ApiErrorCode::TokenExpired
            // Token revoked
            | ApiErrorCode::TokenRevoked
            // Token not found
            | ApiErrorCode::TokenNotFound
    )
}

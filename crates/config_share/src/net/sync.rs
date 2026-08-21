use anyhow::{Context, anyhow};

use crate::{share_settings::ShareSettings, tracked_file_info::TrackedFile};

const ENDPOINT_QUERY_DISCRIM: &'static str = "/query_discrim";

/// Check if the hash of the remote file is the same as the hash of the local file,
/// return `Ok(true)` if they are the same, `Ok(false)` if they are not.
pub(crate) fn is_remote_hash_same(
    file: &TrackedFile,
    settings: &ShareSettings,
) -> anyhow::Result<bool> {
    if !settings.server_url.is_some() {
        return Err(anyhow!(
            "Remote Server URL is not setup, use `config_share init` to set one up"
        ));
    }
    let server_url = settings
        .server_url
        .as_ref()
        .unwrap()
        .join(ENDPOINT_QUERY_DISCRIM)
        .with_context(|| {
            anyhow!(
                "[net.is_remote_hash_same] Failed to join url, current server url {}",
                settings.server_url.as_ref().unwrap().as_str()
            )
        })?;
    let req_body = ureq::get(server_url.as_str())
        .force_send_body()
        .send(
            file.use_custom_name
                .as_ref()
                .unwrap_or(&file.file_path.display().to_string()),
        )
        .with_context(|| anyhow!("[net.is_remote_hash_same] Failed to make request"))?
        .body_mut()
        .read_to_string()?;

    todo!();
}

//! Secrets that live outside the config file: the access token and the encryption
//! passphrase.
//!
//! Both are scoped to the server, so that different servers do not overwrite each
//! other's entries.
//!
//! Secrets are normally kept in the OS keychain (Windows Credential Manager, macOS
//! Keychain, Secret Service on Linux). A machine without a usable keychain, such as
//! a headless Linux box, a container, or a CI runner, falls back to a `0600` JSON
//! file next to the config file instead. See [`configure`] for how to pick a store.

use std::{
    collections::BTreeMap,
    fs::{self, create_dir_all},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, anyhow};
use keyring::{Entry, Error as KeyringError};
use lazy_static::lazy_static;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

use crate::config_share::init::CONFIG_FILE_PATH;

/// The service name used for all keychain entries created by config_share.
const KEYRING_SERVICE: &str = "config_share";

/// The environment variable that selects where secrets are kept, so that cron
/// jobs and systemd units can pick a store without passing a flag.
const STORE_ENV: &str = "CONFIG_SHARE_SECRET_STORE";

lazy_static! {
    /// The fallback secrets file, kept next to the config file.
    static ref SECRETS_FILE: PathBuf = CONFIG_FILE_PATH
        .parent()
        .expect("The config file should have a parent directory")
        .join("secrets.json");
}

/// Where secrets are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preference {
    /// Use the OS keychain, and fall back to the secrets file when it is unusable.
    Auto,
    /// Only use the OS keychain; fail when it is unusable.
    Keyring,
    /// Only use the secrets file, without touching the OS keychain at all.
    File,
}

impl Preference {
    /// Parses a value from the command line or the environment.
    fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "keyring" | "keychain" => Ok(Self::Keyring),
            "file" => Ok(Self::File),
            other => Err(anyhow!(
                "Unknown secret store `{}`, expected `auto`, `keyring`, or `file`",
                other
            )),
        }
    }
}

/// The backend an operation ended up using, so that later operations in the same
/// run do not repeat a keychain call that is known to fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Keychain,
    File,
}

static PREFERENCE: Mutex<Preference> = Mutex::new(Preference::Auto);
static BACKEND: Mutex<Option<Backend>> = Mutex::new(None);
static WARNED: AtomicBool = AtomicBool::new(false);

/// Chooses where secrets are kept for this run.
///
/// The command line value wins over [`STORE_ENV`], which wins over the default of
/// choosing automatically. This has to be called before any secret is accessed;
/// `main` does that right after parsing the command line.
pub(crate) fn configure(option: Option<&str>) -> anyhow::Result<()> {
    let value = option.map(str::to_string).or_else(|| {
        std::env::var(STORE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
    });

    let preference = match value {
        Some(value) => Preference::parse(&value)?,
        None => Preference::Auto,
    };

    debug!("[secret_store.configure] preference: {:?}", preference);
    set_preference(preference);
    Ok(())
}

fn preference() -> Preference {
    *PREFERENCE.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_preference(preference: Preference) {
    *PREFERENCE.lock().unwrap_or_else(|e| e.into_inner()) = preference;
}

fn backend() -> Option<Backend> {
    *BACKEND.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_backend(backend: Backend) {
    *BACKEND.lock().unwrap_or_else(|e| e.into_inner()) = Some(backend);
}

/// Whether the error means the keychain cannot be used at all on this machine,
/// which is when the auto mode falls back to the secrets file.
fn is_unusable(err: &KeyringError) -> bool {
    matches!(
        err,
        KeyringError::NoDefaultStore
            | KeyringError::NoStorageAccess(_)
            | KeyringError::PlatformFailure(_)
    )
}

/// Wraps a keychain error with a hint about what to do when the store is unusable.
fn keychain_error(err: KeyringError) -> anyhow::Error {
    if is_unusable(&err) {
        anyhow!(
            "{} (no usable OS keychain on this machine: start a Secret Service provider such as \
             gnome-keyring, or keep secrets in a local file with `--secret-store file` or {} = file)",
            err,
            STORE_ENV
        )
    } else {
        anyhow!(err)
    }
}

/// Warns once per run that the keychain is not usable and the file is used instead.
fn warn_fallback(err: &KeyringError) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        warn!(
            "The OS keychain is not usable ({}), keeping secrets in {} instead. \
             Use `--secret-store keyring` to make this an error.",
            err,
            SECRETS_FILE.display()
        );
    }
}

/// Runs a load or save against the selected backend.
///
/// `None` from the keychain means it has no value for the account. That is not
/// the same as "nothing is stored": a keychain can be readable but unable to
/// create entries (for example when it has no default collection), in which case
/// a value saved after an earlier fallback lives in the secrets file while the
/// keychain keeps reporting that it has no entry. So in [`Preference::Auto`] mode
/// the file is consulted on a keychain miss as well.
fn run<T>(
    keychain: impl FnOnce() -> Result<Option<T>, KeyringError>,
    file: impl FnOnce() -> anyhow::Result<Option<T>>,
) -> anyhow::Result<Option<T>> {
    match preference() {
        Preference::File => {
            set_backend(Backend::File);
            file()
        }
        Preference::Keyring => keychain().map_err(keychain_error),
        Preference::Auto => {
            // Once this run has fallen back, stay on the file so a single run
            // never mixes the two backends.
            if backend() == Some(Backend::File) {
                return file();
            }

            match keychain() {
                Ok(Some(value)) => {
                    set_backend(Backend::Keychain);
                    Ok(Some(value))
                }
                Ok(None) => match file()? {
                    Some(value) => {
                        debug!(
                            "[secret_store.run] The OS keychain has no entry, using the secrets file"
                        );
                        set_backend(Backend::File);
                        Ok(Some(value))
                    }
                    None => Ok(None),
                },
                Err(e) if is_unusable(&e) => {
                    warn_fallback(&e);
                    set_backend(Backend::File);
                    file()
                }
                Err(e) => Err(keychain_error(e)),
            }
        }
    }
}

/// Describes a failed operation, naming the backend that was used.
fn describe(action: &str, account: &str) -> String {
    match backend() {
        Some(Backend::File) => format!(
            "Failed to {} the value in {} for {}",
            action,
            SECRETS_FILE.display(),
            account
        ),
        _ => format!(
            "Failed to {} the value in the OS keychain for {}",
            action, account
        ),
    }
}

/// The keychain account that a value is stored under, scoped to the server so
/// that different servers do not overwrite each other. `purpose` is empty for
/// the access token, which keeps existing token entries working.
fn account(server_url: &Url, purpose: &str) -> String {
    let server = match (server_url.host_str(), server_url.port()) {
        (Some(host), Some(port)) => format!("{}:{}", host, port),
        (Some(host), None) => host.to_string(),
        (None, _) => server_url.to_string(),
    };

    if purpose.is_empty() {
        server
    } else {
        format!("{}:{}", server, purpose)
    }
}

/// Saves a value for the given server and purpose.
fn save(server_url: &Url, purpose: &str, value: &str) -> anyhow::Result<()> {
    let account = account(server_url, purpose);

    let result = run(
        || keychain_save(&account, value).map(Some),
        || file_save(&account, value).map(Some),
    );

    // When the keychain took the value, drop a copy an earlier fallback left
    // behind so the two backends cannot disagree about it. Failing to clean up
    // is not worth failing the save over.
    if result.is_ok()
        && backend() == Some(Backend::Keychain)
        && let Err(e) = file_delete(&account)
    {
        debug!(
            "[secret_store.save] Could not remove a stale copy from the secrets file: {:#}",
            e
        );
    }

    result
        .map(|_| ())
        .with_context(|| describe("save", &account))
}

/// Loads a value for the given server and purpose, if there is one.
fn load(server_url: &Url, purpose: &str) -> anyhow::Result<Option<String>> {
    let account = account(server_url, purpose);
    run(|| keychain_load(&account), || file_load(&account))
        .with_context(|| describe("read", &account))
}

/// Deletes a value for the given server and purpose.
///
/// Deleting a value that is not there is not an error. In [`Preference::Auto`]
/// mode both backends are cleared, so a logout never leaves a copy behind.
fn delete(server_url: &Url, purpose: &str) -> anyhow::Result<()> {
    let account = account(server_url, purpose);

    match preference() {
        Preference::File => {
            set_backend(Backend::File);
            file_delete(&account).with_context(|| describe("delete", &account))
        }
        Preference::Keyring => {
            keychain_delete(&account)
                .map_err(keychain_error)
                .with_context(|| describe("delete", &account))?;
            set_backend(Backend::Keychain);
            Ok(())
        }
        Preference::Auto => {
            if backend() != Some(Backend::File) {
                match keychain_delete(&account) {
                    Ok(()) => set_backend(Backend::Keychain),
                    Err(e) if is_unusable(&e) => {
                        warn_fallback(&e);
                        set_backend(Backend::File);
                    }
                    Err(e) => {
                        return Err(keychain_error(e))
                            .with_context(|| describe("delete", &account));
                    }
                }
            }

            // Errors from the file already name it, so they are not wrapped with
            // a message about the keychain
            file_delete(&account)
        }
    }
}

/// Saves the access token for the given server.
pub fn save_token(server_url: &Url, token: &str) -> anyhow::Result<()> {
    save(server_url, "", token)
}

/// Loads the access token for the given server, if there is one.
pub fn load_token(server_url: &Url) -> anyhow::Result<Option<String>> {
    load(server_url, "")
}

/// Deletes the access token for the given server.
pub fn delete_token(server_url: &Url) -> anyhow::Result<()> {
    delete(server_url, "")
}

/// Saves the encryption passphrase for the given server.
pub fn save_encryption_passphrase(server_url: &Url, passphrase: &str) -> anyhow::Result<()> {
    save(server_url, "encryption", passphrase)
}

/// Loads the encryption passphrase for the given server, if there is one.
pub fn load_encryption_passphrase(server_url: &Url) -> anyhow::Result<Option<Zeroizing<String>>> {
    Ok(load(server_url, "encryption")?.map(Zeroizing::new))
}

/// Deletes the encryption passphrase for the given server.
pub fn delete_encryption_passphrase(server_url: &Url) -> anyhow::Result<()> {
    delete(server_url, "encryption")
}

/*
 * OS keychain backend
 */

fn keychain_save(account: &str, value: &str) -> Result<(), KeyringError> {
    Entry::new(KEYRING_SERVICE, account)?.set_password(value)
}

fn keychain_load(account: &str) -> Result<Option<String>, KeyringError> {
    match Entry::new(KEYRING_SERVICE, account)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(e),
    }
}

fn keychain_delete(account: &str) -> Result<(), KeyringError> {
    match Entry::new(KEYRING_SERVICE, account)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}

/*
 * Secrets file backend
 */

/// The on-disk format of the fallback store. Keys are the same account names that
/// are used for keychain entries, so the two backends stay interchangeable.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SecretsFile {
    #[serde(default)]
    entries: BTreeMap<String, String>,
}

fn file_save(account: &str, value: &str) -> anyhow::Result<()> {
    save_in(SECRETS_FILE.as_path(), account, value)
}

fn file_load(account: &str) -> anyhow::Result<Option<String>> {
    load_from(SECRETS_FILE.as_path(), account)
}

fn file_delete(account: &str) -> anyhow::Result<()> {
    delete_in(SECRETS_FILE.as_path(), account)
}

fn save_in(path: &Path, account: &str, value: &str) -> anyhow::Result<()> {
    let mut secrets = read_secrets(path)?;
    secrets
        .entries
        .insert(account.to_string(), value.to_string());
    write_secrets(path, &secrets)
}

fn load_from(path: &Path, account: &str) -> anyhow::Result<Option<String>> {
    Ok(read_secrets(path)?.entries.remove(account))
}

fn delete_in(path: &Path, account: &str) -> anyhow::Result<()> {
    let mut secrets = read_secrets(path)?;
    if secrets.entries.remove(account).is_none() {
        return Ok(());
    }
    write_secrets(path, &secrets)
}

fn read_secrets(path: &Path) -> anyhow::Result<SecretsFile> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        // A missing file is simply an empty store
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SecretsFile::default()),
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to read {}", path.display()));
        }
    };

    ensure_private_permissions(path)?;

    serde_json::from_str(&data).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_secrets(path: &Path, secrets: &SecretsFile) -> anyhow::Result<()> {
    let dir = path
        .parent()
        .context("The secrets file should have a parent directory")?;
    create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    create_private_dir(dir)?;

    let mut json = serde_json::to_vec_pretty(secrets).context("Failed to serialize the secrets")?;
    json.push(b'\n');

    // Write to a temp file and rename, so an interrupted run cannot leave a
    // truncated secrets file behind
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file =
        fs::File::create(&tmp).with_context(|| format!("Failed to create {}", tmp.display()))?;
    create_private_file(&tmp)?;
    file.write_all(&json)
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    file.sync_all()
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    drop(file);

    fs::rename(&tmp, path).with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

/// Creates the directory that holds the secrets file with permissions that keep
/// other users out.
#[cfg(unix)]
fn create_private_dir(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("Failed to set the permissions of {}", dir.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn create_private_dir(_dir: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Restricts a newly written secrets file to its owner.
#[cfg(unix)]
fn create_private_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set the permissions of {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn create_private_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Restricts an existing secrets file that other users can read, so a file
/// created by hand or by an older version does not stay world readable.
#[cfg(unix)]
fn ensure_private_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .with_context(|| format!("Failed to read the permissions of {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        warn!(
            "{} is readable by other users, restricting it to 0600",
            path.display()
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set the permissions of {}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty directory for a test, next to the secrets file it will hold.
    fn test_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "config_share_secrets_{}_{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("secrets.json")
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let path = test_path("missing");
        assert!(load_from(&path, "example.com").unwrap().is_none());
        assert!(!path.exists());
    }

    #[test]
    fn save_load_and_delete_round_trip() {
        let path = test_path("round_trip");

        save_in(&path, "a.example.com", "token-a").unwrap();
        save_in(&path, "b.example.com", "token-b").unwrap();

        assert_eq!(
            load_from(&path, "a.example.com").unwrap().as_deref(),
            Some("token-a")
        );
        assert_eq!(
            load_from(&path, "b.example.com").unwrap().as_deref(),
            Some("token-b")
        );
        assert!(load_from(&path, "c.example.com").unwrap().is_none());

        delete_in(&path, "a.example.com").unwrap();
        assert!(load_from(&path, "a.example.com").unwrap().is_none());
        assert_eq!(
            load_from(&path, "b.example.com").unwrap().as_deref(),
            Some("token-b")
        );

        // Deleting something that is not there is not an error
        delete_in(&path, "a.example.com").unwrap();

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saving_replaces_the_previous_value() {
        let path = test_path("replace");

        save_in(&path, "a.example.com", "old").unwrap();
        save_in(&path, "a.example.com", "new").unwrap();
        assert_eq!(
            load_from(&path, "a.example.com").unwrap().as_deref(),
            Some("new")
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn malformed_file_is_an_error() {
        let path = test_path("malformed");
        create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not json").unwrap();

        assert!(load_from(&path, "a.example.com").is_err());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn parses_preferences() {
        assert_eq!(Preference::parse("auto").unwrap(), Preference::Auto);
        assert_eq!(Preference::parse("KEYRING").unwrap(), Preference::Keyring);
        assert_eq!(
            Preference::parse(" keychain ").unwrap(),
            Preference::Keyring
        );
        assert_eq!(Preference::parse("file").unwrap(), Preference::File);
        assert!(Preference::parse("database").is_err());
    }

    // The backend selection is process-wide, so this test is the only one that
    // touches it and restores the defaults at the end.
    #[test]
    fn auto_mode_picks_a_usable_backend() {
        set_preference(Preference::Auto);

        // The keychain is unusable: the file takes over
        let unusable = || {
            Err::<Option<&str>, KeyringError>(KeyringError::NoStorageAccess(
                "no default collection".into(),
            ))
        };
        let value = run(unusable, || Ok(Some("from the file"))).unwrap();
        assert_eq!(value, Some("from the file"));

        // The rest of the run stays on the file instead of retrying the keychain
        let value = run(
            || panic!("the keychain must not be retried after falling back"),
            || Ok(Some("from the file")),
        )
        .unwrap();
        assert_eq!(value, Some("from the file"));

        // A keychain that has no entry is not proof that nothing is stored: a value
        // saved after an earlier fallback is in the file
        set_backend(Backend::Keychain);
        let value = run(
            || Ok::<Option<&str>, KeyringError>(None),
            || Ok(Some("from the file")),
        )
        .unwrap();
        assert_eq!(value, Some("from the file"));

        // A keychain value wins over a leftover file copy
        set_backend(Backend::Keychain);
        let value = run(
            || Ok::<Option<&str>, KeyringError>(Some("from the keychain")),
            || panic!("the file must not be read when the keychain has the value"),
        )
        .unwrap();
        assert_eq!(value, Some("from the keychain"));

        // Neither backend has the value
        set_backend(Backend::Keychain);
        let value = run(|| Ok::<Option<&str>, KeyringError>(None), || Ok(None)).unwrap();
        assert_eq!(value, None);

        // An explicit choice never falls back
        set_preference(Preference::File);
        set_backend(Backend::Keychain);
        let value = run(
            || panic!("the keychain must not be used with `--secret-store file`"),
            || Ok(Some("from the file")),
        )
        .unwrap();
        assert_eq!(value, Some("from the file"));

        set_preference(Preference::Keyring);
        let result = run(
            || Err::<Option<&str>, KeyringError>(KeyringError::NoDefaultStore),
            || panic!("the file must not be used with `--secret-store keyring`"),
        );
        assert!(result.is_err());

        set_preference(Preference::Auto);
        set_backend(Backend::Keychain);
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use chrono::{DateTime, Local, Utc};
use console::style;
use uuid::Uuid;

/// Formats a UTC timestamp in the user's local time zone.
pub fn format_local_time(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local).to_rfc2822()
}

/// Expands a leading `~` and resolves a path to an absolute path.
///
/// This is purely lexical: the path does not have to exist and the filesystem is not
/// touched, so it can never fail just because a parent directory is missing.
pub fn absolute_path(path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let path = path.as_ref();

    if path.as_os_str().is_empty() {
        bail!("The path is empty");
    }

    // Only expand a whole `~` or a `~/` prefix, so that `~otheruser` is left alone
    let path = match path.to_str() {
        Some("~") => {
            std::env::home_dir().context("Could not find the home directory to expand `~`")?
        }
        Some(p) if p.starts_with("~/") || p.starts_with("~\\") => {
            let home =
                std::env::home_dir().context("Could not find the home directory to expand `~`")?;
            home.join(&p[2..])
        }
        _ => path.to_path_buf(),
    };

    let absolute = std::path::absolute(&path);
    absolute.with_context(|| format!("Failed to resolve the path {path:?}"))
}

/// Whether two paths point at the same location, compared the way the platform's
/// filesystem does: case-insensitively on Windows, exactly everywhere else.
///
/// Verbatim (`\\?\`) prefixes are reduced first, so that paths written by older
/// versions of config_share compare equal to plain absolute paths.
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    let a = dunce::simplified(a);
    let b = dunce::simplified(b);

    #[cfg(windows)]
    {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Writes a file by first writing a temporary file in the same directory and then
/// renaming it into place, so that a failed write never leaves a half written file
/// behind. Missing parent directories are created.
pub fn write_file_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create the directory {}", parent.display()))?;

    let file_name = path
        .file_name()
        .with_context(|| format!("{} does not point to a file", path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.{}.config_share.tmp",
        file_name.to_string_lossy(),
        Uuid::new_v4()
    ));

    if let Err(e) = fs::write(&temp_path, contents) {
        cleanup_temp_file(&temp_path);
        return Err(e).with_context(|| format!("Failed to write {}", temp_path.display()));
    }

    // Keep the permissions of the file we are replacing, new files default to
    // only being readable by the current user since they often contain secrets
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(path)
            .map(|metadata| metadata.permissions().mode())
            .unwrap_or(0o600);

        if let Err(e) = fs::set_permissions(&temp_path, fs::Permissions::from_mode(mode)) {
            cleanup_temp_file(&temp_path);
            return Err(e)
                .with_context(|| format!("Failed to set permissions on {}", temp_path.display()));
        }
    }

    if let Err(e) = fs::rename(&temp_path, path) {
        cleanup_temp_file(&temp_path);
        return Err(e).with_context(|| format!("Failed to replace {}", path.display()));
    }

    Ok(())
}

/// Removes a temporary file, warning the user if it could not be deleted.
fn cleanup_temp_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        // Nothing to clean up
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!(
            "{}: failed to remove the temporary file {}: {}",
            style("warning").yellow().bold(),
            path.display(),
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_path_works_for_missing_files_and_parents() {
        let dir = std::env::temp_dir();
        let path = absolute_path(dir.join("config_share_missing_dir").join("file")).unwrap();

        assert!(path.is_absolute());
        assert!(path.ends_with(r"config_share_missing_dir\file"));
    }

    #[test]
    fn absolute_path_keeps_absolute_input() {
        let dir = std::env::temp_dir();
        assert_eq!(absolute_path(&dir).unwrap(), dir);
    }

    #[test]
    fn absolute_path_expands_tilde() {
        let home = std::env::home_dir().unwrap();

        assert_eq!(absolute_path("~").unwrap(), home);
        assert_eq!(
            absolute_path("~/config_share_missing_file").unwrap(),
            home.join("config_share_missing_file")
        );
    }

    #[test]
    fn paths_equal_ignores_case_and_verbatim_prefix() {
        let plain = Path::new(r"C:\Users\me\file");

        assert!(paths_equal(plain, Path::new(r"c:\USERS\ME\FILE")));
        // paths written by older versions of config_share carried this prefix
        assert!(paths_equal(plain, Path::new(r"\\?\C:\Users\me\file")));
        assert!(!paths_equal(plain, Path::new(r"C:\Users\me\other")));
    }

    #[test]
    fn absolute_path_rejects_empty() {
        assert!(absolute_path("").is_err());
    }
}

use std::{fmt::Display, path::{Path, PathBuf}};

use anyhow::{Context, anyhow};

/// Util function for printing debug messages only when debug assertions are present
/// (as determined by `cfg(debug_assertions)`), messages will be printed to `stderr`
#[cfg(not(debug_assertions))]
pub fn dprint<S: Into<String> + Display>(_msg: S) {}

/// Util function for printing debug messages only when debug assertions are present
/// (as determined by `cfg(debug_assertions)`), messages will be printed to `stderr`
#[cfg(debug_assertions)]
pub fn dprint<S: Into<String> + Display>(msg: S) {
    eprintln!("[DEBUG] {}", msg);
}

pub fn get_path_as_string(path: &Path) -> anyhow::Result<String> {
    Ok(path
        .canonicalize()?
        .to_str()
        .with_context(
            || anyhow!("[util.get_path_as_string] Failed to get the canonicalized path and convert it to a string, does the file path contain invalid UTF-8?"))?
            .to_string()
        )
}

pub fn get_pathbuf_as_string(path: &PathBuf) -> anyhow::Result<String> {
    Ok(path
        .canonicalize()?
        .to_str()
        .with_context(
            || anyhow!("[util.get_path_as_string] Failed to get the canonicalized path and convert it to a string, does the file path contain invalid UTF-8?"))?
            .to_string()
        )
}

pub fn display_option(o: Option<impl Display>) -> String {
    match o {
        Some(value) => value.to_string(),
        None => "None".to_string(),
    }
}

pub fn does_file_exist(file_path: &String) -> anyhow::Result<bool> {
    Ok(Path::new(file_path).try_exists()?)
}

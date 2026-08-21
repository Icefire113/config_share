use std::fmt::Display;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

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

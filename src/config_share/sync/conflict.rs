use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use console::style;
use dialoguer::{Confirm, Select};
use similar::{ChangeTag, TextDiff};
use twox_hash::XxHash3_64;

use crate::{
    api::ApiClient,
    config_share::ConfigShare,
    sync_analysis::{LocalFileState, SyncPlan},
    util::write_file_atomic,
};

use super::{Outcome, plan_name};

impl ConfigShare {
    pub(super) fn resolve_conflict(
        &mut self,
        client: &ApiClient,
        plan: &SyncPlan,
        local: &HashMap<String, LocalFileState>,
        yes: bool,
    ) -> anyhow::Result<Outcome> {
        if yes {
            return Ok(Outcome::Skipped);
        }

        let name = plan_name(plan)?;

        if let Some(id) = plan.remote_id
            && let Ok(info) = client.get_file_info(&id)
        {
            println!(
                "  server: updated by {} on {}",
                info.last_updated_from,
                info.updated_at.to_rfc2822()
            );
        }
        if let Some(state) = local.get(&name) {
            println!("  local:  modified on {}", state.last_modified.to_rfc2822());
        }

        let choices = [
            "Push the local file, overwriting the server copy",
            "Pull the server copy, overwriting the local file",
            "Show a diff between the local and server copies",
            "Write the server copy next to the local file and resolve it later",
            "Skip this file",
        ];

        loop {
            let choice = Select::new()
                .with_prompt(format!("{} changed on both sides", plan.display_name))
                .items(choices)
                .default(4)
                .interact()?;

            match choice {
                0 => {
                    self.push_file(client, &name, plan.local_hash, plan.remote_hash)?;
                    return Ok(Outcome::Pushed);
                }
                1 => {
                    self.pull_file(client, &name, plan.remote_hash, plan.local_hash, true)?;
                    return Ok(Outcome::Pulled);
                }
                2 => {
                    self.show_conflict_diff(client, &name)?;
                }
                3 => {
                    self.write_remote_sidecar(client, &name)?;
                    return Ok(Outcome::Skipped);
                }
                _ => return Ok(Outcome::Skipped),
            }
        }
    }

    /// Prints a unified diff between the local file and the server copy.
    fn show_conflict_diff(&self, client: &ApiClient, name: &str) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let id = tracked
            .id
            .context("This file has not been uploaded to the server yet")?;

        let local = fs::read(&tracked.file_path)
            .with_context(|| format!("Failed to read {}", tracked.file_path.display()))?;
        let remote = client
            .get_file_contents(&id)
            .with_context(|| format!("Failed to download {}", tracked.name))?;

        print_diff(&tracked.file_path, &local, &remote);
        Ok(())
    }

    /// Saves the server copy next to the local file so that it can be merged by hand.
    fn write_remote_sidecar(&self, client: &ApiClient, name: &str) -> anyhow::Result<()> {
        let tracked = self.tracked(name)?;
        let id = tracked
            .id
            .context("This file has not been uploaded to the server yet")?;
        let sidecar = sidecar_path(&tracked.file_path)?;

        if sidecar.exists() {
            let overwrite = Confirm::new()
                .with_prompt(format!(
                    "{} already exists, overwrite it?",
                    sidecar.display()
                ))
                .default(false)
                .interact()?;
            if !overwrite {
                return Ok(());
            }
        }

        let remote = client
            .get_file_contents(&id)
            .with_context(|| format!("Failed to download {}", tracked.name))?;

        write_file_atomic(&sidecar, &remote)
            .with_context(|| format!("Failed to write {}", sidecar.display()))?;

        println!("Wrote the server copy to {}", sidecar.display());
        println!(
            "Merge the two files, then run `config_share sync` and choose \"Push the local file\"."
        );

        Ok(())
    }
}

/// Prints a unified diff between the local file and the server copy, falling back to
/// a size/hash summary for files that are not text.
fn print_diff(local_path: &Path, local: &[u8], remote: &[u8]) {
    for line in format_diff(local_path, local, remote).lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            println!("{}", style(line).bold());
        } else if line.starts_with('-') {
            println!("{}", style(line).red());
        } else if line.starts_with('+') {
            println!("{}", style(line).green());
        } else {
            println!("{line}");
        }
    }
}

/// Builds a plain unified diff between the local file and the server copy, falling
/// back to a size/hash summary for files that are not text.
fn format_diff(local_path: &Path, local: &[u8], remote: &[u8]) -> String {
    let (Ok(local_text), Ok(remote_text)) =
        (std::str::from_utf8(local), std::str::from_utf8(remote))
    else {
        return format!(
            "The file is not valid UTF-8, showing sizes and hashes instead:\n  local:  {} bytes, hash {:016x}\n  server: {} bytes, hash {:016x}",
            local.len(),
            XxHash3_64::oneshot(local),
            remote.len(),
            XxHash3_64::oneshot(remote)
        );
    };

    let mut out = format!("--- local ({})\n+++ server\n", local_path.display());
    let diff = TextDiff::from_lines(local_text, remote_text);

    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches(['\n', '\r']);
        let sign = match change.tag() {
            ChangeTag::Delete => '-',
            ChangeTag::Insert => '+',
            ChangeTag::Equal => ' ',
        };
        out.push(sign);
        out.push_str(text);
        out.push('\n');
    }

    out
}

/// The path next to the local file where the server copy is written, e.g.
/// `config.json` becomes `config.json.remote`.
fn sidecar_path(path: &Path) -> anyhow::Result<PathBuf> {
    let file_name = path
        .file_name()
        .with_context(|| format!("{} does not point to a file", path.display()))?;

    let mut sidecar_name = file_name.to_os_string();
    sidecar_name.push(".remote");

    Ok(path.with_file_name(sidecar_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diff_shows_both_sides() {
        let diff = format_diff(Path::new("config.json"), b"one\ntwo\n", b"one\nthree\n");

        assert!(diff.starts_with("--- local (config.json)\n+++ server\n"));
        assert!(diff.contains("-two"));
        assert!(diff.contains("+three"));
        assert!(diff.contains(" one"));
    }

    #[test]
    fn format_diff_falls_back_for_binary() {
        let diff = format_diff(Path::new("blob"), &[0xff, 0xfe], &[0x00]);

        assert!(diff.contains("not valid UTF-8"));
        assert!(diff.contains("local:  2 bytes"));
        assert!(diff.contains("server: 1 bytes"));
    }

    #[test]
    fn sidecar_path_appends_remote() {
        assert_eq!(
            sidecar_path(Path::new("config.json")).unwrap(),
            PathBuf::from("config.json.remote")
        );
    }
}

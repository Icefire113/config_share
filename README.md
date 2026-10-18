# Config Share

A minimal CLI tool for syncing config files across multiple machines, for use with
the [sync server](https://github.com/Icefire113/sync_server).

Track a file once on each machine, then run `config_share sync` to keep the local
copies and the server copy in step. Sync is interactive where it matters and
automatic where it is safe.

- **Three-way sync**: tracks the hash both sides had at the last successful
  `sync`, so it can tell "I changed it" from "another machine changed it".
- **Server is authoritative for identity**: files are matched by server id, not
  by path or name.
- **No secrets in the config file**: the access token lives in the OS keychain
  (Windows Credential Manager, macOS Keychain, or Secret Service on Linux).
- **Safe writes**: downloads are hash-verified, then written to a temp file and
  renamed into place. Existing permissions are preserved; new files are created
  `0600` on Unix.

## Requirements

- Rust (edition 2024, so Rust 1.85 or newer) to build.
- A running [sync server](https://github.com/Icefire113/sync_server) and an
  account on it. TLS verification uses the host's certificate store
  (`rustls-platform-verifier`), so a self-signed root installed in the OS trust
  store is accepted.
- An OS keychain. On headless Linux you need a Secret Service provider such as
  GNOME Keyring or KWallet.

## Install

```sh
git clone https://github.com/Icefire113/config_share.git
cd config_share
cargo install --path .
```

Or build without installing and use `./target/release/config_share`.

## Quick start

```sh
config_share init            # pick a server URL and a machine name
config_share login           # username/password, token is stored in the keychain
config_share add ~/.gitconfig
config_share sync
```

On another machine:

```sh
config_share init
config_share login
config_share sync            # offers to download the tracked files
```

`init` writes `~/.config/config_share/config.json` on every platform (on Windows
that is `C:\Users\<you>\.config\config_share\config.json`). It always asks for
the server URL and machine name; `init --overwrite` resets the config and forgets
the locally saved token for the old server (it is not revoked on the server).

## Commands

| Command                                               | What it does                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- |
| `init [--overwrite]`                                  | Create or reset the config file and prompt for server + machine id              |
| `login [--token <TOKEN>]`                             | Request a token with credentials, or save an existing token                     |
| `logout [--force]`                                    | Revoke the token on the server and forget it; `--force` only forgets it locally |
| `add [--use-path] <file>`                             | Start tracking a file; prompts for a naming method unless `--use-path`          |
| `remove [--remote] <file>`                            | Stop tracking a file; `--remote` also deletes the server copy                   |
| `list`                                                | Show tracked files, paths, and last-modified times (local time)                 |
| `sync [--dry-run] [--yes] [--include-ignored] [file]` | Reconcile with the server                                                       |
| `status [--include-ignored] [file]`                   | Show the sync state without changing anything                                   |
| `check [file]`                                        | Offline: re-hash local files and compare them with the saved anchor             |
| `pull [file]`                                         | Force-download tracked files (refuses to overwrite unsynced local changes)      |
| `push [file]`                                         | Force-upload tracked files (overwrites the server copies)                       |
| `ping`                                                | Print the server version                                                        |

All commands that take `[file]` accept a local path.

### How `sync` decides

`sync` fetches the server file list and hashes the local files, then:

| State                         | What happens                                                                     |
| ----------------------------- | -------------------------------------------------------------------------------- |
| Up to date                    | nothing                                                                          |
| Only the local file changed   | pushed automatically                                                             |
| Only the servler copy changed | downloaded automatically                                                         |
| Both sides changed            | prompt: push, pull, show a diff, write the server copy next to the file, or skip |
| Deleted on the server         | prompt: delete local + untrack, untrack only, re-upload as new, or skip          |
| Missing locally               | prompt: download it back, untrack, or skip                                       |
| New on the server             | prompt: download to a chosen path, ignore it on this machine, or skip            |
| Tracked but never uploaded    | prompt: upload as a new file, link it to an existing server file, or skip        |
| Missing locally and remotely  | untracked automatically                                                          |

`--dry-run` prints the plan table without touching anything. `--yes` applies the
automatic cases and skips everything that needs a decision.

### Resolving a conflict

When both sides changed, the prompt offers a unified diff and a "write it next to
the file" option. The latter saves the server copy as `<file>.remote`; merge it
by hand, then run `sync` again and choose **Push the local file**.

### Ignored server files

Choosing **Ignore it on this machine from now on** records the file id in the
config and hides it from future runs. Use `sync --include-ignored` or
`status --include-ignored` to see those files again; downloading one removes it
from the ignored list.

## How it works

- File contents are compared with xxhash3 (64bit). The server stores the same hash.
- `TrackedFile.synced_hash` is the hash both sides had after the last successful
  sync. `check` compares the current local hash with it, which is why `check`
  works offline.
- Pushes send the hash the server is expected to have (`expected_hash`), so a
  concurrent update from another machine fails with a conflict instead of being
  silently overwritten.
- Server file names are not unique, which is why everything is keyed by id and
  "link to an existing file" is an explicit choice rather than a name match.

## Config file

```json
{
  "server_url": "http://localhost:3000/",
  "machine_id": "desktop",
  "tracked_files": {
    "/home/me/.gitconfig": {
      "id": "…uuid…",
      "name": "/home/me/.gitconfig",
      "file_path": "/home/me/.gitconfig",
      "synced_hash": 123456789,
      "file_last_modified": "2026-10-18T07:00:00Z",
      "last_checked": "2026-10-18T07:00:00Z",
      "updated_at": null,
      "last_updated_from": null,
      "path_per_machine": { "desktop": "/home/me/.gitconfig" }
    }
  },
  "last_sync": "2026-10-18T07:00:00Z",
  "manual_sync_reminder_time": 168,
  "auth": {
    "token_id": 1,
    "name": "desktop",
    "expires_at": "2026-11-17T07:00:00Z"
  },
  "ignored_remote_files": []
}
```

The token itself is **not** in this file; `auth` only keeps the metadata needed to
revoke it. `manual_sync_reminder_time` is in hours and controls the "it has been a
while" warning shown by most commands.

## Notes and limitations

- File permissions are not synced yet.
- `remove` is local-only unless `--remote` is passed.
- There is no automatic three-way text merge; use the diff and `.remote` sidecar.
- `pull` deliberately refuses to overwrite local files that have unsynced changes;
  resolve those with `sync` instead.
- `pull` only pulls files already tracked on this machine. It warns about server
  files that are not tracked here and points at `sync`.
- The config path is fixed; there is no `--config` override yet.
- Ignored files can be viewed again with `--include-ignored`, but there is no
  dedicated command to list or bulk-unignore them.
- `sync` exits non-zero if any file failed; skipped decisions do not count as
  failures.

## License

BSD 3-Clause. See [LICENSE](LICENSE).

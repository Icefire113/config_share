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
- **No secrets in the config file**: the access token and the encryption
  passphrase live in the OS keychain (Windows Credential Manager, macOS Keychain,
  or Secret Service on Linux).
- **Client-side encryption**: with `config_share encrypt`, new uploads are
  encrypted with XChaCha20-Poly1305 before they leave the machine, so the server
  only receives ciphertext.
- **Safe writes**: downloads are hash-verified, then written to a temp file and
  renamed into place. Existing permissions are preserved; new files are created
  `0600` on Unix.

## Requirements

- Rust (edition 2024, so Rust 1.85 or newer) to build.
- A running [sync server](https://github.com/Icefire113/sync_server) and an
  account on it. TLS verification uses the host's certificate store
  (`rustls-platform-verifier`), so a self-signed root installed in the OS trust
  store is accepted.
- Somewhere to keep the access token and encryption passphrase: the OS keychain
  (Windows Credential Manager, macOS Keychain, or Secret Service on Linux), or a
  local `0600` file when there is no keychain, such as on a headless Linux box.
  See [Secret storage](#secret-storage).

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
config_share encrypt         # pick an encryption passphrase (optional but recommended)
config_share add ~/.gitconfig
config_share sync
```

On another machine:

```sh
config_share init
config_share login
config_share encrypt         # enter the same passphrase as on the first machine
config_share sync            # offers to download the tracked files
```

`init` writes `~/.config/config_share/config.json` on every platform (on Windows
that is `C:\Users\<you>\.config\config_share\config.json`). It always asks for
the server URL and machine name; `init --overwrite` resets the config and forgets
the locally saved token and encryption passphrase for the old server (neither is
revoked or deleted on the server).

## Commands

| Command                                               | What it does                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- |
| `init [--overwrite]`                                  | Create or reset the config file and prompt for server + machine id              |
| `login [--token <TOKEN>]`                             | Request a token with credentials, or save an existing token                     |
| `logout [--force]`                                    | Revoke the token on the server and forget it; `--force` only forgets it locally |
| `encrypt [--forget] [--disable]`                      | Set up client-side encryption; `--forget` clears the cached passphrase, `--disable` stops encrypting new uploads |
| `add [--use-path] <file>`                             | Start tracking a file; prompts for a naming method unless `--use-path`          |
| `remove [--remote] <file>`                            | Stop tracking a file; `--remote` also deletes the server copy                   |
| `list`                                                | Show tracked files, paths, and last-modified times (local time)                 |
| `sync [--dry-run] [--yes] [--include-ignored] [file]` | Reconcile with the server                                                       |
| `status [--include-ignored] [file]`                   | Show the sync state without changing anything                                   |
| `check [file]`                                        | Offline: re-hash local files and compare them with the saved anchor             |
| `pull [file]`                                         | Force-download tracked files (refuses to overwrite unsynced local changes)      |
| `push [--force] [file]`                               | Force-upload tracked files (overwrites the server copies); `--force` also re-uploads up-to-date files |
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

## Encryption

`config_share encrypt` enables client-side encryption for files uploaded from
this machine. It prompts for a passphrase, derives a key with Argon2id and saves
the passphrase in the OS keychain, so normal syncs do not prompt. Uploads are
encrypted with XChaCha20-Poly1305 before they are sent; the server only ever
stores ciphertext.

- **Use the same passphrase on every machine.** Run `config_share encrypt` once on
  each machine. Nothing else has to be copied: every encrypted payload carries
  the random Argon2 salt it was created with.
- **No recovery.** If the passphrase is lost, the encrypted files on the server
  cannot be read again. The server cannot help, because it never has the key.
- **Changing the passphrase** makes files uploaded with the previous passphrase
  unreadable.
- **Existing files are not re-uploaded automatically.** Enabling encryption only
  affects future uploads. Run `config_share push --force` to encrypt the files
  that are already on the server.
- `encrypt --forget` removes the cached passphrase from the keychain; the next
  operation prompts for it again. `encrypt --disable` stops encrypting new
  uploads, but files already encrypted on the server stay encrypted and still
  need the passphrase to download.

### Storage format

Every upload is wrapped in a small versioned envelope:

```text
plaintext:  "CSHARE01" | kind=0 | raw contents
encrypted:  "CSHARE01" | kind=1 | version | kdf | salt (16) | nonce (24) | ciphertext + tag
```

The `hash` in the manifest is always the xxhash3 of the local **plaintext**,
computed by the client. The server stores that value and never hashes the bytes
itself, so the three-way sync can keep comparing like with like even though the
stored bytes are encrypted.

The envelope format is a breaking change: files uploaded by an older version
have no header and are rejected on download. Re-upload them with
`config_share push --force` from a machine that still has the local copy.

## How it works

- File contents are compared with xxhash3 (64bit) of the plaintext. The server
  stores the same hash; the client computes it, the server never does.
- `TrackedFile.synced_hash` is the hash both sides had after the last successful
  sync. `check` compares the current local hash with it, which is why `check`
  works offline.
- Pushes send the hash the server is expected to have (`expected_hash`), so a
  concurrent update from another machine fails with a conflict instead of being
  silently overwritten.
- Server file names are not unique, which is why everything is keyed by id and
  "link to an existing file" is an explicit choice rather than a name match.

## Secret storage

The access token and the encryption passphrase never go into the config file.
They are kept in the OS keychain (Windows Credential Manager, macOS Keychain, or
Secret Service on Linux) under the service `config_share` and the account
`<host>:<port>`, with `:encryption` appended for the passphrase.

`--secret-store <STORE>` (or the `CONFIG_SHARE_SECRET_STORE` environment variable,
useful for cron and systemd units) picks the backend:

| Value            | Behaviour                                                                                       |
| ---------------- | ----------------------------------------------------------------------------------------------- |
| `auto` (default) | Use the OS keychain. If it is unusable, warn and use the secrets file for the rest of the run.  |
| `keyring`        | Require the OS keychain and fail otherwise, so a keychain problem is never silently downgraded. |
| `file`           | Never touch the keychain; keep secrets in `~/.config/config_share/secrets.json`.                |

"Unusable" means there is no D-Bus session, no Secret Service provider, no default
collection, or the collection cannot be unlocked (a headless linux box). `auto`
prints a warning each run when it falls back, so the downgrade is never silent;
pass `--secret-store keyring` to turn that into an error instead.

The secrets file is plaintext JSON with `0600` permissions inside a `0700`
directory. Prefer a real keychain when one is available.

### Headless Linux

Without a desktop session there is usually no Secret Service, and the keychain
reports errors such as `No default store has been set` or `No access to platform
storage`. Either skip the keychain:

```sh
config_share --secret-store file login
# or, for cron/systemd units that cannot pass flags:
export CONFIG_SHARE_SECRET_STORE=file
```

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
  "encryption": {
    "salt": "…base64…"
  },
  "auth": {
    "token_id": 1,
    "name": "desktop",
    "expires_at": "2026-11-17T07:00:00Z"
  },
  "ignored_remote_files": []
}
```

The token itself is **not** in this file; `auth` only keeps the metadata needed to
revoke it. The encryption passphrase is not in this file either: `encryption`
only stores the non-secret Argon2 salt used for files uploaded from this machine.
Both live in the secret store described above, which is
`~/.config/config_share/secrets.json` when `--secret-store file` is used.
`manual_sync_reminder_time` is in hours and controls the "it has been a while"
warning shown by most commands.

## Notes and limitations

- File permissions are not synced yet.
- The encryption passphrase cannot be recovered. Losing it means the encrypted
  files on the server cannot be decrypted.
- `push --force` is required to encrypt files that were already on the server
  before encryption was enabled.
- `remove` is local-only unless `--remote` is passed.
- There is no automatic three-way text merge; use the diff and `.remote` sidecar.
- `pull` deliberately refuses to overwrite local files that have unsynced changes;
  resolve those with `sync` instead.
- `pull` only pulls files already tracked on this machine. It warns about server
  files that are not tracked here and points at `sync`.
- The file secret store keeps the token and passphrase in plaintext (mode `0600`);
  use a keychain where one is available.
- The config path is fixed; there is no `--config` override yet.
- Ignored files can be viewed again with `--include-ignored`, but there is no
  dedicated command to list or bulk-unignore them.
- `sync` exits non-zero if any file failed; skipped decisions do not count as
  failures.

## License

BSD 3-Clause. See [LICENSE](LICENSE).

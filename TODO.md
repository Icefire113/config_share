# Config Share CLI TODO

## Commands

- [ ] Implement the `sync` command to sync files with the server.
- [ ] Implement a `list` command to show all tracked files.
- [ ] Implement a `remove` command to stop tracking a file.
- [ ] Implement a `status` command to show the sync status of tracked files (e.g., modified locally, up-to-date, etc.).
- [ ] Add a `pull` command to force pulling down all remote files.
- [ ] Add a `push` command to force pushing all local files.

## Core Functionality

- [ ] When pulling down a config file that we don't have a local path for, prompt the user for a local path and save it.
- [ ] Implement file content hashing to determine if a file has changed.
- [ ] Handle file permissions, especially for sensitive files like SSH keys.

## User Experience

- [ ] Move console output to use the `console` crate for better looking and more interactive output.
- [ ] When adding a file, prompt the user for the sync method (e.g., file path, custom name), with an option to use a default.
- [ ] Improve error handling and messages.

## Code Quality

- [ ] ~~Stop using `Err` for returning error strings, just make the return type a `Result<Option<String>, anyhow::Error>` when `Some(str)` is the error message (if any).~~
- [ ] ~~Add unit and integration tests.~~
- [ ] Add more comments to the code, especially for complex logic.

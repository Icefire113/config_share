# Adding a File

Add a file to the tracked set with:

```sh
config_share add <file_path>
```

By default this prompts for how the file should be named on the server:

- **File Path** — use the absolute local path as the name.
- **Custom Name** — enter a name, defaulting to the absolute path.

For scripts or non-interactive use, skip the prompt with:

```sh
config_share add --use-path <file_path>
```

The path is resolved to an absolute path (`~` is expanded), and adding the same
path twice is an error.

The file is not uploaded until you run `config_share sync` (or `config_share
push`).

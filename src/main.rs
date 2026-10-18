use crate::config_share::ConfigShare;
use crate::config_share::EncryptionAction;
use anyhow::{Context, anyhow};
use clap::{Arg, ArgAction, Command, command};
use log::trace;
use std::result::Result::Ok;

mod config_share;
mod crypto;
mod file_content;
mod secret_store;
mod share_settings;
mod sync_analysis;
mod tracked_file_info;
mod util;

pub mod api;

fn main() -> anyhow::Result<()> {
    init_logger();

    let mut app = ConfigShare::default();
    let mut matches = cli();

    match matches.get_matches_mut().subcommand() {
        Some(("init", sub_matches)) => {
            trace!("[main.sub_command] init");

            app.initialize(sub_matches.get_flag("overwrite"))?;
        }
        Some(("add", sub_matches)) => {
            trace!("[main.sub_command] add");
            prepare_app(&mut app, true)?;

            app.add_file(
                // unwrap is safe here as the subcommand "add" has "file" as a required argument
                sub_matches.get_one::<String>("file").with_context(|| {
                    // if this ever occurs, it's a bug as we set the argument to be required when creating the subcommand in clap
                    anyhow!("[main.sub_command] add: Failed to get the file option, this is a bug")
                })?,
                sub_matches.get_flag("use-path"),
            )?;
        }
        Some(("list", _)) => {
            trace!("[main.sub_command] list");
            prepare_app(&mut app, true)?;

            app.list_files()?;
        }
        Some(("remove", sub_matches)) => {
            trace!("[main.sub_command] remove");
            prepare_app(&mut app, true)?;

            app.remove_file(
                sub_matches.get_one::<String>("file").with_context(|| {
                    // if this ever occurs, it's a bug as we set the argument to be required when creating the subcommand in clap
                    anyhow!(
                        "[main.sub_command] remove: Failed to get the file option, this is a bug"
                    )
                })?,
                sub_matches.get_flag("remote"),
            )?;
        }
        Some(("sync", sub_matches)) => {
            trace!("[main.sub_command] sync");
            prepare_app(&mut app, true)?;

            app.sync_files(
                sub_matches.get_one::<String>("file"),
                sub_matches.get_flag("dry-run"),
                sub_matches.get_flag("yes"),
                sub_matches.get_flag("include-ignored"),
            )?;
        }
        Some(("check", sub_matches)) => {
            trace!("[main.sub_command] check");
            prepare_app(&mut app, true)?;

            app.check_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("status", sub_matches)) => {
            trace!("[main.sub_command] status");
            prepare_app(&mut app, true)?;

            app.status_files(
                sub_matches.get_one::<String>("file"),
                sub_matches.get_flag("include-ignored"),
            )?;
        }
        Some(("pull", sub_matches)) => {
            trace!("[main.sub_command] pull");
            prepare_app(&mut app, true)?;

            app.pull_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("push", sub_matches)) => {
            trace!("[main.sub_command] push");
            prepare_app(&mut app, true)?;

            app.push_files(
                sub_matches.get_one::<String>("file"),
                sub_matches.get_flag("force"),
            )?;
        }
        Some(("ping", _)) => {
            trace!("[main.sub_command] ping");
            prepare_app(&mut app, false)?;

            app.ping_server()?;
        }
        Some(("login", sub_matches)) => {
            trace!("[main.sub_command] login");
            prepare_app(&mut app, false)?;

            app.login(sub_matches.get_one::<String>("token").map(|t| t.as_str()))?;
        }
        Some(("encrypt", sub_matches)) => {
            trace!("[main.sub_command] encrypt");
            prepare_app(&mut app, false)?;

            let action = if sub_matches.get_flag("forget") {
                EncryptionAction::Forget
            } else if sub_matches.get_flag("disable") {
                EncryptionAction::Disable
            } else {
                EncryptionAction::Setup
            };

            app.configure_encryption(action)?;
        }
        Some(("logout", sub_matches)) => {
            trace!("[main.sub_command] logout");
            prepare_app(&mut app, false)?;

            app.logout(sub_matches.get_flag("force"))?;
        }
        None => {
            // this is still an error, but we should also check if the user has
            // initialized the config aswell as show the usage/ help options
            matches.print_long_help()?;
            app.check_is_config_setup()?;
            // return Err("".into());
        }
        _ => unreachable!("Did you forget to add a subcommand?"),
    };
    Ok(())
}

/// Checks that the config exists, loads it, and optionally runs the startup checks.
///
/// Startup checks are skipped for commands that don't touch tracked files (e.g. `ping`, `login`).
fn prepare_app(app: &mut ConfigShare, run_startup_checks: bool) -> anyhow::Result<()> {
    app.check_is_config_setup()?;
    app.load_settings()?;
    if run_startup_checks {
        app.all_startup_checks()?;
    }
    Ok(())
}

fn init_logger() {
    let default_level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        // keyring logs every keychain access at debug level, which is just noise
        .filter_module("keyring_core", log::LevelFilter::Info)
        .filter_module("ureq", log::LevelFilter::Info)
        .filter_module("rustls", log::LevelFilter::Info)
        .init();
}

fn cli() -> Command {
    let mut c = command!()
        .about("Utility for syncing config files across multiple machines")
        // We dont use this as we check this ourselves because we want to check if the config has already
        // been populated
        // .arg_required_else_help(true)
        // init subcommand
        .subcommand(
            Command::new("init")
                .arg(
                    Arg::new("overwrite")
                        .long("overwrite")
                        .action(ArgAction::SetTrue)
                        .help("Overwrites the config file with a new one with default values"),
                )
                .about("Checks if config_share has been initialized and if not initializes it"),
        )
        // add subcommand
        .subcommand(
            Command::new("add")
            .arg(
                Arg::new("file")
                    .help("The file to add to the list of files that will be synced")
                    .required(true)
            )
            // TODO: Permissions for the file, this might be just a linux only thing
            // .arg(
            //     Arg::new("ignore-permissions")
            //     .long("ignore-permissions")
            //     .action(ArgAction::SetTrue)
            //     .help("Skips syncing file permissions for this file")
            // )
            .arg(Arg::new("use-path")
                .long("use-path")
                .action(ArgAction::SetTrue)
                .help("Uses the file path as the name without prompting for a naming method")
            )
            .about("Adds a file to the list of files that will be synced")
        )
        // sync subcommand
        .subcommand(
            Command::new("sync")
            .about("Syncs the saved config files with the specified remote server")
            .arg(
                Arg::new("file")
                .required(false)
                .help("Optional: The specific file to sync with the server")
            )
            .arg(
                Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Shows what would be synced without changing anything")
            )
            .arg(
                Arg::new("yes")
                .long("yes")
                .short('y')
                .action(ArgAction::SetTrue)
                .help("Applies safe changes without prompting and skips files that need a decision")
            )
            .arg(
                Arg::new("include-ignored")
                .long("include-ignored")
                .action(ArgAction::SetTrue)
                .help("Also offers server files that are currently ignored on this machine")
            )
        )
        // list subcommand
        .subcommand(
            Command::new("list")
            .about("Lists the files that are currently tracked locally")
        )
        // remove subcommand
        .subcommand(
            Command::new("remove")
            .about("Removes a file from the ones that are currently tracked")
            .arg(
                Arg::new("file")
                .required(true)
                .help("The file to remove")
            )
            .arg(
                Arg::new("remote")
                .long("remote")
                .action(ArgAction::SetTrue)
                .help("Also deletes the file from the server")
            )
        )
        // check subcommand
        .subcommand(
            Command::new("check")
            .about("Checks the files that are currently tracked locally, but does not sync with the server")
            .arg(
                Arg::new("file")
                .required(false)
                .help("The file to check, if not specified all files currently tracked will be checked")
            )
        )
        // ping subcommand
        .subcommand(
            Command::new("ping")
            .about("Pings the remote server to check if it is up, and returns the version of the server")
        )
        // status subcommand
        .subcommand(
            Command::new("status")
            .about("Shows the sync status of tracked files (modified locally, up to date, etc.) without syncing")
            .arg(
                Arg::new("file")
                    .required(false)
                    .help("The file to show the status of, if not specified all files currently tracked will be shown")
            )
            .arg(
                Arg::new("include-ignored")
                    .long("include-ignored")
                    .action(ArgAction::SetTrue)
                    .help("Also shows server files that are currently ignored on this machine")
            )
        )
        // pull subcommand
        .subcommand(
            Command::new("pull")
            .about("Pulls the remote files down to this machine")
            .arg(
                Arg::new("file")
                    .required(false)
                    .help("The file to pull, if not specified all files currently tracked will be pulled")
            )
        )
        // push subcommand
        .subcommand(
            Command::new("push")
            .about("Pushes the local files to the server")
            .arg(
                Arg::new("file")
                    .required(false)
                    .help("The file to push, if not specified all files currently tracked will be pushed")
            )
            .arg(
                Arg::new("force")
                    .long("force")
                    .action(ArgAction::SetTrue)
                    .help("Also uploads files that are already up to date (useful to encrypt existing files)")
            )
        )
        // login subcommand
        .subcommand(
            Command::new("login")
            .about("Authenticates with the server and saves the access token")
            .arg(
                Arg::new("token")
                    .long("token")
                    .value_name("TOKEN")
                    .help("Uses an existing access token instead of prompting for credentials")
            )
        )
        // logout subcommand
        .subcommand(
            Command::new("logout")
            .about("Revokes the saved access token on the server and removes it locally")
            .arg(
                Arg::new("force")
                    .long("force")
                    .action(ArgAction::SetTrue)
                    .help("Removes the token from this machine without revoking it on the server")
            )
        )
        // encrypt subcommand
        .subcommand(
            Command::new("encrypt")
            .about("Sets up client-side encryption for the files uploaded to the server")
            .arg(
                Arg::new("forget")
                    .long("forget")
                    .action(ArgAction::SetTrue)
                    .conflicts_with("disable")
                    .help("Removes the cached passphrase from the OS keychain, keeping encryption enabled")
            )
            .arg(
                Arg::new("disable")
                    .long("disable")
                    .action(ArgAction::SetTrue)
                    .help("Stops encrypting new uploads and removes the cached passphrase")
            )
        )
        ;
    c.build();

    c
}

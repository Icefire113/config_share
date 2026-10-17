use crate::config_share::ConfigShare;
use anyhow::{Context, anyhow};
use clap::{Arg, ArgAction, Command, command};
use log::trace;
use std::result::Result::Ok;

mod config_share;
mod share_settings;
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

            app.initialize(
                sub_matches.get_flag("overwrite"),
                sub_matches.get_flag("no-setup"),
            )?;
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
                sub_matches.get_flag("ask-method"),
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

            app.remove_file(sub_matches.get_one::<String>("file").with_context(|| {
                // if this ever occurs, it's a bug as we set the argument to be required when creating the subcommand in clap
                anyhow!("[main.sub_command] remove: Failed to get the file option, this is a bug")
            })?)?;
        }
        Some(("sync", sub_matches)) => {
            trace!("[main.sub_command] sync");
            prepare_app(&mut app, true)?;

            app.sync_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("check", sub_matches)) => {
            trace!("[main.sub_command] check");
            prepare_app(&mut app, true)?;

            app.check_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("status", sub_matches)) => {
            trace!("[main.sub_command] status");
            prepare_app(&mut app, true)?;

            app.status_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("pull", sub_matches)) => {
            trace!("[main.sub_command] pull");
            prepare_app(&mut app, true)?;

            app.pull_files(sub_matches.get_one::<String>("file"))?;
        }
        Some(("push", sub_matches)) => {
            trace!("[main.sub_command] push");
            prepare_app(&mut app, true)?;

            app.push_files(sub_matches.get_one::<String>("file"))?;
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
        Some(("logout", _)) => {
            trace!("[main.sub_command] logout");
            prepare_app(&mut app, false)?;

            app.logout()?;
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
                .arg(
                    Arg::new("no-setup")
                        .long("no-setup")
                        .action(ArgAction::SetTrue)
                        .help("Skips the setup for the config, the config file will have default values"),
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
            .arg(Arg::new("ask-method")
                .long("ask-method")
                .action(ArgAction::SetTrue)
                .help("Prompts the user to select a method to name files (default is the local file name)")
            )
            .about("Adds a file to the list of files that will be synced")
        )
        // sync subcommand
        .subcommand(
            Command::new("sync")
            .about("Syncs all files with the server")
            .arg(
                Arg::new("file")
                .required(false)
                .help("Optional: The specific file to sync with the server")
            )
            .about("Syncs the saved config files with the specified remote server")
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
        )
        ;
    c.build();

    c
}

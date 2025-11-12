use crate::config_share::ConfigShare;
use clap::{Arg, ArgAction, Command, command};
use console::style;
use std::process::exit;
use std::result::Result::Ok;
use util::dprint;

mod config_share;
mod file_differentation_method;
mod fn_error;
mod share_settings;
mod tracked_file_info;

fn main() -> anyhow::Result<()> {
    let mut app = ConfigShare::default();

    match run(&mut app) {
        Ok(_) => anyhow::Ok(()),
        Err(e) => {
            eprintln!("{}: {:?}", style("error").red().bold(), e);
            exit(1);
        }
    }
}

fn run(app: &mut ConfigShare) -> anyhow::Result<()> {
    let mut matches = cli();

    match matches.get_matches_mut().subcommand() {
        Some(("init", sub_matches)) => {
            dprint("[main.sub_command] init");

            app.initialize(
                sub_matches.get_flag("overwrite"),
                sub_matches.get_flag("no-setup"),
            )?;
        }
        Some(("add", sub_matches)) => {
            dprint("[main.sub_command] add");
            app.check_initialized()?;
            app.load_settings()?;
            app.all_startup_checks()?;

            app.add_file(
                // unwrap is safe here as the subcommand "add" has "file" as a required argument
                sub_matches.get_one::<String>("file").unwrap(),
                sub_matches.get_flag("ignore-permissions"),
                sub_matches.get_flag("ask-method"),
            )?;
        }
        None => {
            // this is still an error, but we should also check if the user has
            // initialized the config aswell as show the usage/ help options
            matches.print_long_help()?;
            app.check_initialized()?;
            // return Err("".into());
        }
        _ => unreachable!("Did you forget to add a subcommand?"),
    };
    Ok(())
}

fn cli() -> Command {
    let mut c = command!()
        .about("Utility for syncing config files across multiple machines")
        // .arg_required_else_help(true)
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
        .subcommand(
            Command::new("add")
            .arg(
                Arg::new("file")
                    .help("The file to add to the list of files that will be synced")
                    .required(true)
            )
            .arg(
                Arg::new("ignore-permissions")
                .long("ignore-permissions")
                .action(ArgAction::SetTrue)
                .help("Skips syncing file permissions for this file")
            )
            .arg(Arg::new("ask-method")
                .long("ask-method")
                .action(ArgAction::SetTrue)
                .help("Prompts the user to select a method to name files (default is the local file name)")
            )
            .about("Adds a file to the list of files that will be synced")
        );
    c.build();

    c
}

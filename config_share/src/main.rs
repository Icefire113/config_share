use crate::config_share::ConfigShare;
use clap::{Arg, ArgAction, Command, command};
use console::style;
use std::process::exit;
use util::dprint;

mod config_share;
mod share_settings;
mod tracked_file_info;

fn main() {
    let matches = cli().get_matches();

    let mut app = ConfigShare::default();

    match matches.subcommand() {
        Some(("init", sub_matches)) => {
            dprint("[main.sub_command] init");
            match app.initialize(
                sub_matches.get_flag("overwrite"),
                sub_matches.get_flag("no-setup"),
            ) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}: {}", style("Err").red().bold(), e);
                    exit(1);
                }
            };
        }
        Some(("add", sub_matches)) => {
            dprint("[main.sub_command] add");
            // unwrap is safe here as the subcommand "add" has "file" as a required argument
            match app.add_file(sub_matches.get_one::<String>("file").unwrap()) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}: {}", style("Err").red().bold(), e);
                    exit(1);
                }
            }
        }
        _ => unreachable!("Did you forget to add a subcommand?"),
    }
}

fn cli() -> Command {
    command!()
        .about("Utility for syncing config files across multiple machines")
        .arg_required_else_help(true)
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
            .about("Adds a file to the list of files that will be synced")
        )
}

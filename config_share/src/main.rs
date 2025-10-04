use crate::config_share::ConfigShare;
use clap::{Arg, ArgAction, Command, command};
use std::process::exit;
use util::dprint;

mod config_share;
mod share_settings;

fn main() {
    let matches = command!()
        .about("Utility for syncing config files across multiple machines")
        .subcommand(
            Command::new("init")
                .arg(
                    Arg::new("overwrite")
                        .long("overwrite")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("no-setup")
                        .long("no-setup")
                        .action(ArgAction::SetTrue),
                )
                .about("Checks if config_share has been initialized and if not initializes it"),
        )
        .get_matches();

    let mut app = ConfigShare::default();

    if let Some(init) = matches.subcommand_matches("init") {
        dprint("init");
        match app.initialize(init.get_flag("overwrite"), init.get_flag("no-setup")) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("There was an error initializing config_share: {}", e);
                exit(1);
            }
        };
    }
}

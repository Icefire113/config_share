use std::process::exit;

use clap::{Arg, ArgAction, Command, command};

use util::dprint;

use crate::init::initialize;

mod init;
mod share_settings;

fn main() {
    let matches = command!()
        .about("Utility for syncing config files across multiple machines")
        .subcommand(
            Command::new("init")
                .arg(Arg::new("overwrite").long("overwrite").action(ArgAction::SetTrue))
                .about("Checks if config_share has been initialized and if not initializes it"),
        )
        .get_matches();

    if let Some(init) = matches.subcommand_matches("init") {
        dprint("init");
        match initialize(init.get_flag("overwrite")) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("There was an error initializing config_share: {}", e);
                exit(1);
            }
        };
    }
}

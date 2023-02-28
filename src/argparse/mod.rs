mod cache;
mod completions;
mod login;

pub use cache::Cache;
pub use completions::Completions;
pub use login::Login;

use crate::{
    cli::{self},
    VERSION,
};
use clap::{Arg, ArgAction, Command};

use lib::mcu::{
    schema::{self},
    Api,
};

use std::collections::HashMap;

pub struct CommandGen(pub HashMap<Api, HashMap<String, schema::Endpoint>>);

impl CommandGen {
    pub fn command(&self) -> clap::Command {
        let api_subcommands = cli::generate_subcommands(&self.0);

        Command::new("pexshell")
            .version(VERSION.as_str())
            .about("Convenient way to manipulate the Management API.")
            .subcommands(api_subcommands)
            .subcommand(Login.command())
            .subcommand(Cache.command())
            .subcommand(Completions.command())
            .subcommand_required(true)
            .arg(
                Arg::new("insecure")
                    .long("insecure")
                    .help("Do not verify certificates")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("log")
                    .long("log")
                    .help("Output application logs to a file")
                    .action(ArgAction::Set),
            )
    }
}

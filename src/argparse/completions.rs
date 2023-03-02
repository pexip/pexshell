use std::fmt::Display;

use crate::pexshell::PexShell;
use clap::{Arg, ArgAction, ArgMatches, Command};

pub struct Completions;

impl Display for Completions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "completions")
    }
}

impl Completions {
    pub fn command(&self) -> Command {
        Command::new(self.to_string())
            .about(
                "Prints generated shell completions to STDOUT. Remember to regenerate after \
                 updating the schema cache!",
            )
            .arg(
                Arg::new("shell")
                    .help("The shell to generate completions for")
                    .required(true)
                    .action(ArgAction::Set)
                    .value_parser(["bash", "elvish", "fish", "nushell", "powershell", "zsh"]),
            )
    }

    #[allow(clippy::unused_self)]
    pub fn run(&self, pexshell: &mut PexShell, command: &Command, completions_sub: &ArgMatches) {
        let shell = completions_sub
            .get_one::<String>("shell")
            .expect("argument shell is required")
            .as_str();

        let shell = match shell {
            "bash" => clap_complete::Shell::Bash,
            "elvish" => clap_complete::Shell::Elvish,
            "fish" => clap_complete::Shell::Fish,
            "nushell" => {
                clap_complete::generate(
                    clap_complete_nushell::Nushell,
                    &mut command.clone(),
                    "pexshell",
                    &mut pexshell.console,
                );
                return;
            }
            "powershell" => clap_complete::Shell::PowerShell,
            "zsh" => clap_complete::Shell::Zsh,
            _ => panic!("Unhandled shell!"),
        };

        clap_complete::generate(
            shell,
            &mut command.clone(),
            "pexshell",
            &mut pexshell.console,
        );
    }
}

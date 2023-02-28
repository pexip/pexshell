use crate::{
    cli,
    config::{Manager as ConfigManager, Provider},
    pexshell::PexShell,
};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use lib::error;
use std::{fmt::Display, path::Path};

pub struct Login;

impl Display for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "login")
    }
}

impl Login {
    pub fn command(&self) -> Command {
        Command::new(self.to_string())
            .about("Manage credentials for infinity instances")
            .arg(
                Arg::new("offline")
                    .long("offline")
                    .help("Do not connect to the management node to verify credentials")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("list")
                    .long("list")
                    .short('l')
                    .help("List existing accounts")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("delete")
                    .long("delete")
                    .short('d')
                    .help("Delete an account")
                    .action(ArgAction::SetTrue),
            )
            .group(
                ArgGroup::new("function")
                    .args(["list", "delete"])
                    .conflicts_with("offline"),
            )
    }

    pub async fn run<'a>(
        &self,
        pexshell: &mut PexShell<'a>,
        config: &mut ConfigManager,
        config_file: &Path,
        client: reqwest::Client,
        login_sub: &ArgMatches,
    ) -> Result<(), error::UserFriendly> {
        let mut login = cli::login::Login::default();
        if login_sub.get_flag("list") {
            login.list_users(&mut pexshell.console, config);
        } else if login_sub.get_flag("delete") {
            login.delete_user(config)?;
            config.write_to_file(config_file)?;
        } else {
            let user = login
                .select_user(
                    &mut pexshell.console,
                    config,
                    client,
                    !login_sub.get_flag("offline"),
                )
                .await?;

            config.set_current_user(&user);
            config.write_to_file(config_file)?;
        };
        Ok(())
    }
}

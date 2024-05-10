use crate::{cli, config::Manager as ConfigManager, pexshell::PexShell};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use lib::error;
use std::fmt::Display;

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
            .arg(
                Arg::new("store_passwords_in_plaintext")
                    .long("store_passwords_in_plaintext")
                    .help("Stores passwords in plaintext instead of in the system credential store")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("oauth2")
                    .long("oauth2")
                    .help("Add an oauth2 user - specify the management node address and client id as arguments, and pass the private key to stdin")
                    .value_names(["address", "client_id"])
                    .num_args(2)
                    .action(ArgAction::Set),
            )
            .group(
                ArgGroup::new("function")
                    .args(["list", "delete"])
                    .conflicts_with_all(["offline", "store_passwords_in_plaintext", "oauth2"]),
            )
    }

    pub async fn run<'a>(
        &self,
        pexshell: &mut PexShell<'a>,
        config: &mut ConfigManager,
        client: reqwest::Client,
        login_sub: &ArgMatches,
    ) -> Result<(), error::UserFriendly> {
        let mut login = cli::login::Login::default();
        if login_sub.get_flag("list") {
            login.list_users(&mut pexshell.console, config);
        } else if login_sub.get_flag("delete") {
            login.delete_user(config)?;
            config.write_to_file()?;
        } else if let Some(oauth2_values) = login_sub.get_many("oauth2") {
            let oauth2_values: Vec<&String> = oauth2_values.collect();
            assert!(oauth2_values.len() == 2, "oauth2 requires 2 arguments");
            login
                .add_and_select_oauth2_user(
                    config,
                    client,
                    oauth2_values[0].to_owned(),
                    oauth2_values[1].to_owned(),
                    !login_sub.get_flag("offline"),
                    login_sub.get_flag("store_passwords_in_plaintext"),
                )
                .await?;

            config.write_to_file()?;
        } else {
            login
                .select_user(
                    &mut pexshell.console,
                    config,
                    client,
                    !login_sub.get_flag("offline"),
                    login_sub.get_flag("store_passwords_in_plaintext"),
                )
                .await?;

            config.write_to_file()?;
        };
        Ok(())
    }
}

use crate::config;
use clap::{Arg, ArgAction, ArgMatches, Command};
use lib::{
    error,
    mcu::{self, schema},
};
use log::info;
use std::{fmt::Display, path::Path};

pub struct Cache;

impl Display for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cache")
    }
}

impl Cache {
    pub fn command(&self) -> Command {
        Command::new(self.to_string())
            .about("fetch new schema cache")
            .arg(
                Arg::new("clear")
                    .long("clear")
                    .help("Remove existing schema cache")
                    .action(ArgAction::SetTrue),
            )
    }

    pub async fn run<'a>(
        &self,
        config: &mut impl config::Provider,
        cache_dir: &Path,
        client: reqwest::Client,
        cache_matches: &ArgMatches,
    ) -> anyhow::Result<()> {
        if cache_matches.get_flag("clear") {
            info!("Clearing cache...");
            clear_cache(cache_dir)
                .map_err(|err| error::UserFriendly::new(format!("error clearing cache: {err}",)))?;

            info!("Cache cleared.");
            eprintln!("Cache cleared.");
        } else {
            let user = config.get_current_user()?;

            eprintln!("Generating cache...");
            info!("Generating cache...");
            let api_client = mcu::ApiClient::new(
                client,
                &user.address,
                user.username.clone(),
                config.get_password_for_user(user)?,
            );
            schema::cache_schemas(&api_client, cache_dir).await?;
            info!("Cache created.");
            eprintln!("Cache created.");
        }
        Ok(())
    }
}

fn clear_cache(cache_dir: &Path) -> anyhow::Result<()> {
    for f in cache_dir.read_dir()? {
        let dir = f?;
        std::fs::remove_dir_all(
            dir.path()
                .to_str()
                .expect("File path contains invalid unicode characters."),
        )?;
    }
    Ok(())
}

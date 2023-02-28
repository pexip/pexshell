use crate::{
    argparse,
    cli::Console,
    config::{Config, Manager as ConfigManager, Provider},
    LOGGER,
};

use futures::TryStreamExt;
use is_terminal::IsTerminal;
use lazy_static::lazy_static;
use lib::{
    error,
    mcu::{
        self,
        schema::{self, cache_exists},
        ApiResponse, IApiClient,
    },
};
use log::{debug, trace, LevelFilter};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs, future,
    io::Write,
    path::{Path, PathBuf},
};

lazy_static! {
    static ref CONFIG_DIR: PathBuf = {
        let base_dirs = directories::BaseDirs::new().expect("could not find user base directories");
        base_dirs.config_dir().join("pexip/pexshell")
    };
    static ref CACHE_DIR: PathBuf = {
        let base_dirs = directories::BaseDirs::new().expect("could not find user base directories");
        base_dirs.cache_dir().join("pexip/pexshell")
    };
}

fn read_config(
    config_file: &Path,
    env: &HashMap<String, String>,
) -> Result<ConfigManager, Box<dyn std::error::Error>> {
    let config_dir = config_file.parent().expect("no parent directory");
    debug!(
        "Ensuring config directory path is created: {:?}",
        &config_dir
    );
    fs::create_dir_all(config_dir)?;

    let config = ConfigManager::read_from_file(config_file, env.clone())?
        .unwrap_or_else(|| ConfigManager::with_config(Config::default(), env.clone()));

    LOGGER.set_log_to_stderr(config.get_log_to_stderr());

    if let Some(log) = config.get_log_file() {
        LOGGER.set_log_file(Some(log))?;
    }

    if let Some(log_level) = config.get_log_level() {
        LOGGER.set_max_level(match log_level.as_str() {
            "max" => LevelFilter::max(),
            "trace" => LevelFilter::Trace,
            "debug" => LevelFilter::Debug,
            "info" => LevelFilter::Info,
            "warn" => LevelFilter::Warn,
            "error" => LevelFilter::Error,
            "off" => LevelFilter::Off,
            _ => panic!("Invalid log level"),
        });
    }
    trace!("I'M ALIVE!");
    Ok(config)
}

pub struct PexShell<'a> {
    config_dir: &'a Path,
    cache_dir: &'a Path,
    pub console: Console,
    env: HashMap<String, String>,
}

impl<'a> Default for PexShell<'a> {
    fn default() -> Self {
        let stdout = std::io::stdout();
        let is_stdout_interactive = stdout.is_terminal();
        let env: HashMap<String, String> = std::env::vars().collect();

        Self {
            config_dir: &CONFIG_DIR,
            cache_dir: &CACHE_DIR,
            console: Console::new(is_stdout_interactive, stdout),
            env,
        }
    }
}

impl<'a> PexShell<'a> {
    #[cfg(test)]
    pub const fn new(
        config_dir: &'a Path,
        cache_dir: &'a Path,
        console: Console,
        env: HashMap<String, String>,
    ) -> Self {
        Self {
            config_dir,
            cache_dir,
            console,
            env,
        }
    }

    async fn api_request(
        &mut self,
        client: reqwest::Client,
        config: &ConfigManager,
        matches: &clap::ArgMatches,
        schemas: &argparse::CommandGen,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let api_client = mcu::ApiClient::new(
            client,
            &config.get_address()?,
            config.get_username()?,
            config.get_password()?,
        );
        let (api_request, stream_output) = crate::api_request_from_matches(matches, &schemas.0)?;

        match api_client.send(api_request).await? {
            ApiResponse::ContentStream(response_content) => {
                if stream_output {
                    response_content
                        .try_for_each(|x| {
                            self.console.pretty_print_json(&x);
                            future::ready(Ok(()))
                        })
                        .await?;
                } else {
                    let objects: Vec<Value> = response_content.try_collect().await?;
                    let json = serde_json::to_value(objects)?;
                    self.console.pretty_print_json(&json);
                }
            }
            ApiResponse::Content(response_content) => {
                self.console.pretty_print_json(&response_content);
            }
            ApiResponse::Location(location) => {
                writeln!(self.console, "{location}").unwrap();
            }
            ApiResponse::Nothing => (),
        };
        Ok(())
    }

    pub async fn run(&mut self, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        // Read config file
        let config_file = &self.config_dir.join("config.toml");
        let mut config = read_config(config_file, &self.env)?;

        // Read schema from cache directory
        let cache_dir = self.cache_dir.join("schemas");
        let all_schemas = if schema::cache_exists(&cache_dir) {
            schema::read_all_schemas(&cache_dir).await?
        } else {
            HashMap::new()
        };
        let schemas = argparse::CommandGen(all_schemas);

        // Setup clap command based on schema
        let command = schemas.command();
        let matches = match command.clone().try_get_matches_from(args) {
            Ok(matches) => matches,
            Err(error) => {
                if error.kind() != clap::error::ErrorKind::DisplayVersion
                    && !cache_exists(&cache_dir)
                {
                    self.console.display_warning(
                        "schema cache is missing - please generate it with: pexshell cache",
                    );
                }
                error.exit()
            }
        };

        // Log to file
        if let Some(log_file) = matches.get_one::<String>("log") {
            LOGGER.set_log_file(Some(String::from(log_file)))?;
        }

        // Setup web client
        let client = {
            let unsafe_client = matches.get_flag("insecure");
            let client = reqwest::Client::builder().danger_accept_invalid_certs(unsafe_client);
            client.build()
        }?;

        // login
        if let Some(login_sub) = matches.subcommand_matches(&argparse::Login.to_string()) {
            argparse::Login
                .run(self, &mut config, config_file, client, login_sub)
                .await?;
            return Ok(());
        } else if config.get_current_user().is_none() && config.get_env_user().is_none() {
            return Err(error::UserFriendly::new(
                "no user signed in - please sign into a management node with: pexshell login",
            )
            .into());
        }

        // cache
        if let Some(cache_matches) = matches.subcommand_matches(&argparse::Cache.to_string()) {
            argparse::Cache
                .run(&mut config, &cache_dir, client, cache_matches)
                .await?;
            return Ok(());
        } else if !cache_exists(&cache_dir) {
            self.console.display_warning(
                "schema cache is missing - please generate it with: pexshell cache",
            );
            return Err(error::UserFriendly::new(
                "schema cache is missing - please generate it with: pexshell cache",
            )
            .into());
        }

        // completions
        if let Some(completions_sub) =
            matches.subcommand_matches(&argparse::Completions.to_string())
        {
            argparse::Completions.run(self, &command, completions_sub);
            return Ok(());
        }

        // api request
        self.api_request(client, &config, &matches, &schemas)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lib::util::SimpleLogger;
    use log::{Level, Log, Record};

    /// Make sure logging enabled logic is working in the shell crate
    #[test]
    fn test_logging() {
        // Arrange
        let logger = SimpleLogger::new(None).unwrap();
        logger.set_max_level(log::LevelFilter::Debug);
        let record_1 = Record::builder()
            .level(Level::Debug)
            .target(module_path!())
            .args(format_args!("first record"))
            .build();

        let record_2 = Record::builder()
            .level(Level::Trace)
            .target(module_path!())
            .args(format_args!("second record"))
            .build();

        // Act & Assert
        assert!(logger.enabled(record_1.metadata()));
        assert!(!logger.enabled(record_2.metadata()));
    }
}

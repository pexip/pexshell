use crate::{
    argparse,
    cli::Console,
    config::{self, Config, Manager as ConfigManager, Provider},
    Directories, LOGGER,
};

use fd_lock::RwLock;
use futures::TryStreamExt;
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
    fs::{self, File},
    future,
    io::Write,
    path::PathBuf,
};

fn read_config<'a>(
    file_lock: &'a mut Option<RwLock<File>>,
    dirs: &Directories,
    env: &HashMap<String, String>,
) -> anyhow::Result<ConfigManager<'a>> {
    debug!(
        "Ensuring config directory path is created: {:?}",
        &dirs.config_dir
    );
    fs::create_dir_all(&dirs.config_dir)?;

    let config_file_path = dirs.config_dir.join("config.toml");
    debug!("Reading config from file: {:?}", &config_file_path);

    if !config_file_path.exists() {
        return Ok(ConfigManager::with_config(
            Config::new(dirs),
            &config_file_path,
            file_lock,
            env.clone(),
        )?);
    }

    let config = ConfigManager::read_from_file(&config_file_path, file_lock, env.clone())?;

    LOGGER.set_log_to_stderr(config.get_log_to_stderr());

    if let Some(log) = config.get_log_file_path() {
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
    directories: &'a Directories,
    pub console: Console,
    env: HashMap<String, String>,
}

impl<'a> PexShell<'a> {
    pub const fn new(
        directories: &'a Directories,
        console: Console,
        env: HashMap<String, String>,
    ) -> Self {
        Self {
            directories,
            console,
            env,
        }
    }

    async fn api_request(
        &mut self,
        client: reqwest::Client,
        config: &impl config::Provider,
        matches: &clap::ArgMatches,
        schemas: &argparse::CommandGen,
    ) -> anyhow::Result<()> {
        let user = config.get_current_user()?;

        let api_client = mcu::ApiClient::new(
            client,
            &user.address,
            user.username.clone(),
            config.get_password_for_user(user)?,
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

    pub async fn run(&mut self, args: Vec<String>) -> anyhow::Result<()> {
        // File lock option to store the config file lock to maintain the lifetime
        let mut file_lock: Option<RwLock<File>> = None;
        // Read config file
        let mut config = read_config(&mut file_lock, self.directories, &self.env)?;

        // Read schema from cache directory
        let cache_dir = self.directories.cache_dir.join("schemas");
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
        if let Some(log_file) = matches.get_one::<PathBuf>("log") {
            LOGGER.set_log_file(Some(log_file.clone()))?;
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
                .run(self, &mut config, client, login_sub)
                .await?;
            return Ok(());
        }

        // cache
        if let Some(cache_matches) = matches.subcommand_matches(&argparse::Cache.to_string()) {
            argparse::Cache
                .run(&config, &cache_dir, client, cache_matches)
                .await?;
            return Ok(());
        } else if !cache_exists(&cache_dir) {
            config.get_current_user()?; // show config error instead of schema cache error if no current user
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
    use std::collections::HashMap;

    use crate::{pexshell::read_config, test_util::TestContextExtensions};
    use lib::util::SimpleLogger;
    use log::{Level, Log, Record};
    use test_helpers::get_test_context;

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

    #[test]
    pub fn test_read_from_file_not_found() {
        // Arrange
        let test_context = get_test_context();
        let dirs = test_context.get_directories();
        let config_path = dirs.config_dir.join("config.toml");
        assert!(!config_path.exists());

        // Act
        let mut file_lock = None;
        let config = read_config(&mut file_lock, &dirs, &HashMap::default()).unwrap();
        drop(config);

        // Assert
        assert!(config_path.exists());
        let log_file_path = String::from(dirs.tmp_dir.join("pexshell.log").to_str().unwrap());
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            format!(
                r#"users = []

[log]
file = {file_path}
"#,
                file_path = if log_file_path.contains('\\') {
                    format!("'{log_file_path}'")
                } else {
                    format!("\"{log_file_path}\"")
                }
            )
        );

        std::fs::remove_file(config_path).unwrap();
    }
}

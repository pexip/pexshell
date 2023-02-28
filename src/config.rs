use crate::consts::{
    ENV_LOG_FILE, ENV_LOG_LEVEL, ENV_LOG_TO_STDERR, ENV_USER_ADDRESS, ENV_USER_PASSWORD,
    ENV_USER_USERNAME,
};
use crate::{cli::Console, error};
use lib::util::SensitiveString;
use log::debug;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path, sync::Arc};

#[cfg(test)]
use mockall::automock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub address: String,
    pub username: String,
    pub password: Option<SensitiveString>,
    pub current_user: bool,
}

#[cfg_attr(test, automock)]
pub trait Provider {
    fn get_log_file(&self) -> Option<String>;

    fn get_log_level(&self) -> Option<String>;

    fn get_log_to_stderr(&self) -> bool;

    fn get_address(&self) -> Result<String, error::UserFriendly>;

    fn get_username(&self) -> Result<String, error::UserFriendly>;

    fn get_password(&self) -> Result<SensitiveString, error::UserFriendly>;

    fn get_users(&self) -> &[User];

    /// Add a user to the users list.
    fn add_user(&mut self, console: &mut Console, user: User) -> Result<(), error::UserFriendly>;

    /// Removes a user from the users list.
    fn delete_user(&mut self, index: usize) -> Result<(), error::UserFriendly>;

    fn set_current_user(&mut self, user: &User);

    #[allow(clippy::needless_lifetimes)]
    fn get_current_user<'a>(&'a self) -> Option<&'a User>;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Logging {
    file: Option<String>,
    level: Option<String>,
    stderr: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Config {
    log: Option<Logging>,
    users: Vec<User>,
}

pub struct Manager {
    config: Config,
    env: HashMap<String, String>,
    keyring: Arc<Mutex<Box<dyn credentials::Provider>>>,
}

impl Manager {
    fn get_var<T>(
        &self,
        env_name: &str,
        config_value: Option<T>,
        error_message: &str,
    ) -> Result<T, error::UserFriendly>
    where
        String: Into<T>,
    {
        self.env.get(env_name).cloned().map_or_else(
            || config_value.map_or_else(|| Err(error::UserFriendly::new(error_message)), Ok),
            |x| Ok(x.into()),
        )
    }

    pub fn with_config(config: Config, env: HashMap<String, String>) -> Self {
        Self::with_config_and_keyring(config, env, credentials::Keyring {})
    }

    fn with_config_and_keyring(
        config: Config,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + 'static,
    ) -> Self {
        Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
        }
    }

    /// Reads the config from a file, returning the result.
    ///
    /// Will return Ok(None) if the file does not exist,
    /// or an Err if the file can't be read or is invalid.
    pub fn read_from_file(
        config_file: impl AsRef<Path>,
        env: HashMap<String, String>,
    ) -> Result<Option<Self>, error::UserFriendly> {
        Self::read_from_file_with_keyring(config_file, env, credentials::Keyring {})
    }

    fn read_from_file_with_keyring(
        config_file: impl AsRef<Path>,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + 'static,
    ) -> Result<Option<Self>, error::UserFriendly> {
        let config_file = config_file.as_ref();
        debug!("Reading config from file: {:?}", &config_file);
        if !config_file.exists() {
            return Ok(None);
        }

        let config = fs::read_to_string(config_file)
            .map_err(|_| error::UserFriendly::new("failed to read config file"))?;
        let config: Config = toml::from_str(config.as_str())
            .map_err(|_| error::UserFriendly::new("config is invalid"))?;
        debug!("Read the following config: {:?}", &config);

        Ok(Some(Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
        }))
    }

    /// Writes the config to a file.
    ///
    /// Will return an Err if the config cannot be serialised or writing to the file fails.
    pub fn write_to_file(&self, config_file: &Path) -> Result<(), error::UserFriendly> {
        let s = toml::to_string(&self.config).expect("config serialisation should not fail");
        fs::write(config_file, s)
            .map_err(|e| error::UserFriendly::new(format!("could not write config file: {e}")))
    }

    /// Gets a user entirely defined by environment variables (if they are all set)
    pub fn get_env_user(&self) -> Option<User> {
        let address = self.env.get(ENV_USER_ADDRESS)?.clone();
        let username = self.env.get(ENV_USER_USERNAME)?.clone();
        let password = Some(SensitiveString::from(
            self.env.get(ENV_USER_PASSWORD)?.clone(),
        ));
        Some(User {
            address,
            username,
            password,
            current_user: false,
        })
    }
}

impl Provider for Manager {
    fn get_log_file(&self) -> Option<String> {
        self.env.get(ENV_LOG_FILE).cloned().map_or_else(
            || self.config.log.as_ref().and_then(|l| l.file.clone()),
            Some,
        )
    }

    fn get_log_level(&self) -> Option<String> {
        self.env.get(ENV_LOG_LEVEL).cloned().map_or_else(
            || self.config.log.as_ref().and_then(|l| l.level.clone()),
            Some,
        )
    }

    fn get_log_to_stderr(&self) -> bool {
        self.env
            .get(ENV_LOG_TO_STDERR)
            .map_or_else(
                || self.config.log.as_ref().and_then(|l| l.stderr),
                |_| Some(true),
            )
            .unwrap_or(false)
    }

    fn get_address(&self) -> Result<String, error::UserFriendly> {
        self.get_var(
            ENV_USER_ADDRESS,
            self.get_current_user().map(|x| x.address.clone()),
            "Management node address not configured!",
        )
    }

    fn get_username(&self) -> Result<String, error::UserFriendly> {
        self.get_var(
            ENV_USER_USERNAME,
            self.get_current_user().map(|x| x.username.clone()),
            "Username not configured!",
        )
    }

    fn get_password(&self) -> Result<SensitiveString, error::UserFriendly> {
        if let (Some(address), Some(username), None) = (
            self.env.get(ENV_USER_ADDRESS),
            self.env.get(ENV_USER_USERNAME),
            self.env.get(ENV_USER_PASSWORD),
        ) {
            self.config
                .users
                .iter()
                .find(|u| u.address == *address && u.username == *username)
                .map_or_else(
                    || {
                        Err(error::UserFriendly::new(format!(
                            "{ENV_USER_ADDRESS} and {ENV_USER_USERNAME} environment variables are configured, but \
                             {ENV_USER_PASSWORD} is missing and the indicated user has not been logged in!",
                        )))
                    },
                    |user| {
                        user.password.clone().map_or_else(
                            || {
                                self.keyring
                                    .lock()
                                    .retrieve(&user.address, &user.username)
                                    .map_err(|e| {
                                        error::UserFriendly::new(format!(
                                            "Error retrieving credentials from system store: {e}"
                                        ))
                                    })
                            },
                            Ok,
                        )
                    },
                )
        } else {
            self.get_var(
                ENV_USER_PASSWORD,
                self.get_current_user().and_then(|x| x.password.clone()),
                "",
            )
            .or_else(|_| {
                let user = self
                    .get_current_user()
                    .ok_or_else(|| error::UserFriendly::new("Password is not configured"))?;
                self.keyring
                    .lock()
                    .retrieve(&user.address, &user.username)
                    .map_err(|e| {
                        error::UserFriendly::new(format!(
                        "Password is not configured and could not be retrieved from the system \
                         store: {e}"
                    ))
                    })
            })
        }
    }

    #[must_use]
    fn get_users(&self) -> &[User] {
        &self.config.users
    }

    /// Add a user to the users list.
    fn add_user(
        &mut self,
        console: &mut Console,
        mut user: User,
    ) -> Result<(), error::UserFriendly> {
        assert!(user.password.is_some(), "No password specified!");
        if self.keyring.lock().available() {
            self.keyring
                .lock()
                .save(
                    &user.address,
                    &user.username,
                    &user.password.take().unwrap(),
                )
                .map_err(|e| {
                    error::UserFriendly::new(format!(
                        "could not save password to system credential store: {e}"
                    ))
                })?;
        } else {
            console
                .display_warning("Credential store unavailable - storing passwords in plaintext!");
        }
        self.config.users.push(user);
        Ok(())
    }

    fn delete_user(&mut self, index: usize) -> Result<(), error::UserFriendly> {
        let user = self.config.users.remove(index);
        if user.password.is_none() && self.keyring.lock().available() {
            self.keyring
                .lock()
                .delete(&user.address, &user.username)
                .map_err(|e| {
                    error::UserFriendly::new(format!(
                        "could not delete password from system credential store: {e}"
                    ))
                })?;
        }
        Ok(())
    }

    fn set_current_user(&mut self, user: &User) {
        for u in &mut self.config.users {
            u.current_user = false;
            if (&u.username, &u.address) == (&user.username, &user.address) {
                u.current_user = true;
            }
        }
    }

    #[must_use]
    fn get_current_user(&self) -> Option<&User> {
        self.config.users.iter().find(|user| user.current_user)
    }
}

mod credentials {
    use lib::util::SensitiveString;

    #[cfg(test)]
    use mockall::automock;

    const SERVICE: &str = "pexshell";

    #[cfg_attr(test, automock)]
    pub trait Provider {
        /// Checks if the keyring service is available right now.
        fn available(&self) -> bool;
        fn retrieve(&self, address: &str, username: &str) -> keyring::Result<SensitiveString>;
        fn save(
            &mut self,
            address: &str,
            username: &str,
            password: &SensitiveString,
        ) -> keyring::Result<()>;
        fn delete(&mut self, address: &str, username: &str) -> keyring::Result<()>;
    }

    #[derive(Clone)]
    pub struct Keyring {}

    impl Provider for Keyring {
        fn available(&self) -> bool {
            fn inner() -> keyring::Result<bool> {
                // try to save, access and delete an entry.
                const TEST_PASSWORD: &str = "test_password";
                let entry = keyring::Entry::new(SERVICE, "AVAILABILITY CHECK")?;
                entry.set_password(TEST_PASSWORD)?;
                let password = entry.get_password()?;
                entry.delete_password()?;
                Ok(TEST_PASSWORD == password)
            }
            inner().unwrap_or(false)
        }

        fn retrieve(&self, address: &str, username: &str) -> keyring::Result<SensitiveString> {
            let ident = format!("{username}@{address}");
            let entry = keyring::Entry::new(SERVICE, &ident)?;
            entry.get_password().map(SensitiveString::from)
        }

        fn save(
            &mut self,
            address: &str,
            username: &str,
            password: &SensitiveString,
        ) -> keyring::Result<()> {
            let ident = format!("{username}@{address}");
            let entry = keyring::Entry::new(SERVICE, &ident)?;
            entry.set_password(password.secret())
        }

        fn delete(&mut self, address: &str, username: &str) -> keyring::Result<()> {
            let ident = format!("{username}@{address}");
            let entry = keyring::Entry::new(SERVICE, &ident)?;
            entry.delete_password()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use test_helpers::get_test_context;

    use super::*;

    #[test]
    pub fn test_read_from_file_not_found() {
        // Arrange
        let config_path = std::env::temp_dir().join("pex_config_file_that_should_not_exist.toml");

        // Act
        let config = Manager::read_from_file_with_keyring(
            config_path,
            HashMap::default(),
            credentials::MockProvider::new(),
        )
        .unwrap();

        // Assert
        assert!(config.is_none());
    }

    #[test]
    pub fn test_read_empty_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir().to_str().unwrap();

        let config = "";
        let config_path = format!("{}/config.toml", &work_dir);
        std::fs::write(&config_path, config).unwrap();

        // Act
        let config = Manager::read_from_file_with_keyring(
            PathBuf::from(&config_path),
            HashMap::default(),
            credentials::MockProvider::new(),
        );

        // Assert
        assert!(config.is_err());
        let e = config.map(|m| m.unwrap().config).unwrap_err();

        assert_eq!(format!("{e}").as_str(), "config is invalid");
    }

    #[test]
    pub fn test_invalid_read_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir().to_str().unwrap();

        let config = b"\xf0\x28\x8c\x28";
        let config_path = format!("{}/config.toml", &work_dir);
        std::fs::write(&config_path, config).unwrap();

        // Act
        let config = Manager::read_from_file_with_keyring(
            PathBuf::from(&config_path),
            HashMap::default(),
            credentials::MockProvider::new(),
        );

        // Assert
        assert!(config.is_err());
        let e = config.map(|m| m.unwrap().config).unwrap_err();

        assert_eq!(format!("{e}").as_str(), "failed to read config file");
    }

    #[test]
    pub fn test_read_from_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir().to_str().unwrap();
        let config = r#"
        [log]
        file = "/path/to/some/pexshell.log"
        level = "debug"

        [[users]]
        address = "test_address.test.com"
        username = "admin"
        password = "some_admin_password"
        current_user = false

        [[users]]
        address = "test_address.testing.com"
        username = "a_user"
        password = "another_password"
        current_user = true
        "#;
        let config_path = format!("{}/config.toml", &work_dir);
        std::fs::write(&config_path, config).unwrap();

        // Act
        let config = Manager::read_from_file_with_keyring(
            PathBuf::from(&config_path),
            HashMap::default(),
            credentials::MockProvider::new(),
        )
        .unwrap()
        .unwrap()
        .config;

        // Assert
        assert_eq!(config.users.len(), 2);

        assert_eq!(config.users[0].address, "test_address.test.com");
        assert_eq!(config.users[0].username, "admin");
        assert_eq!(
            config.users[0].password.clone().unwrap().secret(),
            "some_admin_password"
        );
        assert!(!config.users[0].current_user);

        assert_eq!(config.users[1].address, "test_address.testing.com");
        assert_eq!(config.users[1].username, "a_user");
        assert_eq!(
            config.users[1].password.clone().unwrap().secret(),
            "another_password"
        );
        assert!(config.users[1].current_user);

        assert_eq!(
            config.log.as_ref().and_then(|l| l.file.as_deref()),
            Some("/path/to/some/pexshell.log")
        );
        assert_eq!(
            config.log.as_ref().and_then(|l| l.level.as_deref()),
            Some("debug")
        );
    }

    #[test]
    fn test_write_to_file() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(String::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let keyring = credentials::MockProvider::new();

        let mgr = Manager::with_config_and_keyring(config, HashMap::default(), keyring);

        // Act
        mgr.write_to_file(&config_path).unwrap();

        // Assert
        let written_config = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            written_config,
            r#"[log]
file = "/path/to/some/pexshell.log"
level = "debug"

[[users]]
address = "test_address.test.com"
username = "admin"
password = "some_admin_password"
current_user = false

[[users]]
address = "test_address.testing.com"
username = "a_user"
current_user = true
"#
        );
    }
}

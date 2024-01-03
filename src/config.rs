#![allow(clippy::significant_drop_tightening)]

use crate::cli::Console;
use crate::consts::{
    ENV_LOG_FILE, ENV_LOG_LEVEL, ENV_LOG_TO_STDERR, ENV_USER_ADDRESS, ENV_USER_PASSWORD,
    ENV_USER_USERNAME,
};
use crate::error;
use crate::Directories;
use chrono::{serde::ts_seconds_option, DateTime, Utc};
use fslock::LockFile;
use lib::util::SensitiveString;
use log::debug;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
use std::ops::Not;
use std::path::PathBuf;
use std::{collections::HashMap, fs::File, path::Path, sync::Arc};

#[cfg(test)]
use mockall::mock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub address: String,
    pub username: String,
    #[cfg(not(test))]
    password: Option<SensitiveString>,
    #[cfg(test)]
    pub password: Option<SensitiveString>,
    #[serde(default, skip_serializing_if = "Not::not")]
    #[allow(clippy::struct_field_names)]
    pub current_user: bool,
    #[serde(with = "ts_seconds_option", default)]
    pub last_used: Option<DateTime<Utc>>,
}

impl User {
    pub fn new(address: String, username: String, password: SensitiveString) -> Self {
        Self {
            address,
            username,
            password: Some(password),
            current_user: false,
            last_used: None,
        }
    }
}

#[cfg(test)]
mock! {
    pub ConfigManager {}

    impl Provider for ConfigManager {
        fn get_log_file_path(&self) -> Option<PathBuf>;
        fn get_log_level(&self) -> Option<String>;
        fn get_log_to_stderr(&self) -> bool;
        fn get_current_user<'a>(&'a self) -> Result<&'a User, error::UserFriendly>;
        fn get_password_for_user(&self, user: &User) -> Result<SensitiveString, error::UserFriendly>;
        fn set_last_used(&mut self) -> Result<(), error::UserFriendly>;
    }

    impl Configurer for ConfigManager {
        fn get_users(&self) -> &[User];
        fn add_user(
            &mut self,
            user: User,
            store_password_in_plaintext: bool,
        ) -> Result<(), error::UserFriendly>;
        fn delete_user(&mut self, index: usize) -> Result<(), error::UserFriendly>;
        fn set_current_user(&mut self, user: &User);
        fn try_get_current_user<'a>(&'a self) -> Option<&'a User>;
    }
}

/// Abstraction for accessing config. Takes into account environment variables.
pub trait Provider: Send + Sync {
    /// Gets the configured log file path.
    fn get_log_file_path(&self) -> Option<PathBuf>;

    /// Gets the configured minimum log level.
    fn get_log_level(&self) -> Option<String>;

    /// Gets whether logs should be written to STDERR.
    fn get_log_to_stderr(&self) -> bool;

    /// Gets the currently active user.
    /// Note that this user may be partially or entirely defined by environment variables.
    ///
    /// # Errors
    /// If the current user cannot be determined, this function will return an [`error::UserFriendly`].
    fn get_current_user(&self) -> Result<&User, error::UserFriendly>;

    /// Retrieves the password of a user.
    fn get_password_for_user(&self, user: &User) -> Result<SensitiveString, error::UserFriendly>;

    // Sets last used
    fn set_last_used(&mut self) -> Result<(), error::UserFriendly>;
}

/// Abstraction for accessing and modifying config.
/// Does NOT take into account environment variables.
pub trait Configurer: Send + Sync {
    fn get_users(&self) -> &[User];

    /// Add a user to the users list.
    fn add_user(
        &mut self,
        user: User,
        store_password_in_plaintext: bool,
    ) -> Result<(), error::UserFriendly>;

    /// Removes a user from the users list.
    fn delete_user(&mut self, index: usize) -> Result<(), error::UserFriendly>;

    /// Sets a given user as the currently active user.
    fn set_current_user(&mut self, user: &User);

    fn try_get_current_user(&self) -> Option<&User>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    file: Option<PathBuf>,
    level: Option<String>,
    stderr: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    log: Option<Logging>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    users: Vec<User>,
}

impl Config {
    pub fn new(dirs: &Directories) -> Self {
        let log_file_path = dirs.tmp_dir.join("pexshell.log");
        Self {
            log: Some(Logging {
                file: Some(log_file_path),
                level: None,
                stderr: None,
            }),
            users: Vec::new(),
        }
    }
}

pub struct Manager {
    config: Config,
    env: HashMap<String, String>,
    keyring: Arc<Mutex<Box<dyn credentials::Provider + Send>>>,
    _file_lock: LockFile,
    file_handle: File,
    env_user: Option<User>,
}

enum UserConfigContext {
    File(usize),
    Env,
}

impl Manager {
    pub fn with_config(
        config: Config,
        config_file: &Path,
        config_lock_file: &Path,
        env: HashMap<String, String>,
        console: &mut Console,
    ) -> Result<Self, error::UserFriendly> {
        Self::with_config_and_keyring(
            config,
            config_file,
            config_lock_file,
            env,
            credentials::Keyring {},
            console,
        )
    }

    fn with_config_and_keyring(
        config: Config,
        config_file_path: &Path,
        config_lock_file_path: &Path,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + Send + 'static,
        console: &mut Console,
    ) -> Result<Self, error::UserFriendly> {
        if let Some(parent) = config_file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error::UserFriendly::new(format!("failed to create config directory: {e}"))
            })?;
        }

        let mut file_lock = LockFile::open(config_lock_file_path)
            .map_err(|e| error::UserFriendly::new(format!("failed to lock config file: {e}")))?;

        debug!("Attempting to acquire lock to config file...");
        if !file_lock
            .try_lock()
            .map_err(|e| error::UserFriendly::new(format!("failed to lock config file: {e}")))?
        {
            debug!("waiting for config file lock...");
            writeln!(console, "waiting for config file lock...").unwrap();
            file_lock.lock().map_err(|e| {
                error::UserFriendly::new(format!("failed to lock config file: {e}"))
            })?;
        }
        debug!("Config file lock acquired");

        let file_handle = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(config_file_path)
            .map_err(|e| error::UserFriendly::new(format!("failed to read config file: {e}")))?;

        let env_user = Self::get_env_user(&env);

        let mut manager = Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
            file_handle,
            _file_lock: file_lock,
            env_user,
        };

        manager.write_to_file()?;

        Ok(manager)
    }

    /// Reads the config from a file, returning the result.
    ///
    /// Will return Err if the file does not exist,
    /// or an Err if the file can't be read or is invalid.
    pub fn read_from_file(
        config_file_path: &Path,
        config_lock_file_path: &Path,
        env: HashMap<String, String>,
        console: &mut Console,
    ) -> Result<Self, error::UserFriendly> {
        Self::read_from_file_with_keyring(
            config_file_path,
            config_lock_file_path,
            env,
            credentials::Keyring {},
            console,
        )
    }

    fn read_from_file_with_keyring(
        config_file_path: &Path,
        config_lock_file_path: &Path,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + Send + 'static,
        console: &mut Console,
    ) -> Result<Self, error::UserFriendly> {
        let mut file_lock = LockFile::open(config_lock_file_path)
            .map_err(|e| error::UserFriendly::new(format!("failed to lock config file: {e}")))?;

        debug!("Attempting to acquire lock to config file...");
        if !file_lock
            .try_lock()
            .map_err(|e| error::UserFriendly::new(format!("failed to lock config file: {e}")))?
        {
            debug!("Another process is holding the lock - waiting for it to be freed...");
            writeln!(
                console,
                "another process has locked the config file - waiting for it to be freed..."
            )
            .unwrap();
            file_lock.lock().map_err(|e| {
                error::UserFriendly::new(format!("failed to lock config file: {e}"))
            })?;
        }
        debug!("Config file lock acquired");

        let mut file_handle = File::options()
            .read(true)
            .write(true)
            .open(config_file_path)
            .map_err(|_| error::UserFriendly::new("failed to read config file"))?;

        let config: Config = {
            let mut config = String::new();
            file_handle
                .read_to_string(&mut config)
                .map_err(|_| error::UserFriendly::new("config is invalid"))?;
            toml::from_str(&config)
                .map_err(|e| error::UserFriendly::new(format!("config is invalid: {e}")))
        }?;

        debug!("Read the following config: {:?}", &config);

        let env_user = Self::get_env_user(&env);

        Ok(Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
            _file_lock: file_lock,
            file_handle,
            env_user,
        })
    }

    /// Writes the config to a file.
    ///
    /// Will return an Err if the config cannot be serialised or writing to the file fails.
    pub fn write_to_file(&mut self) -> Result<(), error::UserFriendly> {
        let s = toml::to_string(&self.config).expect("config serialisation should not fail");

        self.do_write(&s)
            .map_err(|e| error::UserFriendly::new(format!("could not write config file: {e}")))
    }

    fn do_write(&mut self, content: &str) -> Result<(), std::io::Error> {
        self.file_handle.set_len(0)?;
        self.file_handle.rewind()?;
        self.file_handle.write_all(content.as_bytes())
    }

    /// Gets the context required to determine the current user and how they are configured.
    /// Will fail if a current user has not been configured.
    fn get_current_user_config_context(&self) -> Result<UserConfigContext, error::UserFriendly> {
        self.env_user.as_ref().map_or_else(|| {
            let env_address = self.env.get(ENV_USER_ADDRESS);
            let env_username = self.env.get(ENV_USER_USERNAME);

            match (env_address, env_username) {
                (Some(env_address), Some(env_username)) => {
                    self.config.users.iter().position(|u| u.address == *env_address && u.username == *env_username)
                    .map_or_else(|| Err(error::UserFriendly::new(format!(
                            "environment variables {ENV_USER_ADDRESS} and {ENV_USER_USERNAME} were set, \
                            but {ENV_USER_PASSWORD} was not, and couldn't find a matching user in the config file\n\
                            either login with matching credentials, set {ENV_USER_PASSWORD} in the environment, or \
                            unset {ENV_USER_ADDRESS} and {ENV_USER_USERNAME} in the environment"
                        ))), |i| Ok(UserConfigContext::File(i)))
                }
                (Some(_env_address), None) => {
                    Err(error::UserFriendly::new(format!(
                        "{ENV_USER_ADDRESS} was set in the environment, but {ENV_USER_USERNAME} was not\n\
                        please set either both environment variables, or neither"
                    )))
                }
                (None, Some(_env_username)) => {
                    Err(error::UserFriendly::new(format!(
                        "{ENV_USER_USERNAME} was set in the environment, but {ENV_USER_ADDRESS} was not\n\
                        please set either both environment variables, or neither"
                    )))
                }
                (None, None) => {
                    self.config.users.iter().position(|u| u.current_user)
                        .map_or_else(|| Err(error::UserFriendly::new(String::from(
                            "no user signed in - please sign into a management node with: pexshell login"
                        ))), |i| Ok(UserConfigContext::File(i)))
                }
            }
        }, |_env_user| Ok(UserConfigContext::Env))
    }

    /// Gets a user entirely defined by environment variables (if they are all set)
    fn get_env_user(env: &HashMap<String, String>) -> Option<User> {
        let address = env.get(ENV_USER_ADDRESS)?.clone();
        let username = env.get(ENV_USER_USERNAME)?.clone();
        let password = Some(SensitiveString::from(env.get(ENV_USER_PASSWORD)?.clone()));
        Some(User {
            address,
            username,
            password,
            current_user: false,
            last_used: None,
        })
    }
}

impl Provider for Manager {
    fn get_log_file_path(&self) -> Option<PathBuf> {
        self.env.get(ENV_LOG_FILE).map_or_else(
            || self.config.log.as_ref().and_then(|l| l.file.clone()),
            |s| Some(PathBuf::from(s)),
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

    fn get_current_user(&self) -> Result<&User, error::UserFriendly> {
        match self.get_current_user_config_context()? {
            UserConfigContext::File(i) => Ok(&self.config.users[i]),
            UserConfigContext::Env => Ok(self.env_user.as_ref().unwrap()),
        }
    }

    fn get_password_for_user(&self, user: &User) -> Result<SensitiveString, error::UserFriendly> {
        user.password.clone().map_or_else(
            || {
                self.keyring
                    .lock()
                    .retrieve(&user.address, &user.username)
                    .map_err(|e| {
                        error::UserFriendly::new(format!(
                            "Password is not configured and could not be retrieved from the system store: {e}"
                        ))
                    })
            },
            Ok,
        )
    }

    fn set_last_used(&mut self) -> Result<(), error::UserFriendly> {
        match self
            .get_current_user_config_context()
            .expect("no user logged in")
        {
            UserConfigContext::File(i) => {
                let user = &mut self.config.users[i];
                user.last_used = Some(chrono::offset::Utc::now());

                self.write_to_file()?;
            }
            UserConfigContext::Env => debug!("Not updating last used for environmental user"),
        }
        Ok(())
    }
}

impl Configurer for Manager {
    #[must_use]
    fn get_users(&self) -> &[User] {
        &self.config.users
    }

    /// Add a user to the users list.
    fn add_user(
        &mut self,
        mut user: User,
        store_password_in_plaintext: bool,
    ) -> Result<(), error::UserFriendly> {
        assert!(user.password.is_some(), "No password specified!");
        if !store_password_in_plaintext {
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
        }

        self.config.users.push(user);
        Ok(())
    }

    fn delete_user(&mut self, index: usize) -> Result<(), error::UserFriendly> {
        let user = self.config.users.remove(index);
        if user.password.is_none() {
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
    fn try_get_current_user(&self) -> Option<&User> {
        self.config.users.iter().find(|user| user.current_user)
    }
}

mod credentials {
    use lib::util::SensitiveString;

    #[cfg(test)]
    use mockall::automock;

    const SERVICE: &str = "pexshell";

    #[cfg_attr(test, automock)]
    pub trait Provider: Send {
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
    use mockall::predicate::{eq, function};
    use test_helpers::get_test_context;

    use crate::test_util::TestContextExtensions;

    use super::*;
    use chrono::TimeZone;
    use test_case::test_case;

    #[test]
    fn test_read_empty_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir();

        let config = "";
        let config_path = work_dir.join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );
        std::fs::write(&config_path, config).unwrap();

        // Act
        let result = Manager::read_from_file_with_keyring(
            &config_path,
            &lock_path,
            HashMap::default(),
            credentials::MockProvider::new(),
            &mut console,
        );

        // Assert
        let config = result.unwrap().config;
        assert!(config.users.is_empty());
        assert!(config.log.is_none());
    }

    #[test]
    fn test_invalid_read_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir();

        let config = b"\xf0\x28\x8c\x28";
        let config_path = work_dir.join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );
        std::fs::write(&config_path, config).unwrap();

        // Act
        let config = Manager::read_from_file_with_keyring(
            &config_path,
            &lock_path,
            HashMap::default(),
            credentials::MockProvider::new(),
            &mut console,
        );

        // Assert
        assert!(config.is_err());
        let e = config.map(|m| m.config).unwrap_err();

        assert_eq!(format!("{e}").as_str(), "config is invalid");
    }

    #[test]
    fn test_read_from_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir();
        let config = r#"
        [log]
        file = "/path/to/some/pexshell.log"
        level = "debug"

        [[users]]
        address = "test_address.test.com"
        username = "admin"
        password = "some_admin_password"

        [[users]]
        address = "test_address.testing.com"
        username = "a_user"
        password = "another_password"
        current_user = true
        last_used = 1192778584
        "#;
        let config_path = work_dir.join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );
        std::fs::write(&config_path, config).unwrap();

        // Act
        let config = Manager::read_from_file_with_keyring(
            &config_path,
            &lock_path,
            HashMap::default(),
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap()
        .config;

        // Assert
        assert_eq!(config.users.len(), 2);

        assert_eq!(config.users[0].address, "test_address.test.com");
        assert_eq!(config.users[0].username, "admin");
        assert_eq!(
            config.users[0].password.as_ref().unwrap().secret(),
            "some_admin_password"
        );
        assert!(!config.users[0].current_user);

        assert_eq!(config.users[1].address, "test_address.testing.com");
        assert_eq!(config.users[1].username, "a_user");
        assert_eq!(
            config.users[1].password.as_ref().unwrap().secret(),
            "another_password"
        );
        assert!(config.users[1].current_user);
        assert_eq!(
            config.users[1].last_used,
            Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap())
        );

        assert_eq!(
            config.log.as_ref().and_then(|l| l.file.as_deref()),
            Some(Path::new("/path/to/some/pexshell.log"))
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
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let keyring = credentials::MockProvider::new();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();

        // Act
        mgr.write_to_file().unwrap();
        drop(mgr);

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

[[users]]
address = "test_address.testing.com"
username = "a_user"
current_user = true
last_used = 1192778584
"#
        );
    }

    #[test]
    fn test_write_empty_config_file() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: None,
            users: Vec::new(),
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let keyring = credentials::MockProvider::new();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();

        // Act
        mgr.write_to_file().unwrap();
        drop(mgr);

        // Assert
        let written_config = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(written_config, "");
    }

    #[test]
    fn test_multiple_writers() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: None,
            users: vec![User {
                address: String::from("test_address.test.com"),
                username: String::from("admin"),
                password: None,
                current_user: false,
                last_used: None,
            }],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let keyring = credentials::MockProvider::new();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();
        let mut test_lock = LockFile::open(&lock_path).unwrap();

        // Act
        mgr.write_to_file().unwrap();
        let acquired = test_lock.try_lock().unwrap();

        // Assert
        assert!(!acquired);
    }

    #[test]
    fn test_write_to_and_override_file() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: None,
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let keyring = credentials::MockProvider::new();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();

        // Act
        mgr.write_to_file().unwrap();
        mgr.delete_user(0).unwrap();
        mgr.write_to_file().unwrap();
        drop(mgr);

        // Assert
        let written_config = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            written_config,
            r#"[log]
file = "/path/to/some/pexshell.log"
level = "debug"

[[users]]
address = "test_address.testing.com"
username = "a_user"
current_user = true
"#
        );
    }

    #[test]
    fn test_add_user_with_plaintext_password() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: None,
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let keyring = credentials::MockProvider::new();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
            last_used: None,
        };

        // Act
        mgr.add_user(new_user, true).unwrap();

        // Assert
        let users = mgr.get_users();
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].address, "test_address.test.com");
        assert_eq!(users[0].username, "admin");
        assert_eq!(
            users[0].password.as_ref().unwrap().secret(),
            "some_admin_password"
        );
        assert!(!users[0].current_user);
        assert_eq!(users[1].address, "test_address.testing.com");
        assert_eq!(users[1].username, "a_user");
        assert!(users[1].password.is_none());
        assert!(users[1].current_user);
        assert_eq!(users[2].address, "new_address.testing.com");
        assert_eq!(users[2].username, "a_new_user");
        assert_eq!(
            users[2].password.as_ref().unwrap().secret(),
            "some_new_password"
        );
        assert!(!users[2].current_user);
    }

    #[test]
    fn test_add_user_with_credential_store() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: None,
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut keyring = credentials::MockProvider::new();
        keyring
            .expect_save()
            .with(
                eq("new_address.testing.com"),
                eq("a_new_user"),
                function(|s: &SensitiveString| s.secret() == "some_new_password"),
            )
            .once()
            .return_once(|_, _, _| Ok(()));
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
            last_used: None,
        };

        // Act
        mgr.add_user(new_user, false).unwrap();

        // Assert
        let users = mgr.get_users();
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].address, "test_address.test.com");
        assert_eq!(users[0].username, "admin");
        assert_eq!(
            users[0].password.as_ref().unwrap().secret(),
            "some_admin_password"
        );
        assert!(!users[0].current_user);
        assert_eq!(users[1].address, "test_address.testing.com");
        assert_eq!(users[1].username, "a_user");
        assert!(users[1].password.is_none());
        assert!(users[1].current_user);
        assert_eq!(users[2].address, "new_address.testing.com");
        assert_eq!(users[2].username, "a_new_user");
        assert!(users[2].password.is_none());
        assert!(!users[2].current_user);
    }

    #[test]
    fn test_add_user_with_credential_store_fails() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: None,
                },
            ],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut keyring = credentials::MockProvider::new();
        keyring
            .expect_save()
            .with(
                eq("new_address.testing.com"),
                eq("a_new_user"),
                function(|s: &SensitiveString| s.secret() == "some_new_password"),
            )
            .once()
            .return_once(|_, _, _| Err(keyring::Error::NoEntry));
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mut mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::default(),
            keyring,
            &mut console,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
            last_used: None,
        };

        // Act
        let error = mgr.add_user(new_user, false).unwrap_err();

        // Assert
        assert_eq!(error.to_string(), "could not save password to system credential store: No matching entry found in secure storage");

        let users = mgr.get_users();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].address, "test_address.test.com");
        assert_eq!(users[0].username, "admin");
        assert_eq!(
            users[0].password.as_ref().unwrap().secret(),
            "some_admin_password"
        );
        assert!(!users[0].current_user);
        assert_eq!(users[1].address, "test_address.testing.com");
        assert_eq!(users[1].username, "a_user");
        assert!(users[1].password.is_none());
        assert!(users[1].current_user);
    }

    #[test]
    fn test_no_users() {
        // Arrange
        let test_context = get_test_context();
        let dirs = test_context.get_directories();
        let config = Config::new(&dirs);
        let config_path = test_context.get_config_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );
        let mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::new(),
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap();

        // Act
        let result = mgr.get_current_user();

        // Assert
        assert_eq!(
            result.unwrap_err().to_string(),
            "no user signed in - please sign into a management node with: pexshell login"
        );
    }

    #[test]
    fn test_no_current_user() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: false,
                    last_used: None,
                },
            ],
        };
        let config_path = test_context.get_config_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );
        let mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            HashMap::new(),
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap();

        // Act
        let result = mgr.get_current_user();

        // Assert
        assert_eq!(
            result.unwrap_err().to_string(),
            "no user signed in - please sign into a management node with: pexshell login"
        );
    }

    #[test]
    fn test_only_environment_user() {
        // Arrange
        let test_context = get_test_context();
        let dirs = test_context.get_directories();
        let config = Config::new(&dirs);
        let config_path = test_context.get_config_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let env = HashMap::from([
            (
                String::from("PEXSHELL_ADDRESS"),
                String::from("some.address"),
            ),
            (
                String::from("PEXSHELL_USERNAME"),
                String::from("some_username"),
            ),
            (
                String::from("PEXSHELL_PASSWORD"),
                String::from("super_secret_password"),
            ),
        ]);
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            env,
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap();

        // Act
        let result = mgr.get_current_user();

        // Assert
        let user = result.unwrap();
        assert_eq!(user.address, "some.address");
        assert_eq!(user.username, "some_username");
        assert_eq!(
            user.password.as_ref().map(SensitiveString::secret),
            Some("super_secret_password")
        );
    }

    #[test_case(&[
            ("PEXSHELL_ADDRESS", "some.address"),
            ("PEXSHELL_USERNAME", "some_username"),
        ],
        "environment variables PEXSHELL_ADDRESS and PEXSHELL_USERNAME were set, but PEXSHELL_PASSWORD was not, \
         and couldn't find a matching user in the config file\n\
         either login with matching credentials, set PEXSHELL_PASSWORD in the environment, \
         or unset PEXSHELL_ADDRESS and PEXSHELL_USERNAME in the environment"
    )]
    #[test_case(&[
            ("PEXSHELL_ADDRESS", "some.address"),
            ("PEXSHELL_PASSWORD", "super_secret_password"),
        ],
        "PEXSHELL_ADDRESS was set in the environment, but PEXSHELL_USERNAME was not\n\
         please set either both environment variables, or neither"
    )]
    #[test_case(&[
            ("PEXSHELL_USERNAME", "some_username"),
            ("PEXSHELL_PASSWORD", "super_secret_password"),
        ],
        "PEXSHELL_USERNAME was set in the environment, but PEXSHELL_ADDRESS was not\n\
         please set either both environment variables, or neither"
    )]
    fn test_only_environment_user_missing_vars(env: &[(&str, &str)], error_message: &str) {
        // Arrange
        let test_context = get_test_context();
        let dirs = test_context.get_directories();
        let config = Config::new(&dirs);
        let config_path = test_context.get_config_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let env = env
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<HashMap<_, _>>();
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            env,
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap();

        // Act
        let result = mgr.get_current_user();

        // Assert
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), error_message);
    }

    #[test]
    fn test_env_selects_different_user() {
        // Arrange
        let test_context = get_test_context();
        let config = Config {
            log: Some(Logging {
                file: Some(PathBuf::from("/path/to/some/pexshell.log")),
                level: Some(String::from("debug")),
                stderr: None,
            }),
            users: vec![
                User {
                    address: String::from("test_address.test.com"),
                    username: String::from("admin"),
                    password: Some(SensitiveString::from("some_admin_password")),
                    current_user: false,
                    last_used: None,
                },
                User {
                    address: String::from("test_address.testing.com"),
                    username: String::from("a_user"),
                    password: None,
                    current_user: true,
                    last_used: None,
                },
            ],
        };
        let config_path = test_context.get_config_dir().join("config.toml");
        let lock_path = test_context.get_test_dir().join("config.lock");
        let env = HashMap::from([
            (
                String::from("PEXSHELL_ADDRESS"),
                String::from("test_address.test.com"),
            ),
            (String::from("PEXSHELL_USERNAME"), String::from("admin")),
        ]);
        let mut console = Console::new(
            false,
            test_context.get_stdout_wrapper(),
            false,
            test_context.get_stderr_wrapper(),
        );

        let mgr = Manager::with_config_and_keyring(
            config,
            &config_path,
            &lock_path,
            env,
            credentials::MockProvider::new(),
            &mut console,
        )
        .unwrap();

        // Act
        let result = mgr.get_current_user();

        // Assert
        let user = result.unwrap();
        assert_eq!(user.address, "test_address.test.com");
        assert_eq!(user.username, "admin");
        assert_eq!(
            user.password.as_ref().map(SensitiveString::secret),
            Some("some_admin_password")
        );
    }
}

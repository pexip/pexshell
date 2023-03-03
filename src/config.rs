use crate::consts::{
    ENV_LOG_FILE, ENV_LOG_LEVEL, ENV_LOG_TO_STDERR, ENV_USER_ADDRESS, ENV_USER_PASSWORD,
    ENV_USER_USERNAME,
};
use crate::error;
use crate::Directories;
use fd_lock::{RwLock, RwLockWriteGuard};
use lib::util::SensitiveString;
use log::debug;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
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
    pub current_user: bool,
}

impl User {
    pub fn new(address: String, username: String, password: SensitiveString) -> Self {
        Self {
            address,
            username,
            password: Some(password),
            current_user: false,
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
pub trait Provider: Send {
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
}

/// Abstraction for accessing and modifying config.
/// Does NOT take into account environment variables.
pub trait Configurer: Send {
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

pub struct Manager<'a> {
    config: Config,
    env: HashMap<String, String>,
    keyring: Arc<Mutex<Box<dyn credentials::Provider + Send>>>,
    config_file: RwLockWriteGuard<'a, File>,
    env_user: Option<User>,
}

enum UserConfigContext {
    File(usize),
    Env,
}

impl<'a> Manager<'a> {
    pub fn with_config(
        config: Config,
        config_file: &Path,
        file_lock: &'a mut Option<RwLock<File>>,
        env: HashMap<String, String>,
    ) -> Result<Self, error::UserFriendly> {
        Self::with_config_and_keyring(config, config_file, file_lock, env, credentials::Keyring {})
    }

    fn with_config_and_keyring(
        config: Config,
        config_file_path: &Path,
        file_lock: &'a mut Option<RwLock<File>>,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + Send + 'static,
    ) -> Result<Self, error::UserFriendly> {
        let config_file_lock = RwLock::new(
            File::options()
                .read(true)
                .write(true)
                .create_new(true)
                .open(config_file_path)
                .map_err(|_| error::UserFriendly::new("failed to read config file"))?,
        );

        *file_lock = Some(config_file_lock);

        let config_file = file_lock
            .as_mut()
            .unwrap()
            .write()
            .expect("failed to acquire read lock");

        let env_user = Self::get_env_user(&env);

        let mut manager = Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
            config_file,
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
        file_lock: &'a mut Option<RwLock<File>>,
        env: HashMap<String, String>,
    ) -> Result<Self, error::UserFriendly> {
        Self::read_from_file_with_keyring(config_file_path, file_lock, env, credentials::Keyring {})
    }

    fn read_from_file_with_keyring(
        config_file_path: &Path,
        file_lock: &'a mut Option<RwLock<File>>,
        env: HashMap<String, String>,
        keyring: impl credentials::Provider + Send + 'static,
    ) -> Result<Self, error::UserFriendly> {
        let config_file_lock = RwLock::new(
            File::options()
                .read(true)
                .write(true)
                .open(config_file_path)
                .map_err(|_| error::UserFriendly::new("failed to read config file"))?,
        );

        *file_lock = Some(config_file_lock);

        let mut config_file = file_lock
            .as_mut()
            .unwrap()
            .write()
            .expect("failed to acquire read lock");

        let config: Config = {
            let mut config = String::new();
            config_file
                .read_to_string(&mut config)
                .map_err(|_| error::UserFriendly::new("config is invalid"))?;
            toml::from_str(&config).map_err(|_| error::UserFriendly::new("config is invalid"))
        }?;

        debug!("Read the following config: {:?}", &config);

        let env_user = Self::get_env_user(&env);

        Ok(Self {
            config,
            env,
            keyring: Arc::new(Mutex::new(Box::new(keyring))),
            config_file,
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
        let config_file = &mut self.config_file;

        config_file.set_len(0)?;
        config_file.seek(std::io::SeekFrom::Start(0))?;
        config_file.write_all(content.as_bytes())
    }

    /// Gets the current user and a boolean indicating whether they can be found in the config file.
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
                            but {ENV_USER_PASSWORD} was not, and couldn't find a matching user in the config file. \n\
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
        })
    }
}

impl Provider for Manager<'_> {
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
}

impl Configurer for Manager<'_> {
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

    use super::*;
    use std::io::ErrorKind;

    #[test]
    pub fn test_read_empty_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir();

        let config = "";
        let config_path = work_dir.join("config.toml");
        std::fs::write(&config_path, config).unwrap();

        // Act
        let mut file_lock = None;
        let config = Manager::read_from_file_with_keyring(
            &config_path,
            &mut file_lock,
            HashMap::default(),
            credentials::MockProvider::new(),
        );

        // Assert
        assert!(config.is_err());
        let e = config.map(|m| m.config).unwrap_err();

        assert_eq!(format!("{e}").as_str(), "config is invalid");
    }

    #[test]
    pub fn test_invalid_read_config_file() {
        // Arrange
        let test_context = get_test_context();
        let work_dir = test_context.get_test_dir();

        let config = b"\xf0\x28\x8c\x28";
        let config_path = work_dir.join("config.toml");
        std::fs::write(&config_path, config).unwrap();

        // Act
        let mut file_lock = None;
        let config = Manager::read_from_file_with_keyring(
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            credentials::MockProvider::new(),
        );

        // Assert
        assert!(config.is_err());
        let e = config.map(|m| m.config).unwrap_err();

        assert_eq!(format!("{e}").as_str(), "config is invalid");
    }

    #[test]
    pub fn test_read_from_file() {
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
        current_user = false

        [[users]]
        address = "test_address.testing.com"
        username = "a_user"
        password = "another_password"
        current_user = true
        "#;
        let config_path = work_dir.join("config.toml");
        std::fs::write(&config_path, config).unwrap();

        // Act
        let mut file_lock = None;
        let config = Manager::read_from_file_with_keyring(
            &config_path,
            &mut file_lock,
            HashMap::default(),
            credentials::MockProvider::new(),
        )
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

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
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
current_user = false

[[users]]
address = "test_address.testing.com"
username = "a_user"
current_user = true
"#
        );
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
            }],
        };

        let config_path = test_context.get_test_dir().join("config.toml");
        let keyring = credentials::MockProvider::new();

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
        )
        .unwrap();

        let test_lock = RwLock::new(
            File::options()
                .read(true)
                .write(true)
                .open(&config_path)
                .unwrap(),
        );
        // Act
        mgr.write_to_file().unwrap();
        let err = test_lock.try_read().unwrap_err();

        // Assert
        assert!(matches!(err.kind(), ErrorKind::WouldBlock));
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

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
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
    pub fn test_add_user_with_plaintext_password() {
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

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
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
    pub fn test_add_user_with_credential_store() {
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

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
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
    pub fn test_add_user_with_credential_store_fails() {
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

        let mut file_lock = None;
        let mut mgr = Manager::with_config_and_keyring(
            config,
            Path::new(&config_path),
            &mut file_lock,
            HashMap::default(),
            keyring,
        )
        .unwrap();
        let new_user = User {
            address: String::from("new_address.testing.com"),
            username: String::from("a_new_user"),
            password: Some(SensitiveString::from("some_new_password")),
            current_user: false,
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
}

use std::path::PathBuf;

use chrono::{serde::ts_seconds_option, DateTime, Utc};
use serde::Serialize;
use toml::{self, Value};

use crate::TestContext;

#[derive(Serialize)]
struct User {
    address: String,
    username: String,
    password: String,
    current_user: bool,
    #[serde(with = "ts_seconds_option", default)]
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct Config {
    users: Vec<User>,
}

pub struct Configurer {
    config_path: PathBuf,
    config: Config,
}

impl Configurer {
    pub(crate) fn new(test_context: &TestContext) -> Self {
        Self {
            config_path: test_context.get_config_dir().join("config.toml"),
            config: Config { users: vec![] },
        }
    }

    #[must_use]
    pub fn add_user(
        mut self,
        address: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        current_user: bool,
    ) -> Self {
        self.config.users.push(User {
            address: address.into(),
            username: username.into(),
            password: password.into(),
            current_user,
            last_used: None,
        });
        self
    }

    /// Writes the config file to disk. If any parent directories do not exist, they are created.
    ///
    /// # Panics
    /// Panics if creating parent directories or writing the config file fails.
    pub fn write(&self) {
        std::fs::create_dir_all(self.config_path.parent().unwrap()).unwrap();
        let contents = toml::to_string(&self.config).unwrap();
        std::fs::write(&self.config_path, contents).unwrap();
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        toml::Value::try_from(&self.config).unwrap()
    }

    /// Asserts that the contents of the config file is equivalent json.
    pub fn verify(&self) {
        let expected = self.to_value();
        let actual =
            std::fs::read_to_string(&self.config_path).expect("Failed to read config file");
        let actual: Value = toml::from_str(&actual).expect("Failed to parse config file");
        assert_eq!(
            expected,
            actual,
            "Config file contents incorrect -
            \texpected: {:?}
            \t  actual: {:?}",
            toml::to_string(&expected).unwrap(),
            toml::to_string(&actual).unwrap()
        );
    }
}

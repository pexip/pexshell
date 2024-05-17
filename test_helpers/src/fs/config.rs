use std::path::PathBuf;

use chrono::{serde::ts_seconds_option, DateTime, Utc};
use p256::pkcs8::*;
use p256::{ecdsa, pkcs8::LineEnding};
use rand::rngs::OsRng;
use serde::Serialize;
use toml::{self, Value};

use crate::TestContext;

#[derive(Serialize)]
struct OAuth2Token {
    access_token: String,
    expiry: DateTime<Utc>,
}

#[derive(Serialize)]
struct User {
    address: String,
    username: Option<String>,
    password: Option<String>,
    client_id: Option<String>,
    private_key: Option<String>,
    token: Option<OAuth2Token>,
    #[allow(clippy::struct_field_names)]
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
    pub fn add_basic_user(
        mut self,
        address: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        current_user: bool,
    ) -> Self {
        self.config.users.push(User {
            address: address.into(),
            username: Some(username.into()),
            password: Some(password.into()),
            current_user,
            last_used: None,
            client_id: None,
            private_key: None,
            token: None,
        });
        self
    }

    #[must_use]
    pub fn add_oauth2_user(
        mut self,
        address: impl Into<String>,
        credentials: &OAuth2Credentials,
        current_user: bool,
    ) -> Self {
        self.config.users.push(User {
            address: address.into(),
            client_id: Some(credentials.client_id.clone()),
            private_key: Some(credentials.get_client_key_pem()),
            token: credentials.access_token.as_ref().map(|t| OAuth2Token {
                access_token: t.client_secret.clone(),
                expiry: t.expiry,
            }),
            current_user,
            last_used: None,
            username: None,
            password: None,
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

struct OAuth2AccessToken {
    client_secret: String,
    expiry: DateTime<Utc>,
}

pub struct OAuth2Credentials {
    client_id: String,
    client_key: ecdsa::SigningKey,
    server_key: ecdsa::VerifyingKey,
    access_token: Option<OAuth2AccessToken>,
}

impl OAuth2Credentials {
    pub fn new(client_id: impl Into<String>) -> Self {
        let client_key = ecdsa::SigningKey::random(&mut OsRng);
        let server_key = ecdsa::VerifyingKey::from(&client_key);
        Self {
            client_id: client_id.into(),
            client_key,
            server_key,
            access_token: None,
        }
    }

    pub fn new_with_access_token(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        expiry: DateTime<Utc>,
    ) -> Self {
        let client_key = ecdsa::SigningKey::random(&mut OsRng);
        let server_key = ecdsa::VerifyingKey::from(&client_key);
        Self {
            client_id: client_id.into(),
            client_key,
            server_key,
            access_token: Some(OAuth2AccessToken {
                client_secret: client_secret.into(),
                expiry,
            }),
        }
    }

    #[must_use]
    pub fn get_client_key_pem(&self) -> String {
        self.client_key
            .to_pkcs8_pem(LineEnding::LF)
            .unwrap()
            .as_str()
            .to_owned()
    }

    #[must_use]
    pub fn get_server_key_pem(&self) -> String {
        self.server_key.to_public_key_pem(LineEnding::LF).unwrap()
    }
}

use chrono::{Offset, TimeZone};
use lib::mcu::auth::{ApiClientAuth, BasicAuth, OAuth2, OAuth2AccessToken};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::io::{Read, Write};

use crate::{
    config::{self, Provider as ConfigProvider},
    consts::EXIT_CODE_INTERRUPTED,
    set_abort_on_interrupt,
};
use dialoguer::{theme::ColorfulTheme as ColourfulTheme, FuzzySelect, Input, Password};
use futures::TryStreamExt;
use lib::{
    error,
    mcu::{Api, ApiClient, ApiClientError, ApiRequest, ApiResponse, IApiClient},
    util::SensitiveString,
};
use log::error;
#[cfg(test)]
use mockall::automock;
use reqwest::StatusCode;

use super::Console;

#[cfg(test)]
fn local_timezone() -> &'static impl TimeZone<Offset = impl Offset + Display> {
    &chrono::Utc
}

#[cfg(not(test))]
fn local_timezone() -> &'static impl TimeZone<Offset = impl Offset + Display> {
    &chrono::Local
}

#[cfg_attr(test, automock)]
pub trait Interact: Send {
    fn text(&mut self, prompt: &str) -> String;
    fn password(&mut self, prompt: &str) -> SensitiveString;
    fn select<T: ToString + 'static>(&mut self, prompt: &str, default: usize, items: &[T])
        -> usize;
    fn read_to_end(&mut self) -> String;
}

pub struct Interactive {}

impl Interact for Interactive {
    fn select<T: ToString>(&mut self, prompt: &str, default: usize, items: &[T]) -> usize {
        set_abort_on_interrupt(false);
        let result = FuzzySelect::with_theme(&ColourfulTheme::default())
            .with_prompt(prompt)
            .default(default)
            .items(items)
            .interact()
            .map_err(|dialoguer::Error::IO(e)| {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    error!("interactive select operation interrupted - exiting");
                    _ = console::Term::stderr().show_cursor();
                    std::process::exit(EXIT_CODE_INTERRUPTED);
                }
                e
            })
            .unwrap();
        set_abort_on_interrupt(true);
        result
    }

    fn text(&mut self, prompt: &str) -> String {
        Input::with_theme(&ColourfulTheme::default())
            .with_prompt(prompt)
            .interact_text()
            .unwrap()
    }

    fn password(&mut self, prompt: &str) -> SensitiveString {
        SensitiveString::from(
            Password::with_theme(&ColourfulTheme::default())
                .with_prompt(prompt)
                .interact()
                .unwrap(),
        )
    }

    fn read_to_end(&mut self) -> String {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        input
    }
}

fn format_last_used(
    user: &config::User,
    tz: &impl TimeZone<Offset = impl Offset + Display>,
) -> String {
    user.last_used.map_or_else(
        || String::from("(Last Used: Never)"),
        |datetime| {
            format!(
                "(Last Used: {})",
                datetime.with_timezone(tz).format("%Y-%m-%d %H:%M:%S")
            )
        },
    )
}

fn combine_username(
    user: &config::User,
    tz: &impl TimeZone<Offset = impl Offset + Display>,
) -> String {
    format!("{} {}", user.visual_id(), format_last_used(user, tz))
}

pub fn auth_for_user<'config>(
    http_client: reqwest::Client,
    user: &'config mut config::User,
    config: &'config mut impl ConfigProvider,
    save_credentials_if_changed: bool,
) -> Result<Box<dyn ApiClientAuth + 'config>, lib::error::UserFriendly> {
    match config.get_credentials_for_user(user)? {
        config::Credentials::Basic(credentials) => Ok(Box::new(BasicAuth::new(
            credentials.username,
            credentials.password.unwrap(),
        ))),
        config::Credentials::OAuth2(credentials) => {
            let mcu_address = ApiClient::base_url_from_input_address(&user.address);
            let endpoint = mcu_address + "/oauth/token/";
            let state = Mutex::new((config, user));
            Ok(Box::new(OAuth2::new(
                http_client,
                endpoint,
                credentials.client_id,
                credentials
                    .private_key
                    .expect("private key is required for OAuth2"),
                credentials.token.map(|t| OAuth2AccessToken {
                    token: t.access_token,
                    expires_at: t.expiry,
                }),
                move |token| {
                    let mut state = state.lock();
                    let (ref mut config, ref mut user) = *state;
                    if let Err(e) =
                        config.set_oauth2_token(user, token, save_credentials_if_changed)
                    {
                        error!("failed to save OAuth2 token: {e}");
                    }
                },
            )))
        }
    }
}

async fn test_request(
    client: reqwest::Client,
    config: &mut impl ConfigProvider,
    user: &mut config::User,
) -> Result<(), lib::error::UserFriendly> {
    let mcu_address = user.address.clone();
    let auth = auth_for_user(client.clone(), user, config, false)?;
    let api_client = ApiClient::new(client, &mcu_address, auth);

    let ApiResponse::ContentStream(mut stream) = api_client
        .send(ApiRequest::GetAll {
            api: Api::Status,
            resource: String::from("worker_vm"),
            filter_args: HashMap::new(),
            page_size: 1,
            limit: 1,
            offset: 0,
        })
        .await
        .map_err(|e| error::UserFriendly::new(e.to_string()))?
    else {
        unreachable!("a get_all request should always return a content stream")
    };
    // try and get the first element of the stream -
    // if there are no errors, then the credentials must be correct
    let _first = stream.try_next().await.map_err(|e| match e {
        ApiClientError::ApiError(e) if e.status() == Some(StatusCode::UNAUTHORIZED) => {
            error::UserFriendly::new("login failed - credentials may be incorrect?")
        }
        // TODO: Diagnose other common errors (e.g. typo in address)
        e => error::UserFriendly::new(e.to_string()),
    })?;

    Ok(())
}

pub struct Login<Backend: Interact> {
    interact: Backend,
}

impl Default for Login<Interactive> {
    fn default() -> Self {
        Self::new(Interactive {})
    }
}

impl<Backend: Interact> Login<Backend> {
    fn new(backend: Backend) -> Self {
        Self { interact: backend }
    }

    pub async fn select_user(
        &mut self,
        console: &mut Console,
        config: &mut (impl config::Configurer + config::Provider),
        client: reqwest::Client,
        verify_credentials: bool,
        store_password_in_plaintext: bool,
    ) -> Result<(), lib::error::UserFriendly> {
        let mut user_list: Vec<String> = config
            .get_users()
            .iter()
            .map(|user| combine_username(user, local_timezone()))
            .collect();

        let user = if user_list.is_empty() {
            writeln!(
                console,
                "no stored api credentials found; add a new user to continue:"
            )
            .unwrap();
            let mut user = self.input_basic_user();

            if verify_credentials {
                test_request(client, config, &mut user).await?;
                user.last_used = Some(chrono::offset::Utc::now());
            }

            config.add_user(user.clone(), store_password_in_plaintext)?;
            user
        } else {
            const ADD_A_USER_OPTION: &str = "add a user";
            user_list.push(ADD_A_USER_OPTION.to_owned());

            let selection = self.interact.select("select a user", 0, &user_list);

            if user_list[selection] == ADD_A_USER_OPTION {
                let mut user = self.input_basic_user();

                if verify_credentials {
                    test_request(client, config, &mut user).await?;
                    user.last_used = Some(chrono::offset::Utc::now());
                }

                config.add_user(user.clone(), store_password_in_plaintext)?;
                user
            } else {
                config.get_users()[selection].clone()
            }
        };

        config.set_current_user(&user);
        Ok(())
    }

    pub async fn add_and_select_oauth2_user(
        &mut self,
        config: &mut (impl config::Configurer + config::Provider),
        client: reqwest::Client,
        address: String,
        client_id: String,
        verify_credentials: bool,
        store_private_key_in_plaintext: bool,
    ) -> Result<(), lib::error::UserFriendly> {
        let mut user = self.input_oauth2_user(address, client_id);

        if verify_credentials {
            test_request(client, config, &mut user).await?;
            user.last_used = Some(chrono::offset::Utc::now());
        }

        config.add_user(user.clone(), store_private_key_in_plaintext)?;
        config.set_current_user(&user);
        Ok(())
    }

    pub fn input_basic_user(&mut self) -> config::User {
        let input_address: String = self.interact.text("address");

        let input_username: String = self.interact.text("username");

        let input_password = self.interact.password("password");

        config::User::new(input_address, input_username, input_password)
    }

    pub fn input_oauth2_user(&mut self, address: String, client_id: String) -> config::User {
        let client_cert = SensitiveString::from(self.interact.read_to_end());
        config::User::new_oauth2(address, client_id, client_cert)
    }

    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut)]
    pub fn list_users(&mut self, console: &mut Console, config: &impl config::Configurer) {
        let mut output = String::new();
        for user in config.get_users() {
            let mut user_ident = combine_username(user, local_timezone());
            if user.current_user {
                if console.is_stdout_interactive() {
                    user_ident = console::Style::new()
                        .fg(console::Color::Green)
                        .apply_to(user_ident)
                        .to_string();
                }
                writeln!(&mut output, "* {user_ident}").unwrap();
            } else {
                writeln!(&mut output, "  {user_ident}").unwrap();
            }
        }
        write!(console.stdout, "{output}").unwrap();
    }

    pub fn delete_user(
        &mut self,
        config: &mut impl config::Configurer,
    ) -> Result<(), error::UserFriendly> {
        let user_list: Vec<String> = config
            .get_users()
            .iter()
            .map(|user| combine_username(user, local_timezone()))
            .collect();
        if user_list.is_empty() {
            Err(error::UserFriendly::new("no stored api credentials found"))
        } else {
            let selection = self.interact.select("select a user", 0, &user_list);

            config.delete_user(selection)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{FixedOffset, TimeZone, Utc};
    use googletest::prelude::*;
    use jsonwebtoken::{DecodingKey, Validation};
    use lib::util::SensitiveString;
    use mockall::{predicate as mp, Sequence};
    use serde_json::{json, Value};
    use test_helpers::{fs::OAuth2Credentials as OAuth2CredentialsHelper, VirtualFile};
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, Request, ResponseTemplate,
    };

    use crate::{
        cli::Console,
        config::{self, BasicCredentials, Credentials, OAuth2Credentials, OAuth2Token, User},
        test_util::sensitive_string,
    };

    use super::{combine_username, Login, MockInteract};

    fn get_test_users() -> Vec<User> {
        let user_1 = User {
            address: String::from("testing.test.1"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username.1"),
                password: Some(SensitiveString::from("password.1")),
            }),
            current_user: false,
            last_used: None,
        };
        let user_2 = User {
            address: String::from("testing.test.2"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username.2"),
                password: Some(SensitiveString::from("password.2")),
            }),
            current_user: true,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        let user_3 = User {
            address: String::from("testing.test.3"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username.3"),
                password: Some(SensitiveString::from("password.3")),
            }),
            current_user: false,
            last_used: None,
        };
        vec![user_1, user_2, user_3]
    }

    #[test]
    fn test_combine_username() {
        let user = User {
            address: String::from("testing.test"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username"),
                password: Some(SensitiveString::from("password")),
            }),
            current_user: false,
            last_used: None,
        };
        assert_that!(
            combine_username(&user, &Utc),
            eq("username@testing.test (Last Used: Never)")
        );
    }

    #[test]
    fn test_combine_username_with_date() {
        let user = User {
            address: String::from("testing.test"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username"),
                password: Some(SensitiveString::from("password")),
            }),
            current_user: false,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        assert_that!(
            combine_username(&user, &Utc),
            eq("username@testing.test (Last Used: 2007-10-19 07:23:04)")
        );
    }

    #[test]
    fn test_combine_username_with_timezone() {
        let user = User {
            address: String::from("testing.test"),
            credentials: Credentials::Basic(BasicCredentials {
                username: String::from("username"),
                password: Some(SensitiveString::from("password")),
            }),
            current_user: false,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        let tz = FixedOffset::west_opt(5 * 60 * 60).unwrap();
        assert_that!(
            combine_username(&user, &tz),
            eq("username@testing.test (Last Used: 2007-10-19 02:23:04)")
        );
    }

    #[test]
    fn test_list_users() {
        // Arrange
        let mut mock_config = config::MockConfigManager::new();
        mock_config
            .expect_get_users()
            .once()
            .return_const(get_test_users());

        let backend = MockInteract::new();
        let out = VirtualFile::new();
        let mut console = Console::new(false, out.clone(), false, VirtualFile::new());

        let mut login = Login::new(backend);

        // Act
        login.list_users(&mut console, &mock_config);

        // Assert
        let stdout = out.take();
        assert_that!(
            stdout,
            eq("  username.1@testing.test.1 (Last Used: Never)\n* username.2@testing.test.2 (Last Used: 2007-10-19 07:23:04)\n  \
                username.3@testing.test.3 (Last Used: Never)\n")
        );
    }

    #[test]
    fn test_input_user() {
        // Arrange
        let backend = MockInteract::new();
        let mut login = Login::new(backend);
        login
            .interact
            .expect_text()
            .with(mp::eq("address"))
            .once()
            .return_const("testing.test");
        login
            .interact
            .expect_text()
            .with(mp::eq("username"))
            .once()
            .return_const("some_username");
        login
            .interact
            .expect_password()
            .with(mp::eq("password"))
            .once()
            .return_const(SensitiveString::from("some_password"));

        // Act
        let user = login.input_basic_user();

        // Assert
        assert_that!(
            user,
            matches_pattern!(User {
                address: eq("testing.test"),
                credentials: pat!(Credentials::Basic(pat!(BasicCredentials {
                    username: eq("some_username"),
                    password: some(sensitive_string(eq("some_password"))),
                }))),
                current_user: eq(&false),
                last_used: none(),
            })
        );
    }

    #[test]
    fn test_select_user() {
        // Arrange
        let backend = MockInteract::new();
        let mut mock_config = config::MockConfigManager::new();
        mock_config
            .expect_get_users()
            .return_const(get_test_users());

        mock_config
            .expect_set_current_user()
            .withf(move |user: &User| {
                user.address == "testing.test.3"
                    && matches!(
                        user.credentials,
                        Credentials::Basic(BasicCredentials {
                            ref username,
                            password: Some(ref password)
                        }) if username == "username.3" && password.secret() == "password.3"
                    )
                    && user.last_used.is_none()
            })
            .once()
            .return_const(());

        let out = VirtualFile::new();
        let mut console = Console::new(false, out, false, VirtualFile::new());
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                mp::eq("select a user"),
                mp::eq(0),
                mp::eq([
                    String::from("username.1@testing.test.1 (Last Used: Never)"),
                    String::from("username.2@testing.test.2 (Last Used: 2007-10-19 07:23:04)"),
                    String::from("username.3@testing.test.3 (Last Used: Never)"),
                    String::from("add a user"),
                ]),
            )
            .once()
            .return_const(2usize);

        // Act
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(login.select_user(
                &mut console,
                &mut mock_config,
                reqwest::Client::new(),
                true,  // selecting an existing user should *not* trigger verification
                false, // this option should *not* affect selecting a user
            ))
            .unwrap();
    }

    #[test]
    fn test_select_user_add_no_verify() {
        // Arrange
        let backend = MockInteract::new();
        let mut mock_config = config::MockConfigManager::new();
        mock_config
            .expect_get_users()
            .return_const(get_test_users());
        mock_config
            .expect_add_user()
            .withf(|user: &User, plaintext| {
                user.address == "testing.new"

                    && matches!(
                        user.credentials,
                        Credentials::Basic(BasicCredentials {
                            ref username,
                            password: Some(ref password)
                        }) if username == "some_new_username" && password.secret() == "some_new_password"
                    )
                    && *plaintext
            })
            .once()
            .returning(|_, _| Ok(()));

        mock_config
            .expect_set_current_user()
            .withf(move |user: &User| {
                user.address == "testing.new"
                    && matches!(
                        user.credentials,
                        Credentials::Basic(BasicCredentials {
                            ref username,
                            password: Some(ref password)
                        }) if username == "some_new_username" && password.secret() == "some_new_password"
                    )
                    && user.last_used.is_none()
            })
            .once()
            .return_const(());

        let out = VirtualFile::new();
        let mut console = Console::new(false, out, false, VirtualFile::new());
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                mp::eq("select a user"),
                mp::eq(0),
                mp::eq([
                    String::from("username.1@testing.test.1 (Last Used: Never)"),
                    String::from("username.2@testing.test.2 (Last Used: 2007-10-19 07:23:04)"),
                    String::from("username.3@testing.test.3 (Last Used: Never)"),
                    String::from("add a user"),
                ]),
            )
            .once()
            .return_const(3usize);

        login
            .interact
            .expect_text()
            .with(mp::eq("address"))
            .once()
            .return_const("testing.new");
        login
            .interact
            .expect_text()
            .with(mp::eq("username"))
            .once()
            .return_const("some_new_username");
        login
            .interact
            .expect_password()
            .with(mp::eq("password"))
            .once()
            .return_const(SensitiveString::from("some_new_password"));

        // Act
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(login.select_user(
                &mut console,
                &mut mock_config,
                reqwest::Client::new(),
                false,
                true,
            ))
            .unwrap();
    }

    #[expect(clippy::too_many_lines)]
    #[tokio::test]
    async fn test_select_user_add_and_verify() {
        // Arrange
        let server = MockServer::start().await;
        let uri = server.uri();
        let backend = MockInteract::new();
        let mut mock_config = config::MockConfigManager::new();
        mock_config
            .expect_get_users()
            .return_const(get_test_users());

        mock_config
            .expect_get_credentials_for_user()
            .once()
            .withf(|user| matches!(user.credentials, Credentials::Basic(_)))
            .returning(|user| Ok(user.credentials.clone()));

        {
            let uri = uri.clone();
            mock_config
                .expect_add_user()
                .withf(move |user: &User, plaintext| {
                    user.address == uri
                        && matches!(
                            user.credentials,
                            Credentials::Basic(BasicCredentials {
                                ref username,
                                password: Some(ref password)
                            }) if username == "some_new_username" && password.secret() == "some_new_password"
                        )
                        && !*plaintext
                })
                .once()
                .returning(|_, _| Ok(()));
        }

        {
            let uri = uri.clone();

            mock_config
                .expect_set_current_user()
                .withf(move |user: &User| {
                    user.address == uri
                    && matches!(
                        user.credentials,
                        Credentials::Basic(BasicCredentials {
                            ref username,
                            password: Some(ref password)
                        }) if username == "some_new_username" && password.secret() == "some_new_password"
                    )
                        && user.last_used.is_some()
                })
                .once()
                .return_const(());
        }

        let out = VirtualFile::new();
        let mut console = Console::new(false, out, false, VirtualFile::new());
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                mp::eq("select a user"),
                mp::eq(0),
                mp::eq([
                    String::from("username.1@testing.test.1 (Last Used: Never)"),
                    String::from("username.2@testing.test.2 (Last Used: 2007-10-19 07:23:04)"),
                    String::from("username.3@testing.test.3 (Last Used: Never)"),
                    String::from("add a user"),
                ]),
            )
            .once()
            .return_const(3usize);

        {
            login
                .interact
                .expect_text()
                .with(mp::eq("address"))
                .once()
                .return_const(uri);
        }
        login
            .interact
            .expect_text()
            .with(mp::eq("username"))
            .once()
            .return_const("some_new_username");
        login
            .interact
            .expect_password()
            .with(mp::eq("password"))
            .once()
            .return_const(SensitiveString::from("some_new_password"));

        Mock::given(method("GET"))
            .and(path("/api/admin/status/v1/worker_vm/"))
            .and(query_param("limit", "1"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {
                "limit": 1,
                "next": null,
                "offset": 0,
                "previous": null,
                "total_count": 0,
            }, "objects": []})))
            .expect(1)
            .mount(&server)
            .await;

        // Act
        login
            .select_user(
                &mut console,
                &mut mock_config,
                reqwest::Client::new(),
                true,
                false,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_oauth2_add_no_verify() {
        // Arrange
        let backend = MockInteract::new();
        let credentials = OAuth2CredentialsHelper::new("some_client_id");
        let mut mock_config = config::MockConfigManager::new();
        let mut login = Login::new(backend);

        mock_config
            .expect_get_credentials_for_user()
            .returning(|user| Ok(user.credentials.clone()));

        {
            let client_key = credentials.get_client_key_pem();
            login
                .interact
                .expect_read_to_end()
                .once()
                .return_const(client_key);
        }

        let mut login_seq = Sequence::new();

        {
            let client_key = credentials.get_client_key_pem();
            mock_config
                .expect_add_user()
                .withf(move |user: &User, plaintext| {
                    user.address == "testing.new"
                    && matches!(
                        user.credentials,
                        Credentials::OAuth2(OAuth2Credentials {
                            ref client_id,
                            private_key: Some(ref private_key),
                            token: None,
                        }) if client_id == "some_client_id" && private_key.secret() == client_key
                    )
                    && !*plaintext
                })
                .once()
                .in_sequence(&mut login_seq)
                .returning(|_, _| Ok(()));
        }

        {
            let client_key = credentials.get_client_key_pem();
            mock_config
                .expect_set_current_user()
                .withf(move |user: &User| {
                    user.address == "testing.new"
                    && matches!(
                        user.credentials,
                        Credentials::OAuth2(OAuth2Credentials {
                            ref client_id,
                            private_key: Some(ref private_key),
                            token: None,
                        }) if client_id == "some_client_id" && private_key.secret() == client_key
                    )
                    && user.last_used.is_none()
                })
                .once()
                .in_sequence(&mut login_seq)
                .return_const(());
        }

        // Act
        login
            .add_and_select_oauth2_user(
                &mut mock_config,
                reqwest::Client::new(),
                "testing.new".to_owned(),
                "some_client_id".to_owned(),
                false,
                false,
            )
            .await
            .unwrap();
    }

    #[expect(clippy::too_many_lines)]
    #[tokio::test]
    async fn test_oauth2_add_and_verify() {
        // Arrange
        let server = MockServer::start().await;
        let backend = MockInteract::new();
        let credentials = OAuth2CredentialsHelper::new("some_client_id");
        let mut mock_config = config::MockConfigManager::new();
        let mut login = Login::new(backend);

        mock_config
            .expect_get_credentials_for_user()
            .returning(|user| Ok(user.credentials.clone()));

        {
            let client_key = credentials.get_client_key_pem();
            login
                .interact
                .expect_read_to_end()
                .once()
                .return_const(client_key);
        }

        {
            let server_key =
                DecodingKey::from_ec_pem(credentials.get_server_key_pem().as_bytes()).unwrap();
            let endpoint = format!("{}/oauth/token/", server.uri());
            Mock::given(method("POST"))
                .and(path("/oauth/token/"))
                .and(move |req: &Request| {
                    // parse request body form data
                    let form_data: HashMap<_, _> = url::form_urlencoded::parse(&req.body).collect();
                    if !(form_data.len() == 3
                        && form_data.get("grant_type").map(AsRef::as_ref)
                            == Some("client_credentials")
                        && form_data.get("client_assertion_type").map(AsRef::as_ref)
                            == Some("urn:ietf:params:oauth:client-assertion-type:jwt-bearer"))
                    {
                        return false;
                    }

                    let Some(client_assertion) = form_data.get("client_assertion") else {
                        return false;
                    };

                    let mut jwt_validation = Validation::new(jsonwebtoken::Algorithm::ES256);
                    jwt_validation.set_audience(&[&endpoint]);
                    jwt_validation.set_issuer(&["some_client_id"]);

                    let Ok(jwt) = jsonwebtoken::decode::<Value>(
                        client_assertion,
                        &server_key,
                        &jwt_validation,
                    ) else {
                        return false;
                    };

                    let Some(claims) = jwt.claims.as_object() else {
                        return false;
                    };

                    claims.len() == 6
                        && claims.get("iss").and_then(Value::as_str) == Some("some_client_id")
                        && claims.get("aud").and_then(Value::as_str) == Some(endpoint.as_str())
                        && claims.get("sub").and_then(Value::as_str) == Some("some_client_id")
                        && claims
                            .get("iat")
                            .and_then(Value::as_i64)
                            .is_some_and(|iat| {
                                let now = Utc::now().timestamp();
                                iat <= now && now <= iat + 60
                            })
                        && jwt
                            .claims
                            .get("exp")
                            .and_then(Value::as_i64)
                            .is_some_and(|exp| {
                                let now = Utc::now().timestamp();
                                exp <= now + 3600 && now + 3600 <= exp + 60
                            })
                        && jwt
                            .claims
                            .get("jti")
                            .and_then(Value::as_str)
                            .is_some_and(|jti| !jti.is_empty())
                })
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "some_access_token",
                    "expires_in": 3600,
                    "token_type": "Bearer",
                })))
                .mount(&server)
                .await;
        }

        {
            let uri = server.uri();
            mock_config
                .expect_set_oauth2_token()
                .withf(move |user, token, save| {
                    user.address == uri
                        && token.token.secret() == "some_access_token"
                        && token.expires_at.timestamp() > Utc::now().timestamp() + 3540
                        && token.expires_at.timestamp() <= Utc::now().timestamp() + 3600
                        && !save
                })
                .once()
                .returning(|user, token, _| {
                    let Credentials::OAuth2(ref mut credentials) = user.credentials else {
                        panic!("Wrong credential type!")
                    };
                    credentials.token = Some(OAuth2Token {
                        access_token: token.token.clone(),
                        expiry: token.expires_at,
                    });
                    Ok(())
                });
        }

        Mock::given(method("GET"))
            .and(path("/api/admin/status/v1/worker_vm/"))
            .and(header("Authorization", "Bearer some_access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "meta": {
                    "limit": 1,
                    "next": null,
                    "offset": 0,
                    "previous": null,
                    "total_count": 0,
                },
                "objects": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut login_seq = Sequence::new();

        {
            let uri = server.uri();
            let client_key = credentials.get_client_key_pem();
            mock_config
                .expect_add_user()
                .withf(move |user: &User, plaintext| {
                    user.address == uri
                    && matches!(
                        user.credentials,
                        Credentials::OAuth2(OAuth2Credentials {
                            ref client_id,
                            private_key: Some(ref private_key),
                            token: Some(ref token),
                        }) if client_id == "some_client_id" && private_key.secret() == client_key && token.access_token.secret() == "some_access_token"
                    )
                    && !*plaintext
                })
                .once()
                .in_sequence(&mut login_seq)
                .returning(|_, _| Ok(()));
        }

        {
            let uri = server.uri();
            let client_key = credentials.get_client_key_pem();
            mock_config
                .expect_set_current_user()
                .withf(move |user: &User| {
                    user.address == uri
                    && matches!(
                        user.credentials,
                        Credentials::OAuth2(OAuth2Credentials {
                            ref client_id,
                            private_key: Some(ref private_key),
                            token: Some(ref token),
                        }) if client_id == "some_client_id" && private_key.secret() == client_key && token.access_token.secret() == "some_access_token"
                    )
                    && user.last_used.is_some()
                })
                .once()
                .in_sequence(&mut login_seq)
                .return_const(());
        }

        // Act
        login
            .add_and_select_oauth2_user(
                &mut mock_config,
                reqwest::Client::new(),
                server.uri(),
                "some_client_id".to_owned(),
                true,
                false,
            )
            .await
            .unwrap();
    }
}

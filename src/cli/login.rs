use chrono::{Offset, TimeZone};
use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::io::Write;

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
            .map_err(|e| {
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
    format!(
        "{}@{} {}",
        user.username,
        user.address,
        format_last_used(user, tz)
    )
}

async fn test_request(
    client: reqwest::Client,
    config: &impl ConfigProvider,
    user: &config::User,
) -> Result<(), lib::error::UserFriendly> {
    let api_client = ApiClient::new(
        client,
        &user.address,
        user.username.clone(),
        config.get_password_for_user(user)?,
    );

    match api_client
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
    {
        ApiResponse::ContentStream(mut stream) => {
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
        _ => panic!(),
    }
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
            let mut user = self.input_user();

            if verify_credentials {
                test_request(client, config, &user).await?;
                user.last_used = Some(chrono::offset::Utc::now());
            }

            config.add_user(user.clone(), store_password_in_plaintext)?;
            user
        } else {
            const ADD_A_USER_OPTION: &str = "add a user";
            user_list.push(ADD_A_USER_OPTION.to_owned());

            let selection = self.interact.select("select a user", 0, &user_list);

            if user_list[selection] == ADD_A_USER_OPTION {
                let mut user = self.input_user();

                if verify_credentials {
                    test_request(client, config, &user).await?;
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

    pub fn input_user(&mut self) -> config::User {
        let input_address: String = self.interact.text("address");

        let input_username: String = self.interact.text("username");

        let input_password = self.interact.password("password");

        config::User::new(input_address, input_username, input_password)
    }

    #[allow(clippy::unused_self)]
    pub fn list_users(&mut self, console: &mut Console, config: &impl config::Configurer) {
        let mut output = String::new();
        for user in config.get_users() {
            let mut user_ident = combine_username(user, local_timezone());
            if user.current_user {
                if console.is_interactive() {
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
    use chrono::{FixedOffset, TimeZone, Utc};
    use httptest::{
        all_of,
        matchers::{contains, request, url_decoded},
        responders::json_encoded,
        Expectation, Server,
    };
    use lib::util::SensitiveString;
    use mockall::predicate::eq;
    use serde_json::json;
    use test_helpers::VirtualFile;

    use crate::{
        cli::Console,
        config::{self, User},
    };

    use super::{combine_username, Login, MockInteract};

    fn get_test_users() -> Vec<User> {
        let user_1 = User {
            address: String::from("testing.test.1"),
            username: String::from("username.1"),
            password: Some(SensitiveString::from("password.1")),
            current_user: false,
            last_used: None,
        };
        let user_2 = User {
            address: String::from("testing.test.2"),
            username: String::from("username.2"),
            password: Some(SensitiveString::from("password.2")),
            current_user: true,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        let user_3 = User {
            address: String::from("testing.test.3"),
            username: String::from("username.3"),
            password: Some(SensitiveString::from("password.3")),
            current_user: false,
            last_used: None,
        };
        vec![user_1, user_2, user_3]
    }

    #[test]
    fn test_combine_username() {
        let user = User {
            address: String::from("testing.test"),
            username: String::from("username"),
            password: Some(SensitiveString::from("password")),
            current_user: false,
            last_used: None,
        };
        assert_eq!(
            combine_username(&user, &Utc).as_str(),
            "username@testing.test (Last Used: Never)"
        );
    }

    #[test]
    fn test_combine_username_with_date() {
        let user = User {
            address: String::from("testing.test"),
            username: String::from("username"),
            password: Some(SensitiveString::from("password")),
            current_user: false,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        assert_eq!(
            combine_username(&user, &Utc).as_str(),
            "username@testing.test (Last Used: 2007-10-19 07:23:04)"
        );
    }

    #[test]
    fn test_combine_username_with_timezone() {
        let user = User {
            address: String::from("testing.test"),
            username: String::from("username"),
            password: Some(SensitiveString::from("password")),
            current_user: false,
            last_used: Some(Utc.with_ymd_and_hms(2007, 10, 19, 7, 23, 4).unwrap()),
        };
        let tz = FixedOffset::west_opt(5 * 60 * 60).unwrap();
        assert_eq!(
            combine_username(&user, &tz).as_str(),
            "username@testing.test (Last Used: 2007-10-19 02:23:04)"
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
        let mut console = Console::new(false, out.clone());

        let mut login = Login::new(backend);

        // Act
        login.list_users(&mut console, &mock_config);

        // Assert
        let stdout = out.take();
        assert_eq!(
            stdout,
            "  username.1@testing.test.1 (Last Used: Never)\n* username.2@testing.test.2 (Last Used: 2007-10-19 07:23:04)\n  \
             username.3@testing.test.3 (Last Used: Never)\n"
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
            .with(eq("address"))
            .once()
            .return_const("testing.test");
        login
            .interact
            .expect_text()
            .with(eq("username"))
            .once()
            .return_const("some_username");
        login
            .interact
            .expect_password()
            .with(eq("password"))
            .once()
            .return_const(SensitiveString::from("some_password"));

        // Act
        let user = login.input_user();

        // Assert
        assert_eq!(user.address, "testing.test");
        assert_eq!(user.username, "some_username");
        assert_eq!(
            user.password.map(|s| s.secret().to_owned()),
            Some(String::from("some_password"))
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
                    && user.username == "username.3"
                    && user
                        .password
                        .as_ref()
                        .map_or(false, |s| s.secret() == "password.3")
                    && user.last_used.is_none()
            })
            .once()
            .return_const(());

        let out = VirtualFile::new();
        let mut console = Console::new(false, out);
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                eq("select a user"),
                eq(0),
                eq([
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
                    && user.username == "some_new_username"
                    && user
                        .password
                        .as_ref()
                        .map_or(false, |s| s.secret() == "some_new_password")
                    && *plaintext
            })
            .once()
            .returning(|_, _| Ok(()));

        mock_config
            .expect_set_current_user()
            .withf(move |user: &User| {
                user.address == "testing.new"
                    && user.username == "some_new_username"
                    && user
                        .password
                        .as_ref()
                        .map_or(false, |s| s.secret() == "some_new_password")
                    && user.last_used.is_none()
            })
            .once()
            .return_const(());

        let out = VirtualFile::new();
        let mut console = Console::new(false, out);
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                eq("select a user"),
                eq(0),
                eq([
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
            .with(eq("address"))
            .once()
            .return_const("testing.new");
        login
            .interact
            .expect_text()
            .with(eq("username"))
            .once()
            .return_const("some_new_username");
        login
            .interact
            .expect_password()
            .with(eq("password"))
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

    #[allow(clippy::too_many_lines)]
    #[test]
    pub fn test_select_user_add_and_verify() {
        // Arrange
        let server = Server::run();
        let uri = server.url_str("").trim_end_matches('/').to_owned();
        let backend = MockInteract::new();
        let mut mock_config = config::MockConfigManager::new();
        mock_config
            .expect_get_users()
            .return_const(get_test_users());

        mock_config
            .expect_get_password_for_user()
            .once()
            .returning(|user| Ok(user.password.clone().unwrap()));

        {
            let uri = uri.clone();
            mock_config
                .expect_add_user()
                .withf(move |user: &User, plaintext| {
                    user.address == uri
                        && user.username == "some_new_username"
                        && user
                            .password
                            .as_ref()
                            .map_or(false, |s| s.secret() == "some_new_password")
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
                        && user.username == "some_new_username"
                        && user
                            .password
                            .as_ref()
                            .map_or(false, |s| s.secret() == "some_new_password")
                        && user.last_used.is_some()
                })
                .once()
                .return_const(());
        }

        let out = VirtualFile::new();
        let mut console = Console::new(false, out);
        let mut login = Login::new(backend);
        login
            .interact
            .expect_select::<String>()
            .with(
                eq("select a user"),
                eq(0),
                eq([
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
                .with(eq("address"))
                .once()
                .return_const(uri);
        }
        login
            .interact
            .expect_text()
            .with(eq("username"))
            .once()
            .return_const("some_new_username");
        login
            .interact
            .expect_password()
            .with(eq("password"))
            .once()
            .return_const(SensitiveString::from("some_new_password"));

        server.expect(
            Expectation::matching(all_of![
                request::method_path("GET", "/api/admin/status/v1/worker_vm/"),
                request::query(url_decoded(all_of![
                    contains(("limit", "1")),
                    contains(("offset", "0"))
                ])),
            ])
            .respond_with(json_encoded(json!({"meta": {
                "limit": 1,
                "next": null,
                "offset": 0,
                "previous": null,
                "total_count": 0,
            }, "objects": []}))),
        );

        // Act
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(login.select_user(
                &mut console,
                &mut mock_config,
                reqwest::Client::new(),
                true,
                false,
            ))
            .unwrap();
    }
}

#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use log::info;
use serde_json::json;
use test_helpers::{fs::OAuth2Credentials, get_test_context, logging::expect};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn get_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;
    let logger = test_context.logger();
    logger.expect(expect::exact(log::Level::Info, module_path!(), "testerooo"));
    info!("testerooo");

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/5/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5,
            "name": "some_test_conference",
        })))
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get", "5"].map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    logger.verify();
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        output,
        json!({
            "id": 5,
            "name": "some_test_conference",
        })
    );
}

#[tokio::test]
async fn get_conference_config_oauth2() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;
    let logger = test_context.logger();
    logger.expect(expect::exact(log::Level::Info, module_path!(), "testerooo"));
    info!("testerooo");

    let oauth2_credentials = OAuth2Credentials::new("test_client_id");

    test_context
        .get_config_builder()
        .add_oauth2_user(server.uri(), &oauth2_credentials, true)
        .write();

    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("POST"))
        .and(path("/oauth/token/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "some_access_token",
            "expires_in": 3600,
            "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/5/"))
        .and(header("Authorization", "Bearer some_access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5,
            "name": "some_test_conference",
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get", "5"].map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    logger.verify();
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        output,
        json!({
            "id": 5,
            "name": "some_test_conference",
        })
    );
}

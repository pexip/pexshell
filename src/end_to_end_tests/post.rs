#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use googletest::prelude::*;
use serde_json::json;
use test_helpers::get_test_context;
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_command_conference_lock_only,
        configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn post_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("POST"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(wiremock::matchers::body_json(
            json!({"name": "post_test_conf"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("Location", "/api/admin/configuration/v1/conference/54/"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &[
            "pexshell",
            "configuration",
            "conference",
            "post",
            "--name",
            "post_test_conf",
        ]
        .map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_that!(output, eq("/api/admin/configuration/v1/conference/54/\n"));
}

#[tokio::test]
async fn post_conference_lock_command() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_command_conference_lock_only(&test_context);

    Mock::given(method("POST"))
        .and(path("/api/admin/command/v1/conference/lock/"))
        .and(body_json(
            json!({"conference_id": "22ec87ef-92e8-4100-a8be-d12da654f6c3"}),
        ))
        .respond_with(
            ResponseTemplate::new(202).set_body_json(json!({"data": null, "status": "success"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &[
            "pexshell",
            "command",
            "conference",
            "lock",
            "--conference_id",
            "22ec87ef-92e8-4100-a8be-d12da654f6c3",
        ]
        .map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_that!(output, eq(""));
}

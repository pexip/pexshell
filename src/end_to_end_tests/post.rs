#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use googletest::prelude::*;
use httptest::{
    all_of,
    matchers::{eq as heq, json_decoded, request},
    responders::status_code,
    Expectation, Server,
};
use serde_json::json;
use test_helpers::get_test_context;

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
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("POST", "/api/admin/configuration/v1/conference/",),
            request::body(json_decoded(heq(json!({"name": "post_test_conf"})))),
        ])
        .respond_with(
            status_code(200)
                .append_header("Location", "/api/admin/configuration/v1/conference/54/"),
        ),
    );

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
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_command_conference_lock_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("POST", "/api/admin/command/v1/conference/lock/",),
            request::body(json_decoded(heq(
                json!({"conference_id": "22ec87ef-92e8-4100-a8be-d12da654f6c3"})
            ))),
        ])
        .respond_with(status_code(202).body(r#"{"data": null, "status": "success"}"#)),
    );

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

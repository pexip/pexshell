#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use googletest::prelude::*;
use serde_json::json;
use test_helpers::get_test_context;
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer,
};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn patch_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("PATCH"))
        .and(path("/api/admin/configuration/v1/conference/89/"))
        .and(body_json(json!({"name": "patch_test_conf"})))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &[
            "pexshell",
            "configuration",
            "conference",
            "patch",
            "89",
            "--name",
            "patch_test_conf",
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

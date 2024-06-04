#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use googletest::prelude::*;
use test_helpers::get_test_context;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn delete_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("DELETE"))
        .and(path("/api/admin/configuration/v1/conference/52/"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "delete", "52"].map(String::from),
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

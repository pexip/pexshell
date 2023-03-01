use std::collections::HashMap;

use httptest::{matchers::request, responders::json_encoded, Expectation, Server};
use log::info;
use serde_json::json;
use test_helpers::{get_test_context, logging::expect};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[test]
fn get_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();
    let logger = test_context.get_logger();
    logger.expect(expect::exact(log::Level::Info, module_path!(), "testerooo"));
    info!("testerooo");

    configure_config_test_user(&test_context, &server);
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/configuration/v1/conference/5/",
        ))
        .respond_with(json_encoded(json!({
            "id": 5,
            "name": "some_test_conference",
        }))),
    );

    // Act
    test_context
        .block_on(crate::run_with(
            &["pexshell", "configuration", "conference", "get", "5"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
        ))
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

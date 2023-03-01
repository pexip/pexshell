use std::collections::HashMap;

use httptest::{matchers::request, responders::status_code, Expectation, Server};
use test_helpers::get_test_context;

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[test]
fn delete_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(request::method_path(
            "DELETE",
            "/api/admin/configuration/v1/conference/52/",
        ))
        .respond_with(status_code(200)),
    );

    // Act
    test_context
        .block_on(crate::run_with(
            &["pexshell", "configuration", "conference", "delete", "52"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_eq!(&output, "");
}

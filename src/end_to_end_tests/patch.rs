use std::collections::HashMap;

use httptest::{
    all_of,
    matchers::{eq, json_decoded, request},
    responders::status_code,
    Expectation, Server,
};
use serde_json::json;
use test_helpers::get_test_context;

use crate::end_to_end_tests::configuration_helpers::{
    configure_config_test_user, configure_schemas_configuration_conference_only,
};

#[test]
fn patch_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("PATCH", "/api/admin/configuration/v1/conference/89/",),
            request::body(json_decoded(eq(json!({"name": "patch_test_conf"})))),
        ])
        .respond_with(status_code(200)),
    );

    // Act
    test_context
        .block_on(crate::run_with(
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
            test_context.get_config_dir(),
            test_context.get_cache_dir(),
            test_context.get_stdout_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_eq!(&output, "");
}

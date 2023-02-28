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
    configure_config_test_user, configure_schemas_command_conference_lock_only,
    configure_schemas_configuration_conference_only,
};

#[test]
fn post_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("POST", "/api/admin/configuration/v1/conference/",),
            request::body(json_decoded(eq(json!({"name": "post_test_conf"})))),
        ])
        .respond_with(
            status_code(200)
                .append_header("Location", "/api/admin/configuration/v1/conference/54/"),
        ),
    );

    // Act
    test_context
        .block_on(crate::run_with(
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
            test_context.get_config_dir(),
            test_context.get_cache_dir(),
            test_context.get_stdout_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_eq!(&output, "/api/admin/configuration/v1/conference/54/\n");
}

#[test]
fn post_conference_lock_command() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);
    configure_schemas_command_conference_lock_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("POST", "/api/admin/command/v1/conference/lock/",),
            request::body(json_decoded(eq(
                json!({"conference_id": "22ec87ef-92e8-4100-a8be-d12da654f6c3"})
            ))),
        ])
        .respond_with(status_code(202).body(r#"{"data": null, "status": "success"}"#)),
    );

    // Act
    test_context
        .block_on(crate::run_with(
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
            test_context.get_config_dir(),
            test_context.get_cache_dir(),
            test_context.get_stdout_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_eq!(&output, "");
}

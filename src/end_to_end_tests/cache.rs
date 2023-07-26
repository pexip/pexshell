#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use httptest::{
    matchers::request,
    responders::{json_encoded, status_code},
    Expectation, Server,
};
use serde_json::Value;
use test_helpers::get_test_context;

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[test]
fn cache_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);

    let configuration_root_schema = test_context
        .get_root_schema_builder("/api/admin/configuration/v1/")
        .entry("conference");
    let status_root_schema = test_context.get_root_schema_builder("/api/admin/status/v1/");
    let history_root_schema = test_context.get_root_schema_builder("/api/admin/history/v1/");
    let command_conference_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/conference/");
    let command_participant_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/participant/");
    let command_platform_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/platform/");

    let configuration_conference_schema = test_context
        .get_schema_builder()
        .field("id", |f| {
            f.blank(true)
                .nullable(false)
                .unique(true)
                .default(Value::String(String::new()))
        })
        .field("name", |f| f.unique(true).nullable(false));

    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/configuration/v1/"))
            .respond_with(json_encoded(configuration_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/status/v1/"))
            .respond_with(json_encoded(status_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/history/v1/"))
            .respond_with(json_encoded(history_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/conference/",
        ))
        .respond_with(json_encoded(command_conference_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/participant/",
        ))
        .respond_with(json_encoded(command_participant_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/platform/",
        ))
        .respond_with(json_encoded(command_platform_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/configuration/v1/conference/schema/",
        ))
        .respond_with(json_encoded(configuration_conference_schema.to_value())),
    );

    // Act
    test_context
        .block_on(crate::run_with(
            &["pexshell", "cache"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
            test_context.get_stderr_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert_eq!(&output, "");
}

#[test]
fn clear_cache() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    let config = configure_config_test_user(&test_context, &server);
    configure_schemas_configuration_conference_only(&test_context);
    assert!(test_context.get_cache_dir().join("schemas").exists());
    assert_eq!(
        test_context
            .get_cache_dir()
            .join("schemas")
            .read_dir()
            .unwrap()
            .count(),
        4
    );

    // Act
    test_context
        .block_on(crate::run_with(
            &["pexshell", "cache", "--clear"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
            test_context.get_stderr_wrapper(),
        ))
        .unwrap();

    // Assert
    config.verify();
    assert_eq!(
        test_context
            .get_cache_dir()
            .join("schemas")
            .read_dir()
            .unwrap()
            .count(),
        0
    );
    let output = test_context.take_stdout();
    assert_eq!(&output, "");
}

#[test]
fn schema_field_with_dict_type_does_not_cause_crash() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, &server);

    let schema_data = r#"{"allowed_detail_http_methods":["get","delete"],"allowed_list_http_methods":["get","post"],"default_format":"application/json","default_limit":20,"fields":{"activatable":{"blank":false,"help_text":"The available number of activatable licenses.","nullable":false,"readonly":false,"type":"integer","unique":false},"activatable_overdraft":{"blank":false,"help_text":"The available activatable license overdraft.","nullable":false,"readonly":false,"type":"integer","unique":false},"concurrent":{"blank":false,"default":0,"help_text":"The available number of concurrent licenses.","nullable":false,"readonly":false,"type":"integer","unique":false},"concurrent_overdraft":{"blank":false,"default":0,"help_text":"The available concurrent license overdraft.","nullable":false,"readonly":false,"type":"integer","unique":false},"entitlement_id":{"blank":false,"help_text":"The license entitlement key used to activate this license.","nullable":false,"readonly":false,"type":"string","unique":false},"expiration_date":{"blank":true,"default":"","help_text":"The date and time at which this license expires.","nullable":false,"readonly":false,"type":"string","unique":false},"features":{"blank":false,"help_text":"The features this license provides.","nullable":false,"readonly":false,"type":"string","unique":false},"fulfillment_id":{"blank":true,"default":"","help_text":"The identifier for this license.","nullable":false,"readonly":false,"type":"string","unique":true},"fulfillment_type":{"blank":false,"help_text":"The type of this license.","nullable":false,"readonly":false,"type":"string","unique":false},"hybrid":{"blank":false,"help_text":"The available number of hybrid licenses.","nullable":false,"readonly":false,"type":"integer","unique":false},"hybrid_overdraft":{"blank":false,"help_text":"The available hybrid license overdraft.","nullable":false,"readonly":false,"type":"integer","unique":false},"license_type":{"blank":true,"default":"","help_text":"The type of feature this license provides.","nullable":false,"readonly":false,"type":"string","unique":false},"offline_mode":{"blank":false,"default":false,"help_text":"Save this as a Stored license request for manual activation at a later date.","nullable":false,"readonly":false,"type":"boolean","unique":false},"product_id":{"blank":false,"help_text":"The type of this license.","nullable":false,"readonly":false,"type":"string","unique":false},"repair":{"blank":false,"help_text":"The number of times this license has been repaired.","nullable":false,"readonly":false,"type":"integer","unique":false},"resource_uri":{"blank":false,"help_text":"The URI that identifies this resource.","nullable":false,"readonly":true,"type":"string","unique":false},"server_chain":{"blank":true,"default":"","help_text":"The license server chain for this license.","nullable":false,"readonly":false,"type":"string","unique":false},"start_date":{"blank":true,"default":"","help_text":"The date and time at which this license becomes valid.","nullable":false,"readonly":false,"type":"string","unique":false},"status":{"blank":true,"default":"","help_text":"The status of this object.","nullable":false,"readonly":false,"type":"string","unique":false},"trust_flags":{"blank":false,"help_text":"The trust status of this license.","nullable":false,"readonly":false,"type":"integer","unique":false},"vendor_dictionary":{"blank":false,"help_text":"The vendor-specific information associated with this license.","nullable":false,"readonly":false,"type":"dict","unique":false}}}"#;
    let configuration_root_schema = test_context
        .get_root_schema_builder("/api/admin/configuration/v1/")
        .entry("license");
    let status_root_schema = test_context.get_root_schema_builder("/api/admin/status/v1/");
    let history_root_schema = test_context.get_root_schema_builder("/api/admin/history/v1/");
    let command_conference_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/conference/");
    let command_participant_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/participant/");
    let command_platform_root_schema =
        test_context.get_root_schema_builder("/api/admin/command/v1/platform/");

    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/configuration/v1/"))
            .respond_with(json_encoded(configuration_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/status/v1/"))
            .respond_with(json_encoded(status_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path("GET", "/api/admin/history/v1/"))
            .respond_with(json_encoded(history_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/conference/",
        ))
        .respond_with(json_encoded(command_conference_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/participant/",
        ))
        .respond_with(json_encoded(command_participant_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/command/v1/platform/",
        ))
        .respond_with(json_encoded(command_platform_root_schema.to_value())),
    );
    server.expect(
        Expectation::matching(request::method_path(
            "GET",
            "/api/admin/configuration/v1/license/schema/",
        ))
        .respond_with(status_code(200).body(schema_data)),
    );

    // Act 1
    test_context
        .block_on(crate::run_with(
            &["pexshell", "cache"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
            test_context.get_stderr_wrapper(),
        ))
        .unwrap();

    // Assert 1
    let output = test_context.take_stdout();
    assert_eq!(&output, "");

    // Act 2
    test_context
        .block_on(crate::run_with(
            &["pexshell", "--help"].map(String::from),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
            test_context.get_stderr_wrapper(),
        ))
        .unwrap();

    // Assert
    let output = test_context.take_stdout();
    assert!(!output.is_empty());
}

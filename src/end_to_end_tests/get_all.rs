#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use httptest::{
    all_of,
    matchers::{contains, request, url_decoded},
    responders::json_encoded,
    Expectation, Server,
};
use serde_json::json;
use test_helpers::get_test_context;

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn get_returns_zero_objects() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/",),
            request::query(url_decoded(all_of![
                contains(("limit", "500")),
                contains(("offset", "0"))
            ]))
        ])
        .respond_with(json_encoded(json!({"meta": {
            "limit": 500,
            "next": null,
            "offset": 0,
            "previous": null,
            "total_count": 0,
        }, "objects": []}))),
    );

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get"].map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(output, json!([]));
}

#[tokio::test]
async fn get_returns_page() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/",),
            request::query(url_decoded(all_of![
                contains(("limit", "500")),
                contains(("offset", "0"))
            ]))
        ])
        .respond_with(json_encoded(json!({"meta": {
            "limit": 500,
            "next": null,
            "offset": 0,
            "previous": null,
            "total_count": 1,
        }, "objects": [
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
        ]}))),
    );

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get"].map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
    .unwrap();

    // Assert
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        output,
        json!([
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
        ])
    );
}

#[tokio::test]
async fn get_multiple_pages() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/",),
            request::query(url_decoded(all_of![
                contains(("limit", "2")),
                contains(("offset", "0"))
            ]))
        ])
        .respond_with(json_encoded(json!({
            "meta": {
                "limit": 2,
                "next": "/api/admin/configuration/v1/conference/?limit=2&offset=2",
                "offset": 2,
                "previous": null,
                "total_count": 3,
            },
            "objects": [
                {
                    "id": 1,
                    "name": "test_1",
                },
                {
                    "id": 2,
                    "name": "test_2",
                },
            ]
        }))),
    );

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/",),
            request::query(url_decoded(all_of![
                contains(("limit", "2")),
                contains(("offset", "2"))
            ]))
        ])
        .respond_with(json_encoded(json!({"meta": {
            "limit": 2,
            "next": null,
            "offset": 2,
            "previous": "/api/admin/configuration/v1/conference/?limit=2&offset=0",
            "total_count": 3,
        }, "objects": [
            {
                "id": 3,
                "name": "test_3",
            },
        ]}))),
    );

    // Act
    crate::run_with(
        &[
            "pexshell",
            "configuration",
            "conference",
            "get",
            "--page_size",
            "2",
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
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        output,
        json!([
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
            {
                "id": 3,
                "name": "test_3",
            },
        ])
    );
}

#[tokio::test]
async fn get_limited_to_first_page() {
    // Arrange
    let test_context = get_test_context();
    let server = Server::run();

    configure_config_test_user(&test_context, server.url_str("").trim_end_matches('/'));
    configure_schemas_configuration_conference_only(&test_context);

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/",),
            request::query(url_decoded(all_of![
                contains(("limit", "2")),
                contains(("offset", "0"))
            ]))
        ])
        .respond_with(json_encoded(json!({"meta": {
            "limit": 2,
            "next": "/api/admin/configuration/v1/conference/?limit=2&offset=2",
            "offset": 2,
            "previous": null,
            "total_count": 3,
        }, "objects": [
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            }
        ]}))),
    );

    // Act
    crate::run_with(
        &[
            "pexshell",
            "configuration",
            "conference",
            "get",
            "--page_size",
            "2",
            "--limit",
            "2",
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
    let raw = test_context.take_stdout();
    let output: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        output,
        json!([
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
        ])
    );
}

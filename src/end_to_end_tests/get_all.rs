#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use googletest::prelude::*;
use serde_json::json;
use test_helpers::get_test_context;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

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
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {
            "limit": 500,
            "next": null,
            "offset": 0,
            "previous": null,
            "total_count": 0,
        }, "objects": []})))
        .expect(1)
        .mount(&server)
        .await;

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
    assert_that!(output, eq(json!([])));
}

#[tokio::test]
async fn get_returns_page() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {
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
        ]})))
        .expect(1)
        .mount(&server)
        .await;

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
    assert_that!(
        output,
        eq(json!([
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
        ]))
    );
}

#[tokio::test]
async fn get_multiple_pages() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(query_param("limit", "2"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
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
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(query_param("limit", "2"))
        .and(query_param("offset", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "meta": {
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
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

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
    assert_that!(
        output,
        eq(json!([
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
        ]))
    );
}

#[tokio::test]
async fn get_limited_to_first_page() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/"))
        .and(query_param("limit", "2"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {
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
        ]})))
        .expect(1)
        .mount(&server)
        .await;

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
    assert_that!(
        output,
        eq(json!([
            {
                "id": 1,
                "name": "test_1",
            },
            {
                "id": 2,
                "name": "test_2",
            },
        ]))
    );
}

#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use chrono::Utc;
use jsonwebtoken::{DecodingKey, Validation};
use log::info;
use serde_json::{json, Value};
use test_helpers::{fs::OAuth2Credentials, get_test_context, logging::expect};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, Request, ResponseTemplate,
};

use crate::{
    end_to_end_tests::configuration_helpers::{
        configure_config_test_user, configure_schemas_configuration_conference_only,
    },
    test_util::TestContextExtensions,
};

#[tokio::test]
async fn get_conference_config() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;
    let logger = test_context.logger();
    logger.expect(expect::exact(log::Level::Info, module_path!(), "testerooo"));
    info!("testerooo");

    configure_config_test_user(&test_context, server.uri());
    configure_schemas_configuration_conference_only(&test_context);

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/5/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5,
            "name": "some_test_conference",
        })))
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get", "5"].map(String::from),
        HashMap::default(),
        &test_context.get_directories(),
        test_context.get_stdout_wrapper(),
        test_context.get_stderr_wrapper(),
    )
    .await
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

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn get_conference_config_oauth2() {
    // Arrange
    let test_context = get_test_context();
    let server = MockServer::start().await;

    let oauth2_credentials = OAuth2Credentials::new("test_client_id");

    test_context
        .get_config_builder()
        .add_oauth2_user(server.uri(), &oauth2_credentials, true)
        .write();

    configure_schemas_configuration_conference_only(&test_context);

    {
        let endpoint = server.uri() + "/oauth/token/";
        let server_key =
            DecodingKey::from_ec_pem(oauth2_credentials.get_server_key_pem().as_bytes()).unwrap();
        Mock::given(method("POST"))
            .and(path("/oauth/token/"))
            .and(move |req: &Request| {
                // parse request body form data
                let form_data: HashMap<_, _> = url::form_urlencoded::parse(&req.body).collect();
                if !(form_data.len() == 3
                    && form_data.get("grant_type").map(AsRef::as_ref) == Some("client_credentials")
                    && form_data.get("client_assertion_type").map(AsRef::as_ref)
                        == Some("urn:ietf:params:oauth:client-assertion-type:jwt-bearer"))
                {
                    return false;
                }

                let Some(client_assertion) = form_data.get("client_assertion") else {
                    return false;
                };

                let mut jwt_validation = Validation::new(jsonwebtoken::Algorithm::ES256);
                jwt_validation.set_audience(&[&endpoint]);
                jwt_validation.set_issuer(&["test_client_id"]);

                let Ok(jwt) =
                    jsonwebtoken::decode::<Value>(client_assertion, &server_key, &jwt_validation)
                else {
                    return false;
                };

                let Some(claims) = jwt.claims.as_object() else {
                    return false;
                };

                claims.len() == 6
                    && claims.get("iss").and_then(Value::as_str) == Some("test_client_id")
                    && claims.get("aud").and_then(Value::as_str) == Some(endpoint.as_str())
                    && claims.get("sub").and_then(Value::as_str) == Some("test_client_id")
                    && claims
                        .get("iat")
                        .and_then(Value::as_i64)
                        .map_or(false, |iat| {
                            let now = Utc::now().timestamp();
                            now - 60 <= iat && iat <= now
                        })
                    && jwt
                        .claims
                        .get("exp")
                        .and_then(Value::as_i64)
                        .map_or(false, |exp| {
                            let now = Utc::now().timestamp();
                            now + 3540 <= exp && exp <= now + 3600
                        })
                    && jwt
                        .claims
                        .get("jti")
                        .and_then(Value::as_str)
                        .map_or(false, |jti| !jti.is_empty())
            })
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "some_access_token",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .expect(1)
            .mount(&server)
            .await;
    }

    Mock::given(method("GET"))
        .and(path("/api/admin/configuration/v1/conference/5/"))
        .and(header("Authorization", "Bearer some_access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5,
            "name": "some_test_conference",
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Act
    crate::run_with(
        &["pexshell", "configuration", "conference", "get", "5"].map(String::from),
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
        json!({
            "id": 5,
            "name": "some_test_conference",
        })
    );
}

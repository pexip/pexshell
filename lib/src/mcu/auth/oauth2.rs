#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;

use anyhow::Context;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jsonwebtoken::Header;
use log::debug;
use rand::{Rng, TryRngCore};
use serde::ser::SerializeStruct;
use tokio::sync::Mutex;

use crate::{mcu::error, util::SensitiveString};

use super::ApiClientAuth;

#[derive(Clone, Debug)]
pub struct AuthToken {
    pub token: SensitiveString,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Deserialize)]
enum TokenType {
    Bearer,
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: SensitiveString,
    expires_in: i64,
    #[expect(dead_code)]
    token_type: TokenType,
}

struct Claims<'a> {
    client_id: &'a str,
    endpoint: &'a str,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    token_id: &'a str,
}

impl serde::Serialize for Claims<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut claims = serializer.serialize_struct("Claims", 6)?;
        claims.serialize_field("iss", &self.client_id)?;
        claims.serialize_field("aud", &self.endpoint)?;
        claims.serialize_field("sub", &self.client_id)?;
        claims.serialize_field("iat", &self.issued_at.timestamp())?;
        claims.serialize_field("exp", &self.expires_at.timestamp())?;
        claims.serialize_field("jti", &self.token_id)?;
        claims.end()
    }
}

pub struct OAuth2<'callback> {
    http_client: reqwest::Client,
    endpoint: String,
    client_id: String,
    /// Private key (ES256)
    client_key: SensitiveString,
    token: Mutex<Option<AuthToken>>,
    token_callback: Box<dyn Fn(&AuthToken) + Send + Sync + 'callback>,
}

impl<'callback> OAuth2<'callback> {
    #[must_use]
    pub fn new(
        http_client: reqwest::Client,
        endpoint: String,
        client_id: String,
        client_key: SensitiveString,
        current_token: Option<AuthToken>,
        token_callback: impl Fn(&AuthToken) + Send + Sync + 'callback,
    ) -> Self {
        Self {
            http_client,
            endpoint,
            client_id,
            client_key,
            token: Mutex::new(current_token),
            token_callback: Box::new(token_callback),
        }
    }

    fn generate_token_id() -> String {
        let mut rng = rand::rngs::OsRng.unwrap_err(); // Panic on OS-level RNG failure
        let bytes: [u8; 18] = rng.random();
        hex::encode(bytes)
    }

    async fn get_token(
        http_client: &reqwest::Client,
        endpoint: &str,
        client_id: &str,
        client_key: &jsonwebtoken::EncodingKey,
    ) -> reqwest::Result<AuthToken> {
        let issued_at = Utc::now();
        let expires_at = issued_at + chrono::Duration::hours(1);
        let token_id = Self::generate_token_id();
        debug!("Generated token ID: {token_id}");

        let claims = jsonwebtoken::encode(
            &Header::new(jsonwebtoken::Algorithm::ES256),
            &Claims {
                client_id,
                endpoint,
                issued_at,
                expires_at,
                token_id: &token_id,
            },
            client_key,
        )
        .unwrap();

        let mut form_data = HashMap::new();
        form_data.insert("grant_type", "client_credentials");
        form_data.insert(
            "client_assertion_type",
            "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        );
        form_data.insert("client_assertion", claims.as_str());

        let request = http_client.post(endpoint).form(&form_data).build()?;
        let response = http_client.execute(request).await?.error_for_status()?;

        let response_body: TokenResponse = response.json().await?;

        Ok(AuthToken {
            token: response_body.access_token,
            expires_at: issued_at + chrono::Duration::seconds(response_body.expires_in),
        })
    }
}

#[async_trait]
impl ApiClientAuth for OAuth2<'_> {
    async fn add_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, anyhow::Error> {
        debug!("Configuring request with OAuth2 authentication");

        let mut token = self.token.lock().await;
        if let Some(token) = &*token {
            if token.expires_at > Utc::now() + chrono::Duration::minutes(5) {
                debug!(
                    "Using existing OAuth2 token (expires at: {})",
                    token.expires_at
                );
                return Ok(request.bearer_auth(token.token.secret()));
            }

            if token.expires_at < Utc::now() {
                debug!(
                    "Existing OAuth2 token is expired (expires at: {})",
                    token.expires_at
                );
            } else {
                debug!(
                    "Existing OAuth2 token expires soon (expires at: {})",
                    token.expires_at
                );
            }
        }

        debug!("Fetching new OAuth2 token");

        let client_key =
            jsonwebtoken::EncodingKey::from_ec_pem(self.client_key.secret().as_bytes())
                .context("invalid EC PEM key")?;

        let new_token = Self::get_token(
            &self.http_client,
            &self.endpoint,
            &self.client_id,
            &client_key,
        )
        .await
        .map_err(error::ReqwestDebugPrintWrapper)
        .context("failed to get OAuth2 token")?;

        debug!(
            "Fetched new OAuth2 token (expires at: {})",
            new_token.expires_at
        );

        *token = Some(new_token.clone());

        (self.token_callback)(&new_token);

        Ok(request.bearer_auth(new_token.token.secret()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use googletest::prelude::*;
    use serde_json::json;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::mcu::auth::AuthWith;
    use crate::test_util::sensitive_string;

    use super::*;

    #[test]
    fn test_generate_token_id() {
        let token_id = OAuth2::generate_token_id();
        assert_that!(token_id.len(), eq(36));
    }

    #[expect(clippy::too_many_lines)]
    #[tokio::test]
    async fn auth_with() {
        let server = MockServer::start().await;
        let _test_context = test_helpers::get_test_context();

        let client = reqwest::Client::new();

        #[rustfmt::skip]
        let client_key = SensitiveString::from(
r"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgQdyCbYBe50EeXqxW
5r9DHQGEfk9NPhC4k7pBWzh/liihRANCAAQ9/OCBrz6FL+OGFDOuJKhmNlIrXhnD
Hb3Esc1sspNDZRV/RPEFJyIJgvN/QncWLPhUGSYuF2BNpgQuM2KVdnLK
-----END PRIVATE KEY-----
"
        );

        let token_callback_count = std::sync::atomic::AtomicUsize::new(0);
        let token_from_callback: std::sync::Mutex<Option<AuthToken>> = std::sync::Mutex::new(None);

        let auth = OAuth2::new(
            client.clone(),
            server.uri() + "/oauth/token/",
            "test_client".to_string(),
            client_key,
            None,
            |token| {
                token_from_callback.lock().unwrap().replace(token.clone());
                token_callback_count.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            },
        );

        // Test initial token retrieval and application
        Mock::given(method("POST"))
            .and(path("/oauth/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "test_token",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/admin/configuration/v1/something/"))
            .and(header("Authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "test": "response"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let request = client
            .get(server.uri() + "/api/admin/configuration/v1/something/")
            .auth_with(&auth)
            .await
            .unwrap()
            .build()
            .unwrap();

        let response = client
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let response_content = response.json::<serde_json::Value>().await.unwrap();
        assert_that!(response_content, eq(&json!({"test": "response"})));

        server.verify().await;
        server.reset().await;
        assert_that!(
            token_callback_count.load(std::sync::atomic::Ordering::Acquire),
            eq(1)
        );

        {
            let token = token_from_callback.lock().unwrap();
            let token: &AuthToken = token.as_ref().unwrap();

            assert_that!(token.token, sensitive_string(eq("test_token")));

            assert_that!(
                (Utc::now() + Duration::hours(1)) - token.expires_at,
                lt(Duration::seconds(60))
            );
        }

        // Test token reuse
        Mock::given(method("GET"))
            .and(path("/api/admin/configuration/v1/anything/"))
            .and(header("Authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "test": "response_2"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let request = client
            .get(server.uri() + "/api/admin/configuration/v1/anything/")
            .auth_with(&auth)
            .await
            .unwrap()
            .build()
            .unwrap();

        let response = client
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let response_content = response.json::<serde_json::Value>().await.unwrap();
        assert_that!(response_content, eq(&json!({"test": "response_2"})));

        server.verify().await;
        server.reset().await;

        // Test expired token behaviour
        auth.token.lock().await.as_mut().unwrap().expires_at =
            Utc::now() - chrono::Duration::minutes(1);

        Mock::given(method("POST"))
            .and(path("/oauth/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "test_token_2",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/admin/configuration/v1/someone/"))
            .and(header("Authorization", "Bearer test_token_2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "test": "response_3"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let request = client
            .get(server.uri() + "/api/admin/configuration/v1/someone/")
            .auth_with(&auth)
            .await
            .unwrap()
            .build()
            .unwrap();

        let response = client
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let response_content = response.json::<serde_json::Value>().await.unwrap();
        assert_that!(response_content, eq(&json!({"test": "response_3"})));

        server.verify().await;
        server.reset().await;

        // Test expiring token behaviour
        auth.token.lock().await.as_mut().unwrap().expires_at =
            Utc::now() + chrono::Duration::minutes(4);

        Mock::given(method("POST"))
            .and(path("/oauth/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "test_token_3",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/admin/configuration/v1/somebody/"))
            .and(header("Authorization", "Bearer test_token_3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "test": "response_4"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let request = client
            .get(server.uri() + "/api/admin/configuration/v1/somebody/")
            .auth_with(&auth)
            .await
            .unwrap()
            .build()
            .unwrap();

        let response = client
            .execute(request)
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let response_content = response.json::<serde_json::Value>().await.unwrap();
        assert_that!(response_content, eq(&json!({"test": "response_4"})));

        server.verify().await;
        server.reset().await;
    }
}

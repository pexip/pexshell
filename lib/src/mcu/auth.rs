#![allow(clippy::module_name_repetitions)]

mod basic;
mod oauth2;

pub use self::oauth2::OAuth2;
use async_trait::async_trait;
pub use basic::BasicAuth;

pub struct NoAuth;

#[async_trait]
impl ApiClientAuth for NoAuth {
    async fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
    }
}

#[async_trait]
pub trait ApiClientAuth: Send + Sync {
    async fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder;
}

#[allow(opaque_hidden_inferred_bound)]
#[async_trait]
pub trait AuthWith: Send {
    async fn auth_with(self, auth: &impl ApiClientAuth) -> Self;
}

#[async_trait]
impl AuthWith for reqwest::RequestBuilder {
    async fn auth_with(self, auth: &impl ApiClientAuth) -> Self {
        auth.add_auth(self).await
    }
}

#![allow(clippy::module_name_repetitions)]

mod basic;
mod oauth2;

use async_trait::async_trait;
pub use basic::BasicAuth;
pub use oauth2::AuthToken as OAuth2AccessToken;
pub use oauth2::OAuth2;

pub struct NoAuth;

#[async_trait]
impl ApiClientAuth for NoAuth {
    async fn add_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::RequestBuilder> {
        Ok(request)
    }
}

#[async_trait]
pub trait ApiClientAuth: Send + Sync {
    async fn add_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::RequestBuilder>;
}

#[allow(opaque_hidden_inferred_bound)]
#[async_trait]
pub trait AuthWith: Send + Sized {
    async fn auth_with<Auth: ApiClientAuth + ?Sized>(self, auth: &Auth) -> anyhow::Result<Self>;
}

#[async_trait]
impl AuthWith for reqwest::RequestBuilder {
    async fn auth_with<Auth: ApiClientAuth + ?Sized>(self, auth: &Auth) -> anyhow::Result<Self> {
        auth.add_auth(self).await
    }
}

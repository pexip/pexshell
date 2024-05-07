#![allow(clippy::module_name_repetitions)]

mod basic;
mod oauth2;

pub use basic::BasicAuth;
pub use oauth2::OAuth2;

pub struct NoAuth;

impl ApiClientAuth for NoAuth {
    async fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
    }
}

pub trait ApiClientAuth: Send + Sync {
    fn add_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> impl std::future::Future<Output = reqwest::RequestBuilder> + std::marker::Send;
}

#[allow(opaque_hidden_inferred_bound)]
pub trait AuthWith: Send {
    fn auth_with(self, auth: &impl ApiClientAuth)
        -> impl std::future::Future<Output = Self> + Send;
}

impl AuthWith for reqwest::RequestBuilder {
    async fn auth_with(self, auth: &impl ApiClientAuth) -> Self {
        auth.add_auth(self).await
    }
}

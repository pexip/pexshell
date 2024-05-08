use async_trait::async_trait;

use crate::util::SensitiveString;

use super::ApiClientAuth;

pub struct BasicAuth {
    username: String,
    password: SensitiveString,
}

impl BasicAuth {
    #[must_use]
    pub fn new(username: String, password: SensitiveString) -> Self {
        Self { username, password }
    }
}

#[async_trait]
impl ApiClientAuth for BasicAuth {
    async fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.basic_auth(&self.username, Some(&self.password.secret()))
    }
}

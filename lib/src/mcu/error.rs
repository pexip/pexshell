#![allow(clippy::module_name_repetitions)]
use std::{error::Error, fmt::Display};

pub struct ApiError {
    status: Option<reqwest::StatusCode>,
    message: String,
    inner: Option<anyhow::Error>,
}

impl ApiError {
    #[must_use]
    pub fn new(
        status: Option<reqwest::StatusCode>,
        message: impl Into<String>,
        inner: Option<anyhow::Error>,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            inner,
        }
    }

    #[must_use]
    pub const fn status(&self) -> Option<reqwest::StatusCode> {
        self.status
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn inner(&self) -> Option<&(dyn Error + Send + Sync)> {
        self.inner.as_ref().map(std::convert::AsRef::as_ref)
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(s) => f.write_fmt(format_args!(
                "api error with status {}: {}",
                s, &self.message
            )),
            None => f.write_fmt(format_args!("api error: {}", self.message)),
        }
    }
}

impl std::fmt::Debug for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => f.write_str(&format!(
                "\n\tresponse code: {}\n\tmessage: {}",
                status, &self.message
            )),
            None => f.write_str(&self.message),
        }
    }
}

impl Error for ApiError {}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;

    #[test]
    fn test_api_error() {
        let error = ApiError::new(None, "Test message.", None);
        assert_eq!(error.message(), "Test message.");
        assert!(error.status().is_none());
        assert!(error.inner().is_none());
        assert_eq!(format!("{}", &error), "api error: Test message.");
        assert_eq!(format!("{:?}", &error), "Test message.");
    }

    #[test]
    fn test_api_error_with_status() {
        let error = ApiError::new(Some(StatusCode::NOT_FOUND), "Test message.", None);
        assert_eq!(error.message(), "Test message.");
        assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
        assert!(error.inner().is_none());
        assert_eq!(
            format!("{}", &error),
            "api error with status 404 Not Found: Test message."
        );
        assert_eq!(
            format!("{:?}", &error),
            "\n\tresponse code: 404 Not Found\n\tmessage: Test message."
        );
    }
}

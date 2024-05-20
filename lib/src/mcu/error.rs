#![allow(clippy::module_name_repetitions)]
use std::{
    error::Error,
    fmt::{Debug, Display},
};

/// A wrapper for [`reqwest::Error`] that prevents it from printing repetitive error chains.
///
/// When printing `anyhow::Error` chains with `{:#}`, the `#` gets applied to the inner `reqwest::Error` as well.
/// This leads to it printing the same error chain multiple times, which is confusing and unnecessary.
/// Instead, we can simply wrap the inner `reqwest::Error` in this struct that ignores the `#` flag.
pub struct ReqwestDebugPrintWrapper(pub reqwest::Error);

impl Display for ReqwestDebugPrintWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Debug for ReqwestDebugPrintWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Error for ReqwestDebugPrintWrapper {}

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
    use googletest::prelude::*;
    use reqwest::StatusCode;

    use super::*;

    #[test]
    fn test_api_error() {
        let error = ApiError::new(None, "Test message.", None);
        assert_that!(error.message(), eq("Test message."));
        assert_that!(error.status(), none());
        assert_that!(error.inner(), none());
        assert_that!(error, displays_as(eq("api error: Test message.")));
        assert_that!(format!("{:?}", &error), eq("Test message."));
    }

    #[test]
    fn test_api_error_with_status() {
        let error = ApiError::new(Some(StatusCode::NOT_FOUND), "Test message.", None);
        assert_that!(error.message(), eq("Test message."));
        assert_that!(error.status(), some(eq(StatusCode::NOT_FOUND)));
        assert_that!(error.inner(), none());
        assert_that!(
            error,
            displays_as(eq("api error with status 404 Not Found: Test message."))
        );
        assert_that!(
            format!("{:?}", &error),
            eq("\n\tresponse code: 404 Not Found\n\tmessage: Test message.")
        );
    }
}

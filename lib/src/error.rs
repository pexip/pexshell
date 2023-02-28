use std::fmt::{Debug, Display};

pub struct UserFriendly {
    message: String,
}

impl Display for UserFriendly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.message, f)
    }
}

impl Debug for UserFriendly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.message, f)
    }
}

impl std::error::Error for UserFriendly {}

impl UserFriendly {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_friendly_error() {
        let error = UserFriendly::new("Some error message.");
        assert_eq!(format!("{error}").as_str(), "Some error message.");
        assert_eq!(format!("{error:?}").as_str(), "Some error message.");
    }
}

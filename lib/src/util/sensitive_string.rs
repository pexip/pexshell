use std::fmt::{Debug, Display};

use serde::{de::Visitor, Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SensitiveString {
    value: String,
}

impl SensitiveString {
    #[must_use]
    pub fn secret(&self) -> &str {
        &self.value
    }
}

impl Display for SensitiveString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("*")
    }
}

impl From<String> for SensitiveString {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl From<&str> for SensitiveString {
    fn from(value: &str) -> Self {
        Self {
            value: String::from(value),
        }
    }
}

impl Debug for SensitiveString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\"*\"")
    }
}

impl Serialize for SensitiveString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.value)
    }
}

impl<'de> Deserialize<'de> for SensitiveString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(SecureStringVisitor {})
    }
}

struct SecureStringVisitor;

impl<'de> Visitor<'de> for SecureStringVisitor {
    type Value = SensitiveString;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(SensitiveString {
            value: value.to_owned(),
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(SensitiveString { value })
    }
}

#[cfg(test)]
mod tests {
    use serde::de::{Error, Visitor};
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use zeroize::Zeroize;

    use crate::util::sensitive_string::SecureStringVisitor;

    use super::SensitiveString;

    const TEST_DATA: &str = "TESTING!";

    #[test]
    fn basic_test() {
        let sensitive = SensitiveString::from(TEST_DATA);
        assert_eq!(sensitive.to_string(), "*");
        assert_eq!(format!("{}", &sensitive), "*");
        assert_eq!(format!("{:?}", &sensitive), r#""*""#);
        assert_eq!(sensitive.secret(), String::from(TEST_DATA));
    }

    #[derive(Serialize, Deserialize)]
    struct TestSerialise {
        payload: SensitiveString,
    }

    #[derive(Debug, derive_more::Display, derive_more::Error)]
    struct DeError {}

    impl serde::de::Error for DeError {
        fn custom<T>(_msg: T) -> Self
        where
            T: std::fmt::Display,
        {
            unimplemented!()
        }
    }

    #[test]
    #[should_panic]
    fn test_de_error_custom() {
        let _error = DeError::custom("some error message");
    }

    struct FormatExpecting;

    impl std::fmt::Display for FormatExpecting {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            SecureStringVisitor.expecting(f)
        }
    }

    #[test]
    fn test_serialize() {
        let sensitive_wrapper = TestSerialise {
            payload: SensitiveString::from(TEST_DATA),
        };

        // test visitor
        assert_eq!(format!("{FormatExpecting}"), "a string");
        let v_str: SensitiveString = SecureStringVisitor
            .visit_str::<DeError>("Testing...")
            .unwrap();
        assert_eq!(v_str.secret(), "Testing...");
        let v_str: SensitiveString = SecureStringVisitor
            .visit_string::<DeError>(String::from("Testing."))
            .unwrap();
        assert_eq!(v_str.secret(), "Testing.");

        // test with serde
        assert_eq!(
            serde_json::to_value(sensitive_wrapper).unwrap(),
            json!({ "payload": TEST_DATA })
        );
    }

    #[test]
    fn test_deserialize() {
        let sensitive_wrapper: TestSerialise =
            serde_json::from_value(json!({ "payload": TEST_DATA })).unwrap();
        assert_eq!(sensitive_wrapper.payload.to_string(), "*");
        assert_eq!(sensitive_wrapper.payload.secret(), TEST_DATA);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_clone() {
        let sensitive_string = SensitiveString::from("Testing");
        let dup = sensitive_string.clone();
        assert_eq!(dup.secret(), "Testing");
    }

    #[test]
    fn test_from_str() {
        let sensitive_string = SensitiveString::from("Test");
        assert_eq!(sensitive_string.secret(), "Test");
    }

    #[test]
    fn test_from_string() {
        let sensitive_string = SensitiveString::from(String::from("Test"));
        assert_eq!(sensitive_string.secret(), "Test");
    }

    #[test]
    fn test_to_string() {
        let sensitive_string = SensitiveString::from("Testing");
        assert_eq!(sensitive_string.to_string(), "*");
    }

    #[test]
    fn test_display() {
        let sensitive_string = SensitiveString::from("Test");
        assert_eq!(format!("{sensitive_string}"), "*");
    }

    #[test]
    fn test_debug() {
        let sensitive_string = SensitiveString::from("Test");
        assert_eq!(format!("{sensitive_string:?}"), r#""*""#);
    }

    #[test]
    fn test_zeroize() {
        let mut sensitive_string = SensitiveString::from("Test");
        assert_eq!(sensitive_string.secret(), "Test");
        sensitive_string.zeroize();
        assert_eq!(sensitive_string.secret(), "");
    }
}

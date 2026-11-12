//! A secret value that never shows up in logs.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A secret handed to a connector for one session: an OAuth token, an API key.
///
/// Its `Debug` and `Display` output is `[secret]`, so that a secret printed by mistake
/// in a log or an error message does not leak. Use [`Secret::expose`] where the value
/// is really needed.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret value. Never log it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[secret]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[secret]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_printed() {
        let secret = Secret::new("ya29.a0Af");
        assert_eq!(format!("{secret:?}"), "[secret]");
        assert_eq!(secret.to_string(), "[secret]");
        assert_eq!(secret.expose(), "ya29.a0Af");
    }

    #[test]
    fn serializes_as_a_plain_string() {
        let json = serde_json::to_string(&Secret::new("abc")).unwrap();
        assert_eq!(json, "\"abc\"");
    }
}

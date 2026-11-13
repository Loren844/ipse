//! The vault's language, used for the words Ipse writes in files (`docs/spec/vault.md`, Q2).

use std::fmt;

/// A language Ipse writes section headings and relation lines in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Language {
    /// English, the default.
    #[default]
    En,
    /// French.
    Fr,
}

impl Language {
    /// Every supported language.
    pub const ALL: [Self; 2] = [Self::En, Self::Fr];

    /// The language's code, as written in `.ipse/vault.toml`: `en`, `fr`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    /// Reads a language code or a locale (`fr`, `fr-FR`, `fr_CA.UTF-8`). Returns `None`
    /// for an unsupported language.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let code = text.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.code() == code)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_locales() {
        assert_eq!(Language::parse("fr"), Some(Language::Fr));
        assert_eq!(Language::parse("fr_FR.UTF-8"), Some(Language::Fr));
        assert_eq!(Language::parse("EN-us"), Some(Language::En));
        assert_eq!(Language::parse("de"), None);
        assert_eq!(Language::parse(""), None);
    }
}

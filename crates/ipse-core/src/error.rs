//! The error type shared by the whole core.
//!
//! Messages are written for a person, not a developer, and never contain personal
//! data: they name a file and a line, never the content of that file.

use std::path::PathBuf;

/// Result type of the core: `Ok(T)` or an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong in the core.
///
/// Each variant's message says what happened and, when it can, what to do about it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file or folder could not be read or written.
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        /// What Ipse was doing, as a verb: "read", "write", "create".
        action: &'static str,
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying operating-system error.
        #[source]
        source: std::io::Error,
    },

    /// A file does not follow its format. Ipse leaves it untouched.
    #[error("{}{}: {problem}", path.display(), line.map(|l| format!(", line {l}")).unwrap_or_default())]
    Invalid {
        /// The file involved.
        path: PathBuf,
        /// The line where the problem is, when known (1-based).
        line: Option<u32>,
        /// What is wrong and how to fix it, without quoting the file's content.
        problem: String,
    },

    /// A vault, journal or protocol version newer than this Ipse understands.
    #[error(
        "{what} uses format version {found}, but this version of Ipse only reads up to {supported}. Update Ipse to open it."
    )]
    UnsupportedVersion {
        /// What carries the version: "this vault", "this journal".
        what: &'static str,
        /// The version found.
        found: u32,
        /// The highest version this Ipse supports.
        supported: u32,
    },

    /// A request Ipse cannot carry out as asked. The message says what to do instead.
    #[error("{0}")]
    Refused(String),

    /// The vault's history could not be read or updated. The files themselves are as
    /// they are on disk; only the record of who changed them is missing.
    #[error(
        "Ipse could not {action} the history of this memory. Your files are as they are on disk; only the record of this change is missing."
    )]
    History {
        /// What Ipse was doing, as a verb: "read", "update", "create".
        action: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_names_file_and_line() {
        let err = Error::Invalid {
            path: PathBuf::from("people/marie-dupont.md"),
            line: Some(3),
            problem: "the `id` key is missing".into(),
        };
        assert_eq!(
            err.to_string(),
            "people/marie-dupont.md, line 3: the `id` key is missing"
        );
    }

    #[test]
    fn invalid_without_line() {
        let err = Error::Invalid {
            path: PathBuf::from(".ipse/vault.toml"),
            line: None,
            problem: "the `format` key is missing".into(),
        };
        assert_eq!(
            err.to_string(),
            ".ipse/vault.toml: the `format` key is missing"
        );
    }

    #[test]
    fn unsupported_version_says_what_to_do() {
        let err = Error::UnsupportedVersion {
            what: "this vault",
            found: 2,
            supported: 1,
        };
        assert!(err.to_string().ends_with("Update Ipse to open it."));
    }
}

//! `ipse access-code`: the interface's own access code, for `ipse serve` (decision
//! 0019). Only its BLAKE3 hash is kept, in `.ipse/interface.toml`.

use std::path::{Path, PathBuf};

use ipse_core::vault::{Vault, write_atomic};
use ipse_core::{Error, Result};

/// Where the hash of the interface's code is kept, relative to the vault.
const FILE: &str = ".ipse/interface.toml";

/// What every code starts with, so that the person knows which code it is.
const PREFIX: &str = "ipse_ui_";

/// Makes a new access code for the interface of the memory in `vault`, replacing the
/// old one, and shows it once.
///
/// # Errors
///
/// Errors of [`Vault::open`], or when the file cannot be written.
pub(crate) fn run(vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let code = replace(vault.root())?;
    println!("The interface's access code (shown only this once; keep it like a password):");
    println!();
    println!("    {code}");
    println!();
    println!(
        "Enter it on the login page of `ipse serve`. Running `ipse access-code` again \
         replaces it and closes every session opened with this one."
    );
    Ok(())
}

/// Makes a new code for the memory at `root`, keeps its hash, and returns the code.
///
/// # Errors
///
/// When no random code can be made, or the file cannot be written.
pub(crate) fn replace(root: &Path) -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| Error::Refused("This computer could not make a random code.".to_owned()))?;
    let code = format!("{PREFIX}{}", blake3::Hash::from_bytes(bytes).to_hex());
    let text = format!(
        "# The interface's access code, as its BLAKE3 hash (decision 0019).\n\
         # `ipse access-code` replaces it.\n\
         code = \"{}\"\n",
        blake3::hash(code.as_bytes()).to_hex()
    );
    write_atomic(&root.join(FILE), text.as_bytes())?;
    Ok(code)
}

/// The hash of the current code of the memory at `root`; `None` when none was made.
pub(crate) fn current(root: &Path) -> Option<blake3::Hash> {
    let text = std::fs::read_to_string(root.join(FILE)).ok()?;
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "code")
        .and_then(|(_, value)| blake3::Hash::from_hex(value.trim().trim_matches('"')).ok())
}

/// Whether `code` is the current code of the memory at `root`. Returns the hash it
/// matched, for the session it opens.
pub(crate) fn check(root: &Path, code: &str) -> Option<blake3::Hash> {
    let expected = current(root)?;
    let code = code.trim();
    // `blake3::Hash` compares in constant time.
    (!code.is_empty() && blake3::hash(code.as_bytes()) == expected).then_some(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_replaces_the_one_before() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ipse")).unwrap();
        assert_eq!(current(dir.path()), None);
        assert_eq!(check(dir.path(), "anything"), None);

        let first = replace(dir.path()).unwrap();
        assert!(first.starts_with(PREFIX));
        assert!(check(dir.path(), &first).is_some());
        assert!(check(dir.path(), &format!(" {first}\n")).is_some());
        assert_eq!(check(dir.path(), ""), None);

        let second = replace(dir.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(check(dir.path(), &first), None);
        assert!(check(dir.path(), &second).is_some());
        let kept = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!kept.contains(&second), "only the hash is kept");
    }
}

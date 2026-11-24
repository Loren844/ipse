//! The system keychain, where Ipse keeps the secrets of the sources that sync:
//! Keychain on macOS, Credential Manager on Windows, Secret Service on Linux; on a
//! server, a file of their own (decision 0019).

use ipse_core::connector::SecretStore;
use ipse_core::{Error, Result};
use ipse_protocol::Secret;
use keyring::Entry;

/// The service name Ipse's secrets are filed under.
const SERVICE: &str = "ipse";

/// The person's system keychain.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Keychain;

impl Keychain {
    fn entry(key: &str) -> Result<Entry> {
        Entry::new(SERVICE, key).map_err(unreachable)
    }
}

impl SecretStore for Keychain {
    fn get(&self, key: &str) -> Result<Option<Secret>> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(unreachable(e)),
        }
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<()> {
        Self::entry(key)?
            .set_password(secret.expose())
            .map_err(unreachable)
    }

    fn delete(&self, key: &str) -> Result<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(unreachable(e)),
        }
    }
}

/// The variable that names a file to keep the secrets in, instead of the system
/// keychain: for a server, which has none (decision 0019).
pub(crate) const SECRETS_FILE_VAR: &str = "IPSE_SECRETS_FILE";

/// Where this program keeps secrets: the system keychain, or the file that
/// `IPSE_SECRETS_FILE` names (a server, a container). A development build run by the
/// interface's tests (`IPSE_TEST_SECRETS`, a file path) keeps them in that file too, as
/// a test machine has no keychain to unlock; a released build ignores that one.
pub(crate) fn secrets() -> Box<dyn SecretStore> {
    if let Some(path) = std::env::var_os(SECRETS_FILE_VAR).filter(|p| !p.is_empty()) {
        return Box::new(FileSecrets(std::path::PathBuf::from(path)));
    }
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("IPSE_TEST_SECRETS") {
        return Box::new(FileSecrets(std::path::PathBuf::from(path)));
    }
    Box::new(Keychain)
}

/// Secrets in a JSON file, readable by its owner only (see [`secrets`]).
struct FileSecrets(std::path::PathBuf);

impl FileSecrets {
    fn read(&self) -> Result<std::collections::BTreeMap<String, String>> {
        match std::fs::read(&self.0) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
                Error::Refused(format!(
                    "The secrets file {} is not one Ipse wrote: it cannot be read.",
                    self.0.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(std::collections::BTreeMap::default())
            }
            Err(source) => Err(Error::Io {
                action: "read",
                path: self.0.clone(),
                source,
            }),
        }
    }

    /// Writes the whole file again, through a file of its own beside it that only its
    /// owner can read, moved over the old one.
    fn write(&self, all: &std::collections::BTreeMap<String, String>) -> Result<()> {
        let io = |source| Error::Io {
            action: "write",
            path: self.0.clone(),
            source,
        };
        let bytes = serde_json::to_vec(all).map_err(|e| Error::Refused(e.to_string()))?;
        let mut temporary = self.0.clone().into_os_string();
        temporary.push(".new");
        let temporary = std::path::PathBuf::from(temporary);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary).map_err(io)?;
        std::io::Write::write_all(&mut file, &bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        std::fs::rename(&temporary, &self.0).map_err(io)
    }
}

impl SecretStore for FileSecrets {
    fn get(&self, key: &str) -> Result<Option<Secret>> {
        Ok(self.read()?.remove(key).map(Secret::new))
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<()> {
        let mut all = self.read()?;
        all.insert(key.to_owned(), secret.expose().to_owned());
        self.write(&all)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut all = self.read()?;
        if all.remove(key).is_some() {
            self.write(&all)?;
        }
        Ok(())
    }
}

/// What to do when the keychain cannot be used. The keychain's own error is left out:
/// it may quote what was stored.
fn unreachable(_: keyring::Error) -> Error {
    Error::Refused(
        "Ipse keeps calendar links in your system's keychain, and could not use it. \
         On macOS, allow Ipse when the system asks. On Linux, a keyring such as GNOME Keyring \
         or KWallet must be installed and unlocked; on a server, set IPSE_SECRETS_FILE to a file \
         outside your memory's folder."
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secrets_file_is_its_owners_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSecrets(dir.path().join("secrets.json"));
        assert!(store.get("perso").unwrap().is_none());
        store.set("perso", &Secret::new("webcal://secret")).unwrap();
        store.set("pro", &Secret::new("app-password")).unwrap();
        assert_eq!(
            store.get("perso").unwrap().unwrap().expose(),
            "webcal://secret"
        );
        store.delete("perso").unwrap();
        store.delete("absent").unwrap();
        assert!(store.get("perso").unwrap().is_none());
        assert_eq!(store.get("pro").unwrap().unwrap().expose(), "app-password");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&store.0).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&store.0, "not json").unwrap();
        assert!(
            store.get("pro").is_err(),
            "a file Ipse did not write is not trusted"
        );
    }
}

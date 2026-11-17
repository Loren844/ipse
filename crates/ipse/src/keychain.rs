//! The system keychain, where Ipse keeps the secrets of the sources that sync:
//! Keychain on macOS, Credential Manager on Windows, Secret Service on Linux.

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

/// Where this program keeps secrets: the system keychain. A development build run by
/// the interface's tests (`IPSE_TEST_SECRETS`, a file path) keeps them in that file
/// instead, as a test machine has no keychain to unlock; a released build ignores it.
pub(crate) fn secrets() -> Box<dyn SecretStore> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("IPSE_TEST_SECRETS") {
        return Box::new(TestSecrets(std::path::PathBuf::from(path)));
    }
    Box::new(Keychain)
}

/// Secrets in a JSON file, for the interface's tests only (see [`secrets`]).
#[cfg(debug_assertions)]
struct TestSecrets(std::path::PathBuf);

#[cfg(debug_assertions)]
impl TestSecrets {
    fn read(&self) -> std::collections::BTreeMap<String, String> {
        std::fs::read(&self.0)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, all: &std::collections::BTreeMap<String, String>) -> Result<()> {
        let bytes = serde_json::to_vec(all).map_err(|e| Error::Refused(e.to_string()))?;
        std::fs::write(&self.0, bytes).map_err(|source| Error::Io {
            action: "write",
            path: self.0.clone(),
            source,
        })
    }
}

#[cfg(debug_assertions)]
impl SecretStore for TestSecrets {
    fn get(&self, key: &str) -> Result<Option<Secret>> {
        Ok(self.read().remove(key).map(Secret::new))
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<()> {
        let mut all = self.read();
        all.insert(key.to_owned(), secret.expose().to_owned());
        self.write(&all)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut all = self.read();
        all.remove(key);
        self.write(&all)
    }
}

/// What to do when the keychain cannot be used. The keychain's own error is left out:
/// it may quote what was stored.
fn unreachable(_: keyring::Error) -> Error {
    Error::Refused(
        "Ipse keeps calendar links in your system's keychain, and could not use it. \
         On macOS, allow Ipse when the system asks. On Linux, a keyring such as GNOME Keyring \
         or KWallet must be installed and unlocked."
            .to_owned(),
    )
}

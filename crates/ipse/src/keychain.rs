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

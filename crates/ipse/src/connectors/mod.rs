//! First-party connectors, built into the `ipse` program (decision 0002, connectors
//! spec Q1). Each runs in its own process, started as `ipse connector <name>`.

pub(crate) mod contacts;
mod session;

use ipse_core::{Error, Result};

/// Runs the connector `name` on standard input and output.
pub(crate) fn run(name: &str) -> Result<()> {
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    let outcome = match name {
        contacts::ID => contacts::run(stdin, stdout),
        _ => {
            return Err(Error::Refused(format!(
                "There is no built-in connector named {name:?}."
            )));
        }
    };
    outcome.map_err(|e| Error::Refused(e.to_string()))
}

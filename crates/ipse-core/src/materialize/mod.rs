//! Turning journal records into vault files (decision 0005: data a connector produces
//! deterministically is written directly).

mod contacts;

pub use contacts::{PeopleSummary, contacts_to_people};

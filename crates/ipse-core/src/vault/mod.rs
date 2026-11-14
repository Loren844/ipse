//! The Markdown vault, source of truth of a person's memory (`docs/spec/vault.md`).
//!
//! This module reads and writes memory files: their front matter (typed keys plus any
//! key a person or a plug-in added, kept as written), their body, their file names,
//! and writes them atomically.

mod atomic;
pub mod body;
mod check;
mod emit;
pub mod event;
pub mod fields;
mod file;
mod front;
pub mod history;
pub mod lang;
pub mod naming;
pub mod settings;
pub mod state;
mod store;
mod yaml;

pub use atomic::{sync_files, write_atomic, write_atomic_unsynced};
pub use body::{Body, Fact, FactId, Relation, RelationKind, Section};
pub use check::{Problem, Report, check};
pub use event::{EventFields, EventStatus, EventTime};
pub use fields::{
    Birthday, Category, Common, DateTime, EntityFields, FileType, Geo, Id, LinkOrText, WikiLink,
};
pub use file::MemoryFile;
pub use history::{Change, History, Source};
pub use lang::Language;
pub use settings::{CustomCategory, Settings};
pub use state::{Seen, StateLogFields, StateRow, Status, Subject, Topic};
pub use store::{SETTINGS_FILE, Vault};

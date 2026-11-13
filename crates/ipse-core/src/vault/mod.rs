//! The Markdown vault, source of truth of a person's memory (`docs/spec/vault.md`).
//!
//! This module reads and writes memory files: their front matter (typed keys plus any
//! key a person or a plug-in added, kept as written), their body, their file names,
//! and writes them atomically.

mod atomic;
mod emit;
pub mod fields;
mod file;
mod front;
pub mod naming;
mod yaml;

pub use atomic::write_atomic;
pub use fields::{Birthday, Category, Common, DateTime, EntityFields, FileType, Geo, Id, WikiLink};
pub use file::MemoryFile;

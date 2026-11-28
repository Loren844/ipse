//! Notes of the day (issue #277): what the person jots down without an AI, from the
//! home screen or the tray menu, kept in one file a day, `notes/<YYYY>/<YYYY-MM-DD>.md`
//! (`docs/spec/vault.md`, section 7.1).
//!
//! Each note is one line of the file's `Notes` section, with the time it was written:
//! `- 14:32 : Aaron part vivre à [[lyon|Lyon]] en mars.` A mention is a link, so the
//! file shows among what links to that person or place. What the person writes is
//! theirs: it is written at once, never proposed, as one change of the history.

use std::path::Path;

use crate::vault::body::{Section, replace_section, section_text};
use crate::vault::{Common, DateTime, FileType, Language, MemoryFile, Source, Vault, WikiLink};
use crate::{Error, Result};

/// The folder of the notes of the day, relative to the vault.
pub const FOLDER: &str = "notes";

/// Longest note, in characters.
pub const MAX_NOTE: usize = 500;

/// The notes of one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayNote {
    /// The day, like `2026-11-28`.
    pub day: String,
    /// Its file, relative to the vault with `/`.
    pub file: String,
    /// Its lines, oldest first, as written: `14:32 : Aaron part…`, links included.
    pub lines: Vec<String>,
}

/// The file of the notes of `day` (`2026-11-28`), relative to the vault with `/`.
#[must_use]
pub fn file_of(day: &str) -> String {
    let year = day.get(..4).unwrap_or(day);
    format!("{FOLDER}/{year}/{day}.md")
}

/// `text` as one line, its spaces made single, refused when empty or longer than
/// [`MAX_NOTE`].
///
/// # Errors
///
/// [`Error::Refused`] with what to do.
pub fn one_line(text: &str) -> Result<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(Error::Refused("Write something to note.".to_owned()));
    }
    if text.chars().count() > MAX_NOTE {
        return Err(Error::Refused(format!(
            "A note holds at most {MAX_NOTE} characters."
        )));
    }
    Ok(text)
}

/// `text` after `when`, in the vault's language: `2026-11-28 : …` or `14:32 : …` in
/// French, `2026-11-28: …` in English.
#[must_use]
pub fn dated(when: &str, text: &str, language: Language) -> String {
    match language {
        Language::Fr => format!("{when} : {text}"),
        _ => format!("{when}: {text}"),
    }
}

/// Adds `text` to today's notes, in the vault's time zone, as one change of the history
/// by the person. A mention is written as a link, like `[[aaron-petit|Aaron]]`. Returns
/// the day's notes as they are now.
///
/// # Errors
///
/// [`Error::Refused`] for an empty or too long note; [`Error::Io`], [`Error::Invalid`]
/// or [`Error::History`] when the file or the history cannot be written.
pub fn add(vault: &Vault, text: &str) -> Result<DayNote> {
    let text = one_line(text)?;
    let now = DateTime::now(vault.settings().time_zone()).to_string();
    let (Some(day), Some(time)) = (now.get(..10), now.get(11..16)) else {
        return Err(Error::Refused(
            "This computer's clock cannot be read.".to_owned(),
        ));
    };
    let language = vault.settings().language();
    let rel = file_of(day);
    let path = vault.root().join(&rel);
    let line = format!("- {}", dated(time, &text, language));

    let history = vault.history()?;
    // Holds the vault's write lock: read and written by no one else meanwhile.
    let mut record = history.change(Source::Person)?;
    let file = if path.is_file() {
        let mut file = MemoryFile::read(&path)?;
        let notes = section_text(file.body(), Section::Notes).unwrap_or_default();
        let notes = notes.trim_end();
        let joined = if notes.trim().is_empty() {
            line
        } else {
            format!("{notes}\n{line}")
        };
        let body = replace_section(file.body(), Section::Notes, &joined, language);
        file.set_body(body);
        file
    } else {
        let common = Common::new(FileType::Note, vault.settings().time_zone())
            .ok_or_else(|| Error::Refused("A note of the day cannot be created.".to_owned()))?;
        let body = replace_section("", Section::Notes, &line, language);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                action: "create",
                path: parent.to_owned(),
                source,
            })?;
        }
        MemoryFile::new(&path, common, None, body)
    };
    file.write()?;
    record.touched(&path);
    record.commit("Add a note of the day")?;
    Ok(read_file(&file, day, rel))
}

/// The notes of `day` (`2026-11-28`) in the vault at `root`; `None` when there are
/// none, or the file cannot be read.
#[must_use]
pub fn read(root: &Path, day: &str) -> Option<DayNote> {
    let rel = file_of(day);
    let file = MemoryFile::read(root.join(&rel)).ok()?;
    let note = read_file(&file, day, rel);
    (!note.lines.is_empty()).then_some(note)
}

fn read_file(file: &MemoryFile, day: &str, rel: String) -> DayNote {
    let lines = section_text(file.body(), Section::Notes)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().trim_start_matches(['-', '*']).trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    DayNote {
        day: day.to_owned(),
        file: rel,
        lines,
    }
}

/// The links written in `text`, in order: `[[aaron-petit|Aaron]]` gives `aaron-petit`.
#[must_use]
pub fn links_in(text: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start..].find("]]") else {
            break;
        };
        if let Some(link) = WikiLink::parse(&rest[start..start + end + 2]) {
            links.push(link);
        }
        rest = &rest[start + end + 2..];
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_found_in_a_line() {
        let found: Vec<String> = links_in("12:00 : vu [[aaron-petit|Aaron]] à [[lyon]], [[cassé")
            .into_iter()
            .map(|l| l.target)
            .collect();
        assert_eq!(found, ["aaron-petit", "lyon"]);
    }

    #[test]
    fn a_day_has_its_file() {
        assert_eq!(file_of("2026-11-28"), "notes/2026/2026-11-28.md");
    }

    #[test]
    fn a_note_is_one_line() {
        assert_eq!(one_line(" a \n b ").unwrap(), "a b");
        assert!(one_line(" ").is_err());
        assert!(one_line(&"x".repeat(501)).is_err());
    }
}

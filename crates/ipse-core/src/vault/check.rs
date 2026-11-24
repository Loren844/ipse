//! Checking a vault's files: those Ipse cannot read, and what it reads but should be
//! fixed (`docs/spec/vault.md`, sections 3.2 and 10). Nothing is changed.

use std::path::{Path, PathBuf};

use super::body::duplicate_fact_ids;
use super::{FileType, MemoryFile};
use crate::{Error, Result};

/// Something to fix in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The file, relative to the vault's folder.
    pub path: PathBuf,
    /// The line, when known (1-based).
    pub line: Option<u32>,
    /// What is wrong and how to fix it, in plain words, without quoting the file.
    pub message: String,
}

/// What a check found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// How many files were looked at.
    pub files: usize,
    /// What to fix, by file.
    pub problems: Vec<Problem>,
}

/// A file that holds the same `id` as another one: most often a copy made by hand, in
/// a file manager or with Obsidian's *Make a copy* (issue #230).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedFile {
    /// The copy, relative to the vault's folder.
    pub path: PathBuf,
    /// The file kept as the one with that id: the oldest by `created`, then the
    /// shortest path.
    pub original: PathBuf,
}

/// The files of the vault's folders whose `id` another file already has, each with the
/// file kept as the one with that id. Unreadable files are left out, as [`check`]
/// reports them.
///
/// # Errors
///
/// [`Error::Io`] when a folder exists but cannot be listed.
pub fn copies(root: &Path) -> Result<Vec<CopiedFile>> {
    let mut by_id: std::collections::HashMap<String, Vec<PathBuf>> =
        std::collections::HashMap::new();
    for path in memory_files(root)? {
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
        by_id
            .entry(file.common().id.to_string())
            .or_default()
            .push(relative);
    }
    Ok(copies_among(
        root,
        by_id.into_values().filter(|g| g.len() > 1).collect(),
    ))
}

/// The copies among `groups`, files of the vault at `root` (relative paths) that share
/// one id each, as the search index finds them: in each group, every file but the
/// oldest by `created` (then the shortest path, then by path). Only these files are
/// read.
#[must_use]
pub fn copies_among(root: &Path, groups: Vec<Vec<PathBuf>>) -> Vec<CopiedFile> {
    let mut found = Vec::new();
    for group in groups {
        // The oldest is kept; between files of the same age, as a copy keeps the
        // original's `created`, the shortest name ("marie.md" before "marie 1.md").
        let mut files: Vec<(jiff::Timestamp, usize, PathBuf)> = group
            .into_iter()
            .filter_map(|p| {
                let file = MemoryFile::read(root.join(&p)).ok()?;
                Some((file.common().created.timestamp(), p.as_os_str().len(), p))
            })
            .collect();
        if files.len() < 2 {
            continue;
        }
        files.sort();
        let original = files[0].2.clone();
        found.extend(files.into_iter().skip(1).map(|(_, _, path)| CopiedFile {
            path,
            original: original.clone(),
        }));
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The `.md` files of the vault's folders, sorted.
fn memory_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut folders: Vec<PathBuf> = FileType::ALL
        .into_iter()
        .map(|t| root.join(t.folder()))
        .collect();
    folders.dedup();
    let mut files = Vec::new();
    for folder in folders {
        markdown_files(&folder, &mut files)?;
    }
    files.sort();
    Ok(files)
}

/// Looks at every file of the vault's folders (`people/`, `events/`, `states/`,
/// `summaries/`…): files that cannot be read, fact ids used twice in a file, files that
/// hold another file's `id` ([`copies`]); and at
/// the journal's archived months that do not match their checksum. Other files, the
/// person's own notes elsewhere included, are left alone.
///
/// # Errors
///
/// [`Error::Io`] when a folder exists but cannot be listed.
pub fn check(root: &Path) -> Result<Report> {
    let mut report = Report::default();
    let files = memory_files(root)?;
    for path in crate::journal::damaged_segments(root)? {
        report.problems.push(Problem {
            path: path.strip_prefix(root).unwrap_or(&path).to_owned(),
            line: None,
            message: "this archived month of the journal does not match its checksum: Ipse keeps it as it is and reads the other months; restore it from a backup if you have one".to_owned(),
        });
    }
    for path in files {
        report.files += 1;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
        match MemoryFile::read(&path) {
            Ok(file) => {
                for id in duplicate_fact_ids(&file.sections()) {
                    report.problems.push(Problem {
                        path: relative.clone(),
                        line: None,
                        message: format!(
                            "the fact id ^{id} is used by more than one fact: give one of them another id, or remove it and Ipse will add a new one"
                        ),
                    });
                }
            }
            Err(Error::Invalid { line, problem, .. }) => report.problems.push(Problem {
                path: relative,
                line,
                message: problem,
            }),
            Err(error) => report.problems.push(Problem {
                path: relative,
                line: None,
                message: error.to_string(),
            }),
        }
    }
    for copy in copies(root)? {
        report.problems.push(Problem {
            message: format!(
                "this file has the same id as {}: it looks like a copy. Open it in Ipse's Memory screen to give it an id of its own, or delete it",
                copy.original.to_string_lossy().replace('\\', "/")
            ),
            path: copy.path,
            line: None,
        });
    }
    report.problems.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

/// The `.md` files under `dir`, hidden files and folders aside. A missing folder has
/// none.
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::Io {
                action: "read",
                path: dir.to_owned(),
                source,
            });
        }
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => markdown_files(&path, out)?,
            Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "md") => {
                out.push(path);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_what_to_fix_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let people = dir.path().join("people");
        std::fs::create_dir_all(&people).unwrap();
        let head = "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8a0\ntype: person\ncategory: people\ncreated: 2026-11-12T19:30:00+01:00\nupdated: 2026-11-12T19:30:00+01:00\nname: Marie\n---\n";
        std::fs::write(
            people.join("marie.md"),
            format!("{head}\n## Faits\n\n- A. ^f-3k9w\n- B. ^f-3k9w\n"),
        )
        .unwrap();
        std::fs::write(
            people.join("paul.md"),
            format!("{head}\n## Faits\n\n- A. ^f-3k9w\n"),
        )
        .unwrap();
        std::fs::write(people.join("broken.md"), "---\nid: [\n---\n").unwrap();
        std::fs::write(dir.path().join("my-notes.md"), "anything").unwrap();

        let report = check(dir.path()).unwrap();
        assert_eq!(report.files, 3, "the person's own notes are not looked at");
        let found: Vec<(String, bool)> = report
            .problems
            .iter()
            .map(|p| {
                (
                    p.path.to_string_lossy().replace('\\', "/"),
                    p.message.contains("f-3k9w"),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("people/broken.md".to_owned(), false),
                ("people/marie.md".to_owned(), true),
                ("people/marie.md".to_owned(), false),
            ]
        );
        assert!(
            report.problems[2]
                .message
                .contains("the same id as people/paul.md"),
            "of the same age: the shortest name is kept"
        );
        let copies = copies(dir.path()).unwrap();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].path, Path::new("people/marie.md"));
    }
}

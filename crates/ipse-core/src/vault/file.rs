//! Reading and writing one memory file: front matter and body.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use saphyr_parser::ScalarStyle;

use super::atomic::write_atomic;
use super::emit::{self, Out};
use super::fields::{
    Birthday, Category, Common, DateTime, EntityFields, FileType, Geo, Id, WikiLink,
};
use super::front::{self, RawEntry};
use super::yaml::{self, Node};
use crate::{Error, Result};

const NO_FRONT_MATTER: &str =
    "the file does not start with a front matter block: its first line must be `---`";
const UNCLOSED: &str = "the front matter block is not closed: add a line with only `---` after it";
const NOT_A_MAP: &str = "the front matter must be a list of `key: value` lines";
const NOT_TEXT_KEY: &str = "every key of the front matter must be plain text";
const NOT_UTF8: &str = "the file is not UTF-8 text";
const CANNOT_REWRITE: &str = "Ipse cannot update this file without changing its layout: \
     write each key at the start of its own line, without quotes around the key";

/// What the owned keys were before Ipse changed them.
#[derive(Debug, Clone)]
enum Baseline {
    /// A new file: every owned key is written.
    New,
    /// Read from disk and not changed since: the front matter is written as read.
    Unchanged,
    /// Read from disk, then changed through `common_mut` or `entity_mut`: the values as
    /// read, kept on the first change so that reading stays allocation-light.
    Changed(Box<(Common, Option<EntityFields>)>),
}

/// One memory file of the vault: typed front-matter keys, every other key as written,
/// and the body.
///
/// Reading then writing a file changes only the keys Ipse changed: other keys keep
/// their text, order and comments, and the body is untouched.
#[derive(Debug, Clone)]
pub struct MemoryFile {
    path: PathBuf,
    /// The front matter cut into entries; empty for a new file.
    entries: Vec<RawEntry>,
    trailer: String,
    /// The exact front matter, kept when it could not be cut into entries.
    unsplittable: Option<String>,
    /// The closing line as written, `---\n` for a new file.
    closing: String,
    body: String,
    common: Common,
    entity: Option<EntityFields>,
    baseline: Baseline,
}

impl MemoryFile {
    /// A new file, not yet on disk. `entity` is used only for entity types.
    #[must_use]
    pub fn new(
        path: impl Into<PathBuf>,
        common: Common,
        entity: Option<EntityFields>,
        body: impl Into<String>,
    ) -> Self {
        let entity = if common.file_type.is_entity() {
            Some(entity.unwrap_or_default())
        } else {
            None
        };
        Self {
            path: path.into(),
            entries: Vec::new(),
            trailer: String::new(),
            unsplittable: None,
            closing: "---\n".to_owned(),
            body: body.into(),
            common,
            entity,
            baseline: Baseline::New,
        }
    }

    /// Reads and parses the file at `path`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file cannot be read; [`Error::Invalid`] when it is not
    /// UTF-8 or does not follow the format (see [`MemoryFile::parse`]).
    pub fn read(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let bytes = std::fs::read(&path).map_err(|source| Error::Io {
            action: "read",
            path: path.clone(),
            source,
        })?;
        let text = String::from_utf8(bytes).map_err(|_| invalid(&path, None, NOT_UTF8))?;
        Self::parse(path, &text)
    }

    /// Parses the text of a memory file. `path` is used in error messages and for writing.
    ///
    /// A byte order mark is ignored and `\r\n` line endings are read as `\n`.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`], naming the line when known, when the front matter is missing,
    /// unclosed, not valid YAML, or when a required key is missing or a known key has
    /// the wrong form. Messages never quote the file's content.
    pub fn parse(path: impl Into<PathBuf>, text: &str) -> Result<Self> {
        let path = path.into();
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let text: Cow<'_, str> = if text.contains('\r') {
            Cow::Owned(text.replace("\r\n", "\n"))
        } else {
            Cow::Borrowed(text)
        };

        let (front_text, closing, body) =
            split_document(&text).map_err(|problem| invalid(&path, Some(1), problem))?;

        let root = yaml::parse(front_text)
            .map_err(|e| invalid(&path, e.line.map(|l| l.saturating_add(1)), e.problem))?;
        let map = match root {
            None => Vec::new(),
            Some(Node::Map { entries, .. }) => entries,
            Some(other) => return Err(invalid(&path, file_line(&other), NOT_A_MAP)),
        };

        let mut keys: Vec<(&str, &Node<'_>)> = Vec::with_capacity(map.len());
        for (key, value) in &map {
            let Some(name) = key.as_text() else {
                return Err(invalid(&path, file_line(key), NOT_TEXT_KEY));
            };
            if keys.iter().any(|(k, _)| *k == name) {
                let problem = if is_known_key(name) {
                    format!("`{name}` appears twice")
                } else {
                    "a key appears twice".to_owned()
                };
                return Err(invalid(&path, file_line(key), problem));
            }
            keys.push((name, value));
        }

        let reader = Reader {
            path: &path,
            keys: &keys,
        };
        let common = reader.common()?;
        let entity = if common.file_type.is_entity() {
            Some(reader.entity()?)
        } else {
            None
        };

        let raw = front::split(front_text);
        let splits_cleanly = raw.entries.len() == keys.len()
            && raw.entries.iter().zip(&keys).all(|(e, (k, _))| e.key == *k);
        let (entries, trailer, unsplittable) = if splits_cleanly {
            (raw.entries, raw.trailer, None)
        } else {
            (Vec::new(), String::new(), Some(front_text.to_owned()))
        };

        Ok(Self {
            path,
            entries,
            trailer,
            unsplittable,
            closing: closing.to_owned(),
            body: body.to_owned(),
            common,
            entity,
            baseline: Baseline::Unchanged,
        })
    }

    /// Where the file is, or will be written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The keys every memory file has.
    #[must_use]
    pub fn common(&self) -> &Common {
        &self.common
    }

    /// The keys every memory file has, to change them.
    pub fn common_mut(&mut self) -> &mut Common {
        self.keep_baseline();
        &mut self.common
    }

    /// The entity keys, for entity types only.
    #[must_use]
    pub fn entity(&self) -> Option<&EntityFields> {
        self.entity.as_ref()
    }

    /// The entity keys, to change them. `None` for events, state logs and summaries.
    pub fn entity_mut(&mut self) -> Option<&mut EntityFields> {
        self.keep_baseline();
        self.entity.as_mut()
    }

    fn keep_baseline(&mut self) {
        if matches!(self.baseline, Baseline::Unchanged) {
            self.baseline = Baseline::Changed(Box::new((self.common.clone(), self.entity.clone())));
        }
    }

    /// Everything after the front matter.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Replaces everything after the front matter.
    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = body.into();
    }

    /// The file's text as Ipse would write it.
    ///
    /// Keys Ipse did not change keep their exact text; changed keys are rewritten in
    /// place; new keys are added after the existing ones.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when a key changed but the front matter's layout could not be
    /// mapped to lines (quoted keys, for example), so it cannot be updated safely.
    pub fn render(&self) -> Result<String> {
        let (now, old) = match &self.baseline {
            Baseline::Unchanged => (Vec::new(), None),
            Baseline::New => (emit::canonical(&self.common, self.entity.as_ref()), None),
            Baseline::Changed(old) => (
                emit::canonical(&self.common, self.entity.as_ref()),
                Some(emit::canonical(&old.0, old.1.as_ref())),
            ),
        };
        let changed = |key: &str, value: &Option<Out>| match &old {
            None => matches!(self.baseline, Baseline::New),
            Some(old) => old.iter().find(|(k, _)| *k == key).map(|(_, v)| v) != Some(value),
        };

        let mut out = String::with_capacity(self.body.len() + 512);
        out.push_str("---\n");
        if let Some(front) = &self.unsplittable {
            if now.iter().any(|(k, v)| changed(k, v)) {
                return Err(invalid(&self.path, None, CANNOT_REWRITE));
            }
            out.push_str(front);
        } else {
            for entry in &self.entries {
                out.push_str(&entry.leading);
                match now.iter().find(|(k, _)| *k == entry.key) {
                    Some((key, value)) if changed(key, value) => {
                        if let Some(value) = value {
                            out.push_str(&emit::entry(key, value));
                        }
                    }
                    _ => out.push_str(&entry.text),
                }
            }
            for (key, value) in &now {
                if let Some(value) = value
                    && !self.entries.iter().any(|e| e.key == *key)
                {
                    out.push_str(&emit::entry(key, value));
                }
            }
            out.push_str(&self.trailer);
        }
        out.push_str(&self.closing);
        out.push_str(&self.body);
        Ok(out)
    }

    /// Writes the file atomically, unless the file on disk already has this content
    /// (`\r\n` endings aside). Returns whether it wrote.
    ///
    /// Before writing, the new text is read back: Ipse never writes a file it could not
    /// read again.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] from [`MemoryFile::render`], or if the text would not read
    /// back the same; [`Error::Io`] when reading the current file or writing fails.
    pub fn write(&self) -> Result<bool> {
        let text = self.render()?;
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let current = String::from_utf8_lossy(&bytes);
                let current = current.strip_prefix('\u{feff}').unwrap_or(&current);
                if current == text || current.replace("\r\n", "\n") == text {
                    return Ok(false);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path: self.path.clone(),
                    source,
                });
            }
        }

        let back = Self::parse(&self.path, &text)?;
        if back.common != self.common || back.entity != self.entity || back.body != self.body {
            return Err(invalid(
                &self.path,
                None,
                "Ipse prepared a version of this file that does not read back the same; nothing was written",
            ));
        }
        write_atomic(&self.path, text.as_bytes())?;
        Ok(true)
    }
}

/// Front matter, closing line and body of a document whose line endings are `\n`.
fn split_document(text: &str) -> std::result::Result<(&str, &str, &str), &'static str> {
    let rest = text.strip_prefix("---\n").ok_or(NO_FRONT_MATTER)?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.strip_suffix('\n').unwrap_or(line), "---" | "...") {
            let end = offset + line.len();
            return Ok((&rest[..offset], &rest[offset..end], &rest[end..]));
        }
        offset += line.len();
    }
    Err(UNCLOSED)
}

fn invalid(path: &Path, line: Option<u32>, problem: impl Into<String>) -> Error {
    Error::Invalid {
        path: path.to_owned(),
        line,
        problem: problem.into(),
    }
}

/// Front-matter lines are numbered from the file's second line.
#[allow(clippy::unnecessary_wraps)] // every error site takes an optional line
fn file_line(node: &Node<'_>) -> Option<u32> {
    Some(node.line().saturating_add(1))
}

fn is_known_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "type"
            | "category"
            | "created"
            | "updated"
            | "sources"
            | "name"
            | "aliases"
            | "owner"
            | "birthday"
            | "emails"
            | "phones"
            | "address"
            | "geo"
            | "located_in"
    )
}

/// Reads typed values out of the parsed front matter.
struct Reader<'k, 'n> {
    path: &'k Path,
    keys: &'k [(&'n str, &'k Node<'n>)],
}

impl<'n> Reader<'_, 'n> {
    fn common(&self) -> Result<Common> {
        let id = self.required("id")?;
        let id = Id::parse(self.text_of("id", id)?)
            .ok_or_else(|| self.wrong(id, "`id` must be a lowercase ULID of 26 characters"))?;

        let ty = self.required("type")?;
        let file_type = FileType::parse(self.text_of("type", ty)?).ok_or_else(|| {
            let names: Vec<_> = FileType::ALL.iter().map(|t| t.as_str()).collect();
            self.wrong(ty, format!("`type` must be one of: {}", names.join(", ")))
        })?;

        let cat = self.required("category")?;
        let category = Category::new(self.text_of("category", cat)?).ok_or_else(|| {
            self.wrong(
                cat,
                "`category` must use lowercase letters, digits and dashes",
            )
        })?;

        Ok(Common {
            id,
            file_type,
            category,
            created: self.date_time("created")?,
            updated: self.date_time("updated")?,
            sources: self.list("sources")?,
        })
    }

    fn entity(&self) -> Result<EntityFields> {
        Ok(EntityFields {
            name: self.optional_text("name")?,
            aliases: self.list("aliases")?,
            owner: self.owner()?,
            birthday: self.birthday()?,
            emails: self.list("emails")?,
            phones: self.list("phones")?,
            address: self.optional_text("address")?,
            geo: self.geo()?,
            located_in: self.link("located_in")?,
        })
    }

    /// The value of `key`, unless absent or null.
    fn get(&self, key: &str) -> Option<&'_ Node<'n>> {
        self.keys
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .filter(|v| !v.is_null())
    }

    fn required(&self, key: &str) -> Result<&'_ Node<'n>> {
        self.get(key)
            .ok_or_else(|| invalid(self.path, None, format!("the `{key}` key is missing")))
    }

    fn wrong(&self, node: &Node<'_>, problem: impl Into<String>) -> Error {
        invalid(self.path, file_line(node), problem)
    }

    fn text_of<'v>(&self, key: &str, node: &'v Node<'n>) -> Result<&'v str> {
        node.as_text()
            .ok_or_else(|| self.wrong(node, format!("`{key}` must be text")))
    }

    fn optional_text(&self, key: &str) -> Result<Option<String>> {
        self.get(key)
            .map(|node| self.text_of(key, node).map(str::to_owned))
            .transpose()
    }

    /// A list of text values. A single value is read as a list of one.
    fn list(&self, key: &str) -> Result<Vec<String>> {
        let Some(node) = self.get(key) else {
            return Ok(Vec::new());
        };
        let not_a_list = || self.wrong(node, format!("`{key}` must be a list of text values"));
        match node {
            Node::Scalar { text, .. } => Ok(vec![text.to_string()]),
            Node::Seq { items, .. } => items
                .iter()
                .filter(|item| !item.is_null())
                .map(|item| item.as_text().map(str::to_owned).ok_or_else(not_a_list))
                .collect(),
            Node::Map { .. } => Err(not_a_list()),
        }
    }

    fn date_time(&self, key: &str) -> Result<DateTime> {
        let node = self.required(key)?;
        DateTime::parse(self.text_of(key, node)?).ok_or_else(|| {
            self.wrong(
                node,
                format!(
                    "`{key}` must be a date and time with its offset, like 2026-11-12T19:30:00+01:00"
                ),
            )
        })
    }

    fn owner(&self) -> Result<bool> {
        let Some(node) = self.get("owner") else {
            return Ok(false);
        };
        match node.as_text().map(str::to_ascii_lowercase).as_deref() {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(self.wrong(node, "`owner` must be true or false")),
        }
    }

    fn birthday(&self) -> Result<Option<Birthday>> {
        let Some(node) = self.get("birthday") else {
            return Ok(None);
        };
        node.as_text()
            .and_then(Birthday::parse)
            .map(Some)
            .ok_or_else(|| {
                self.wrong(
                    node,
                    "`birthday` must be a date like 1965-03-12, or --03-12 when the year is unknown",
                )
            })
    }

    fn geo(&self) -> Result<Option<Geo>> {
        let Some(node) = self.get("geo") else {
            return Ok(None);
        };
        let pair = match node {
            Node::Seq { items, .. } if items.len() == 2 => {
                let number = |n: &Node<'_>| n.as_text().and_then(|t| t.parse::<f64>().ok());
                number(&items[0]).zip(number(&items[1]))
            }
            _ => None,
        };
        pair.and_then(|(lat, lon)| Geo::new(lat, lon))
            .map(Some)
            .ok_or_else(|| {
                self.wrong(
                    node,
                    "`geo` must be [latitude, longitude] in decimal degrees",
                )
            })
    }

    /// A wikilink, written `"[[lyon]]"`, or `[[lyon]]` without quotes, which YAML reads
    /// as a list inside a list.
    fn link(&self, key: &str) -> Result<Option<WikiLink>> {
        let Some(node) = self.get(key) else {
            return Ok(None);
        };
        let link = match node {
            Node::Scalar { text, .. } => WikiLink::parse(text),
            Node::Seq { items, .. } => match items.as_slice() {
                [Node::Seq { items, .. }] => match items.as_slice() {
                    [
                        Node::Scalar {
                            text,
                            style: ScalarStyle::Plain,
                            ..
                        },
                    ] => WikiLink::parse(&format!("[[{text}]]")),
                    _ => None,
                },
                _ => None,
            },
            Node::Map { .. } => None,
        };
        link.map(Some)
            .ok_or_else(|| self.wrong(node, format!("`{key}` must be a link like \"[[lyon]]\"")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str = "---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: person
category: people
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
# set by the contacts connector
sources: [contacts:c-1]
name: Élodie Martin
aliases:
  - Lolo
phones:
  - \"0612345678\"
favourite_colour: green   # not an Ipse key
---

## Faits

- Aime l'escalade. ^f-3k9w
";

    fn parse(text: &str) -> Result<MemoryFile> {
        MemoryFile::parse("people/elodie-martin.md", text)
    }

    fn problem(text: &str) -> (Option<u32>, String) {
        match parse(text) {
            Err(Error::Invalid { line, problem, .. }) => (line, problem),
            other => panic!("expected an invalid file, got {other:?}"),
        }
    }

    #[test]
    fn reads_typed_keys() {
        let file = parse(PERSON).unwrap();
        assert_eq!(file.common().file_type, FileType::Person);
        assert_eq!(file.common().sources, ["contacts:c-1"]);
        let entity = file.entity().unwrap();
        assert_eq!(entity.name.as_deref(), Some("Élodie Martin"));
        assert_eq!(entity.aliases, ["Lolo"]);
        assert_eq!(entity.phones, ["0612345678"]);
        assert!(file.body().starts_with("\n## Faits"));
    }

    #[test]
    fn unchanged_file_renders_byte_for_byte() {
        assert_eq!(parse(PERSON).unwrap().render().unwrap(), PERSON);
    }

    #[test]
    fn changing_one_key_keeps_everything_else() {
        let mut file = parse(PERSON).unwrap();
        file.entity_mut().unwrap().phones = vec!["+33612345678".into()];
        let text = file.render().unwrap();
        assert_eq!(
            text,
            PERSON.replace("  - \"0612345678\"\n", "  - \"+33612345678\"\n")
        );
    }

    #[test]
    fn new_keys_are_added_after_existing_ones_and_removed_keys_disappear() {
        let mut file = parse(PERSON).unwrap();
        let entity = file.entity_mut().unwrap();
        entity.emails = vec!["elodie@example.org".into()];
        entity.aliases.clear();
        let text = file.render().unwrap();
        assert!(!text.contains("aliases"));
        assert!(text.contains(
            "favourite_colour: green   # not an Ipse key\nemails:\n  - elodie@example.org\n---\n"
        ));
        assert!(text.contains("# set by the contacts connector\nsources:"));
    }

    #[test]
    fn crlf_and_bom_are_read() {
        let crlf = format!("\u{feff}{}", PERSON.replace('\n', "\r\n"));
        let file = parse(&crlf).unwrap();
        assert_eq!(file.render().unwrap(), PERSON);
    }

    #[test]
    fn new_file_lists_keys_in_order() {
        let common = Common {
            id: Id::parse("01jcx8m2p4q6r8t0v2w4y6z8a0").unwrap(),
            file_type: FileType::Person,
            category: Category::new("people").unwrap(),
            created: DateTime::parse("2026-11-12T19:30:00+01:00").unwrap(),
            updated: DateTime::parse("2026-11-12T19:30:00+01:00").unwrap(),
            sources: vec!["contacts:c-1".into()],
        };
        let entity = EntityFields {
            name: Some("Marc Leroy".into()),
            phones: vec!["0698765432".into()],
            ..EntityFields::default()
        };
        let file = MemoryFile::new("people/marc-leroy.md", common, Some(entity), "\n## Faits\n");
        let text = file.render().unwrap();
        assert_eq!(
            text,
            "---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: person
category: people
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
sources:
  - contacts:c-1
name: Marc Leroy
phones:
  - \"0698765432\"
---

## Faits
"
        );
        let back = parse(&text).unwrap();
        assert_eq!(back.common(), file.common());
        assert_eq!(back.entity(), file.entity());
    }

    #[test]
    fn non_entity_files_keep_entity_like_keys_as_written() {
        let text = "---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: event
category: agenda
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
name: [this is not checked for events]
---
";
        let file = parse(text).unwrap();
        assert!(file.entity().is_none());
        assert_eq!(file.render().unwrap(), text);
    }

    #[test]
    fn errors_name_the_line_and_never_the_content() {
        let secret = "Élodie Martin";
        let cases = [
            (
                "no front matter\n".to_owned(),
                Some(1),
                "first line must be `---`",
            ),
            ("---\nid: x\n".to_owned(), Some(1), "not closed"),
            (
                PERSON.replace("name: Élodie Martin", "name: [Élodie Martin"),
                Some(10),
                "not valid YAML",
            ),
            (
                PERSON.replace("category: people\n", ""),
                None,
                "the `category` key is missing",
            ),
            (
                PERSON.replace("type: person", "type: friend"),
                Some(3),
                "`type` must be one of",
            ),
            (
                PERSON.replace("01jcx8m2p4q6r8t0v2w4y6z8a0", "Élodie Martin"),
                Some(2),
                "lowercase ULID",
            ),
            (
                PERSON.replace("+01:00\nupdated", "\nupdated"),
                Some(5),
                "`created` must be a date and time",
            ),
            (
                PERSON.replace("name: Élodie Martin", "name:\n  first: Élodie Martin"),
                Some(10),
                "`name` must be text",
            ),
            (
                PERSON.replace("  - Lolo", "  - {a: Élodie Martin}"),
                Some(11),
                "`aliases` must be a list",
            ),
            (
                PERSON.replace(
                    "name: Élodie Martin",
                    "name: Élodie Martin\nname: Élodie Martin",
                ),
                Some(10),
                "`name` appears twice",
            ),
        ];
        for (text, line, expected) in cases {
            let (got_line, got) = problem(&text);
            assert!(
                got.contains(expected),
                "{got:?} should contain {expected:?}"
            );
            assert_eq!(got_line, line, "line for {expected:?}");
            assert!(!got.contains(secret), "{got:?} leaks content");
        }
    }

    #[test]
    fn unquoted_wikilinks_are_understood() {
        let text = PERSON.replace(
            "name: Élodie Martin",
            "name: Élodie Martin\nlocated_in: [[lyon]]",
        );
        let file = parse(&text).unwrap();
        assert_eq!(
            file.entity().unwrap().located_in.as_ref().unwrap().target,
            "lyon"
        );
    }

    #[test]
    fn unsplittable_front_matter_is_read_but_not_rewritten() {
        let text = PERSON.replace("favourite_colour: green", "\"favourite colour\": green");
        let mut file = parse(&text).unwrap();
        assert_eq!(file.render().unwrap(), text, "unchanged: written as is");
        file.entity_mut().unwrap().name = Some("Élodie M.".into());
        assert!(matches!(file.render(), Err(Error::Invalid { .. })));
    }
}

//! Reading and writing memory files as they sit on disk.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use ipse_core::time::TimeZone;
use ipse_core::vault::{Common, EntityFields, FileType, MemoryFile, naming};

const SPEC: &str = include_str!("../../../docs/spec/vault.md");

/// Every complete memory file shown in the spec: Markdown code blocks starting with `---`.
fn spec_files() -> Vec<String> {
    let mut files = Vec::new();
    let mut current: Option<String> = None;
    for line in SPEC.lines() {
        match (line, &mut current) {
            ("```markdown", None) => current = Some(String::new()),
            ("```", Some(text)) => {
                if text.starts_with("---\n") {
                    files.push(std::mem::take(text));
                }
                current = None;
            }
            (line, Some(text)) => {
                text.push_str(line);
                text.push('\n');
            }
            _ => {}
        }
    }
    files
}

#[test]
fn every_spec_example_reads_and_renders_byte_for_byte() {
    let files = spec_files();
    assert!(
        files.len() >= 2,
        "the spec should show an event and a state file"
    );
    for text in files {
        let file = MemoryFile::parse("example.md", &text)
            .unwrap_or_else(|e| panic!("spec example does not read: {e}\n{text}"));
        assert_eq!(file.render().unwrap(), text);
    }
}

#[test]
fn write_skips_unchanged_files_and_writes_changes() {
    let vault = tempfile::tempdir().unwrap();
    let path = vault.path().join("people/ana-silva.md");
    let text = "---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: person
category: people
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
name: Ana Silva
---
Notes written by hand.
";
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();

    let mut file = MemoryFile::read(&path).unwrap();
    assert!(!file.write().unwrap(), "nothing changed: nothing written");

    file.entity_mut().unwrap().emails = vec!["ana@example.org".into()];
    assert!(file.write().unwrap());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("name: Ana Silva\nemails:\n  - ana@example.org\n---\n"));
    assert!(written.ends_with("Notes written by hand.\n"));
}

#[test]
fn crlf_file_unchanged_is_not_rewritten() {
    let vault = tempfile::tempdir().unwrap();
    let path = vault.path().join("note.md");
    let text = "---\r\nid: 01jcx8m2p4q6r8t0v2w4y6z8a0\r\ntype: topic\r\ncategory: media\r\n\
                created: 2026-11-12T19:30:00+01:00\r\nupdated: 2026-11-12T19:30:00+01:00\r\n---\r\nJazz\r\n";
    std::fs::write(&path, text).unwrap();
    let file = MemoryFile::read(&path).unwrap();
    assert!(!file.write().unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn new_entity_is_named_written_and_read_back() {
    let vault = tempfile::tempdir().unwrap();
    let tz = TimeZone::get("Europe/Paris").unwrap();
    let common = Common::new(FileType::Person, &tz).unwrap();
    let entity = EntityFields {
        name: Some("Léa Moreau".into()),
        phones: vec!["0611223344".into()],
        ..EntityFields::default()
    };
    let path = naming::entity_path(
        vault.path(),
        FileType::Person,
        entity.name.as_deref(),
        &common.id,
    )
    .unwrap();
    assert!(path.ends_with("people/lea-moreau.md"));

    let file = MemoryFile::new(&path, common, Some(entity), "\n## Faits\n");
    assert!(file.write().unwrap());
    let back = MemoryFile::read(&path).unwrap();
    assert_eq!(back.common(), file.common());
    assert_eq!(back.entity(), file.entity());
    assert_eq!(back.body(), "\n## Faits\n");
}

#[test]
fn non_utf8_file_is_reported() {
    let vault = tempfile::tempdir().unwrap();
    let path = vault.path().join("bad.md");
    std::fs::write(&path, b"---\nname: \xff\n---\n").unwrap();
    let err = MemoryFile::read(&path).unwrap_err();
    assert!(err.to_string().contains("not UTF-8"));
}

/// The spec's body snippets: Markdown code blocks starting with a `## ` heading.
fn spec_snippets() -> Vec<String> {
    let mut snippets = Vec::new();
    let mut current: Option<String> = None;
    for line in SPEC.lines() {
        match (line, &mut current) {
            ("```markdown", None) => current = Some(String::new()),
            ("```", Some(text)) => {
                if text.starts_with("## ") {
                    snippets.push(std::mem::take(text));
                }
                current = None;
            }
            (line, Some(text)) => {
                text.push_str(line);
                text.push('\n');
            }
            _ => {}
        }
    }
    snippets
}

#[test]
fn spec_facts_and_relations_are_read() {
    let snippets = spec_snippets();
    let all: String = snippets.concat();
    let body = ipse_core::vault::body::parse(&all);
    assert_eq!(body.facts.len(), 3, "the spec shows three facts");
    assert_eq!(body.facts.iter().filter(|f| f.id.is_some()).count(), 2);
    let targets: Vec<_> = body
        .relations
        .iter()
        .map(|r| r.link.target.as_str())
        .collect();
    assert_eq!(targets, ["marie-dupont", "hopital-pasteur", "lyon"]);
}

#[test]
fn new_entity_with_sections_reads_back() {
    use ipse_core::vault::{Body, Fact, Language, Relation, WikiLink, body};

    let vault = tempfile::tempdir().unwrap();
    let common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    let sections = Body {
        facts: vec![Fact {
            text: "Works as a nurse.".into(),
            id: None,
        }],
        relations: vec![Relation {
            word: "mother".into(),
            link: WikiLink::parse("[[marie-dupont]]").unwrap(),
        }],
        notes: None,
    };
    let path = vault.path().join("people/zoe-bernard.md");
    let file = MemoryFile::new(
        &path,
        common,
        Some(EntityFields {
            name: Some("Zoé Bernard".into()),
            ..EntityFields::default()
        }),
        body::render(&sections, Language::En),
    );
    assert!(file.write().unwrap());

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("name: Zoé Bernard\n---\n\n## Facts\n\n- Works as a nurse. ^f-"),
        "{text}"
    );
    assert!(text.contains("\n\n## Relations\n\n- mother: [[marie-dupont]]\n"));
    assert!(!text.contains("## Notes"), "empty sections are left out");
    let back = MemoryFile::read(&path).unwrap().sections();
    assert_eq!(back.relations, sections.relations);
    assert_eq!(back.facts.len(), 1);
    assert_eq!(back.facts[0].text, "Works as a nurse.");
    assert!(back.facts[0].id.is_some(), "an id is added on write");
}

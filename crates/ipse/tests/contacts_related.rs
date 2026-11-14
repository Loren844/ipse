//! Related names on the owner's own contact card become relations of the owner's file
//! (#60). All contacts here are invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn ipse(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(args)
        .output()
        .expect("the ipse binary runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/contacts/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// A French memory whose owner's file holds `keys` (front matter lines).
fn memory(keys: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    ipse(&[
        "init",
        vault,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    let owner = owner_path(dir.path());
    let text = std::fs::read_to_string(&owner).unwrap();
    std::fs::write(
        &owner,
        text.replace("owner: true\n", &format!("{keys}owner: true\n")),
    )
    .unwrap();
    dir
}

fn owner_path(root: &Path) -> std::path::PathBuf {
    root.join("people/camille-durand.md")
}

fn owner(root: &Path) -> String {
    std::fs::read_to_string(owner_path(root)).unwrap()
}

fn import(root: &Path, file: &str) -> String {
    ipse(&["import", &fixture(file), "--vault", root.to_str().unwrap()])
}

#[test]
fn an_apple_card_names_mother_and_godfather() {
    let dir = memory("emails:\n  - camille.durand@example.org\n");
    let said = import(dir.path(), "apple-related.vcf");
    assert!(
        said.contains("2 relations added to your own file"),
        "{said}"
    );
    let text = owner(dir.path());
    assert!(
        text.contains("## Relations\n\n- mère : [[maman]]\n- Parrain : [[paul-bernard]]\n"),
        "{text}"
    );
    assert!(
        !text.contains("[[lea"),
        "two people are named Léa: no relation\n{text}"
    );
    assert!(
        !text.contains("Personne Inconnue"),
        "nobody by that name: nothing\n{text}"
    );
    assert!(
        text.contains("<!-- Écrivez ici"),
        "the rest of the file is kept\n{text}"
    );

    // The person removes a relation: importing again does not add it back.
    std::fs::write(
        owner_path(dir.path()),
        text.replace("- Parrain : [[paul-bernard]]\n", ""),
    )
    .unwrap();
    let again = import(dir.path(), "apple-related.vcf");
    assert!(!again.contains("relation"), "{again}");
    let after = owner(dir.path());
    assert!(after.contains("- mère : [[maman]]\n"));
    assert!(!after.contains("paul-bernard"), "{after}");
}

#[test]
fn a_google_card_found_by_phone_with_typed_labels() {
    let dir = memory("phones:\n  - 06 98 76 54 32\n");
    let said = import(dir.path(), "google-related.vcf");
    assert!(said.contains("2 relations added"), "{said}");
    let text = owner(dir.path());
    assert!(text.contains("- frère : [[tom-durand]]\n"), "{text}");
    assert!(
        text.contains("- conjoint : [[samuel-petit]]\n"),
        "found by their other name: {text}"
    );
}

#[test]
fn a_vcard_4_card_points_to_another_card() {
    let dir = memory("emails:\n  - camille.durand@example.org\n");
    import(dir.path(), "related-v4.vcf");
    let text = owner(dir.path());
    assert!(text.contains("- parent : [[papa]]\n"), "{text}");
    assert!(text.contains("- collègue : [[marc-leroy]]\n"), "{text}");
}

#[test]
fn without_the_owners_email_or_phone_nothing_is_guessed() {
    let dir = memory("");
    let said = import(dir.path(), "apple-related.vcf");
    assert!(!said.contains("relation"), "{said}");
    assert!(!owner(dir.path()).contains("## Relations"));
}

#[test]
fn a_relation_the_person_wrote_is_not_doubled() {
    let dir = memory("emails:\n  - camille.durand@example.org\n");
    let owner_file = owner_path(dir.path());
    let text = std::fs::read_to_string(&owner_file).unwrap();
    std::fs::write(
        &owner_file,
        format!("{text}\n## Relations\n\n- maman : [[maman]]\n"),
    )
    .unwrap();
    import(dir.path(), "apple-related.vcf");
    let after = owner(dir.path());
    assert_eq!(after.matches("[[maman]]").count(), 1, "{after}");
    assert!(
        after.contains("- maman : [[maman]]\n- Parrain : [[paul-bernard]]\n"),
        "{after}"
    );
}

//! The body of an entity file: facts, relations and notes (`docs/spec/vault.md`,
//! sections 2.5, 3.2 and 3.3).
//!
//! Sections are level-2 headings. Ipse recognizes the headings of every supported
//! language, whatever the vault's language, so changing the language never breaks a
//! vault. Text outside known sections is ignored here and left untouched in the file.

use std::collections::HashSet;
use std::fmt;

use super::fields::WikiLink;
use super::lang::Language;
use crate::slug::to_file_stem;

/// A section Ipse knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Section {
    /// One bullet line per fact.
    Facts,
    /// One bullet line per relation: `word : [[target]]`.
    Relations,
    /// Free text, never rewritten by Ipse.
    Notes,
    /// Generated list of events.
    Events,
    /// A summary's text.
    Summary,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::Facts,
        Self::Relations,
        Self::Notes,
        Self::Events,
        Self::Summary,
    ];

    /// The heading text in `language`, without `## `.
    #[must_use]
    pub fn heading(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Facts, Language::Fr) => "Faits",
            (Self::Facts, _) => "Facts",
            (Self::Relations, _) => "Relations",
            (Self::Notes, _) => "Notes",
            (Self::Events, Language::Fr) => "Événements",
            (Self::Events, _) => "Events",
            (Self::Summary, Language::Fr) => "Résumé",
            (Self::Summary, _) => "Summary",
        }
    }

    /// The section a heading names, in any supported language, ignoring case and accents.
    #[must_use]
    pub fn from_heading(text: &str) -> Option<Self> {
        let folded = to_file_stem(text)?;
        Self::ALL.into_iter().find(|section| {
            Language::ALL
                .iter()
                .any(|&l| to_file_stem(section.heading(l)).as_deref() == Some(folded.as_str()))
        })
    }
}

/// The identifier Ipse gives a fact: `f-` and 4 lowercase Crockford base 32 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FactId(String);

impl FactId {
    /// Reads `f-3k9w` (without the `^`). Returns `None` for any other form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let code = text.strip_prefix("f-")?;
        let valid = code.len() == 4
            && code
                .bytes()
                .all(|b| b"0123456789abcdefghjkmnpqrstvwxyz".contains(&b));
        valid.then(|| Self(text.to_owned()))
    }

    /// The identifier, without the `^`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One fact: a sentence written for a human, and its identifier when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// The fact's text, without the bullet and the identifier.
    pub text: String,
    /// The `^f-xxxx` identifier, if the line has one.
    pub id: Option<FactId>,
}

/// One relation: a relation word and the entity it points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    /// The relation word as written: `mère`, `works at`.
    pub word: String,
    /// The linked entity.
    pub link: WikiLink,
}

/// The fixed vocabulary relation words are mapped to, for the index
/// (`docs/spec/vault.md`, section 3.3). A word outside it is a free relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RelationKind {
    /// Mother, father.
    Parent,
    /// Son, daughter.
    Child,
    /// Brother, sister.
    Sibling,
    /// Spouse, partner.
    Partner,
    /// Friend.
    Friend,
    /// Colleague.
    Colleague,
    /// Works at an organization.
    WorksAt,
    /// Lives in a place.
    LivesIn,
    /// Member of an organization.
    MemberOf,
    /// Owns a thing.
    Owns,
    /// A place inside another.
    LocatedIn,
    /// A part of a larger whole: a sub-project, a room of a home.
    PartOf,
}

/// Relation words, as file stems (no case, accents or punctuation), by kind.
const KINDS: [(RelationKind, &[&str]); 12] = [
    (
        RelationKind::Parent,
        &[
            "parent", "mere", "pere", "maman", "papa", "mother", "father", "mom", "mum", "dad",
        ],
    ),
    (
        RelationKind::Child,
        &["enfant", "fils", "fille", "child", "son", "daughter"],
    ),
    (
        RelationKind::Sibling,
        &[
            "frere",
            "soeur",
            "frere-ou-soeur",
            "brother",
            "sister",
            "sibling",
        ],
    ),
    (
        RelationKind::Partner,
        &[
            "conjoint",
            "conjointe",
            "mari",
            "epoux",
            "epouse",
            "femme",
            "compagnon",
            "compagne",
            "partenaire",
            "spouse",
            "husband",
            "wife",
            "partner",
            "boyfriend",
            "girlfriend",
        ],
    ),
    (
        RelationKind::Friend,
        &["ami", "amie", "copain", "copine", "pote", "friend"],
    ),
    (
        RelationKind::Colleague,
        &["collegue", "colleague", "coworker", "co-worker"],
    ),
    (
        RelationKind::WorksAt,
        &[
            "travaille-chez",
            "travaille-a",
            "travaille-pour",
            "employe-chez",
            "employee-chez",
            "works-at",
            "works-for",
            "employed-by",
        ],
    ),
    (
        RelationKind::LivesIn,
        &["vit-a", "habite", "habite-a", "lives-in", "lives-at"],
    ),
    (
        RelationKind::MemberOf,
        &["membre-de", "adherent-de", "adherente-de", "member-of"],
    ),
    (
        RelationKind::Owns,
        &["possede", "proprietaire-de", "owns", "owner-of"],
    ),
    (
        RelationKind::LocatedIn,
        &[
            "situe-a",
            "situe-dans",
            "se-trouve-a",
            "se-trouve-dans",
            "located-in",
            "is-in",
        ],
    ),
    (
        RelationKind::PartOf,
        &["fait-partie-de", "partie-de", "part-of", "belongs-to"],
    ),
];

impl RelationKind {
    /// The kind a relation word stands for, in French or English, ignoring case and
    /// accents; `None` for a free relation.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        let stem = to_file_stem(word)?;
        KINDS
            .iter()
            .find(|(_, words)| words.contains(&stem.as_str()))
            .map(|(kind, _)| *kind)
    }

    /// The kind as written in the index: `parent`, `works_at`…
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parent => "parent",
            Self::Child => "child",
            Self::Sibling => "sibling",
            Self::Partner => "partner",
            Self::Friend => "friend",
            Self::Colleague => "colleague",
            Self::WorksAt => "works_at",
            Self::LivesIn => "lives_in",
            Self::MemberOf => "member_of",
            Self::Owns => "owns",
            Self::LocatedIn => "located_in",
            Self::PartOf => "part_of",
        }
    }
}

impl Relation {
    /// The fixed kind of this relation's word, if it has one.
    #[must_use]
    pub fn kind(&self) -> Option<RelationKind> {
        RelationKind::of_word(&self.word)
    }
}

/// What Ipse reads from an entity's body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Body {
    /// The `Facts` section, in order.
    pub facts: Vec<Fact>,
    /// The `Relations` section, in order. Lines that are not `word : [[link]]` are skipped.
    pub relations: Vec<Relation>,
    /// The `Notes` section, as written, without surrounding blank lines.
    pub notes: Option<String>,
}

impl Body {
    /// True when there is nothing to write.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.relations.is_empty() && self.notes.is_none()
    }
}

/// Reads the known sections of a body. Never fails: anything Ipse does not recognize is
/// skipped.
#[must_use]
pub fn parse(body: &str) -> Body {
    let mut out = Body::default();
    let mut section: Option<Section> = None;
    let mut fence: Option<&str> = None;
    let mut in_comment = false;
    let mut notes = String::new();

    for line in body.lines() {
        let trimmed = line.trim_start();
        // HTML comments (`<!-- … -->`) are hints for the reader: never facts or relations.
        if in_comment || trimmed.starts_with("<!--") {
            let closes = trimmed.contains("-->")
                && (in_comment || trimmed.find("-->") > trimmed.find("<!--"));
            in_comment = !closes;
            if section == Some(Section::Notes) {
                push_line(&mut notes, line);
            }
            continue;
        }
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            if section == Some(Section::Notes) {
                push_line(&mut notes, line);
            }
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            section = Section::from_heading(heading);
            continue;
        }
        if line.starts_with("# ") {
            section = None;
            continue;
        }
        for marker in ["```", "~~~"] {
            if trimmed.starts_with(marker) {
                fence = Some(marker);
            }
        }
        match section {
            Some(Section::Facts) => read_fact(&mut out.facts, line),
            Some(Section::Relations) => {
                if let Some(relation) = bullet(line).and_then(read_relation) {
                    out.relations.push(relation);
                }
            }
            Some(Section::Notes) => push_line(&mut notes, line),
            _ => {}
        }
    }

    let notes = notes.trim_matches('\n');
    if !notes.trim().is_empty() {
        out.notes = Some(notes.to_owned());
    }
    out
}

fn push_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

/// The text after a `- `, `* ` or `+ ` bullet at the start of the line.
fn bullet(line: &str) -> Option<&str> {
    ["- ", "* ", "+ "]
        .into_iter()
        .find_map(|marker| line.strip_prefix(marker))
        .map(str::trim)
}

/// A bullet starts a fact; an indented line continues the previous one.
fn read_fact(facts: &mut Vec<Fact>, line: &str) {
    if let Some(text) = bullet(line) {
        if !text.is_empty() {
            facts.push(split_fact_id(text));
        }
    } else if line.starts_with([' ', '\t'])
        && let Some(last) = facts.last_mut()
    {
        let more = line.trim();
        if !more.is_empty() {
            let mut text = std::mem::take(&mut last.text);
            text.push(' ');
            text.push_str(more);
            if let Some(id) = &last.id {
                text.push_str(" ^");
                text.push_str(id.as_str());
            }
            *last = split_fact_id(&text);
        }
    }
}

fn split_fact_id(text: &str) -> Fact {
    if let Some((before, after)) = text.rsplit_once(" ^")
        && let Some(id) = FactId::parse(after)
    {
        return Fact {
            text: before.trim_end().to_owned(),
            id: Some(id),
        };
    }
    Fact {
        text: text.to_owned(),
        id: None,
    }
}

/// `word : [[target]]` or `word: [[target|display]]`.
fn read_relation(text: &str) -> Option<Relation> {
    let open = text.find("[[")?;
    let word = text[..open].trim_end().strip_suffix(':')?.trim();
    let close = text[open..].find("]]")? + open + 2;
    let link = WikiLink::parse(&text[open..close])?;
    (!word.is_empty()).then(|| Relation {
        word: word.to_owned(),
        link,
    })
}

/// The body of a new entity file: `Facts`, `Relations` and `Notes` in `language`,
/// leaving out empty sections. Starts with a blank line, after the front matter.
#[must_use]
pub fn render(body: &Body, language: Language) -> String {
    let mut out = String::new();
    if !body.facts.is_empty() {
        open_section(&mut out, Section::Facts, language);
        for fact in &body.facts {
            out.push_str("- ");
            out.push_str(&one_line(&fact.text));
            if let Some(id) = &fact.id {
                out.push_str(" ^");
                out.push_str(id.as_str());
            }
            out.push('\n');
        }
    }
    if !body.relations.is_empty() {
        open_section(&mut out, Section::Relations, language);
        let separator = match language {
            Language::Fr => " : ",
            _ => ": ",
        };
        for relation in &body.relations {
            out.push_str("- ");
            out.push_str(&one_line(&relation.word));
            out.push_str(separator);
            out.push_str(&relation.link.to_string());
            out.push('\n');
        }
    }
    if let Some(notes) = &body.notes {
        open_section(&mut out, Section::Notes, language);
        out.push_str(notes.trim_matches('\n'));
        out.push('\n');
    }
    out
}

/// The body of the owner's new file: an empty facts section with a hint, so that the
/// person sees where to write.
#[must_use]
pub fn owner_hint(language: Language) -> String {
    let hint = match language {
        Language::Fr => {
            "<!-- Écrivez ici ce que votre IA doit savoir sur vous, une ligne par fait, par exemple : - Allergique aux noix. -->"
        }
        _ => {
            "<!-- Write here what your AI should know about you, one line per fact, for example: - Allergic to nuts. -->"
        }
    };
    let mut out = String::new();
    open_section(&mut out, Section::Facts, language);
    out.push_str(hint);
    out.push('\n');
    out
}

/// The fact identifiers that appear more than once in `body`. Each needs another id:
/// proposals and history point at a fact by it.
#[must_use]
pub fn duplicate_fact_ids(body: &Body) -> Vec<FactId> {
    let mut seen = HashSet::new();
    let mut twice: Vec<FactId> = Vec::new();
    for id in body.facts.iter().filter_map(|f| f.id.as_ref()) {
        if !seen.insert(id.as_str()) && !twice.contains(id) {
            twice.push(id.clone());
        }
    }
    twice
}

/// `body` with an identifier added to every fact that has none, at the end of its last
/// line; every other byte is kept. `None` when every fact already has one. New
/// identifiers are random and unique in the body.
#[must_use]
pub fn with_fact_ids(body: &str) -> Option<String> {
    with_fact_ids_from(body, random_code)
}

/// As [`with_fact_ids`], with the 4-character codes drawn from `code`.
fn with_fact_ids_from(body: &str, mut code: impl FnMut() -> String) -> Option<String> {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let last_lines = facts_without_id(&lines);
    if last_lines.is_empty() {
        return None;
    }
    let mut taken: HashSet<String> = parse(body)
        .facts
        .iter()
        .filter_map(|f| f.id.as_ref().map(|id| id.as_str().to_owned()))
        .collect();
    let mut out = String::with_capacity(body.len() + last_lines.len() * 8);
    for (i, line) in lines.iter().enumerate() {
        if !last_lines.contains(&i) {
            out.push_str(line);
            continue;
        }
        let content = line.trim_end_matches(['\n', '\r']);
        let ending = &line[content.len()..];
        let id = loop {
            let id = format!("f-{}", code());
            if FactId::parse(&id).is_some() && taken.insert(id.clone()) {
                break id;
            }
        };
        out.push_str(content);
        out.push_str(" ^");
        out.push_str(&id);
        out.push_str(ending);
    }
    Some(out)
}

/// The last line of each fact without an identifier, in `Facts` sections, outside code
/// blocks and comments.
fn facts_without_id(lines: &[&str]) -> HashSet<usize> {
    let mut found = HashSet::new();
    let mut section: Option<Section> = None;
    let mut fence: Option<&str> = None;
    let mut in_comment = false;
    // The last line of the fact being read, and whether it has an id.
    let mut open: Option<(usize, bool)> = None;
    let mut close = |open: &mut Option<(usize, bool)>| {
        if let Some((last, false)) = open.take() {
            found.insert(last);
        }
    };
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        if in_comment || trimmed.starts_with("<!--") {
            let closes = trimmed.contains("-->")
                && (in_comment || trimmed.find("-->") > trimmed.find("<!--"));
            in_comment = !closes;
            close(&mut open);
            continue;
        }
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            close(&mut open);
            section = Section::from_heading(heading);
            continue;
        }
        if line.starts_with("# ") {
            close(&mut open);
            section = None;
            continue;
        }
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            close(&mut open);
            fence = Some(marker);
            continue;
        }
        if section != Some(Section::Facts) {
            continue;
        }
        if let Some(text) = bullet(line) {
            close(&mut open);
            if !text.is_empty() {
                open = Some((i, split_fact_id(text).id.is_some()));
            }
        } else if line.starts_with([' ', '\t']) && !trimmed.is_empty() {
            // A fact goes on on an indented line; its id may be on any of its lines.
            if let Some((_, has_id)) = open {
                open = Some((i, has_id || split_fact_id(trimmed).id.is_some()));
            }
        } else {
            close(&mut open);
        }
    }
    close(&mut open);
    found
}

/// 4 random characters of lowercase Crockford base 32.
fn random_code() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut bytes = [0_u8; 4];
    // Without randomness, a time-based value still differs between files; uniqueness
    // in the file is checked by the caller.
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        bytes = nanos.to_le_bytes();
    }
    bytes
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(b % 32)]))
        .collect()
}

/// The text of `section` in `body`, in any supported language, as written, without
/// surrounding blank lines. `None` when the body has no such section or it is empty.
#[must_use]
pub fn section_text(body: &str, section: Section) -> Option<String> {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let headings = headings(&lines);
    let at = headings.iter().position(|(_, s)| *s == Some(section))?;
    let start = headings[at].0 + 1;
    let end = headings.get(at + 1).map_or(lines.len(), |(i, _)| *i);
    let text: String = lines[start..end].concat();
    let text = text.trim_matches('\n').trim_end();
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// `body` with `relations` added at the end of its `Relations` section, in any
/// language. Without one, a section is added before `Notes`, or at the end. Everything
/// else is kept byte for byte.
#[must_use]
pub fn add_relations(body: &str, relations: &[Relation], language: Language) -> String {
    if relations.is_empty() {
        return body.to_owned();
    }
    let separator = match language {
        Language::Fr => " : ",
        _ => ": ",
    };
    let mut new_lines = String::new();
    for relation in relations {
        new_lines.push_str("- ");
        new_lines.push_str(&one_line(&relation.word));
        new_lines.push_str(separator);
        new_lines.push_str(&relation.link.to_string());
        new_lines.push('\n');
    }

    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let headings = headings(&lines);
    let position = |section| headings.iter().position(|(_, s)| *s == Some(section));
    let mut out = String::with_capacity(body.len() + new_lines.len() + 32);
    if let Some(at) = position(Section::Relations) {
        let start = headings[at].0 + 1;
        let end = headings.get(at + 1).map_or(lines.len(), |(i, _)| *i);
        // After the section's last line that is not blank.
        let insert = (start..end)
            .rev()
            .find(|&i| !lines[i].trim().is_empty())
            .map_or(start, |i| i + 1);
        lines[..insert].iter().for_each(|l| out.push_str(l));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if insert == start {
            out.push('\n');
        }
        out.push_str(&new_lines);
        if insert == end && end < lines.len() {
            out.push('\n');
        }
        lines[insert..].iter().for_each(|l| out.push_str(l));
    } else if let Some(at) = position(Section::Notes) {
        let notes = headings[at].0;
        lines[..notes].iter().for_each(|l| out.push_str(l));
        out.push_str("## ");
        out.push_str(Section::Relations.heading(language));
        out.push_str("\n\n");
        out.push_str(&new_lines);
        out.push('\n');
        lines[notes..].iter().for_each(|l| out.push_str(l));
    } else {
        out.push_str(body);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        open_section(&mut out, Section::Relations, language);
        out.push_str(&new_lines);
    }
    out
}

/// The level-1 and level-2 headings of a body, by line, outside code blocks and
/// comments, with the section each names.
fn headings(lines: &[&str]) -> Vec<(usize, Option<Section>)> {
    let mut found = Vec::new();
    let mut fence: Option<&str> = None;
    let mut in_comment = false;
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        if in_comment || trimmed.starts_with("<!--") {
            let closes = trimmed.contains("-->")
                && (in_comment || trimmed.find("-->") > trimmed.find("<!--"));
            in_comment = !closes;
            continue;
        }
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            found.push((i, Section::from_heading(heading)));
        } else if line.starts_with("# ") {
            found.push((i, None));
        } else if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
        }
    }
    found
}

fn open_section(out: &mut String, section: Section, language: Language) {
    out.push_str("\n## ");
    out.push_str(section.heading(language));
    out.push_str("\n\n");
}

/// Facts and relation words are one line each.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FR: &str = "
Texte libre avant les sections.

## Faits

- Allergique aux noix. ^f-3k9w
- Travaille comme infirmière à l'hôpital Pasteur depuis 2019. ^f-8m2p
- Préfère être appelée le soir.

## Relations

- mère : [[marie-dupont]]
- travaille chez : [[hopital-pasteur|l'hôpital]]
- pas une relation
- vit à : [[lyon]] (depuis 2020)

## Notes

Rencontrée au club.

```
## Faits
- pas un fait
```

## Une section inconnue

- ignorée
";

    #[test]
    fn reads_the_french_example() {
        let body = parse(FR);
        assert_eq!(body.facts.len(), 3);
        assert_eq!(body.facts[0].text, "Allergique aux noix.");
        assert_eq!(
            body.facts[0].id.as_ref().map(FactId::as_str),
            Some("f-3k9w")
        );
        assert_eq!(body.facts[2].text, "Préfère être appelée le soir.");
        assert_eq!(body.facts[2].id, None);

        let words: Vec<_> = body.relations.iter().map(|r| r.word.as_str()).collect();
        assert_eq!(words, ["mère", "travaille chez", "vit à"]);
        assert_eq!(body.relations[1].link.target, "hopital-pasteur");
        assert_eq!(body.relations[1].link.display.as_deref(), Some("l'hôpital"));
        assert_eq!(body.relations[2].link.target, "lyon");

        let notes = body.notes.unwrap();
        assert!(notes.starts_with("Rencontrée au club."));
        assert!(notes.contains("- pas un fait"), "code blocks stay in notes");
    }

    #[test]
    fn reads_english_headings_in_any_vault() {
        let body = parse("## Facts\n\n- Likes jazz.\n\n## Relations\n\n- sister: [[julie]]\n");
        assert_eq!(body.facts[0].text, "Likes jazz.");
        assert_eq!(body.relations[0].word, "sister");
    }

    #[test]
    fn headings_ignore_case_and_accents() {
        assert_eq!(Section::from_heading("faits"), Some(Section::Facts));
        assert_eq!(Section::from_heading("EVENEMENTS"), Some(Section::Events));
        assert_eq!(Section::from_heading("Résumé "), Some(Section::Summary));
        assert_eq!(Section::from_heading("Recettes"), None);
    }

    #[test]
    fn indented_lines_continue_a_fact() {
        let body = parse("## Faits\n\n- Joue du piano\n  depuis l'enfance. ^f-a1b2\n");
        assert_eq!(body.facts[0].text, "Joue du piano depuis l'enfance.");
        assert!(body.facts[0].id.is_some());
    }

    #[test]
    fn a_user_block_id_stays_in_the_text() {
        let body = parse("## Faits\n\n- Aime le thé. ^mon-bloc\n");
        assert_eq!(body.facts[0].text, "Aime le thé. ^mon-bloc");
        assert_eq!(body.facts[0].id, None);
    }

    #[test]
    fn html_comments_are_never_read_as_facts() {
        let body = parse(
            "## Faits\n\n<!-- Par exemple : - Allergique aux noix. -->\n<!--\n- caché\n-->\n- Vrai fait.\n",
        );
        let texts: Vec<_> = body.facts.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, ["Vrai fait."]);
    }

    #[test]
    fn no_sections_at_all() {
        assert!(parse("Juste une note.\n- avec une puce\n").is_empty());
        assert!(parse("").is_empty());
    }

    #[test]
    fn odd_markdown_never_panics() {
        for text in [
            "## ",
            "##",
            "## Faits\n-",
            "## Faits\n- ^f-",
            "## Relations\n- : [[",
            "## Relations\n- a : ]][[b",
            "```\n## Faits\n- x",
            "## Notes\n\n\n",
            "\u{feff}## Faits\n- é ^f-zzzz",
        ] {
            let _ = parse(text);
        }
    }

    #[test]
    fn renders_in_the_vault_language_and_reads_back() {
        let body = Body {
            facts: vec![
                Fact {
                    text: "Allergique aux noix.".into(),
                    id: FactId::parse("f-3k9w"),
                },
                Fact {
                    text: "Préfère le soir.".into(),
                    id: None,
                },
            ],
            relations: vec![Relation {
                word: "mère".into(),
                link: WikiLink::parse("[[marie-dupont]]").unwrap(),
            }],
            notes: Some("Rencontrée au club.".into()),
        };
        let text = render(&body, Language::Fr);
        assert_eq!(
            text,
            "\n## Faits\n\n- Allergique aux noix. ^f-3k9w\n- Préfère le soir.\n\
             \n## Relations\n\n- mère : [[marie-dupont]]\n\
             \n## Notes\n\nRencontrée au club.\n"
        );
        assert_eq!(parse(&text), body);
        assert!(render(&body, Language::En).contains("## Facts\n\n- Allergique"));
    }

    #[test]
    fn empty_sections_are_left_out() {
        let body = Body {
            notes: Some("Seulement des notes.".into()),
            ..Body::default()
        };
        assert_eq!(
            render(&body, Language::Fr),
            "\n## Notes\n\nSeulement des notes.\n"
        );
        assert_eq!(render(&Body::default(), Language::Fr), "");
    }

    fn relation(word: &str, target: &str) -> Relation {
        Relation {
            word: word.into(),
            link: WikiLink::parse(&format!("[[{target}]]")).unwrap(),
        }
    }

    #[test]
    fn missing_fact_ids_are_added_and_nothing_else_changes() {
        let body = "Texte libre.\r\n\r\n## Faits\r\n\r\n- Aime le thé. ^f-3k9w\r\n- Joue du piano\r\n  depuis 2010.\r\n* Habite Lyon.   \r\n- \r\n<!-- - Un exemple, pas un fait. -->\r\n```\r\n- dans un bloc de code\r\n```\r\n\r\n## Notes\r\n\r\n- une note, pas un fait\r\n";
        let mut codes = ["3k9w", "a2b4", "zz00"].into_iter().map(str::to_owned);
        let out = with_fact_ids_from(body, || codes.next().unwrap()).unwrap();
        assert_eq!(
            out,
            "Texte libre.\r\n\r\n## Faits\r\n\r\n- Aime le thé. ^f-3k9w\r\n- Joue du piano\r\n  depuis 2010. ^f-a2b4\r\n* Habite Lyon.    ^f-zz00\r\n- \r\n<!-- - Un exemple, pas un fait. -->\r\n```\r\n- dans un bloc de code\r\n```\r\n\r\n## Notes\r\n\r\n- une note, pas un fait\r\n",
            "f-3k9w is taken, so the second fact gets the next code"
        );
        let facts = parse(&out).facts;
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[1].text, "Joue du piano depuis 2010.");
        assert_eq!(facts[2].text, "Habite Lyon.");
        assert!(facts.iter().all(|f| f.id.is_some()));
        assert_eq!(with_fact_ids(&out), None, "nothing more to add");

        let first_line = "## Faits\n\n- Joue du piano ^f-3k9w\n  depuis 2010.\n";
        assert_eq!(
            with_fact_ids(first_line),
            None,
            "its id is on its first line"
        );

        let english = "## Facts\n\n- Allergic to nuts.\n";
        let out = with_fact_ids(english).unwrap();
        assert!(
            out.starts_with("## Facts\n\n- Allergic to nuts. ^f-"),
            "{out}"
        );
        assert!(out.ends_with('\n'));
        assert_eq!(parse(&out).facts[0].text, "Allergic to nuts.");
    }

    #[test]
    fn duplicate_fact_ids_are_found() {
        let body = parse("## Faits\n\n- A. ^f-3k9w\n- B. ^f-3k9w\n- C. ^f-a2b4\n- D.\n");
        let twice = duplicate_fact_ids(&body);
        assert_eq!(
            twice.iter().map(FactId::as_str).collect::<Vec<_>>(),
            ["f-3k9w"]
        );
    }

    #[test]
    fn relation_words_map_to_the_fixed_vocabulary() {
        let kind = |w: &str| RelationKind::of_word(w).map(RelationKind::as_str);
        assert_eq!(kind("mère"), Some("parent"));
        assert_eq!(kind("Father"), Some("parent"));
        assert_eq!(kind("sœur"), Some("sibling"));
        assert_eq!(kind("travaille chez"), Some("works_at"));
        assert_eq!(kind("Works at"), Some("works_at"));
        assert_eq!(kind("vit à"), Some("lives_in"));
        assert_eq!(kind("fait partie de"), Some("part_of"));
        assert_eq!(kind("part of"), Some("part_of"));
        assert_eq!(kind("co-worker"), Some("colleague"));
        assert_eq!(kind("parrain"), None, "a free relation");
        assert_eq!(kind("cousine"), None);
    }

    #[test]
    fn the_text_of_one_section() {
        let body =
            "\n## Résumé\n\nUne journée calme.\nPromenade au parc.\n\n## Événements\n\n- [[x]]\n";
        assert_eq!(
            section_text(body, Section::Summary).as_deref(),
            Some("Une journée calme.\nPromenade au parc.")
        );
        assert_eq!(section_text(body, Section::Notes), None);
        assert_eq!(section_text("## Summary\n\n", Section::Summary), None);
    }

    #[test]
    fn relations_go_at_the_end_of_their_section() {
        let body = "\n## Faits\n\n- Aime le thé.\n\n## Relations\n\n- ami : [[paul]]\n\n## Notes\n\nMa note.\n";
        let out = add_relations(body, &[relation("mère", "marie-dupont")], Language::Fr);
        assert_eq!(
            out,
            "\n## Faits\n\n- Aime le thé.\n\n## Relations\n\n- ami : [[paul]]\n- mère : [[marie-dupont]]\n\n## Notes\n\nMa note.\n"
        );
        let parsed = parse(&out);
        assert_eq!(parsed.relations.len(), 2);
        assert_eq!(parsed.notes.as_deref(), Some("Ma note."));
    }

    #[test]
    fn a_relations_section_is_added_when_there_is_none() {
        let hint = owner_hint(Language::Fr);
        let out = add_relations(&hint, &[relation("sœur", "lea")], Language::Fr);
        assert_eq!(out, format!("{hint}\n## Relations\n\n- sœur : [[lea]]\n"));

        let with_notes = "\n## Notes\n\n```\n## Relations\n```\n";
        let out = add_relations(with_notes, &[relation("friend", "ana")], Language::En);
        assert_eq!(
            out, "\n## Relations\n\n- friend: [[ana]]\n\n## Notes\n\n```\n## Relations\n```\n",
            "a heading inside a code block is not a section"
        );

        let out = add_relations("Texte", &[relation("ami", "paul")], Language::Fr);
        assert_eq!(out, "Texte\n\n## Relations\n\n- ami : [[paul]]\n");
    }

    #[test]
    fn an_empty_relations_section_is_filled() {
        let out = add_relations(
            "## Relations\n## Notes\nx\n",
            &[relation("ami", "paul")],
            Language::Fr,
        );
        assert_eq!(out, "## Relations\n\n- ami : [[paul]]\n\n## Notes\nx\n");
        assert_eq!(
            add_relations("## Relations", &[relation("ami", "paul")], Language::Fr),
            "## Relations\n\n- ami : [[paul]]\n"
        );
    }
}

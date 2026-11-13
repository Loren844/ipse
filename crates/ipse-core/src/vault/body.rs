//! The body of an entity file: facts, relations and notes (`docs/spec/vault.md`,
//! sections 2.5, 3.2 and 3.3).
//!
//! Sections are level-2 headings. Ipse recognizes the headings of every supported
//! language, whatever the vault's language, so changing the language never breaks a
//! vault. Text outside known sections is ignored here and left untouched in the file.

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
}

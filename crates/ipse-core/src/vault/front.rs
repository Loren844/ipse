//! Splitting a front matter into its top-level entries, line by line.
//!
//! Ipse rewrites only the keys it owns. Every other key keeps its exact text, comments
//! and layout. To do that, the front matter is cut into one block of lines per
//! top-level key. The cut is checked against a real YAML parse before it is trusted
//! (see `file.rs`): when the two disagree, Ipse reads the file but refuses to rewrite it.

/// One top-level `key: value` with its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawEntry {
    /// Blank and comment lines just before the key line.
    pub(crate) leading: String,
    /// The key, as written before its colon.
    pub(crate) key: String,
    /// The key line and every line of its value, each ending with `\n`.
    pub(crate) text: String,
}

/// A front matter cut into entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RawFront {
    pub(crate) entries: Vec<RawEntry>,
    /// Blank and comment lines after the last entry.
    pub(crate) trailer: String,
}

/// Cuts the text between the `---` lines into entries. Never fails: a cut that does not
/// match the YAML structure is detected afterwards.
pub(crate) fn split(front: &str) -> RawFront {
    let mut entries: Vec<RawEntry> = Vec::new();
    let mut pending = String::new();

    for line in front.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = content.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            pending.push_str(line);
        } else if let Some(key) = key_of(content) {
            entries.push(RawEntry {
                leading: std::mem::take(&mut pending),
                key: key.to_owned(),
                text: line.to_owned(),
            });
        } else if let Some(last) = entries.last_mut() {
            last.text.push_str(&pending);
            pending.clear();
            last.text.push_str(line);
        } else {
            pending.push_str(line);
        }
    }
    if let Some(last) = entries.last_mut()
        && !last.text.ends_with('\n')
    {
        last.text.push('\n');
    }

    RawFront {
        entries,
        trailer: pending,
    }
}

/// The key of a line that starts a top-level entry: `name: value` or `aliases:`.
fn key_of(line: &str) -> Option<&str> {
    let first = line.chars().next()?;
    if first.is_whitespace() || "#-?:,[]{}&*!|>'\"%@`".contains(first) {
        return None;
    }
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(found) = line[from..].find(':') {
        let colon = from + found;
        if matches!(bytes.get(colon + 1), None | Some(b' ' | b'\t')) {
            let key = line[..colon].trim_end();
            return (!key.is_empty()).then_some(key);
        }
        from = colon + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(front: &RawFront) -> Vec<&str> {
        front.entries.iter().map(|e| e.key.as_str()).collect()
    }

    #[test]
    fn one_entry_per_key_with_its_value_lines() {
        let front = split("id: abc\nemails:\n  - a@example.org\n  - b@example.org\nname: Marie\n");
        assert_eq!(keys(&front), ["id", "emails", "name"]);
        assert_eq!(
            front.entries[1].text,
            "emails:\n  - a@example.org\n  - b@example.org\n"
        );
    }

    #[test]
    fn sequence_at_column_zero_belongs_to_its_key() {
        let front = split("aliases:\n- Maman\n- Mum\nname: x\n");
        assert_eq!(keys(&front), ["aliases", "name"]);
        assert_eq!(front.entries[0].text, "aliases:\n- Maman\n- Mum\n");
    }

    #[test]
    fn comments_go_with_the_next_key_or_stay_inside_a_value() {
        let front = split("a: 1\n\n# about b\nb:\n  - x\n# inside\n  - y\n# end\n");
        assert_eq!(front.entries[1].leading, "\n# about b\n");
        assert_eq!(front.entries[1].text, "b:\n  - x\n# inside\n  - y\n");
        assert_eq!(front.trailer, "# end\n");
    }

    #[test]
    fn colons_inside_values_and_keys() {
        let front = split("source: calendar:5f2a@example.org\nurl: https://example.org\n");
        assert_eq!(keys(&front), ["source", "url"]);
        assert_eq!(split("a:b: c\n").entries[0].key, "a:b");
    }

    #[test]
    fn quoted_keys_are_not_recognized() {
        assert!(split("\"name\": x\n").entries.is_empty());
    }

    #[test]
    fn text_is_rebuilt_exactly() {
        let text = "# top\nid: 1\nlist:\n  - a\n\n# c\nname: x\n# tail\n";
        let front = split(text);
        let mut back = String::new();
        for e in &front.entries {
            back.push_str(&e.leading);
            back.push_str(&e.text);
        }
        back.push_str(&front.trailer);
        assert_eq!(back, text);
    }
}

//! Folding text for matching: lowercase, without accents, words separated by spaces.

use crate::slug::fold_accent;

/// `text` in lowercase, with Latin accents removed (`Élodie` → `elodie`, `Œuvre` →
/// `oeuvre`) and every character that is not a letter or a digit turned into a space.
/// Letters of other scripts are kept as they are.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            continue;
        }
        let folded = fold_accent(c);
        if !folded.is_empty() {
            out.push_str(folded);
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_and_punctuation() {
        assert_eq!(fold("Élodie Martin"), "elodie martin");
        assert_eq!(fold("Café de l'Œuvre"), "cafe de l oeuvre");
        assert_eq!(fold("STRASSE Straße"), "strasse strasse");
        assert_eq!(fold("06-12-34"), "06 12 34");
        assert_eq!(fold("東京 Tokyo"), "東京 tokyo");
    }
}

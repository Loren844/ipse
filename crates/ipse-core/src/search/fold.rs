//! Folding text for matching: lowercase, without accents, words separated by spaces,
//! and plurals brought back to the singular.

use crate::slug::fold_accent;
use crate::vault::Language;

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

/// The words of `text` as they are matched: folded, then each brought back to its
/// singular in `language` (see [`stem`]).
#[must_use]
pub fn words(text: &str, language: Language) -> Vec<String> {
    fold(text)
        .split_whitespace()
        .map(|w| stem(w, language).into_owned())
        .collect()
}

/// A folded word without its plural ending, by the regular rules of `language`. Only
/// plurals: `maisons` → `maison`, `chevaux` → `cheval`, `cities` → `city`. The same
/// rules apply to the vault's text and to the words searched, so that a word left as
/// it is on both sides still matches. Words of 3 letters or fewer, and numbers, are
/// kept.
#[must_use]
pub fn stem(word: &str, language: Language) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if word.len() <= 3 || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return Cow::Borrowed(word);
    }
    let cut = |n: usize| Cow::Borrowed(&word[..word.len() - n]);
    match language {
        Language::Fr => {
            if word.ends_with("eaux") || word.ends_with("eux") || word.ends_with("oux") {
                cut(1)
            } else if let Some(base) = word.strip_suffix("aux").filter(|b| b.len() >= 2) {
                Cow::Owned(format!("{base}al"))
            } else if word.ends_with('s') && !word.ends_with("ss") {
                cut(1)
            } else {
                Cow::Borrowed(word)
            }
        }
        _ => {
            if let Some(base) = word.strip_suffix("ies").filter(|b| b.len() >= 2) {
                Cow::Owned(format!("{base}y"))
            } else if word.ends_with("sses")
                || word.ends_with("ches")
                || word.ends_with("shes")
                || word.ends_with("xes")
                || word.ends_with("zes")
            {
                cut(2)
            } else if word.ends_with('s')
                && !(word.ends_with("ss") || word.ends_with("us") || word.ends_with("is"))
            {
                cut(1)
            } else {
                Cow::Borrowed(word)
            }
        }
    }
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

    #[test]
    fn french_plurals() {
        let fr = |w| stem(w, Language::Fr).into_owned();
        for (plural, singular) in [
            ("maisons", "maison"),
            ("chevaux", "cheval"),
            ("journaux", "journal"),
            ("chateaux", "chateau"),
            ("jeux", "jeu"),
            ("bijoux", "bijou"),
            ("series", "serie"),
        ] {
            assert_eq!(fr(plural), fr(singular), "{plural}");
            assert_eq!(fr(plural), singular, "{plural}");
        }
        for kept in ["bus", "1990s", "classe", "stress"] {
            assert_eq!(fr(kept), kept);
        }
    }

    #[test]
    fn english_plurals() {
        let en = |w| stem(w, Language::En).into_owned();
        for (plural, singular) in [
            ("houses", "house"),
            ("cities", "city"),
            ("churches", "church"),
            ("boxes", "box"),
            ("classes", "class"),
            ("dishes", "dish"),
        ] {
            assert_eq!(en(plural), singular, "{plural}");
            assert_eq!(en(singular), singular, "{singular}");
        }
        for kept in ["campus", "tennis", "glass", "bus"] {
            assert_eq!(en(kept), kept);
        }
    }

    #[test]
    fn words_fold_then_stem() {
        assert_eq!(
            words("Les Châteaux de la Loire", Language::Fr),
            ["les", "chateau", "de", "la", "loire"]
        );
    }
}

//! Turning a human name into a stable, portable file stem.
//!
//! Entities in the vault (people, places, projects…) are Markdown files. Their file
//! names must be the same on Windows, macOS and Linux, readable by a human, and
//! derived from the entity's name without surprises.

/// Maximum length of a file stem, in bytes.
///
/// Kept well under the 255-byte file name limit of common file systems, so that an
/// extension and a de-duplication suffix (`-2`) always fit.
pub const MAX_STEM_LEN: usize = 120;

/// Converts a name into a lowercase ASCII file stem made of `a-z`, `0-9` and `-`.
///
/// - Common Latin accents are folded (`é` → `e`, `ß` → `ss`, `œ` → `oe`).
/// - Every run of other characters becomes a single `-`.
/// - The result never starts or ends with `-`.
/// - The result is at most [`MAX_STEM_LEN`] bytes long.
///
/// Returns `None` when nothing usable is left (for example a name made only of emoji),
/// so the caller can fall back to another identifier.
///
/// ```
/// use ipse_core::slug::to_file_stem;
///
/// assert_eq!(to_file_stem("Café de l'Œuvre").as_deref(), Some("cafe-de-l-oeuvre"));
/// assert_eq!(to_file_stem("🎉"), None);
/// ```
#[must_use]
pub fn to_file_stem(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len().min(MAX_STEM_LEN));
    let mut need_dash = false;
    let mut buf = [0u8; 4];

    for c in name.chars() {
        let piece: &str = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase().encode_utf8(&mut buf)
        } else {
            fold_accent(c)
        };
        if piece.is_empty() {
            need_dash = true;
            continue;
        }
        // A dash is only ever written *before* a piece, so the stem can neither
        // start nor end with one.
        let dash = need_dash && !out.is_empty();
        if out.len() + usize::from(dash) + piece.len() > MAX_STEM_LEN {
            break;
        }
        if dash {
            out.push('-');
        }
        out.push_str(piece);
        need_dash = false;
    }

    if out.is_empty() { None } else { Some(out) }
}

/// ASCII replacement for a non-ASCII Latin letter; an empty string means "separator".
pub(crate) fn fold_accent(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'æ' | 'Æ' => "ae",
        'ç' | 'Ç' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
        'ñ' | 'Ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "o",
        'œ' | 'Œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => "y",
        'ß' => "ss",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_joins_words() {
        assert_eq!(
            to_file_stem("Marie Dupont").as_deref(),
            Some("marie-dupont")
        );
    }

    #[test]
    fn folds_french_accents_and_ligatures() {
        assert_eq!(
            to_file_stem("Élodie Gaëlle").as_deref(),
            Some("elodie-gaelle")
        );
        assert_eq!(
            to_file_stem("Sœur Straße").as_deref(),
            Some("soeur-strasse")
        );
    }

    #[test]
    fn collapses_runs_of_separators_and_trims() {
        assert_eq!(
            to_file_stem("  --Paris, 75011 !!  ").as_deref(),
            Some("paris-75011")
        );
    }

    #[test]
    fn keeps_digits() {
        assert_eq!(to_file_stem("Semaine 46").as_deref(), Some("semaine-46"));
    }

    #[test]
    fn returns_none_when_nothing_usable_is_left() {
        assert_eq!(to_file_stem(""), None);
        assert_eq!(to_file_stem("🎉 ✨"), None);
        assert_eq!(to_file_stem("---"), None);
    }

    #[test]
    fn output_is_always_portable() {
        for name in ["Ça va ?", "日本 Tokyo", "a/b\\c:d*e", "Maman ❤️", "x 🎉 y"] {
            if let Some(stem) = to_file_stem(name) {
                assert!(
                    stem.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                    "{name:?} gave {stem:?}"
                );
                assert!(!stem.starts_with('-') && !stem.ends_with('-'));
                assert!(!stem.contains("--"));
            }
        }
    }

    #[test]
    fn is_truncated_to_max_len_without_trailing_dash() {
        let long = "mot ".repeat(100);
        let stem = to_file_stem(&long).unwrap_or_default();
        assert!(!stem.is_empty());
        assert!(stem.len() <= MAX_STEM_LEN);
        assert!(!stem.ends_with('-'));
    }

    #[test]
    fn is_idempotent() {
        for name in ["Café de l'Œuvre", "Marie Dupont", "Semaine 46"] {
            let once = to_file_stem(name).unwrap_or_default();
            assert_eq!(to_file_stem(&once).as_deref(), Some(once.as_str()));
        }
    }
}

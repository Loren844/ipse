//! A streaming reader of vCard 3.0 and 4.0 files (RFC 2426 and RFC 6350).
//!
//! Cards are read one at a time, so a file of any size uses little memory. Only the
//! properties Ipse uses are kept; photos and the rest are skipped as they are read.

use std::io::BufRead;

use ipse_core::vault::Birthday;

use crate::connectors::content_line::{Lines, Property, non_empty, split_unescaped, text};

/// What Ipse keeps from one card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Card {
    pub(crate) uid: Option<String>,
    pub(crate) full_name: String,
    pub(crate) given_name: Option<String>,
    pub(crate) family_name: Option<String>,
    pub(crate) nicknames: Vec<String>,
    pub(crate) emails: Vec<String>,
    pub(crate) phones: Vec<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` when the year is unknown.
    pub(crate) birthday: Option<String>,
    pub(crate) organization: Option<String>,
    pub(crate) note: Option<String>,
}

/// Why a card was not returned.
#[derive(Debug)]
pub(crate) enum CardError {
    /// The card cannot be read (no end, no name); it is skipped.
    Unreadable,
    /// The file itself cannot be read.
    Io,
}

/// The cards of a vCard file, in order.
pub(crate) struct Cards<R> {
    lines: Lines<R>,
    done: bool,
}

impl<R: BufRead> Cards<R> {
    pub(crate) fn new(input: R) -> Self {
        Self {
            lines: Lines::new(input),
            done: false,
        }
    }
}

impl<R: BufRead> Iterator for Cards<R> {
    type Item = Result<Card, CardError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut card: Option<Builder> = None;
        loop {
            let line = match self.lines.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => {
                    self.done = true;
                    return card.map(|_| Err(CardError::Unreadable));
                }
                Err(_) => {
                    self.done = true;
                    return Some(Err(CardError::Io));
                }
            };
            let Some(property) = Property::parse(&line) else {
                continue;
            };
            match (property.name.as_ref(), &mut card) {
                ("BEGIN", None) if property.value.eq_ignore_ascii_case("VCARD") => {
                    card = Some(Builder::default());
                }
                ("BEGIN", Some(_)) if property.value.eq_ignore_ascii_case("VCARD") => {
                    // A card that never ended: skip it; the next call starts the new one.
                    self.lines.requeue(line);
                    return Some(Err(CardError::Unreadable));
                }
                ("END", Some(_)) if property.value.eq_ignore_ascii_case("VCARD") => {
                    let built = card.take().and_then(Builder::build);
                    return Some(built.ok_or(CardError::Unreadable));
                }
                (_, Some(builder)) => builder.add(&property),
                (_, None) => {}
            }
        }
    }
}

#[derive(Default)]
struct Builder {
    card: Card,
}

impl Builder {
    fn add(&mut self, p: &Property) {
        // Legacy encodings come with the complete version (#13).
        if p.param("ENCODING").is_some() || p.param("CHARSET").is_some() {
            return;
        }
        let card = &mut self.card;
        match p.name.as_ref() {
            "UID" => card.uid = text(p.value),
            "FN" => {
                if let Some(name) = text(p.value) {
                    card.full_name = name;
                }
            }
            "N" => {
                let parts = split_unescaped(p.value, ';');
                card.family_name = parts.first().and_then(|s| non_empty(s));
                card.given_name = parts.get(1).and_then(|s| non_empty(s));
            }
            "NICKNAME" => card.nicknames.extend(
                split_unescaped(p.value, ',')
                    .into_iter()
                    .filter_map(|s| non_empty(&s)),
            ),
            "EMAIL" => card.emails.extend(text(p.value)),
            "TEL" => card.phones.extend(text(p.value).map(|t| {
                t.strip_prefix("tel:")
                    .or_else(|| t.strip_prefix("TEL:"))
                    .map_or(t.clone(), str::to_owned)
            })),
            "BDAY" => card.birthday = birthday(p.value, p.param("X-APPLE-OMIT-YEAR")),
            "ORG" => {
                let parts: Vec<String> = split_unescaped(p.value, ';')
                    .into_iter()
                    .filter_map(|s| non_empty(&s))
                    .collect();
                if !parts.is_empty() {
                    card.organization = Some(parts.join(", "));
                }
            }
            "NOTE" => card.note = text(p.value),
            _ => {}
        }
    }

    /// The card, unless it has no name at all.
    fn build(self) -> Option<Card> {
        let mut card = self.card;
        if card.full_name.is_empty() {
            let parts: Vec<&str> = [&card.given_name, &card.family_name]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
            card.full_name = parts.join(" ");
        }
        (!card.full_name.is_empty()).then_some(card)
    }
}

/// `1965-03-12`, `19650312`, `--03-12`, `--0312`, with or without a time, as
/// `YYYY-MM-DD` or `--MM-DD`. Apple writes a stand-in year it names in
/// `X-APPLE-OMIT-YEAR`; that year is dropped.
fn birthday(value: &str, omit_year: Option<&str>) -> Option<String> {
    let date = value.trim().split('T').next()?;
    let (year, rest) = if let Some(rest) = date.strip_prefix("--") {
        (None, rest.replace('-', ""))
    } else {
        let digits = date.replace('-', "");
        if digits.len() != 8 {
            return None;
        }
        let (year, rest) = digits.split_at(4);
        let year = (Some(year) != omit_year).then_some(year.parse::<i16>().ok()?);
        (year, rest.to_owned())
    };
    if rest.len() != 4 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let month = rest[..2].parse().ok()?;
    let day = rest[2..].parse().ok()?;
    Birthday::new(year, month, day).map(|b| b.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(text: &str) -> Vec<Result<Card, CardError>> {
        Cards::new(text.as_bytes()).collect()
    }

    fn ok(text: &str) -> Vec<Card> {
        cards(text).into_iter().map(Result::unwrap).collect()
    }

    #[test]
    fn vcard_3() {
        let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Martin;Élodie;;;\r\nFN:Élodie Martin\r\n\
                    NICKNAME:Lolo,Élo\r\nEMAIL;TYPE=INTERNET:elodie@example.org\r\n\
                    TEL;TYPE=CELL:+33 6 12 34 56 78\r\nBDAY:1990-04-02\r\n\
                    ORG:Boulangerie du Coin;Vente\r\nNOTE:Rencontrée au club\\, le mardi.\\nAime le thé.\r\n\
                    UID:card-1\r\nEND:VCARD\r\n";
        let card = &ok(text)[0];
        assert_eq!(card.full_name, "Élodie Martin");
        assert_eq!(card.given_name.as_deref(), Some("Élodie"));
        assert_eq!(card.family_name.as_deref(), Some("Martin"));
        assert_eq!(card.nicknames, ["Lolo", "Élo"]);
        assert_eq!(card.emails, ["elodie@example.org"]);
        assert_eq!(card.phones, ["+33 6 12 34 56 78"]);
        assert_eq!(card.birthday.as_deref(), Some("1990-04-02"));
        assert_eq!(
            card.organization.as_deref(),
            Some("Boulangerie du Coin, Vente")
        );
        assert_eq!(
            card.note.as_deref(),
            Some("Rencontrée au club, le mardi.\nAime le thé.")
        );
        assert_eq!(card.uid.as_deref(), Some("card-1"));
    }

    #[test]
    fn vcard_4() {
        let text = "BEGIN:VCARD\nVERSION:4.0\nFN:Marc Leroy\nTEL;VALUE=uri;TYPE=\"voice,cell\":tel:+33698765432\n\
                    BDAY:--0315\nitem1.EMAIL:marc@example.org\nEND:VCARD\n";
        let card = &ok(text)[0];
        assert_eq!(card.phones, ["+33698765432"]);
        assert_eq!(card.birthday.as_deref(), Some("--03-15"));
        assert_eq!(card.emails, ["marc@example.org"], "groups are ignored");
    }

    #[test]
    fn folded_lines_are_joined() {
        let text =
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ana\n Silva\nNOTE:une note\n\t qui continue\nEND:VCARD\n";
        let card = &ok(text)[0];
        assert_eq!(
            card.full_name, "AnaSilva",
            "folding removes the line break and one space"
        );
        assert_eq!(card.note.as_deref(), Some("une note qui continue"));
    }

    #[test]
    fn name_from_n_when_fn_is_missing() {
        let card = &ok("BEGIN:VCARD\nN:Durand;Camille\nEND:VCARD\n")[0];
        assert_eq!(card.full_name, "Camille Durand");
    }

    #[test]
    fn apple_stand_in_year_is_dropped() {
        let text = "BEGIN:VCARD\nFN:A\nBDAY;X-APPLE-OMIT-YEAR=1604:1604-03-12\nEND:VCARD\n";
        assert_eq!(ok(text)[0].birthday.as_deref(), Some("--03-12"));
        assert_eq!(
            birthday("19650312T000000Z", None).as_deref(),
            Some("1965-03-12")
        );
        assert_eq!(birthday("1965-02-30", None), None);
        assert_eq!(birthday("---12", None), None);
    }

    #[test]
    fn broken_cards_are_skipped_and_the_rest_read() {
        let text = "BEGIN:VCARD\nNOTE:no name\nEND:VCARD\n\
                    BEGIN:VCARD\nFN:never ends\n\
                    BEGIN:VCARD\nFN:Léa Moreau\nEND:VCARD\n\
                    garbage line\n\
                    BEGIN:VCARD\nFN:cut off";
        let results = cards(text);
        let kinds: Vec<_> = results
            .iter()
            .map(|r| match r {
                Ok(card) => card.full_name.clone(),
                Err(CardError::Unreadable) => "skipped".into(),
                Err(CardError::Io) => "io".into(),
            })
            .collect();
        assert_eq!(kinds, ["skipped", "skipped", "Léa Moreau", "skipped"]);
    }

    #[test]
    fn legacy_encodings_are_not_read_yet() {
        let text = "BEGIN:VCARD\nVERSION:2.1\nFN;ENCODING=QUOTED-PRINTABLE:L=C3=A9a\nN:Moreau;L\nEND:VCARD\n";
        assert_eq!(ok(text)[0].full_name, "L Moreau");
    }

    #[test]
    fn invalid_utf8_never_fails() {
        let bytes = b"BEGIN:VCARD\nFN:Caf\xe9\nEND:VCARD\n";
        let cards: Vec<_> = Cards::new(&bytes[..]).collect();
        assert!(
            cards[0]
                .as_ref()
                .is_ok_and(|c| c.full_name.starts_with("Caf"))
        );
    }

    #[test]
    fn empty_file() {
        assert!(cards("").is_empty());
    }
}

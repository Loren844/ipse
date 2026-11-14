//! Phone numbers in the international format, E.164: `+33612345678`.
//!
//! A number written with its country code (`+33 6…`, `0033 6…`) is always understood.
//! A national number (`06 12 34 56 78`) needs a country: the one of the vault's time
//! zone, when this table knows it. A number Ipse cannot read with certainty is kept as
//! written: a wrong number is worse than an unformatted one.

/// How the numbers of one country are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Country {
    /// The country calling code: `33`.
    code: &'static str,
    /// The national prefix dropped in the international format (`0` in France), if any.
    trunk: Option<char>,
    /// How many digits the number has without the country code and the national prefix.
    digits: (usize, usize),
}

const fn country(code: &'static str, trunk: Option<char>, min: usize, max: usize) -> Country {
    Country {
        code,
        trunk,
        digits: (min, max),
    }
}

const NANP: Country = country("1", Some('1'), 10, 10);

/// Time zones and the country they say, for the countries Ipse knows how numbers are
/// written.
const ZONES: [(&str, Country); 31] = [
    ("Europe/Paris", country("33", Some('0'), 9, 9)),
    ("Europe/Brussels", country("32", Some('0'), 8, 9)),
    ("Europe/Zurich", country("41", Some('0'), 9, 9)),
    ("Europe/Luxembourg", country("352", None, 4, 11)),
    ("Europe/Monaco", country("377", None, 8, 9)),
    ("Europe/Berlin", country("49", Some('0'), 6, 13)),
    ("Europe/Vienna", country("43", Some('0'), 4, 13)),
    ("Europe/Amsterdam", country("31", Some('0'), 9, 9)),
    ("Europe/London", country("44", Some('0'), 9, 10)),
    ("Europe/Dublin", country("353", Some('0'), 7, 9)),
    ("Europe/Madrid", country("34", None, 9, 9)),
    ("Europe/Lisbon", country("351", None, 9, 9)),
    // Italian numbers keep their leading 0 after the country code.
    ("Europe/Rome", country("39", None, 6, 11)),
    ("Africa/Casablanca", country("212", Some('0'), 9, 9)),
    ("Africa/Algiers", country("213", Some('0'), 8, 9)),
    ("Africa/Tunis", country("216", None, 8, 8)),
    ("Africa/Dakar", country("221", None, 9, 9)),
    ("Africa/Abidjan", country("225", None, 10, 10)),
    ("Africa/Douala", country("237", None, 9, 9)),
    ("America/New_York", NANP),
    ("America/Chicago", NANP),
    ("America/Denver", NANP),
    ("America/Phoenix", NANP),
    ("America/Los_Angeles", NANP),
    ("America/Anchorage", NANP),
    ("Pacific/Honolulu", NANP),
    ("America/Toronto", NANP),
    ("America/Montreal", NANP),
    ("America/Vancouver", NANP),
    ("America/Edmonton", NANP),
    ("America/Halifax", NANP),
];

impl Country {
    /// The country of the vault's time zone, when Ipse knows how its numbers are written.
    pub(crate) fn of_time_zone(time_zone: &str) -> Option<Self> {
        ZONES
            .iter()
            .find(|(zone, _)| *zone == time_zone)
            .map(|(_, country)| *country)
    }
}

/// `raw` in E.164, or as written (trimmed) when it cannot be read with certainty:
/// letters, an extension, a national number without a known country, or a number
/// whose length does not fit.
pub(crate) fn e164(raw: &str, country: Option<Country>) -> String {
    let text = raw.trim();
    let allowed = |c: char| c.is_ascii_digit() || " .-/()+".contains(c);
    if text.is_empty() || !text.chars().all(allowed) || text.rfind('+').is_some_and(|i| i > 0) {
        return text.to_owned();
    }
    // `+33 (0)6…`: the national prefix written in brackets is not dialled from abroad.
    let digits: String = text
        .replace("(0)", "")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let international = if text.starts_with('+') {
        Some(digits.as_str())
    } else if let Some(rest) = digits.strip_prefix("00") {
        Some(rest)
    } else {
        digits.strip_prefix("011").filter(|_| country == Some(NANP))
    };
    let number = match (international, country) {
        (Some(number), _) => number.to_owned(),
        (None, Some(c)) => {
            let national = match c.trunk {
                Some(trunk) if c == NANP => digits.strip_prefix(trunk).unwrap_or(&digits),
                Some(trunk) => match digits.strip_prefix(trunk) {
                    Some(national) => national,
                    None => return text.to_owned(),
                },
                None => digits.as_str(),
            };
            let (min, max) = c.digits;
            if !(min..=max).contains(&national.len()) {
                return text.to_owned();
            }
            format!("{}{national}", c.code)
        }
        (None, None) => return text.to_owned(),
    };
    if (8..=15).contains(&number.len()) && !number.starts_with('0') {
        format!("+{number}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr() -> Option<Country> {
        Country::of_time_zone("Europe/Paris")
    }

    #[test]
    fn numbers_with_their_country_code() {
        for raw in [
            "+33 6 12 34 56 78",
            "+33612345678",
            "0033 6 12 34 56 78",
            "+33 (0)6 12 34 56 78",
        ] {
            assert_eq!(e164(raw, None), "+33612345678", "{raw}");
        }
        assert_eq!(
            e164("+351 912 345 678", fr()),
            "+351912345678",
            "the country written wins"
        );
        assert_eq!(e164("+1 (415) 555-0123", None), "+14155550123");
    }

    #[test]
    fn national_numbers_take_the_vaults_country() {
        assert_eq!(e164("06 12 34 56 78", fr()), "+33612345678");
        assert_eq!(e164("06.12.34.56.78", fr()), "+33612345678");
        assert_eq!(e164("01 23 45 67 89", fr()), "+33123456789");
        let it = Country::of_time_zone("Europe/Rome");
        assert_eq!(
            e164("06 1234 5678", it),
            "+390612345678",
            "Italy keeps its 0"
        );
        let us = Country::of_time_zone("America/New_York");
        assert_eq!(e164("(415) 555-0123", us), "+14155550123");
        assert_eq!(e164("1 415 555 0123", us), "+14155550123");
        assert_eq!(e164("011 33 6 12 34 56 78", us), "+33612345678");
    }

    #[test]
    fn what_is_not_certain_is_kept_as_written() {
        assert_eq!(e164("06 12 34 56 78", None), "06 12 34 56 78", "no country");
        assert_eq!(e164(" 06 12 34 ", fr()), "06 12 34", "too short for France");
        assert_eq!(
            e164("6 12 34 56 78", fr()),
            "6 12 34 56 78",
            "no national prefix"
        );
        assert_eq!(
            e164("+33 6 12 34 56 78 ext. 12", fr()),
            "+33 6 12 34 56 78 ext. 12"
        );
        assert_eq!(e164("3615", fr()), "3615");
        assert_eq!(e164("06+12", fr()), "06+12");
        assert_eq!(e164("", fr()), "");
        assert!(Country::of_time_zone("Asia/Tokyo").is_none());
    }
}

//! Time types used across Ipse.
//!
//! Ipse stores instants as RFC 3339 timestamps and civil dates in a person's time
//! zone (`docs/spec/vault.md`, section 2.2). It relies on [`jiff`] for both, so that
//! time zones and daylight-saving transitions are handled from the IANA database.

pub use jiff::civil::Date;
pub use jiff::tz::TimeZone;
pub use jiff::{SignedDuration, Timestamp, Zoned};

/// The current instant, in UTC.
#[must_use]
pub fn now() -> Timestamp {
    Timestamp::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_round_trip_keeps_the_offset() {
        let text = "2026-11-12T19:30:00+01:00[Europe/Paris]";
        let zoned: Zoned = text.parse().unwrap();
        assert_eq!(zoned.to_string(), text);
        assert_eq!(
            zoned.timestamp().to_string(),
            "2026-11-12T18:30:00Z",
            "the instant is the same in UTC"
        );
    }
}

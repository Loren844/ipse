//! The vault's settings, `.ipse/vault.toml` (`docs/spec/vault.md`, section 8).

use std::collections::BTreeMap;
use std::path::Path;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Span, SpanRelativeTo};
use serde::Deserialize;

use super::lang::Language;
use super::state::Topic;
use crate::{Error, Result};

/// Version of the vault format this Ipse reads and writes.
pub const FORMAT: u32 = 1;

/// The settings of one vault.
#[derive(Debug, Clone)]
pub struct Settings {
    language: Language,
    time_zone_name: String,
    time_zone: TimeZone,
    /// State windows set by the person, by topic.
    freshness: BTreeMap<String, SignedDuration>,
}

#[derive(Deserialize)]
struct Raw {
    format: Option<i64>,
    language: Option<String>,
    time_zone: Option<String>,
    #[serde(default)]
    freshness: BTreeMap<String, String>,
}

impl Settings {
    /// Settings with this language and IANA time zone. Returns `None` for an unknown
    /// time zone name.
    #[must_use]
    pub fn new(language: Language, time_zone_name: &str) -> Option<Self> {
        let time_zone = TimeZone::get(time_zone_name).ok()?;
        Some(Self {
            language,
            time_zone_name: time_zone_name.to_owned(),
            time_zone,
            freshness: BTreeMap::new(),
        })
    }

    /// The computer's language and time zone, falling back to English and UTC.
    #[must_use]
    pub fn from_system() -> Self {
        let language = sys_locale::get_locale()
            .and_then(|locale| Language::parse(&locale))
            .unwrap_or_default();
        TimeZone::try_system()
            .ok()
            .and_then(|tz| tz.iana_name().map(str::to_owned))
            .and_then(|name| Self::new(language, &name))
            .unwrap_or_else(|| Self {
                language,
                time_zone_name: "UTC".to_owned(),
                time_zone: TimeZone::UTC,
                freshness: BTreeMap::new(),
            })
    }

    /// Reads and checks the settings file at `path`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file cannot be read; [`Error::UnsupportedVersion`] when
    /// its format is newer than this Ipse; [`Error::Invalid`] when it is not valid TOML
    /// or a key is missing or wrong.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            action: "read",
            path: path.to_owned(),
            source,
        })?;
        Self::parse(path, &text)
    }

    /// Parses the text of a settings file. `path` is used in error messages.
    ///
    /// # Errors
    ///
    /// As [`Settings::read`], without [`Error::Io`].
    pub fn parse(path: &Path, text: &str) -> Result<Self> {
        let invalid = |line: Option<u32>, problem: &str| Error::Invalid {
            path: path.to_owned(),
            line,
            problem: problem.to_owned(),
        };
        let raw: Raw = toml::from_str(text).map_err(|e| {
            let line = e.span().map(|span| line_at(text, span.start));
            invalid(
                line,
                "the settings are not valid TOML here: check quotes and `=` signs",
            )
        })?;

        match raw.format {
            None => return Err(invalid(None, "the `format` key is missing")),
            Some(format) if format > i64::from(FORMAT) => {
                return Err(Error::UnsupportedVersion {
                    what: "this memory",
                    found: u32::try_from(format).unwrap_or(u32::MAX),
                    supported: FORMAT,
                });
            }
            Some(format) if format < 1 => {
                return Err(invalid(None, "`format` must be 1"));
            }
            Some(_) => {}
        }

        let language = match raw.language {
            None => Language::default(),
            Some(code) => Language::parse(&code)
                .ok_or_else(|| invalid(None, "`language` must be one of: en, fr"))?,
        };
        let time_zone_name = raw.time_zone.unwrap_or_else(|| "UTC".to_owned());
        let mut settings = Self::new(language, &time_zone_name).ok_or_else(|| {
            invalid(
                None,
                "`time_zone` must be a time zone name like Europe/Paris",
            )
        })?;
        for (topic, text) in raw.freshness {
            if Topic::new(&topic).is_none() {
                return Err(invalid(
                    None,
                    "a topic in `[freshness]` must use lowercase letters, digits and dashes",
                ));
            }
            let window = duration(&text).ok_or_else(|| {
                invalid(
                    None,
                    &format!("`freshness.{topic}` must be a duration like \"21d\" or \"15m\""),
                )
            })?;
            settings.freshness.insert(topic, window);
        }
        Ok(settings)
    }

    /// The text of the settings file.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "# Settings of this Ipse memory. See docs/spec/vault.md, section 8.\n\
             format = {FORMAT}\n\
             language = \"{}\"\n\
             time_zone = \"{}\"\n",
            self.language, self.time_zone_name
        )
    }

    /// The language of the words Ipse writes in files.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// The vault's time zone.
    #[must_use]
    pub fn time_zone(&self) -> &TimeZone {
        &self.time_zone
    }

    /// The vault's time zone name, like `Europe/Paris`.
    #[must_use]
    pub fn time_zone_name(&self) -> &str {
        &self.time_zone_name
    }

    /// How long a state of `topic` stays current: the person's setting, else the spec's
    /// default. `None` for a topic with neither.
    #[must_use]
    pub fn window(&self, topic: &Topic) -> Option<SignedDuration> {
        self.freshness
            .get(topic.as_str())
            .copied()
            .or_else(|| topic.default_window())
    }
}

/// `21d`, `15m`, `1h30m`, `2 weeks`: a positive duration, days counting 24 hours.
fn duration(text: &str) -> Option<SignedDuration> {
    let span: Span = text.trim().parse().ok()?;
    span.to_duration(SpanRelativeTo::days_are_24_hours())
        .ok()
        .filter(SignedDuration::is_positive)
}

fn line_at(text: &str, offset: usize) -> u32 {
    let before = text.get(..offset).unwrap_or(text);
    u32::try_from(before.matches('\n').count() + 1).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Settings> {
        Settings::parse(Path::new(".ipse/vault.toml"), text)
    }

    #[test]
    fn round_trip() {
        let settings = Settings::new(Language::Fr, "Europe/Paris").unwrap();
        let back = parse(&settings.render()).unwrap();
        assert_eq!(back.language(), Language::Fr);
        assert_eq!(back.time_zone_name(), "Europe/Paris");
    }

    #[test]
    fn newer_format_is_refused() {
        assert!(matches!(
            parse("format = 2\n"),
            Err(Error::UnsupportedVersion {
                found: 2,
                supported: 1,
                ..
            })
        ));
    }

    #[test]
    fn problems_are_reported_without_content() {
        for (text, expected) in [
            ("language = \"fr\"\n", "`format` key is missing"),
            ("format = 0\n", "`format` must be 1"),
            (
                "format = 1\nlanguage = \"klingon\"\n",
                "`language` must be one of",
            ),
            (
                "format = 1\ntime_zone = \"Mars/Olympus\"\n",
                "`time_zone` must be",
            ),
            ("format = 1\nlanguage = fr\n", "not valid TOML"),
        ] {
            let err = parse(text).unwrap_err().to_string();
            assert!(
                err.contains(expected),
                "{err:?} should contain {expected:?}"
            );
            assert!(!err.contains("klingon") && !err.contains("Mars"), "{err:?}");
        }
    }

    #[test]
    fn toml_error_names_its_line() {
        let Err(Error::Invalid { line, .. }) = parse("format = 1\n\nlanguage = fr\n") else {
            panic!("expected an invalid file")
        };
        assert_eq!(line, Some(3));
    }

    #[test]
    fn defaults_and_unknown_keys() {
        let settings = parse("format = 1\n[categories.studies]\nname = \"Études\"\n").unwrap();
        assert_eq!(settings.language(), Language::En);
        assert_eq!(settings.time_zone_name(), "UTC");
    }

    #[test]
    fn freshness_windows() {
        let settings = parse(
            "format = 1\n[freshness]\nmusic = \"14d\"\nhome = \"1h30m\"\npodcasts = \"2 weeks\"\n",
        )
        .unwrap();
        let window = |t: &str| settings.window(&Topic::new(t).unwrap());
        assert_eq!(window("music"), Some(SignedDuration::from_hours(14 * 24)));
        assert_eq!(window("home"), Some(SignedDuration::from_mins(90)));
        assert_eq!(
            window("podcasts"),
            Some(SignedDuration::from_hours(14 * 24))
        );
        assert_eq!(
            window("watching"),
            Some(SignedDuration::from_hours(30 * 24)),
            "default"
        );
        assert_eq!(window("unknown"), None);

        for bad in [
            "music = \"soon\"",
            "music = \"-3d\"",
            "music = \"0m\"",
            "Music = \"3d\"",
        ] {
            let err = parse(&format!("format = 1\n[freshness]\n{bad}\n")).unwrap_err();
            assert!(err.to_string().contains("freshness"), "{bad}: {err}");
        }
    }

    #[test]
    fn system_settings_always_work() {
        let settings = Settings::from_system();
        assert!(Settings::new(settings.language(), settings.time_zone_name()).is_some());
    }
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::environment;

/// What language this session reads, and how it writes a time of day and a measurement. Every
/// setting here defaults to what the environment already says, so an empty table is the right
/// answer on a machine whose locale is set up.
///
/// The three keys are read by different owners: the daemon resolves `units` and stamps the answer
/// onto every weather reading, while the panel, the lock screen and the wallpaper resolve
/// `language` and `hour-format` for themselves. Nothing else in glimpse asks the environment
/// these questions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Regional {
    /// Which translation catalog the interface is drawn in, such as `ru`. Empty follows the
    /// environment, which is `LANGUAGE`, then `LC_MESSAGES`, then `LANG`. Naming one here sets
    /// `LANGUAGE` for the process, so it applies whether or not that locale has been generated.
    ///
    /// This chooses the catalog and nothing else: weekday names, the twelve-hour question and the
    /// unit system keep following the environment, because a person reading a Russian interface in
    /// Chicago still wants their own clock and their own thermometer.
    ///
    /// A change here takes effect when the binary next starts. A GTK template resolves its text as
    /// each widget is built, so re-reading it in a running process would leave what is already on
    /// screen in the old language and everything opened afterwards in the new one.
    pub language: String,
    /// How a time of day reads wherever glimpse composes one itself — the world clock, the times
    /// inside event rows, sunrise and sunset. A `strftime` format you write yourself, such as the
    /// clock applet's `label-format`, is yours and is not affected by this.
    pub hour_format: HourFormat,
    /// Whether temperatures, wind speeds and rainfall are reported in metric or imperial units.
    /// Every reading in one weather report uses the same system, and the panel prints the symbols
    /// that go with it.
    pub units: Units,
}

impl Regional {
    pub fn language(&self) -> Option<&str> {
        Some(self.language.trim()).filter(|language| !language.is_empty())
    }

    pub fn twelve_hour(&self) -> bool {
        match self.hour_format {
            HourFormat::Twelve => true,
            HourFormat::TwentyFour => false,
            HourFormat::Locale => environment::locale_is_twelve_hour(),
        }
    }

    pub fn is_metric(&self) -> bool {
        match self.units {
            Units::Metric => true,
            Units::Imperial => false,
            Units::Locale => environment::locale_is_metric(),
        }
    }
}

/// How a time of day reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HourFormat {
    /// Follow `LC_TIME`, which is what the locale's own `%X` resolves to.
    #[default]
    Locale,
    /// `3:30 PM`.
    #[serde(rename = "12h")]
    Twelve,
    /// `15:30`.
    #[serde(rename = "24h")]
    TwentyFour,
}

/// Which units every reading is reported in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Units {
    /// Follow `LC_MEASUREMENT`. Every locale glibc ships is metric except `en_US`, so this is
    /// metric unless the machine is set to American English.
    #[default]
    Locale,
    /// Celsius, kilometres per hour, millimetres.
    Metric,
    /// Fahrenheit, miles per hour, inches.
    Imperial,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_follows_the_environment() {
        let regional = load("").expect("an absent table is fine").regional;

        assert_eq!(regional.hour_format, HourFormat::Locale);
        assert_eq!(regional.units, Units::Locale);
        assert_eq!(regional.language(), None);
    }

    #[test]
    fn every_key_reads_kebab_case() {
        let parsed =
            load("[regional]\nlanguage = \"ru\"\nhour-format = \"12h\"\nunits = \"imperial\"\n")
                .expect("kebab-case keys")
                .regional;

        assert_eq!(parsed.language(), Some("ru"));
        assert_eq!(parsed.hour_format, HourFormat::Twelve);
        assert_eq!(parsed.units, Units::Imperial);
    }

    /// An explicit setting must not consult the environment at all, or a test on one machine says
    /// something different from the same test on another.
    #[test]
    fn an_explicit_setting_answers_without_asking_the_environment() {
        let twelve = Regional {
            hour_format: HourFormat::Twelve,
            units: Units::Imperial,
            ..Regional::default()
        };
        assert!(twelve.twelve_hour());
        assert!(!twelve.is_metric());
        assert_eq!(environment::clock(twelve.twelve_hour()), "%-I:%M %p");

        let twenty_four = Regional {
            hour_format: HourFormat::TwentyFour,
            units: Units::Metric,
            ..Regional::default()
        };
        assert!(!twenty_four.twelve_hour());
        assert!(twenty_four.is_metric());
        assert_eq!(environment::clock(twenty_four.twelve_hour()), "%H:%M");
    }

    /// Whitespace is what a person leaves behind when they clear the key rather than delete it.
    #[test]
    fn a_blank_language_is_the_same_as_an_absent_one() {
        assert_eq!(
            Regional {
                language: "   ".to_owned(),
                ..Regional::default()
            }
            .language(),
            None
        );
        assert_eq!(
            Regional {
                language: " ru ".to_owned(),
                ..Regional::default()
            }
            .language(),
            Some("ru")
        );
    }

    /// It is not a key of an applet's own table, and it must not come back out when one is
    /// written to disk.
    #[test]
    fn it_is_not_an_applet_setting() {
        load("[applets.clock]\nregional = {}\n").expect_err("`regional` is not an applet setting");

        let applet = crate::Applet::from_name("clock").expect("the clock is an applet");
        let rendered = toml::to_string(&applet).expect("an applet serializes");
        assert!(
            !rendered.contains("regional"),
            "the copy must not round-trip, got {rendered}"
        );
    }

    #[test]
    fn an_unknown_value_is_refused_rather_than_falling_back() {
        load("[regional]\nunits = \"kelvin\"\n").expect_err("`kelvin` is not a unit system");
        load("[regional]\nhour-format = \"36h\"\n").expect_err("`36h` is not an hour format");
        load("[regional]\nlanguages = \"ru\"\n").expect_err("`languages` is not a key");
    }
}

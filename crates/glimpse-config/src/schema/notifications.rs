use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How much notification history the daemon keeps and which notifications it suppresses. Do not
/// disturb is a runtime state the panel toggles rather than a setting written here.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Notifications {
    /// How many notifications are kept once they have been read. Dismissing one puts it here
    /// rather than destroying it, which is what makes an accidental dismissal recoverable; past
    /// this many, the oldest is dropped for good. Senders choose how often they notify, so this
    /// is the bound that stops a chatty one growing the daemon's memory without limit. Clamped
    /// to 1..=1000.
    pub keep: u32,
    /// Regex patterns matched against the application identity, application name, title and body.
    /// Matching notifications are not stored or published.
    pub suppress: Vec<String>,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            keep: 100,
            suppress: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_keeps_a_hundred() {
        assert_eq!(
            load("")
                .expect("an absent table is fine")
                .notifications
                .keep,
            100
        );
    }

    #[test]
    fn the_bound_reads_from_the_document() {
        assert_eq!(
            load("[notifications]\nkeep = 20\n")
                .expect("keep is a key of this table")
                .notifications
                .keep,
            20
        );
    }

    #[test]
    fn a_setting_this_table_does_not_have_is_refused() {
        let rendered = load("[notifications]\ngroups = 6\n")
            .expect_err("`groups` belongs to the applet, not to this table")
            .to_string();

        assert!(
            rendered.contains("groups"),
            "the error must name `groups`, got {rendered}"
        );
    }

    #[test]
    fn suppression_patterns_are_read_and_default_to_empty() {
        assert!(
            load("")
                .expect("an absent table is fine")
                .notifications
                .suppress
                .is_empty()
        );
        assert_eq!(
            load("[notifications]\nsuppress = [\"(?i)spotify\", \"build succeeded\"]\n")
                .expect("suppression patterns are a key of this table")
                .notifications
                .suppress,
            ["(?i)spotify", "build succeeded"]
        );
    }
}

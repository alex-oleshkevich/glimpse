use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

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
    pub enabled: bool,
    #[serde(deserialize_with = "optional_monitor")]
    pub monitor: Option<String>,
    pub edge: NotificationEdge,
    #[serde(deserialize_with = "hide_delay")]
    #[schemars(range(min = 1, max = 86400))]
    pub hide_delay: u64,
    #[serde(deserialize_with = "positive_u32")]
    #[schemars(range(min = 1))]
    pub max_items: u32,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            keep: 100,
            suppress: Vec::new(),
            enabled: true,
            monitor: None,
            edge: NotificationEdge::TopRight,
            hide_delay: 4,
            max_items: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationEdge {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

fn optional_monitor<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let monitor = Option::<String>::deserialize(deserializer)?;
    if monitor
        .as_deref()
        .is_some_and(|name| name.is_empty() || name.trim() != name)
    {
        return Err(D::Error::custom(
            "monitor must be a non-empty exact connector name without surrounding whitespace",
        ));
    }
    Ok(monitor)
}

fn hide_delay<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    (1..=86_400)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 1 and 86400 seconds"))
}

fn positive_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be greater than zero"))
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
        let notifications = load("").expect("an absent table is fine").notifications;

        assert_eq!(notifications.keep, 100);
        assert!(notifications.suppress.is_empty());
        assert!(notifications.enabled);
        assert_eq!(notifications.monitor, None);
        assert_eq!(notifications.edge, super::NotificationEdge::TopRight);
        assert_eq!(super::NotificationEdge::default(), notifications.edge);
        assert_eq!(notifications.hide_delay, 4);
        assert_eq!(notifications.max_items, 6);
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

    #[test]
    fn popup_settings_read_every_supported_edge() {
        use super::NotificationEdge;

        for (name, expected) in [
            ("top-left", NotificationEdge::TopLeft),
            ("top-center", NotificationEdge::TopCenter),
            ("top-right", NotificationEdge::TopRight),
            ("bottom-left", NotificationEdge::BottomLeft),
            ("bottom-center", NotificationEdge::BottomCenter),
            ("bottom-right", NotificationEdge::BottomRight),
        ] {
            let text = format!(
                "[notifications]\nenabled = false\nmonitor = \"DP-2\"\nedge = \"{name}\"\nhide-delay = 9\nmax-items = 3\n"
            );
            let notifications = load(&text).expect("popup settings load").notifications;
            assert!(!notifications.enabled);
            assert_eq!(notifications.monitor.as_deref(), Some("DP-2"));
            assert_eq!(notifications.edge, expected);
            assert_eq!(notifications.hide_delay, 9);
            assert_eq!(notifications.max_items, 3);
        }
    }

    #[test]
    fn popup_bounds_and_monitor_are_validated() {
        for text in [
            "[notifications]\nhide-delay = 0\n",
            "[notifications]\nhide-delay = 86401\n",
            "[notifications]\nmax-items = 0\n",
            "[notifications]\nmonitor = \"\"\n",
            "[notifications]\nmonitor = \" DP-2\"\n",
            "[notifications]\nedge = \"left\"\n",
        ] {
            assert!(load(text).is_err(), "must reject {text:?}");
        }
    }

    #[test]
    fn daemon_and_popup_settings_share_one_table() {
        let notifications = load(
            "[notifications]\nkeep = 20\nsuppress = [\"private\"]\nenabled = false\nedge = \"bottom-right\"\n",
        )
        .expect("the shared table loads")
        .notifications;

        assert_eq!(notifications.keep, 20);
        assert_eq!(notifications.suppress, ["private"]);
        assert!(!notifications.enabled);
        assert_eq!(notifications.edge, super::NotificationEdge::BottomRight);
    }

    #[test]
    fn popup_settings_round_trip_with_their_document_spellings() {
        let notifications = super::Notifications {
            monitor: Some("DP-2".to_owned()),
            edge: super::NotificationEdge::BottomLeft,
            hide_delay: 7,
            max_items: 4,
            ..super::Notifications::default()
        };
        let document = toml::to_string(&notifications).expect("serializes");
        assert!(document.contains("edge = \"bottom-left\""));
        assert!(document.contains("hide-delay = 7"));
        assert!(document.contains("max-items = 4"));
        assert_eq!(
            toml::from_str::<super::Notifications>(&document).expect("deserializes"),
            notifications
        );
    }
}

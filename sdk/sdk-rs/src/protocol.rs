use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StatusItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
}

impl StatusItem {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            ..Self::default()
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Push a single class onto `css_classes`. Builder-style for parity with
    /// `icon()` / `label()`. The shell will merge these on top of its own
    /// `exec-status-item` base class.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::StatusItem;

    /// `css_classes` is omitted from the wire when empty so older shells
    /// keep parsing payloads emitted by the latest SDK.
    #[test]
    fn empty_css_classes_is_omitted_from_json() {
        let json = serde_json::to_value(StatusItem::new("cpu").label("12%")).unwrap();
        assert!(json.get("css_classes").is_none(), "json was {json}");
    }

    /// Builder pushes onto `css_classes` and the field appears in the JSON.
    #[test]
    fn populated_css_classes_round_trips() {
        let item = StatusItem::new("cpu")
            .label("95%")
            .css_class("threshold-warn")
            .css_class("sysmonitor-cpu");
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json["css_classes"],
            serde_json::json!(["threshold-warn", "sysmonitor-cpu"])
        );
    }
}

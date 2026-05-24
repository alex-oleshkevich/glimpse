use glimpse_core::services::notifications::model::{NotificationEntry, State};
use relm4::gtk::gdk;

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{count} notifications";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePresentation {
    Content,
    AppIcon,
}

pub struct NotificationImage {
    pub texture: gdk::Texture,
    pub presentation: ImagePresentation,
}

pub fn icon_name(state: &State) -> &'static str {
    if state.dnd {
        "notifications-disabled-symbolic"
    } else {
        "preferences-system-notifications-symbolic"
    }
}

pub fn label(format: &str, state: &State) -> String {
    render(format, state.notifications.len(), state.dnd)
}

pub fn tooltip(format: &str, state: &State) -> String {
    if state.dnd {
        return "Do Not Disturb".into();
    }

    render(format, state.notifications.len(), state.dnd)
}

pub fn count_label(count: usize) -> String {
    match count {
        0 => "No notifications".into(),
        1 => "1 notification".into(),
        count => format!("{count} notifications"),
    }
}

pub fn source_name(notification: &NotificationEntry) -> &str {
    if notification.app_name.is_empty() {
        "Notification"
    } else {
        &notification.app_name
    }
}

pub fn relative_time(now_ms: u64, timestamp_ms: u64) -> String {
    let elapsed = now_ms.saturating_sub(timestamp_ms) / 1000;
    match elapsed {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", elapsed / 60),
        3600..=86399 => format!("{}h", elapsed / 3600),
        _ => format!("{}d", elapsed / 86400),
    }
}

pub fn visible_actions(notification: &NotificationEntry) -> impl Iterator<Item = (&str, &str)> {
    notification
        .actions
        .iter()
        .filter(|action| action.key != "default")
        .map(|action| (action.key.as_str(), action.label.as_str()))
}

pub fn app_icon(notification: &NotificationEntry) -> &str {
    if notification.app_icon.is_empty() {
        "dialog-information-symbolic"
    } else {
        notification.app_icon.as_str()
    }
}

pub fn load_image(notification: &NotificationEntry) -> Option<NotificationImage> {
    let image = notification.image.as_deref()?.trim();
    if image.is_empty() {
        return None;
    }

    let path = image.strip_prefix("file://").unwrap_or(image);
    if !path.starts_with('/') {
        return None;
    }

    let texture = gdk::Texture::from_filename(path).ok()?;
    Some(NotificationImage {
        texture,
        presentation: image_presentation_for_path(notification, path),
    })
}

pub fn image_presentation_for_path(
    notification: &NotificationEntry,
    image_path: &str,
) -> ImagePresentation {
    let Some(image_key) = icon_key(image_path) else {
        return ImagePresentation::Content;
    };

    if notification_icon_keys(notification).any(|key| key == image_key) {
        ImagePresentation::AppIcon
    } else {
        ImagePresentation::Content
    }
}

fn notification_icon_keys(notification: &NotificationEntry) -> impl Iterator<Item = String> + '_ {
    [
        notification.app_icon.as_str(),
        notification.desktop_entry.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .filter_map(icon_key)
}

fn icon_key(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix("file://").unwrap_or(value.trim());
    if value.is_empty() {
        return None;
    }

    let name = value.rsplit('/').next().unwrap_or(value).trim();
    let name = strip_known_suffix(
        name,
        &[".desktop", ".png", ".svg", ".jpg", ".jpeg", ".webp", ".ico"],
    );
    let name = strip_known_suffix(name, &["-symbolic"]);
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

fn strip_known_suffix<'a>(value: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes
        .iter()
        .find_map(|suffix| value.strip_suffix(suffix))
        .unwrap_or(value)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn render(format: &str, count: usize, dnd: bool) -> String {
    format
        .replace("{count}", &count.to_string())
        .replace("{state}", if dnd { "dnd" } else { "enabled" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::notifications::model::{NotificationAction, State};

    fn notification() -> NotificationEntry {
        NotificationEntry {
            id: 1,
            app_name: "Example".into(),
            app_icon: "org.example.App".into(),
            desktop_entry: Some("org.example.App.desktop".into()),
            summary: "Hello".into(),
            body: String::new(),
            urgency: 1,
            actions: vec![NotificationAction {
                key: "default".into(),
                label: "Open".into(),
            }],
            image: None,
            timestamp: 0,
            resident: false,
        }
    }

    #[test]
    fn renders_count_and_state_placeholders() {
        let state = State {
            dnd: true,
            ..State::default()
        };

        assert_eq!(label("{count}:{state}", &state), "0:dnd");
    }

    #[test]
    fn app_icon_theme_image_paths_use_app_icon_presentation() {
        let notification = notification();

        assert_eq!(
            image_presentation_for_path(
                &notification,
                "/usr/share/icons/hicolor/128x128/apps/org.example.App.png"
            ),
            ImagePresentation::AppIcon
        );
    }

    #[test]
    fn unrelated_image_paths_use_content_presentation() {
        let notification = notification();

        assert_eq!(
            image_presentation_for_path(&notification, "/home/alex/Pictures/screenshot.png"),
            ImagePresentation::Content
        );
    }
}

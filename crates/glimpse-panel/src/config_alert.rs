use gettextrs::gettext;
use glimpse_dbus::notifications::NotificationsProviderHandle;
use tokio::sync::watch;

const APP_ID: &str = "me.aresa.Glimpse.Config";
const ICON: &str = "dialog-warning-symbolic";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Problems {
    pub failure: Option<String>,
    pub rejected: Option<String>,
    pub unknown_applets: Vec<String>,
}

impl Problems {
    pub fn body(&self) -> Option<String> {
        let mut lines = Vec::new();
        if let Some(failure) = &self.failure {
            lines.push(failure.clone());
            lines.push(gettext(
                "Idle and night light are off until it is fixed; everything else uses its defaults.",
            ));
        }
        if let Some(rejected) = &self.rejected {
            lines.push(rejected.clone());
            lines.push(gettext(
                "The last change was not applied; the previous configuration is still running.",
            ));
        }
        lines.extend(self.unknown_applets.iter().cloned());
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    pub fn reject(&mut self, reason: String) {
        match &mut self.failure {
            Some(failure) => *failure = reason,
            None => self.rejected = Some(reason),
        }
    }

    pub fn recover(&mut self) {
        self.failure = None;
        self.rejected = None;
    }
}

pub fn spawn(
    notifications: NotificationsProviderHandle,
    body: Option<String>,
) -> watch::Sender<Option<String>> {
    let (sender, mut bodies) = watch::channel(body);
    let mut providers = notifications.subscribe();
    relm4::spawn(async move {
        let mut serving = false;
        let mut shown: Option<(u32, String)> = None;
        loop {
            let now = providers.borrow_and_update().view.is_some();
            if now && !serving {
                if let Err(error) = notifications.clear_application(APP_ID.to_owned()).await {
                    tracing::debug!(%error, "could not withdraw an earlier configuration alert");
                }
                shown = None;
            }
            serving = now;
            if serving {
                let wanted = bodies.borrow_and_update().clone();
                shown = show(&notifications, shown, wanted).await;
            }
            let alive = tokio::select! {
                changed = bodies.changed() => changed.is_ok(),
                changed = providers.changed() => changed.is_ok(),
            };
            if !alive {
                return;
            }
        }
    });
    sender
}

async fn show(
    notifications: &NotificationsProviderHandle,
    shown: Option<(u32, String)>,
    wanted: Option<String>,
) -> Option<(u32, String)> {
    match (shown, wanted) {
        (Some((id, body)), Some(wanted)) if body == wanted => Some((id, body)),
        (shown, Some(wanted)) => {
            let replaces = shown.map_or(0, |(id, _)| id);
            let posted = notifications
                .alert(
                    replaces,
                    &gettext("Glimpse"),
                    APP_ID,
                    ICON,
                    &gettext("Glimpse configuration has errors"),
                    &wanted,
                )
                .await;
            match posted {
                Ok(id) => Some((id, wanted)),
                Err(error) => {
                    tracing::warn!(%error, "could not show the configuration alert");
                    None
                }
            }
        }
        (Some(_), None) => {
            if let Err(error) = notifications.clear_application(APP_ID.to_owned()).await {
                tracing::warn!(%error, "could not withdraw the configuration alert");
            }
            None
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_document_has_nothing_to_say() {
        assert_eq!(Problems::default().body(), None);
    }

    #[test]
    fn a_failed_load_says_what_is_off_and_every_problem_is_listed() {
        let problems = Problems {
            failure: Some("config.toml: unknown field `panle`".to_owned()),
            rejected: None,
            unknown_applets: vec!["unknown applet `command-test`".to_owned()],
        };
        let body = problems.body().expect("there is something to say");
        assert!(body.contains("panle"));
        assert!(body.contains("Idle and night light are off"));
        assert!(body.contains("command-test"));
    }

    #[test]
    fn a_rejected_reload_while_on_defaults_replaces_the_failure() {
        let mut problems = Problems {
            failure: Some("first".to_owned()),
            ..Problems::default()
        };
        problems.reject("second".to_owned());
        assert_eq!(problems.failure.as_deref(), Some("second"));
        assert_eq!(problems.rejected, None);

        problems.recover();
        problems.reject("third".to_owned());
        assert_eq!(problems.rejected.as_deref(), Some("third"));

        problems.recover();
        assert_eq!(problems.body(), None);
    }
}

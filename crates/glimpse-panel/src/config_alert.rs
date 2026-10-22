use gettextrs::gettext;
use std::time::Duration;

use glimpse_dbus::notifications::{
    NotificationUrgency, NotificationsProviderError, NotificationsProviderHandle,
};
use tokio::sync::watch;

const ICON: &str = "dialog-warning-symbolic";
const RETRY: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Problems {
    pub failure: Option<String>,
    pub panels_only: bool,
    pub rejected: Option<String>,
    pub theme: Option<String>,
    pub unknown_applets: Vec<String>,
}

impl Problems {
    pub fn verdict(&self) -> Result<(), String> {
        self.failure
            .as_ref()
            .or(self.rejected.as_ref())
            .or(self.theme.as_ref())
            .map_or(Ok(()), |reason| Err(reason.clone()))
    }

    pub fn body(&self) -> Option<String> {
        let mut lines = Vec::new();
        if let Some(failure) = &self.failure {
            lines.push(failure.clone());
            lines.push(match self.panels_only {
                true => gettext(
                    "The panels use their defaults until it is fixed; everything else is as configured.",
                ),
                false => gettext(
                    "Idle and night light are off until it is fixed; everything else uses its defaults.",
                ),
            });
        }
        if let Some(rejected) = &self.rejected {
            lines.push(rejected.clone());
            lines.push(gettext(
                "The last change was not applied; the previous configuration is still running.",
            ));
        }
        if let Some(theme) = &self.theme {
            lines.push(
                gettext("A stylesheet could not be read: {reason}").replace("{reason}", theme),
            );
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
    let app_id = format!("{}.Config", crate::applet::app_id());
    relm4::spawn(async move {
        let mut serving = false;
        let mut shown: Option<Option<String>> = None;
        loop {
            let now = providers.borrow_and_update().view.is_some();
            if now && !serving {
                shown = None;
            }
            serving = now;
            let mut failed = false;
            if serving {
                let wanted = bodies.borrow_and_update().clone();
                if shown.as_ref() != Some(&wanted) {
                    match show(&notifications, &app_id, wanted.as_deref()).await {
                        Ok(()) => shown = Some(wanted),
                        Err(error) => {
                            tracing::warn!(%error, "could not update the configuration alert");
                            shown = None;
                            failed = true;
                        }
                    }
                }
            }
            let alive = tokio::select! {
                changed = bodies.changed() => changed.is_ok(),
                changed = providers.changed() => changed.is_ok(),
                () = tokio::time::sleep(RETRY), if failed => true,
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
    app_id: &str,
    body: Option<&str>,
) -> Result<(), NotificationsProviderError> {
    notifications.clear_application(app_id.to_owned()).await?;
    if let Some(body) = body {
        notifications
            .post(
                &gettext("Glimpse"),
                app_id,
                ICON,
                &gettext("Glimpse configuration has errors"),
                body,
                NotificationUrgency::Normal,
            )
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

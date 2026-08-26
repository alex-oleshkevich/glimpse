use anyhow::Result;
use glimpse_contracts::{ServiceState, SystemServices};

use super::{ABSENT, Session, absent};
use crate::render::{Table, styled};

pub(super) const SERVICES: &str = "system.services";

pub async fn services(session: &Session) -> Result<()> {
    let Some(event) = session.client.get(SERVICES).await? else {
        return absent(SERVICES, false);
    };

    let report: SystemServices = serde_json::from_value(event.data)?;

    Table::new()
        .with_headers(["SERVICE", "STATE", "DETAIL"])
        .with_empty("the daemon has no registered services")
        .with_rows(report.services.iter().map(|(name, state)| {
            let (state, detail) = described(state);
            [name.clone(), state, styled::key(&detail)]
        }))
        .print()
}

/// The state as it should read, and whatever detail belongs beside it.
pub(super) fn described(state: &ServiceState) -> (String, String) {
    match state {
        ServiceState::Starting => (styled::warn("starting"), String::new()),
        ServiceState::Running => (styled::good("running"), String::new()),
        ServiceState::Degraded { reason } => (styled::warn("degraded"), reason.clone()),
        ServiceState::Stopped { reason } => (
            styled::bad("stopped"),
            reason.clone().unwrap_or_else(|| ABSENT.to_owned()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_degraded_service_shows_its_reason() {
        let (state, detail) = described(&ServiceState::Degraded {
            reason: "no system bus".into(),
        });
        assert!(state.contains("degraded"));
        assert_eq!(detail, "no system bus");
    }
}

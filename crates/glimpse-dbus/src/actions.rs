use std::collections::HashMap;

use zbus::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Signature, Str, Value};

pub struct ComponentActions {
    version: String,
    status: (bool, String),
    status_value: OwnedValue,
}

#[zbus::interface(name = "org.gtk.Actions")]
impl ComponentActions {
    fn list(&self) -> Vec<String> {
        vec!["version".into(), "config-status".into()]
    }

    fn describe(
        &self,
        action_name: &str,
    ) -> zbus::fdo::Result<((bool, Signature, Vec<OwnedValue>),)> {
        match action_name {
            "version" => Ok((self.version_description(),)),
            "config-status" => Ok((self.status_description(),)),
            _ => Err(zbus::fdo::Error::InvalidArgs(format!(
                "unknown action: {action_name}"
            ))),
        }
    }

    fn describe_all(&self) -> HashMap<String, (bool, Signature, Vec<OwnedValue>)> {
        HashMap::from([
            ("version".into(), self.version_description()),
            ("config-status".into(), self.status_description()),
        ])
    }

    fn activate(
        &self,
        action_name: &str,
        parameter: Vec<OwnedValue>,
        platform_data: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        let _ = (parameter, platform_data);
        Err(zbus::fdo::Error::InvalidArgs(format!(
            "action is read-only or unknown: {action_name}"
        )))
    }

    fn set_state(
        &self,
        action_name: &str,
        value: OwnedValue,
        platform_data: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        let _ = (value, platform_data);
        Err(zbus::fdo::Error::InvalidArgs(format!(
            "action is read-only or unknown: {action_name}"
        )))
    }

    #[zbus(signal)]
    async fn changed(
        emitter: &SignalEmitter<'_>,
        removals: Vec<String>,
        enable_changes: HashMap<String, bool>,
        state_changes: HashMap<String, OwnedValue>,
        additions: HashMap<String, (bool, Signature, Vec<OwnedValue>)>,
    ) -> zbus::Result<()>;
}

impl ComponentActions {
    fn version_description(&self) -> (bool, Signature, Vec<OwnedValue>) {
        (
            true,
            Signature::Unit,
            vec![OwnedValue::from(Str::from(self.version.as_str()))],
        )
    }

    fn status_description(&self) -> (bool, Signature, Vec<OwnedValue>) {
        (true, Signature::Unit, vec![self.status_value.clone()])
    }
}

pub struct ActionsHandle {
    connection: Connection,
    path: &'static str,
}

impl ActionsHandle {
    pub async fn report(&self, verdict: Result<(), String>) -> zbus::Result<()> {
        let status = status(verdict);
        let interface = self
            .connection
            .object_server()
            .interface::<_, ComponentActions>(self.path)
            .await?;
        let mut actions = interface.get_mut().await;
        if actions.status == status {
            return Ok(());
        }
        let value = OwnedValue::try_from(Value::from(status.clone()))?;
        actions.status = status;
        actions.status_value = value.clone();
        ComponentActions::changed(
            interface.signal_emitter(),
            Vec::new(),
            HashMap::new(),
            HashMap::from([("config-status".into(), value)]),
            HashMap::new(),
        )
        .await
    }
}

pub async fn serve(
    connection: &Connection,
    path: &'static str,
    version: &str,
    initial: Result<(), String>,
) -> zbus::Result<ActionsHandle> {
    let status = status(initial);
    let status_value = OwnedValue::try_from(Value::from(status.clone()))?;
    connection
        .object_server()
        .at(
            path,
            ComponentActions {
                version: version.into(),
                status,
                status_value,
            },
        )
        .await?;
    Ok(ActionsHandle {
        connection: connection.clone(),
        path,
    })
}

fn status(verdict: Result<(), String>) -> (bool, String) {
    match verdict {
        Ok(()) => (true, String::new()),
        Err(message) => (false, message),
    }
}

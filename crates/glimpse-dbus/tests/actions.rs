use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use glimpse_dbus::actions::serve;
use glimpse_dbus::testing::PrivateBus;
use zbus::zvariant::{OwnedValue, Signature, Structure};

const NAME: &str = "org.test.ComponentActions";
const PATH: &str = "/org/test/ComponentActions";

type Descriptions = HashMap<String, (bool, Signature, Vec<OwnedValue>)>;

fn state(value: &OwnedValue) -> (bool, String) {
    let structure = Structure::try_from(value.clone()).expect("a structure state");
    structure.try_into().expect("a boolean and message")
}

fn invalid_args(error: zbus::Error) {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.as_str(), "org.freedesktop.DBus.Error.InvalidArgs");
        }
        other => panic!("expected InvalidArgs, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actions_match_gtk_wire_contract_and_report_changes() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    glimpse_dbus::own_name(&server, NAME)
        .await
        .expect("the name");
    let handle = serve(&server, PATH, "1.2.3", Ok(()))
        .await
        .expect("the actions");

    let client = bus.connection().await;
    let reply = client
        .call_method(
            Some(NAME),
            PATH,
            Some("org.freedesktop.DBus.Introspectable"),
            "Introspect",
            &(),
        )
        .await
        .expect("introspection answers");
    let xml: String = reply.body().deserialize().expect("the XML");
    let start = xml
        .find("<interface name=\"org.gtk.Actions\">")
        .expect("the interface");
    let end = start + xml[start..].find("</interface>").expect("the end") + "</interface>".len();
    assert_eq!(
        &xml[start..end],
        r#"<interface name="org.gtk.Actions">
    <method name="List">
      <arg type="as" direction="out"/>
    </method>
    <method name="Describe">
      <arg name="action_name" type="s" direction="in"/>
      <arg type="(bgav)" direction="out"/>
    </method>
    <method name="DescribeAll">
      <arg type="a{s(bgav)}" direction="out"/>
    </method>
    <method name="Activate">
      <arg name="action_name" type="s" direction="in"/>
      <arg name="parameter" type="av" direction="in"/>
      <arg name="platform_data" type="a{sv}" direction="in"/>
    </method>
    <method name="SetState">
      <arg name="action_name" type="s" direction="in"/>
      <arg name="value" type="v" direction="in"/>
      <arg name="platform_data" type="a{sv}" direction="in"/>
    </method>
    <signal name="Changed">
      <arg name="removals" type="as"/>
      <arg name="enable_changes" type="a{sb}"/>
      <arg name="state_changes" type="a{sv}"/>
      <arg name="additions" type="a{s(bgav)}"/>
    </signal>
  </interface>"#
    );

    let proxy = zbus::Proxy::new(&client, NAME, PATH, "org.gtk.Actions")
        .await
        .expect("the proxy");
    let listed: Vec<String> = proxy.call("List", &()).await.expect("the list");
    assert_eq!(listed, ["version", "config-status"]);
    let descriptions: Descriptions = proxy
        .call("DescribeAll", &())
        .await
        .expect("the descriptions");
    assert_eq!(descriptions.len(), 2);
    assert!(descriptions.values().all(|(enabled, parameter, state)| {
        *enabled && *parameter == Signature::Unit && state.len() == 1
    }));
    assert_eq!(
        String::try_from(descriptions["version"].2[0].clone()).expect("version"),
        "1.2.3"
    );
    assert_eq!(
        state(&descriptions["config-status"].2[0]),
        (true, String::new())
    );
    let described: ((bool, Signature, Vec<OwnedValue>),) = proxy
        .call("Describe", &("config-status",))
        .await
        .expect("one description");
    assert_eq!(state(&described.0.2[0]), (true, String::new()));

    let mut changes = proxy
        .receive_signal("Changed")
        .await
        .expect("the signal stream");
    handle
        .report(Err("bad".into()))
        .await
        .expect("reporting works");
    let changed = tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .expect("a signal arrives")
        .expect("a live signal");
    let (removals, enabled, states, additions): (
        Vec<String>,
        HashMap<String, bool>,
        HashMap<String, OwnedValue>,
        Descriptions,
    ) = changed.body().deserialize().expect("changed arguments");
    assert!(removals.is_empty());
    assert!(enabled.is_empty());
    assert!(additions.is_empty());
    assert_eq!(states.len(), 1);
    assert_eq!(state(&states["config-status"]), (false, "bad".into()));
    let descriptions: Descriptions = proxy
        .call("DescribeAll", &())
        .await
        .expect("new descriptions");
    assert_eq!(
        state(&descriptions["config-status"].2[0]),
        (false, "bad".into())
    );
    handle
        .report(Err("bad".into()))
        .await
        .expect("same verdict");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changes.next())
            .await
            .is_err()
    );

    let no_values: Vec<OwnedValue> = Vec::new();
    let no_data: HashMap<String, OwnedValue> = HashMap::new();
    invalid_args(
        proxy
            .call::<_, _, ()>("Activate", &("version", no_values, no_data.clone()))
            .await
            .expect_err("read-only"),
    );
    invalid_args(
        proxy
            .call::<_, _, ()>(
                "SetState",
                &("config-status", OwnedValue::from(true), no_data),
            )
            .await
            .expect_err("read-only"),
    );
    invalid_args(
        proxy
            .call::<_, _, ((bool, Signature, Vec<OwnedValue>),)>("Describe", &("missing",))
            .await
            .expect_err("unknown"),
    );
    let descriptions: Descriptions = proxy
        .call("DescribeAll", &())
        .await
        .expect("descriptions after the refused calls");
    assert_eq!(
        state(&descriptions["config-status"].2[0]),
        (false, "bad".into())
    );
}

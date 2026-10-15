use glimpse_dbus::clipboard::{
    ClipboardItem, GLIMPSE_CLIPBOARD_BUS_NAME, GLIMPSE_CLIPBOARD_OBJECT_PATH,
};
use glimpse_dbus::testing::PrivateBus;

struct Clipboard;

#[zbus::interface(name = "me.aresa.Glimpse.Clipboard1")]
impl Clipboard {
    fn list(&self) -> Vec<ClipboardItem> {
        Vec::new()
    }

    fn get(&self, id: u64) -> zbus::fdo::Result<(String, Vec<u8>)> {
        if id != 7 {
            return Err(zbus::fdo::Error::InvalidArgs("unknown entry".into()));
        }
        Ok(("application/octet-stream".into(), vec![0, 255, 10]))
    }

    fn forget(&self, id: u64) -> zbus::fdo::Result<()> {
        if id != 7 {
            return Err(zbus::fdo::Error::InvalidArgs("unknown entry".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn clipboard_json_preserves_list_arrays_and_represents_exact_bytes() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(GLIMPSE_CLIPBOARD_OBJECT_PATH, Clipboard)
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_CLIPBOARD_BUS_NAME)
        .await
        .unwrap();
    let run = |args: &[&str]| {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"));
        command
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", bus.address());
        command
    };
    let output = run(&["clipboard", "list", "--json"])
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!([])
    );
    let output = run(&["clipboard", "get", "7"]).output().await.unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, [0, 255, 10]);
    let output = run(&["clipboard", "get", "7", "--json"])
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"id": 7, "mime": "application/octet-stream", "data": [0, 255, 10]})
    );
    for verb in ["remove", "forget"] {
        let output = run(&["clipboard", verb, "7", "--json"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("deprecated"),
            verb == "forget"
        );
    }
}

#[test]
fn json_usage_errors_end_with_the_documented_error_object() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["--json", "clipboard", "get", "invalid"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let error: serde_json::Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
    assert_eq!(error["exit"], 2);
    assert!(error["error"].as_str().unwrap().contains("invalid"));
}

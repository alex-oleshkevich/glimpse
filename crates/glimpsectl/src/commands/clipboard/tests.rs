use super::*;
use glimpse_dbus::testing::PrivateBus;

fn item(kind: &str, preview: &str, size: u64) -> ClipboardItem {
    ClipboardItem {
        id: 7,
        kind: kind.to_owned(),
        mime: "image/png".to_owned(),
        pinned: true,
        copied_at_secs: 0,
        size,
        preview: preview.to_owned(),
    }
}

#[test]
fn an_image_reads_as_its_mime_and_size_rather_than_its_preview() {
    assert_eq!(
        preview(&item("image", "ignored", 188_416)),
        "image/png · 184 KiB"
    );
}

#[test]
fn a_text_preview_is_cleaned_and_capped() {
    let long = "a\u{1b}[31m".to_owned() + &"x".repeat(200);
    let shown = preview(&item("text", &long, 0));
    assert!(!shown.contains('\u{1b}'));
    assert!(shown.chars().count() <= PREVIEW + 1);
}

#[test]
fn sizes_step_through_their_units() {
    assert_eq!(size(512), "512 B");
    assert_eq!(size(2048), "2 KiB");
    assert_eq!(size(3 * 1_048_576), "3.0 MiB");
}

#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "me.aresa.Glimpse.Clipboard1.Error")]
enum ServedError {
    NoSuchEntry(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

struct Served;

#[zbus::interface(name = "me.aresa.Glimpse.Clipboard1")]
impl Served {
    fn get(&self, id: u64) -> Result<(String, Vec<u8>), ServedError> {
        match id {
            1 => Ok(("text/plain".to_owned(), Vec::new())),
            _ => Err(ServedError::NoSuchEntry(format!("no entry {id}"))),
        }
    }
}

#[tokio::test]
async fn missing_clipboard_is_unreachable() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let error = clipboard_get(&client, 1).await.unwrap_err();
    assert_eq!(
        crate::errors::exit(&error),
        crate::errors::Exit::Unreachable
    );
}

#[tokio::test]
async fn get_round_trips_and_an_unknown_entry_is_rejected() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    glimpse_dbus::own_name(&server, "me.aresa.Glimpse.Clipboard")
        .await
        .expect("the name");
    server
        .object_server()
        .at("/me/aresa/Glimpse/Clipboard", Served)
        .await
        .unwrap();
    let client = bus.connection().await;

    clipboard_get(&client, 1).await.unwrap();

    let error = clipboard_get(&client, 2).await.unwrap_err();
    assert_eq!(crate::errors::exit(&error), crate::errors::Exit::Rejected);
}

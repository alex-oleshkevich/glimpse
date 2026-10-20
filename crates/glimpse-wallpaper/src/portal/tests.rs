use super::*;
use glimpse_dbus::testing::PrivateBus;
use gtk4::gdk_pixbuf::{Colorspace, Pixbuf};
use validate::Reject;
use zbus::zvariant::Value;

fn options(pairs: &[(&str, &str)]) -> HashMap<String, OwnedValue> {
    pairs
        .iter()
        .map(|(key, value)| {
            (
                (*key).to_owned(),
                OwnedValue::try_from(Value::from(*value)).expect("a value"),
            )
        })
        .collect()
}

fn handle(path: &str) -> ObjectPath<'static> {
    ObjectPath::try_from(path)
        .expect("an object path")
        .into_owned()
}

fn image_file(dir: &Path, name: &str) -> PathBuf {
    sized_image_file(dir, name, 4, 4)
}

fn sized_image_file(dir: &Path, name: &str, width: i32, height: i32) -> PathBuf {
    let path = dir.join(name);
    let pixbuf = Pixbuf::new(Colorspace::Rgb, false, 8, width, height).expect("a pixbuf");
    pixbuf.fill(0xff0000ff);
    pixbuf.savev(&path, "png", &[]).expect("saves");
    path
}

#[test]
fn only_a_file_uri_with_no_host_becomes_a_path() {
    assert_eq!(
        file_path_from_uri("file:///tmp/a%20b.png"),
        Ok(PathBuf::from("/tmp/a b.png"))
    );
    assert_eq!(
        file_path_from_uri("file://localhost/tmp/a.png"),
        Ok(PathBuf::from("/tmp/a.png"))
    );
    for uri in [
        "https://example.com/a.png",
        "file://evil.example/a.png",
        "a.png",
        "",
        "data:image/png;base64,AAAA",
    ] {
        assert_eq!(file_path_from_uri(uri), Err(Reject::NotFileUri), "{uri}");
    }
}

#[test]
fn lockscreen_is_the_only_refused_target() {
    assert_eq!(
        target_of(&options(&[("set-on", "lockscreen")])),
        Target::Lockscreen
    );
    assert_eq!(
        target_of(&options(&[("set-on", "background")])),
        Target::Background
    );
    assert_eq!(
        target_of(&options(&[("set-on", "both")])),
        Target::Background
    );
    assert_eq!(target_of(&HashMap::new()), Target::Background);
}

#[test]
fn previous_values_round_trip_through_restore() {
    let prev =
        prev_from("[wallpaper]\nimage = \"a.jpg\"\nimage-dark = \"b.jpg\"\nfit = \"fill\"\n")
            .expect("parses");
    assert_eq!(prev.image.as_deref(), Some("a.jpg"));
    assert_eq!(prev.image_dark.as_deref(), Some("b.jpg"));
    let (values, remove) = restore_values(&prev);
    assert!(remove.is_empty());
    assert_eq!(
        values
            .get("image")
            .and_then(toml_edit::Item::as_str)
            .expect("image"),
        "a.jpg"
    );

    let (values, remove) = restore_values(&Prev::default());
    assert!(values.is_empty());
    assert_eq!(remove, ["image", "image-dark"]);
}

#[test]
fn outputs_with_their_own_image_are_counted() {
    let mut config = glimpse_config::Config::default();
    assert_eq!(outputs_kept(&config), 0);
    config.wallpaper.outputs = vec![
        glimpse_config::WallpaperOutput {
            monitor: "DP-1".to_owned(),
            image: Some(PathBuf::from("a.jpg")),
            ..Default::default()
        },
        glimpse_config::WallpaperOutput {
            monitor: "DP-2".to_owned(),
            ..Default::default()
        },
        glimpse_config::WallpaperOutput {
            monitor: "HDMI-A-1".to_owned(),
            image_dark: Some(PathBuf::from("b.jpg")),
            ..Default::default()
        },
    ];
    assert_eq!(outputs_kept(&config), 2);
    assert_eq!(notify::changed_body(0), "");
    assert!(notify::changed_body(1).contains('1'));
    assert!(notify::changed_body(2).contains('2'));
    assert!(notify::lockscreen_body().contains("background"));
    assert!(notify::shadowed_body(Path::new("/home/x/config.d/10-a.toml")).contains("10-a.toml"));
}

#[test]
fn every_rejection_has_its_own_text() {
    let texts: Vec<String> = [
        Reject::NotFileUri,
        Reject::TooLarge,
        Reject::TooBig,
        Reject::NotImage,
        Reject::Unreadable,
    ]
    .into_iter()
    .map(reason_text)
    .collect();
    assert!(texts.iter().all(|text| !text.is_empty()));
    assert!(texts[1].contains("64"));
    assert!(texts[2].contains("8192"));
    let unique: std::collections::HashSet<&String> = texts.iter().collect();
    assert_eq!(unique.len(), texts.len());
}

#[tokio::test]
async fn a_valid_image_is_copied_and_a_rejected_one_leaves_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let copies = dir.path().join("copies");
    let source = image_file(dir.path(), "My Shot.PNG");

    let copy = copy_image(&source, &copies).await.expect("copies");
    assert!(copy.is_file());
    assert_eq!(copy.parent(), Some(copies.as_path()));
    assert!(copy.to_string_lossy().ends_with(".png"));

    let text = dir.path().join("note.txt");
    std::fs::write(&text, "not an image").expect("writes");
    assert_eq!(copy_image(&text, &copies).await, Err(Reject::NotImage));
    assert_eq!(
        copy_image(&dir.path().join("missing.png"), &copies).await,
        Err(Reject::Unreadable)
    );
    assert_eq!(
        std::fs::read_dir(&copies).expect("lists").count(),
        1,
        "only the accepted copy remains"
    );
}

#[tokio::test]
async fn an_image_wider_than_the_ceiling_is_too_big_and_leaves_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let copies = dir.path().join("copies");
    let source = sized_image_file(dir.path(), "panorama.png", 9000, 16);

    assert_eq!(copy_image(&source, &copies).await, Err(Reject::TooBig));
    assert_eq!(std::fs::read_dir(&copies).expect("lists").count(), 0);
}

#[test]
fn collecting_copies_keeps_only_the_named_files() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let keep = dir.path().join("keep.png");
    let drop = dir.path().join("drop.png");
    std::fs::write(&keep, "x").expect("writes");
    std::fs::write(&drop, "x").expect("writes");

    collect_copies(dir.path(), std::slice::from_ref(&keep));

    assert!(keep.is_file());
    assert!(!drop.exists());
}

struct Started {
    _bus: PrivateBus,
    _server: Connection,
    directory: tempfile::TempDir,
    client: Connection,
    config: PathBuf,
    copies: PathBuf,
}

impl Started {
    async fn new(dropin: Option<&str>) -> Self {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let config = directory.path().join("config.toml");
        std::fs::write(
            &config,
            "[wallpaper]\nimage = \"old.jpg\"\nimage-dark = \"dark.jpg\"\n",
        )
        .expect("writes");
        if let Some(text) = dropin {
            let dropins = directory.path().join("config.d");
            std::fs::create_dir_all(&dropins).expect("creates");
            std::fs::write(dropins.join("10.toml"), text).expect("writes");
        }
        let copies = directory.path().join("state").join("wallpaper");
        let bus = PrivateBus::start();
        let server = bus.connection().await;
        glimpse_dbus::own_name(&server, BUS_NAME)
            .await
            .expect("the portal name");
        serve_at(
            &server,
            Settings {
                config_path: dropin.is_none().then(|| config.clone()),
                user_config: config.clone(),
                user_dir: dropin.map(|_| directory.path().to_path_buf()),
                copies: copies.clone(),
            },
        )
        .await
        .expect("the portal is served");
        let client = bus.connection().await;
        Self {
            _bus: bus,
            _server: server,
            directory,
            client,
            config,
            copies,
        }
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.impl.portal.Wallpaper",
    default_service = "me.aresa.Glimpse.Wallpaper.Portal",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait WallpaperPortalTest {
    #[zbus(name = "SetWallpaperURI")]
    fn set_wallpaper_uri(
        &self,
        handle: &ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        uri: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<u32>;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setting_the_background_copies_the_image_and_rewrites_the_config() {
    let started = Started::new(None).await;
    let proxy = WallpaperPortalTestProxy::new(&started.client)
        .await
        .expect("a proxy");
    let source = image_file(started.directory.path(), "shot.png");
    let uri = format!("file://{}", source.display());

    let response = proxy
        .set_wallpaper_uri(
            &handle("/req/background"),
            "org.gnome.Loupe",
            "",
            &uri,
            HashMap::from([("set-on", Value::from("background"))]),
        )
        .await
        .expect("SetWallpaperURI");
    assert_eq!(response, SUCCESS);

    let written = std::fs::read_to_string(&started.config).expect("reads");
    let prev = prev_from(&written).expect("parses");
    let image = prev.image.as_deref().expect("an image");
    assert!(image.contains("state/wallpaper/wallpaper-"), "{written}");
    assert_eq!(prev.image_dark.as_deref(), Some(image), "{written}");
    assert_eq!(
        std::fs::read_dir(&started.copies).expect("lists").count(),
        1
    );

    let before = std::fs::read_to_string(&started.config).expect("reads");
    let response = proxy
        .set_wallpaper_uri(
            &handle("/req/lockscreen"),
            "",
            "",
            &uri,
            HashMap::from([("set-on", Value::from("lockscreen"))]),
        )
        .await
        .expect("SetWallpaperURI");
    assert_eq!(response, OTHER);
    assert_eq!(
        std::fs::read_to_string(&started.config).expect("reads"),
        before
    );

    let response = proxy
        .set_wallpaper_uri(
            &handle("/req/remote"),
            "",
            "",
            "https://example.com/a.png",
            HashMap::new(),
        )
        .await
        .expect("SetWallpaperURI");
    assert_eq!(response, OTHER);
    assert_eq!(
        std::fs::read_to_string(&started.config).expect("reads"),
        before
    );
    assert_eq!(
        std::fs::read_dir(&started.copies).expect("lists").count(),
        1
    );
}

async fn set_background(started: &Started, path: &str) -> u32 {
    let proxy = WallpaperPortalTestProxy::new(&started.client)
        .await
        .expect("a proxy");
    let source = image_file(started.directory.path(), "shot.png");
    proxy
        .set_wallpaper_uri(
            &handle(path),
            "",
            "",
            &format!("file://{}", source.display()),
            HashMap::new(),
        )
        .await
        .expect("SetWallpaperURI")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dropin_setting_image_dark_refuses_the_set_and_leaves_the_config() {
    let started = Started::new(Some("[wallpaper]\nimage-dark = \"mine.jpg\"\n")).await;
    let before = std::fs::read_to_string(&started.config).expect("reads");

    assert_eq!(set_background(&started, "/req/shadowed").await, OTHER);
    assert_eq!(
        std::fs::read_to_string(&started.config).expect("reads"),
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unwritable_config_refuses_the_set_and_removes_the_copy() {
    use std::os::unix::fs::PermissionsExt;
    let started = Started::new(None).await;
    std::fs::set_permissions(&started.config, std::fs::Permissions::from_mode(0o444))
        .expect("sets permissions");

    assert_eq!(set_background(&started, "/req/unwritable").await, OTHER);
    assert_eq!(
        std::fs::read_dir(&started.copies).expect("lists").count(),
        0
    );
}

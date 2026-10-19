use std::collections::VecDeque;
use std::time::Duration;

use glimpse_dbus::testing::PrivateBus;
use glimpse_shot::doc::wire::TargetV1;
use tokio::sync::{oneshot, watch};
use zbus::zvariant::Value;

use super::*;

#[test]
fn a_screenshot_is_silent_only_when_the_portal_already_checked_permission() {
    assert_eq!(decide(true, false), CaptureKind::Overlay);
    assert_eq!(decide(true, true), CaptureKind::Overlay);
    assert_eq!(decide(false, false), CaptureKind::Overlay);
    assert_eq!(decide(false, true), CaptureKind::Headless);
}

#[test]
fn a_missing_or_mistyped_option_is_false() {
    let mut options = HashMap::new();
    assert!(!option_flag(&options, "interactive"));
    assert!(!option_flag(&options, "permission_store_checked"));
    options.insert(
        "interactive".to_owned(),
        OwnedValue::try_from(Value::from("true")).expect("a string"),
    );
    assert!(!option_flag(&options, "interactive"));
    options.insert(
        "permission_store_checked".to_owned(),
        OwnedValue::try_from(Value::from(true)).expect("a bool"),
    );
    assert!(option_flag(&options, "permission_store_checked"));
}

#[test]
fn the_app_name_is_capped_and_an_empty_one_reads_an_application() {
    let raw = format!("<b>{}</b>", "x".repeat(300));
    assert_eq!(app_label(&raw), glimpse_utils::text::clean(&raw, APP_CAP));
    assert!(app_label(&raw).chars().count() < raw.chars().count());
    assert_eq!(app_label(""), gettextrs::gettext("An application"));
    assert_eq!(app_label(" \n\t"), gettextrs::gettext("An application"));
    assert_eq!(app_label("<b>GIMP</b>"), "<b>GIMP</b>");
}

#[test]
fn a_file_uri_is_absolute_and_percent_encoded() {
    assert_eq!(file_uri(Path::new("shot.png")), None);
    assert_eq!(
        file_uri(Path::new("/tmp/my shot.png")).as_deref(),
        Some("file:///tmp/my%20shot.png")
    );
    assert_eq!(
        file_uri(Path::new("/tmp/déjà vu.png")).as_deref(),
        Some("file:///tmp/d%C3%A9j%C3%A0%20vu.png")
    );
}

#[test]
fn child_exit_codes_map_onto_portal_responses() {
    let shot = result_stdout("/tmp/shot.png");
    let saved = response_of(
        RequestKind::Shot,
        &Finished {
            stdout: shot.into_bytes(),
            code: Some(0),
        },
    );
    assert_eq!(saved.0, SUCCESS);
    assert_eq!(
        saved
            .1
            .get("uri")
            .and_then(|value| String::try_from(value.clone()).ok()),
        Some("file:///tmp/shot.png".to_owned())
    );

    let refused = response_of(
        RequestKind::Shot,
        &Finished {
            stdout: Vec::new(),
            code: Some(4),
        },
    );
    assert_eq!(refused.0, CANCELLED);
    assert!(refused.1.is_empty());

    for code in [None, Some(1), Some(7), Some(9)] {
        let failed = response_of(
            RequestKind::Shot,
            &Finished {
                stdout: result_stdout("/tmp/shot.png").into_bytes(),
                code,
            },
        );
        assert_eq!(failed.0, OTHER, "{code:?}");
        assert!(failed.1.is_empty());
    }

    let garbage = response_of(
        RequestKind::Shot,
        &Finished {
            stdout: b"not json".to_vec(),
            code: Some(0),
        },
    );
    assert_eq!(garbage.0, OTHER);

    let relative = response_of(
        RequestKind::Shot,
        &Finished {
            stdout: result_stdout("shot.png").into_bytes(),
            code: Some(0),
        },
    );
    assert_eq!(relative.0, OTHER);

    let color = response_of(
        RequestKind::Color,
        &Finished {
            stdout: br#"{"format":"rgb","value":"rgb(224 86 63)","red":224,"green":86,"blue":63}"#
                .to_vec(),
            code: Some(0),
        },
    );
    assert_eq!(color.0, SUCCESS);
    let (red, green, blue) =
        <(f64, f64, f64)>::try_from(color.1.get("color").expect("color").clone()).expect("(ddd)");
    assert_eq!(
        (red, green, blue),
        (224.0 / 255.0, 86.0 / 255.0, 63.0 / 255.0)
    );

    let cancelled_color = response_of(
        RequestKind::Color,
        &Finished {
            stdout: Vec::new(),
            code: Some(4),
        },
    );
    assert_eq!(cancelled_color.0, CANCELLED);
}

#[test]
fn an_executable_beside_this_binary_wins_over_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let current = dir.path().join("glimpse-shot");
    let picker = dir.path().join("glimpse-lens");
    std::fs::write(&current, b"").expect("writes");
    std::fs::write(&picker, b"").expect("writes");
    assert_eq!(resolve_executable("glimpse-lens", Some(&current)), picker);
    assert_eq!(
        resolve_executable("missing", Some(&current)),
        PathBuf::from("missing")
    );
}

#[derive(Clone)]
enum Script {
    Done { stdout: String, code: i32 },
    Hang,
}

struct Fake {
    scripts: Mutex<VecDeque<Script>>,
    argv: Mutex<Vec<Vec<OsString>>>,
    launches: watch::Sender<usize>,
    killed: Arc<std::sync::atomic::AtomicBool>,
}

impl Fake {
    fn new() -> Self {
        Self {
            scripts: Mutex::new(VecDeque::new()),
            argv: Mutex::new(Vec::new()),
            launches: watch::channel(0).0,
            killed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn push(&self, script: Script) {
        lock(&self.scripts).push_back(script);
    }

    fn argv(&self) -> Vec<Vec<OsString>> {
        lock(&self.argv).clone()
    }
}

impl Spawner for Fake {
    fn launch(&self, argv: Vec<OsString>) -> BoxFuture<'static, std::io::Result<Running>> {
        let script = lock(&self.scripts).pop_front();
        let Some(script) = script else {
            return Box::pin(async { Err(std::io::Error::other("no scripted child")) });
        };
        lock(&self.argv).push(argv);
        let killed = Arc::clone(&self.killed);
        let running = match script {
            Script::Done { stdout, code } => Running {
                kill: Arc::new(move || {
                    killed.store(true, Ordering::SeqCst);
                }),
                wait: Box::pin(async move {
                    Finished {
                        stdout: stdout.into_bytes(),
                        code: Some(code),
                    }
                }),
            },
            Script::Hang => {
                let (sender, receiver) = oneshot::channel();
                let sender = Arc::new(Mutex::new(Some(sender)));
                let held = Arc::clone(&sender);
                Running {
                    kill: Arc::new(move || {
                        killed.store(true, Ordering::SeqCst);
                        if let Some(sender) = lock(&held).take() {
                            let _ = sender.send(());
                        }
                    }),
                    wait: Box::pin(async move {
                        let _ = receiver.await;
                        Finished {
                            stdout: Vec::new(),
                            code: None,
                        }
                    }),
                }
            }
        };
        let count = self.launches.borrow().wrapping_add(1);
        self.launches.send_replace(count);
        Box::pin(async move { Ok(running) })
    }
}

struct Started {
    _bus: PrivateBus,
    _server: Connection,
    directory: tempfile::TempDir,
    client: Connection,
    fake: Arc<Fake>,
}

impl Started {
    async fn new() -> Self {
        Self::with_directory(None).await
    }

    async fn with_directory(override_directory: Option<PathBuf>) -> Self {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let fake = Arc::new(Fake::new());
        let bus = PrivateBus::start();
        let server = bus.connection().await;
        let template = FileTemplate::try_from("shot.png".to_owned()).expect("template");
        let screenshot = ScreenshotConfig {
            directory: Some(override_directory.unwrap_or_else(|| directory.path().to_path_buf())),
            filename: template,
            ..ScreenshotConfig::default()
        };
        glimpse_dbus::own_name(&server, BUS_NAME)
            .await
            .expect("the portal name");
        serve_at(&server, Arc::clone(&fake) as Arc<dyn Spawner>, screenshot)
            .await
            .expect("the portal is served");
        let client = bus.connection().await;
        Self {
            _bus: bus,
            _server: server,
            directory,
            client,
            fake,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_screenshot_directory_does_not_stop_color_or_interactive_requests() {
    let started = Started::with_directory(Some(PathBuf::from("/dev/null/glimpse-shot"))).await;
    let proxy = ShotPortalProxy::new(&started.client)
        .await
        .expect("a proxy");

    started.fake.push(Script::Done {
        stdout: r#"{"red":1,"green":2,"blue":3}"#.to_owned(),
        code: 0,
    });
    let (color, _) = proxy
        .pick_color(&handle("/req/no_folder_color"), "", "", HashMap::new())
        .await
        .expect("PickColor");
    assert_eq!(color, SUCCESS);

    started.fake.push(Script::Done {
        stdout: result_stdout("/tmp/portal-overlaid.png"),
        code: 0,
    });
    let (interactive, _) = proxy
        .screenshot(
            &handle("/req/no_folder_interactive"),
            "org.gimp.GIMP",
            "",
            flags(&[("interactive", true)]),
        )
        .await
        .expect("interactive Screenshot");
    assert_eq!(interactive, SUCCESS);

    let (headless, _) = proxy
        .screenshot(
            &handle("/req/no_folder_headless"),
            "org.gimp.GIMP",
            "",
            flags(&[("permission_store_checked", true)]),
        )
        .await
        .expect("headless Screenshot");
    assert_eq!(headless, OTHER);
    assert_eq!(started.fake.argv().len(), 2);
}

#[zbus::proxy(
    interface = "org.freedesktop.impl.portal.Screenshot",
    default_service = "me.aresa.Glimpse.Shot.Portal",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait ShotPortal {
    #[zbus(name = "Screenshot")]
    fn screenshot(
        &self,
        handle: &ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<Reply>;

    #[zbus(name = "PickColor")]
    fn pick_color(
        &self,
        handle: &ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<Reply>;

    #[zbus(property, name = "AvailableTargets")]
    fn available_targets(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "version")]
    fn version(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(interface = "org.freedesktop.impl.portal.Request")]
trait PortalRequestTest {
    #[zbus(name = "Close")]
    fn close(&self) -> zbus::Result<()>;
}

fn flags<'a>(pairs: &'a [(&'a str, bool)]) -> HashMap<&'a str, Value<'a>> {
    pairs
        .iter()
        .map(|(key, value)| (*key, Value::from(*value)))
        .collect()
}

fn targeted<'a>(target: Value<'a>, extra: &'a [(&'a str, bool)]) -> HashMap<&'a str, Value<'a>> {
    let mut options = flags(extra);
    options.insert("target", target);
    options
}

fn handle(path: &str) -> ObjectPath<'static> {
    ObjectPath::try_from(path)
        .expect("an object path")
        .into_owned()
}

fn result_stdout(path: &str) -> String {
    serde_json::to_string(&ResultV1 {
        version: 1,
        path: Some(PathBuf::from(path)),
        copied: false,
        width: 8,
        height: 8,
        scale: 1.0,
        output: Some("DP-1".to_owned()),
        target: TargetV1::Screen {
            name: "DP-1".to_owned(),
        },
        crop: None,
        dropped: 0,
        annotations: Vec::new(),
    })
    .expect("result json")
}

fn program_name(argv: &[OsString]) -> Option<&str> {
    Path::new(argv.first()?)
        .file_name()
        .and_then(|name| name.to_str())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_kills_the_child_and_the_screenshot_call_does_not_return() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Hang);
        let mut seen = started.fake.launches.subscribe();
        let client = started.client.clone();
        let call = tokio::spawn(async move {
            let proxy = ShotPortalProxy::new(&client).await.expect("a proxy");
            let path = handle("/req/close");
            proxy
                .screenshot(&path, "org.gimp.GIMP", "", flags(&[("interactive", true)]))
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), seen.wait_for(|count| *count >= 1))
            .await
            .expect("the first child starts promptly")
            .expect("the child was spawned");

        let request = PortalRequestTestProxy::builder(&started.client)
            .path(handle("/req/close"))
            .expect("a path")
            .destination(BUS_NAME)
            .expect("a destination")
            .build()
            .await
            .expect("a request proxy");
        tokio::time::timeout(Duration::from_secs(2), request.close())
            .await
            .expect("Close returns promptly")
            .expect("Close returns");
        let killed = tokio::time::timeout(Duration::from_secs(2), async {
            while !started.fake.killed.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(killed.is_ok(), "Close sends SIGTERM to the child");

        let pending = tokio::time::timeout(Duration::from_millis(400), call).await;
        assert!(
            pending.is_err(),
            "a closed request sends no Screenshot response"
        );

        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/after-close.png"),
            code: 0,
        });
        let again = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = proxy
                    .screenshot(&handle("/req/after"), "org.gimp.GIMP", "", HashMap::new())
                    .await
                    .expect("a reply");
                if reply.0 == SUCCESS {
                    return reply;
                }
                assert_eq!(reply.0, OTHER);
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the slot is free after Close");
        assert_eq!(again.0, SUCCESS);
    })
    .await
    .expect("Close test finishes on the private bus");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pick_color_returns_fractions_of_255_and_cancel_is_response_1() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Done {
            stdout: r#"{"format":"rgb","value":"rgb(224 86 63)","red":224,"green":86,"blue":63}"#
                .to_owned(),
            code: 0,
        });
        let (response, results) = proxy
            .pick_color(
                &handle("/req/color"),
                "org.inkscape.Inkscape",
                "",
                HashMap::new(),
            )
            .await
            .expect("PickColor");
        assert_eq!(response, SUCCESS);
        let (red, green, blue) =
            <(f64, f64, f64)>::try_from(results.get("color").expect("color").clone())
                .expect("(ddd)");
        assert_eq!(
            (red, green, blue),
            (224.0 / 255.0, 86.0 / 255.0, 63.0 / 255.0)
        );
        assert_eq!(program_name(&started.fake.argv()[0]), Some("glimpse-lens"));
        assert_eq!(
            &started.fake.argv()[0][1..],
            &[
                OsString::from("--tool"),
                OsString::from("picker"),
                OsString::from("--json"),
            ]
        );

        started.fake.push(Script::Done {
            stdout: String::new(),
            code: 4,
        });
        let (response, results) = proxy
            .pick_color(&handle("/req/color_cancel"), "", "", HashMap::new())
            .await
            .expect("PickColor");
        assert_eq!(response, CANCELLED);
        assert!(results.is_empty());

        started.fake.push(Script::Done {
            stdout: "nope".to_owned(),
            code: 1,
        });
        let (response, results) = proxy
            .pick_color(&handle("/req/color_fail"), "", "", HashMap::new())
            .await
            .expect("PickColor");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());
    })
    .await
    .expect("PickColor test finishes on the private bus");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_request_while_one_is_open_returns_2() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Hang);
        let mut seen = started.fake.launches.subscribe();
        let client = started.client.clone();
        let first = tokio::spawn(async move {
            let proxy = ShotPortalProxy::new(&client).await.expect("a proxy");
            let path = handle("/req/busy");
            proxy
                .screenshot(&path, "org.gimp.GIMP", "", flags(&[("interactive", true)]))
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), seen.wait_for(|count| *count >= 1))
            .await
            .expect("the first child starts promptly")
            .expect("the first child was spawned");

        let (response, results) = tokio::time::timeout(
            Duration::from_secs(2),
            proxy.pick_color(&handle("/req/busy_2"), "org.other.App", "", HashMap::new()),
        )
        .await
        .expect("a busy request returns at once")
        .expect("the second call returns");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());
        assert_eq!(started.fake.argv().len(), 1, "the busy call spawns nothing");

        let request = PortalRequestTestProxy::builder(&started.client)
            .path(handle("/req/busy"))
            .expect("a path")
            .destination(BUS_NAME)
            .expect("a destination")
            .build()
            .await
            .expect("a request proxy");
        tokio::time::timeout(Duration::from_secs(2), request.close())
            .await
            .expect("Close returns promptly")
            .expect("Close");
        let pending = tokio::time::timeout(Duration::from_millis(400), first).await;
        assert!(pending.is_err());
    })
    .await
    .expect("busy request test finishes on the private bus");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_spawned_child_follows_the_decision_and_the_child_result() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        assert_eq!(proxy.version().await.expect("version"), 3);
        assert_eq!(proxy.available_targets().await.expect("targets"), 13);

        let raw = format!("<b>{}</b>", "é".repeat(300));
        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/portal overlay.png"),
            code: 0,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/name"),
                &raw,
                "ignored",
                flags(&[("interactive", true), ("permission_store_checked", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, SUCCESS);
        assert_eq!(
            results
                .get("uri")
                .and_then(|value| String::try_from(value.clone()).ok())
                .as_deref(),
            Some("file:///tmp/portal%20overlay.png")
        );
        let argv = started.fake.argv();
        assert_eq!(program_name(&argv[0]), Some("glimpse-shot"));
        assert_eq!(
            &argv[0][1..],
            &[
                OsString::from("--screen"),
                OsString::from("--json"),
                OsString::from("--portal"),
                OsString::from(app_label(&raw)),
                OsString::from("--timeout"),
                OsString::from(OVERLAY_TIMEOUT_SECS.to_string()),
            ]
        );

        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/empty-app.png"),
            code: 0,
        });
        proxy
            .screenshot(
                &handle("/req/empty"),
                "   ",
                "",
                flags(&[("interactive", false)]),
            )
            .await
            .expect("Screenshot");
        let argv = started.fake.argv();
        let fallback = gettextrs::gettext("An application");
        assert_eq!(
            argv[1]
                .windows(2)
                .find(|pair| pair[0] == "--portal")
                .map(|pair| pair[1].as_os_str()),
            Some(std::ffi::OsStr::new(fallback.as_str()))
        );
        assert_eq!(argv[1][1], OsString::from("--screen"));

        let output = started.directory.path().join("shot.png");
        started.fake.push(Script::Done {
            stdout: result_stdout(output.to_str().expect("utf-8")),
            code: 0,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/headless"),
                "org.gimp.GIMP",
                "",
                flags(&[("interactive", false), ("permission_store_checked", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, SUCCESS);
        assert_eq!(
            results
                .get("uri")
                .and_then(|value| String::try_from(value.clone()).ok())
                .as_deref(),
            file_uri(&output).as_deref()
        );
        let argv = &started.fake.argv()[2];
        assert_eq!(program_name(argv), Some("glimpse-shot"));
        assert_eq!(argv[1], OsString::from("capture"));
        assert_eq!(argv[2], OsString::from("--screen"));
        assert_eq!(argv[3], OsString::from("--json"));
        assert_eq!(argv[4], OsString::from("-o"));
        assert_eq!(argv[5], output.as_os_str());
        assert!(output.starts_with(started.directory.path()));

        started.fake.push(Script::Done {
            stdout: String::new(),
            code: 4,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/esc"),
                "org.gimp.GIMP",
                "",
                flags(&[("interactive", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, CANCELLED);
        assert!(results.is_empty());

        started.fake.push(Script::Done {
            stdout: "crash".to_owned(),
            code: 9,
        });
        let (response, results) = proxy
            .screenshot(&handle("/req/crash"), "org.gimp.GIMP", "", HashMap::new())
            .await
            .expect("Screenshot");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());

        let silent = [("interactive", false), ("permission_store_checked", true)];
        let stdout = result_stdout(output.to_str().expect("utf-8"));
        for (n, (target, extra, want)) in [
            (1u32, &silent[..], "capture"),
            (1, &[][..], "--screen"),
            (4, &silent[..], "--screen"),
            (8, &silent[..], "capture"),
            (
                8,
                &[("interactive", true), ("permission_store_checked", true)][..],
                "capture",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            started.fake.push(Script::Done {
                stdout: stdout.clone(),
                code: 0,
            });
            let (response, _) = proxy
                .screenshot(
                    &handle(&format!("/req/target{n}")),
                    "org.gimp.GIMP",
                    "",
                    targeted(Value::from(target), extra),
                )
                .await
                .expect("Screenshot");
            assert_eq!(response, SUCCESS);
            let argv = started.fake.argv().pop().expect("argv");
            assert_eq!(argv[1], OsString::from(want));
            if target == 4 {
                assert!(argv.contains(&OsString::from("--portal")));
            }
            if target == 8 {
                assert_eq!(argv[2], OsString::from("--window"));
                assert_eq!(argv[3], OsString::from("focused"));
            }
        }

        let before = started.fake.argv().len();
        let refused: [(Value<'static>, &[(&'static str, bool)]); 7] = [
            (Value::from(2u32), &silent[..]),
            (Value::from(3u32), &silent[..]),
            (Value::from(0u32), &silent[..]),
            (Value::from(16u32), &silent[..]),
            (Value::from("1"), &silent[..]),
            (Value::from(8u32), &[("interactive", true)][..]),
            (Value::from(8u32), &[("interactive", false)][..]),
        ];
        for (n, (target, extra)) in refused.into_iter().enumerate() {
            let (response, results) = proxy
                .screenshot(
                    &handle(&format!("/req/refused{n}")),
                    "org.gimp.GIMP",
                    "",
                    targeted(target, extra),
                )
                .await
                .expect("Screenshot");
            assert_eq!(response, OTHER);
            assert!(results.is_empty());
        }
        assert_eq!(started.fake.argv().len(), before);
    })
    .await
    .expect("Screenshot flow test finishes on the private bus");
}

#[test]
fn overlay_and_headless_commands_name_the_binaries() {
    let overlay = overlay_command("GIMP");
    assert_eq!(program_name(&overlay), Some("glimpse-shot"));
    assert_eq!(
        &overlay[1..],
        &[
            OsString::from("--screen"),
            OsString::from("--json"),
            OsString::from("--portal"),
            OsString::from("GIMP"),
            OsString::from("--timeout"),
            OsString::from(OVERLAY_TIMEOUT_SECS.to_string()),
        ]
    );
    let output = Path::new("/tmp/shots/shot.png");
    let headless = headless_command(output, false);
    assert_eq!(headless[1], OsString::from("capture"));
    assert_eq!(headless[5], output.as_os_str());
    let window = headless_command(output, true);
    assert_eq!(
        &window[1..4],
        &[
            OsString::from("capture"),
            OsString::from("--window"),
            OsString::from("focused")
        ]
    );
    assert_eq!(window[6], output.as_os_str());
    let picker = picker_command();
    assert_eq!(program_name(&picker), Some("glimpse-lens"));
    assert_eq!(
        &picker[1..],
        &[
            OsString::from("--tool"),
            OsString::from("picker"),
            OsString::from("--json"),
        ]
    );

    let app_id = |command: &Command| {
        command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == std::ffi::OsStr::new(APP_ID_VARIABLE))
            .and_then(|(_, value)| value)
            .and_then(|value| value.to_str())
            .map(str::to_owned)
    };
    let first = child_command(overlay.clone()).expect("portal child command");
    let second = child_command(overlay).expect("next portal child command");
    let first_id = app_id(&first).expect("portal child ID");
    let second_id = app_id(&second).expect("next portal child ID");
    assert!(gtk4::gio::Application::id_is_valid(&first_id));
    assert!(gtk4::gio::Application::id_is_valid(&second_id));
    assert_ne!(first_id, "me.aresa.Glimpse.Shot");
    assert_ne!(first_id, second_id);
    assert!(app_id(&child_command(headless).expect("headless command")).is_none());
    assert!(app_id(&child_command(picker).expect("picker command")).is_none());
}

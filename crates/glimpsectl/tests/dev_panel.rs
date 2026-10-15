use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use glimpse_dbus::testing::PrivateBus;

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn setup(root: &Path, mode: &str) -> Result<std::path::PathBuf> {
    let cli = root.join("glimpsectl");
    fs::copy(env!("CARGO_BIN_EXE_glimpsectl"), &cli)?;
    let panel = root.join("glimpse-panel");
    fs::write(
        &panel,
        format!(
            "#!/usr/bin/python3\nimport dbus, json, os, sys, time\nfrom gi.repository import GLib\nroot = os.path.dirname(__file__)\nopen(os.path.join(root, 'panel.pid'), 'w').write(str(os.getpid()))\nopen(os.path.join(root, 'panel.config'), 'w').write(sys.argv[2])\nassert sys.argv[1] == '--config'\nif {mode:?} == 'fail': sys.exit(7)\nif {mode:?} == 'pending': time.sleep(30)\nbus = dbus.SessionBus()\nbus.request_name(os.environ['GLIMPSE_PANEL_APP_ID'] + '.AppletHost', 4)\nGLib.MainLoop().run()\n"
        ),
    )?;
    fs::set_permissions(&panel, fs::Permissions::from_mode(0o755))?;
    fs::create_dir(root.join("runtime"))?;
    fs::create_dir_all(root.join("user/glimpse"))?;
    fs::write(
        root.join("user/glimpse/config.toml"),
        "invalid user config must not be read",
    )?;
    Ok(cli)
}

async fn wait_file(path: &Path) -> Result<String> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Ok(contents) = fs::read_to_string(path)
                && !contents.is_empty()
            {
                return Ok::<_, anyhow::Error>(contents);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_panel_reports_fresh_id_and_removes_only_its_child_and_config() -> Result<()> {
    for mode in ["ready", "fail", "pending"] {
        let root = tempfile::tempdir()?;
        let cli = setup(root.path(), mode)?;
        let bus = PrivateBus::start();
        let mut child = Process(
            Command::new(cli)
                .args(["--json", "dev", "panel"])
                .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
                .env("GLIMPSE_PANEL_APP_ID", "org.example.UserPanel")
                .env_remove("GLIMPSE_CONFIG_PATH")
                .env("XDG_RUNTIME_DIR", root.path().join("runtime"))
                .env("XDG_CONFIG_HOME", root.path().join("user"))
                .stdout(Stdio::piped())
                .stderr(fs::File::create(root.path().join("cli.stderr"))?)
                .spawn()?,
        );
        let pid: i32 = wait_file(&root.path().join("panel.pid"))
            .await
            .with_context(|| {
                format!(
                    "mode {mode}, cli status {:?}, stderr: {}",
                    child.0.try_wait(),
                    fs::read_to_string(root.path().join("cli.stderr")).unwrap_or_default()
                )
            })?
            .parse()?;
        let config_path = wait_file(&root.path().join("panel.config")).await?;
        if mode == "ready" {
            let stdout = child.0.stdout.take().context("no stdout")?;
            let value: serde_json::Value = tokio::task::spawn_blocking(move || {
                let mut line = String::new();
                let mut reader = BufReader::new(stdout);
                let mut json = String::new();
                loop {
                    if reader.read_line(&mut line)? == 0 {
                        anyhow::bail!("panel did not report readiness");
                    }
                    json.push_str(&line);
                    if let Ok(value) = serde_json::from_str(&json) {
                        return Ok::<_, anyhow::Error>(value);
                    }
                    line.clear();
                }
            })
            .await??;
            let app_id = value["app_id"].as_str().context("no app id")?;
            assert!(app_id.starts_with("me.aresa.GlimpsePanel.Dev."));
            assert_ne!(app_id, "org.example.UserPanel");
            let config: glimpse_config::Config =
                toml::from_str(&fs::read_to_string(&config_path)?)?;
            assert!(
                config.panels[0].left.is_empty()
                    && config.panels[0].center.is_empty()
                    && config.panels[0].right.is_empty()
            );
        }
        if mode != "fail" {
            unsafe {
                libc::kill(child.0.id() as i32, libc::SIGTERM);
            }
        }
        let status = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = child.0.try_wait()? {
                    return Ok::<_, anyhow::Error>(status);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await??;
        if mode == "ready" {
            assert!(status.success());
        } else {
            assert!(!status.success());
        }
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "test panel survived");
        assert!(!Path::new(&config_path).exists());
        assert_eq!(
            fs::read_to_string(root.path().join("user/glimpse/config.toml"))?,
            "invalid user config must not be read"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolated_run_detaches_before_cleaning_up_after_foreground_group_interrupt() -> Result<()> {
    let root = tempfile::tempdir()?;
    let cli = setup(root.path(), "ready")?;
    fs::write(
        root.path().join("glimpse-panel"),
        r#"#!/usr/bin/python3
import dbus, dbus.service, os, signal, sys
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib
DBusGMainLoop(set_as_default=True)
root = os.path.dirname(__file__)
def record(name, value):
    with open(os.path.join(root, name), 'w') as file:
        file.write(str(value))
def interrupted(number, frame):
    record('panel.interrupted', number)
    os._exit(130)
signal.signal(signal.SIGINT, interrupted)
record('panel.pid', os.getpid())
record('panel.config', sys.argv[2])
bus = dbus.SessionBus()
class Host(dbus.service.Object):
    @dbus.service.method('me.aresa.Glimpse.AppletHost1', in_signature='susssa{ss}h', out_signature='s')
    def Attach(self, desktop, panel, output, zone, options, env, stderr):
        record('panel.attached', 'dev-test')
        return 'dev-test'
    @dbus.service.method('me.aresa.Glimpse.AppletHost1', in_signature='s', out_signature='')
    def Detach(self, handle):
        record('panel.detached', handle)
host = Host(bus, '/me/aresa/Glimpse/AppletHost')
name = dbus.service.BusName(os.environ['GLIMPSE_PANEL_APP_ID'] + '.AppletHost', bus=bus, do_not_queue=True)
GLib.MainLoop().run()
"#,
    )?;
    let source = root.path().join("applet");
    fs::create_dir(&source)?;
    fs::write(
        source.join("org.example.Interrupt.desktop"),
        "[Desktop Entry]\nType=Application\nName=Interrupt\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
    )?;
    fs::write(source.join("config.toml"), "")?;
    let bus = PrivateBus::start();
    let mut child = Process(
        Command::new(cli)
            .args(["applet", "run", "--isolated"])
            .arg(&source)
            .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
            .env_remove("GLIMPSE_CONFIG_PATH")
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.path().join("cli.stderr"))?)
            .process_group(0)
            .spawn()?,
    );
    let panel_pid: i32 = wait_file(&root.path().join("panel.pid")).await?.parse()?;
    let config = wait_file(&root.path().join("panel.config")).await?;
    wait_file(&root.path().join("panel.attached")).await?;
    tokio::time::timeout(Duration::from_secs(8), async {
        while !fs::read_to_string(root.path().join("cli.stderr"))
            .is_ok_and(|text| text.contains("Running "))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        unsafe { libc::kill(-(child.0.id() as i32), libc::SIGINT) },
        0
    );
    let status = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Some(status) = child.0.try_wait()? {
                return Ok::<_, anyhow::Error>(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert!(
        !root.path().join("panel.interrupted").exists(),
        "the panel received its parent's foreground interrupt"
    );
    assert!(
        status.success(),
        "{}",
        fs::read_to_string(root.path().join("cli.stderr"))?
    );
    assert_eq!(
        fs::read_to_string(root.path().join("panel.detached"))?,
        "dev-test"
    );
    assert_eq!(unsafe { libc::kill(panel_pid, 0) }, -1);
    assert!(!Path::new(&config).exists());
    Ok(())
}

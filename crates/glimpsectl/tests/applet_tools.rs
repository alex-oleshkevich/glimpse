use std::fs;
use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;

fn fixture(root: &Path, body: &str) -> Result<()> {
    fs::write(
        root.join("org.example.Check.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Check\nExec=/usr/bin/python3 {}\nImplements=me.aresa.Glimpse.Applet1;\n",
            root.join("applet.py").display()
        ),
    )?;
    fs::write(root.join("config.toml"), "options = { test = true }\n")?;
    fs::write(
        root.join("applet.py"),
        format!(
            "import json, os, socket, time\nopen({:?}, 'w').write(str(os.getpid()))\ns = socket.socket(socket.AF_UNIX)\ns.connect(os.environ['GLIMPSE_APPLET_SOCKET'])\nr = s.makefile('r')\ndef send(value): s.sendall((value + '\\n').encode())\n{body}\n",
            root.join("pid").to_str().unwrap()
        ),
    )?;
    Ok(())
}

#[tokio::test]
async fn check_runs_without_bus_and_cleans_state_and_process_on_every_outcome() -> Result<()> {
    let cases = [
        (
            "send('{\"t\":\"hello\",\"v\":1}')\nh = json.loads(r.readline())\nassert h['t'] == 'hello' and h['v'] == 1 and h['options']['test']\nopen(os.path.join(os.path.dirname(__file__), 'state'), 'w').write(h['state'])\nsend('{\"t\":\"commit\",\"ops\":[]}')\ntime.sleep(30)",
            true,
        ),
        ("send('{\"t\":\"hello\",\"v\":2}')\ntime.sleep(30)", false),
        (
            "send('{\"t\":\"commit\",\"ops\":[]}')\ntime.sleep(30)",
            false,
        ),
        (
            "send('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend('{\"t\":\"commit\",\"ops\":[{\"op\":\"remove\",\"parent\":0,\"id\":999}]}')\ntime.sleep(30)",
            false,
        ),
        ("s.sendall(b'x' * (1048576 + 2))\ntime.sleep(30)", false),
        ("time.sleep(30)", false),
    ];
    for (body, success) in cases {
        let root = tempfile::tempdir()?;
        fixture(root.path(), body)?;
        let runtime = root.path().join("runtime");
        fs::create_dir(&runtime)?;
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
                .args(["--json", "applet", "check", "--run", "--timeout", "1"])
                .arg(root.path())
                .env("XDG_RUNTIME_DIR", &runtime)
                .env(
                    "DBUS_SESSION_BUS_ADDRESS",
                    "unix:path=/missing-glimpse-test-bus",
                )
                .stdin(Stdio::null())
                .output(),
        )
        .await??;
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if success {
            let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(value["id"], "org.example.Check");
            assert_eq!(value["protocol"], true);
        } else {
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
        }
        let pid: i32 = fs::read_to_string(root.path().join("pid"))?.parse()?;
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "smoke child survived");
        assert!(
            fs::read_dir(runtime.join("glimpse/checks"))?
                .next()
                .is_none()
        );
        if let Ok(state) = fs::read_to_string(root.path().join("state")) {
            assert!(Path::new(&state).starts_with(&runtime));
            assert!(!Path::new(&state).exists());
        }
    }
    Ok(())
}

#[tokio::test]
async fn static_check_has_no_runtime_or_bus_requirement() -> Result<()> {
    let root = tempfile::tempdir()?;
    fixture(root.path(), "raise RuntimeError('must not run')")?;
    let output = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["--json", "applet", "check"])
        .arg(root.path())
        .env_remove("XDG_RUNTIME_DIR")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/missing-glimpse-test-bus",
        )
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.path().join("pid").exists());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["valid"], true);
    Ok(())
}

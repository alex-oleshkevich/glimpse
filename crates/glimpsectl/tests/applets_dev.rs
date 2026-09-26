use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn dev_unlinks_its_desktop_file_on_term() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let applet = root.path().join("applet");
    let data = root.path().join("data");
    fs::create_dir_all(&applet)?;
    let script = applet.join("main.sh");
    fs::write(
        &script,
        "printf '%s\\n' '{\"t\":\"hello\",\"v\":1}'\nread -r line\nprintf '%s\\n' '{\"t\":\"commit\",\"ops\":[]}'\n",
    )?;
    fs::write(
        applet.join("org.example.SignalTest.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Signal Test\nExec=/bin/sh {}\nNoDisplay=true\nImplements=me.aresa.Glimpse.Applet1\n",
            script.display()
        ),
    )?;
    let config = root.path().join("config.toml");
    fs::write(&config, "")?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .arg("--config")
        .arg(&config)
        .args(["applets", "dev"])
        .arg(&applet)
        .env("XDG_DATA_HOME", &data)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let link = data
        .join("applications")
        .join("org.example.SignalTest.desktop");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !link.is_symlink() && Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("dev exited before linking: {status}");
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(link.is_symlink());
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            assert!(status.success(), "dev exited with {status}");
            assert!(!link.exists() && !link.is_symlink());
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    child.kill()?;
    child.wait()?;
    anyhow::bail!("dev did not exit after TERM")
}

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;

#[test]
fn bundle_preserves_actions_and_uninstall_refuses_other_desktops() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let data = home.join("data files");
    let applet = root.path().join("applet");
    fs::create_dir_all(&home)?;
    let ctl = env!("CARGO_BIN_EXE_glimpsectl");
    let new = Command::new(ctl)
        .args([
            "applets",
            "new",
            "--id",
            "me.example.Bundle",
            "--yes",
            "--dir",
        ])
        .arg(&applet)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_DATA_DIRS", root.path().join("empty"))
        .output()?;
    assert!(
        new.status.success(),
        "{}",
        String::from_utf8_lossy(&new.stderr)
    );

    let handshake = root.path().join("handshake.sh");
    fs::write(
        &handshake,
        "printf '%s\\n' '{\"t\":\"hello\",\"v\":1}'\nread -r line\nprintf '%s\\n' '{\"t\":\"commit\",\"ops\":[]}'\n",
    )?;
    let desktop = applet.join("me.example.Bundle.desktop");
    let original = fs::read_to_string(&desktop)?;
    let rewritten = original
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!(
                    "Exec=/bin/sh {} --watch {}",
                    handshake.display(),
                    applet.join("main.tsx").display()
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        &desktop,
        format!("{rewritten}\n[Desktop Action a]\nExec=leave-action\nTryExec=leave-action\n"),
    )?;
    fs::write(applet.join("icon.svg"), "<svg/>")?;

    let deno = root.path().join("deno.sh");
    fs::write(
        &deno,
        "#!/bin/sh\nif [ \"$1\" = bundle ]; then printf bundled > \"$5\"; fi\n",
    )?;
    fs::set_permissions(&deno, fs::Permissions::from_mode(0o755))?;
    let out = root.path().join("out");
    let prefix = root.path().join("install prefix");
    let applications = out.join("share/applications");
    let icons = out.join("share/icons/hicolor/scalable/apps");
    let applet_out = out.join("share/me.example.Bundle");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    fs::create_dir_all(&applet_out)?;
    let victim = root.path().join("victim");
    fs::write(&victim, "untouched")?;
    symlink(&victim, applications.join("me.example.Bundle.desktop"))?;
    symlink(&victim, icons.join("me.example.Bundle.svg"))?;
    symlink(&victim, applet_out.join("applet.js"))?;
    let bundle = Command::new(ctl)
        .args(["applets", "bundle"])
        .arg(&applet)
        .arg("--prefix")
        .arg(&prefix)
        .arg("--out")
        .arg(&out)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_DATA_DIRS", root.path().join("empty"))
        .env("GLIMPSE_DENO", &deno)
        .output()?;
    assert!(
        bundle.status.success(),
        "{}",
        String::from_utf8_lossy(&bundle.stderr)
    );
    let installed = fs::read_to_string(applications.join("me.example.Bundle.desktop"))?;
    assert!(installed.contains("Exec=leave-action\nTryExec=leave-action\n"));
    assert!(installed.contains(&format!(
        "\"{}\"\n[Desktop Action a]",
        prefix.join("share/me.example.Bundle/applet.js").display()
    )));
    assert_eq!(fs::read_to_string(victim)?, "untouched");
    for path in [
        applications.join("me.example.Bundle.desktop"),
        icons.join("me.example.Bundle.svg"),
        applet_out.join("applet.js"),
    ] {
        assert!(!path.is_symlink(), "{}", path.display());
    }
    assert_eq!(fs::read_to_string(applet_out.join("applet.js"))?, "bundled");
    for dir in [&applications, &icons, &applet_out] {
        assert_eq!(fs::read_dir(dir)?.count(), 1, "{}", dir.display());
    }
    assert_eq!(
        fs::read_to_string(icons.join("me.example.Bundle.svg"))?,
        "<svg/>"
    );

    let install = Command::new(ctl)
        .args(["applets", "install"])
        .arg(&applet)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_DATA_DIRS", root.path().join("empty"))
        .env("GLIMPSE_DENO", &deno)
        .output()?;
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(
        data.join("applications/me.example.Bundle.desktop")
            .is_file()
    );
    assert!(data.join("me.example.Bundle/applet.js").is_file());
    let uninstall_applet = Command::new(ctl)
        .args(["applets", "uninstall", "me.example.Bundle"])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .output()?;
    assert!(
        uninstall_applet.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall_applet.stderr)
    );
    assert!(!data.join("applications/me.example.Bundle.desktop").exists());

    let user_applications = data.join("applications");
    fs::create_dir_all(&user_applications)?;
    let other = user_applications.join("me.example.Other.desktop");
    fs::write(
        &other,
        "[Desktop Entry]\nType=Application\nName=Other\nExec=/bin/true\n",
    )?;
    let uninstall = Command::new(ctl)
        .args(["applets", "uninstall", "me.example.Other"])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .output()?;
    assert!(!uninstall.status.success());
    assert!(String::from_utf8_lossy(&uninstall.stderr).contains("non-applet"));
    assert!(other.exists());
    Ok(())
}

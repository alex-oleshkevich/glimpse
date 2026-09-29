use std::env;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

#[test]
fn bundle_install_and_uninstall() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let data = home.join("data files");
    let applet = root.path().join("applet");
    fs::create_dir_all(&home)?;
    let ctl = env!("CARGO_BIN_EXE_glimpsectl");
    let path = env::join_paths(
        [Path::new(ctl).parent().unwrap().to_path_buf()]
            .into_iter()
            .chain(env::split_paths(&env::var_os("PATH").unwrap_or_default())),
    )?;
    let command = || {
        let mut command = Command::new(ctl);
        command
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", &data)
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_RUNTIME_DIR", home.join("runtime"))
            .env("XDG_DATA_DIRS", root.path().join("empty"))
            .env("PATH", &path);
        command
    };
    fs::create_dir_all(&applet)?;
    fs::write(
        applet.join("me.example.Bundle.desktop"),
        "[Desktop Entry]\nType=Application\nName=Bundle\nExec=glimpsectl applets run build/applet.mjs\nImplements=me.aresa.Glimpse.Applet1\n",
    )?;
    let missing = root.path().join("missing.mjs");
    let failed = command().args(["applets", "run"]).arg(&missing).output()?;
    assert_eq!(failed.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&failed.stderr).contains(&missing.display().to_string()));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("cannot execute"));

    let script = "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"commit\",\"ops\":[]}\\n'\n";
    let exe = applet.join("build/applet.mjs");
    fs::create_dir_all(exe.parent().unwrap())?;
    fs::write(&exe, script)?;
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755))?;
    let alternate = applet.join("alternate.mjs");
    fs::write(&alternate, format!("{script}\n"))?;
    fs::set_permissions(&alternate, fs::Permissions::from_mode(0o755))?;
    fs::write(applet.join("icon.svg"), "<svg/>")?;
    let runnable = applet.join("runnable.js");
    fs::write(&runnable, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&runnable, fs::Permissions::from_mode(0o755))?;
    let run = command().args(["applets", "run"]).arg(&runnable).output()?;
    assert_eq!(run.status.code(), Some(0));
    let denied_exe = applet.join("denied.mjs");
    fs::write(&denied_exe, script)?;
    let denied = command()
        .args(["applets", "run"])
        .arg(&denied_exe)
        .output()?;
    assert_eq!(denied.status.code(), Some(126));
    assert!(String::from_utf8_lossy(&denied.stderr).contains(&denied_exe.display().to_string()));

    let missing_out = root.path().join("missing-output");
    let missing_bundle = command()
        .args(["applets", "bundle"])
        .arg(&applet)
        .args(["--exe", "missing.mjs", "--out"])
        .arg(&missing_out)
        .output()?;
    assert!(!missing_bundle.status.success());
    assert!(!missing_out.exists());
    assert!(
        String::from_utf8_lossy(&missing_bundle.stderr).contains(&format!(
            "no such file: {}; run npm run build first",
            applet.join("missing.mjs").display()
        ))
    );
    fs::write(&exe, "#!/bin/sh\nexit 1\n")?;

    let out = root.path().join("out");
    let prefix = root.path().join("install prefix");
    let applications = out.join("share/applications");
    let icons = out.join("share/icons/hicolor/scalable/apps");
    let applet_out = out.join("share/glimpse/applets/me.example.Bundle");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    fs::create_dir_all(&applet_out)?;
    let victim = root.path().join("victim");
    fs::write(&victim, "untouched")?;
    symlink(&victim, applications.join("me.example.Bundle.desktop"))?;
    symlink(&victim, icons.join("me.example.Bundle.svg"))?;
    symlink(&victim, applet_out.join("applet.mjs"))?;
    let bundle = command()
        .args(["applets", "bundle"])
        .arg(&applet)
        .arg("--prefix")
        .arg(&prefix)
        .arg("--out")
        .arg(&out)
        .args(["--exe", "alternate.mjs"])
        .output()?;
    assert!(
        bundle.status.success(),
        "{}",
        String::from_utf8_lossy(&bundle.stderr)
    );
    let installed = fs::read_to_string(applications.join("me.example.Bundle.desktop"))?;
    assert!(installed.contains(&format!(
        "Exec=glimpsectl applets run \"{}\"",
        prefix.join("share/glimpse/applets/me.example.Bundle/applet.mjs").display()
    )));
    assert_eq!(fs::read_to_string(&victim)?, "untouched");
    for file in [
        applications.join("me.example.Bundle.desktop"),
        icons.join("me.example.Bundle.svg"),
        applet_out.join("applet.mjs"),
    ] {
        assert!(!file.is_symlink(), "{}", file.display());
    }
    assert_eq!(
        fs::read_to_string(applet_out.join("applet.mjs"))?,
        format!("{script}\n")
    );
    assert_eq!(
        fs::metadata(applet_out.join("applet.mjs"))?
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read_to_string(icons.join("me.example.Bundle.svg"))?,
        "<svg/>"
    );

    fs::write(&exe, script)?;
    let install = command()
        .args(["applets", "install"])
        .arg(&applet)
        .output()?;
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let user_exe = data.join("glimpse/applets/me.example.Bundle/applet.mjs");
    let user_desktop = data.join("applications/me.example.Bundle.desktop");
    assert_eq!(fs::read_to_string(&user_exe)?, script);
    assert_eq!(fs::metadata(&user_exe)?.permissions().mode() & 0o777, 0o755);
    assert!(fs::read_to_string(&user_desktop)?.contains(&format!(
        "Exec=glimpsectl applets run \"{}\"",
        user_exe.display()
    )));
    let uninstall = command()
        .args(["applets", "uninstall", "me.example.Bundle"])
        .output()?;
    assert!(
        uninstall.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    assert!(!user_exe.exists());
    assert!(!user_desktop.exists());

    let other = data.join("applications/me.example.Other.desktop");
    fs::write(
        &other,
        "[Desktop Entry]\nType=Application\nName=Other\nExec=/bin/true\n",
    )?;
    let uninstall = command()
        .args(["applets", "uninstall", "me.example.Other"])
        .output()?;
    assert!(!uninstall.status.success());
    assert!(String::from_utf8_lossy(&uninstall.stderr).contains("non-applet"));
    assert!(other.exists());
    Ok(())
}

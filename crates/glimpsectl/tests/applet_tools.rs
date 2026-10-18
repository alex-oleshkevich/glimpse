use std::fs;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;

fn closed_stdout() -> Result<Stdio> {
    let mut descriptors = [-1; 2];
    anyhow::ensure!(
        unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) } == 0,
        "cannot create stdout pipe"
    );
    let reader = unsafe { OwnedFd::from_raw_fd(descriptors[0]) };
    let writer = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
    drop(reader);
    Ok(Stdio::from(writer))
}

#[tokio::test]
async fn one_shot_applet_commands_complete_when_stdout_reader_is_closed() -> Result<()> {
    for json in [false, true] {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("source");
        let bin = root.path().join("bin");
        fs::create_dir(&bin)?;
        for tool in ["sh", "just", "node", "mkdir", "cp"] {
            let paths = std::env::var_os("PATH").unwrap_or_default();
            let found = std::env::split_paths(&paths)
                .map(|dir| dir.join(tool))
                .find(|candidate| candidate.exists())
                .unwrap_or_else(|| panic!("{tool} on PATH"));
            std::os::unix::fs::symlink(found, bin.join(tool))?;
        }
        std::os::unix::fs::symlink("/bin/true", bin.join("uv"))?;
        for operation in ["new", "install"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_glimpsectl"));
            command.args(["applet", operation]);
            if json {
                command.arg("--json");
            }
            if operation == "new" {
                command
                    .args([
                        "--id",
                        "org.example.ClosedPipe",
                        "--name",
                        "Closed pipe",
                        "--language",
                        "py",
                        "--yes",
                        "--dir",
                    ])
                    .arg(&dir);
            } else {
                command.arg(&dir);
            }
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                command
                    .env("PATH", &bin)
                    .env("XDG_DATA_HOME", root.path().join("data"))
                    .env(
                        "GLIMPSE_SDK_TEMPLATES_DIR",
                        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/templates"),
                    )
                    .env(
                        "DBUS_SESSION_BUS_ADDRESS",
                        "unix:path=/missing-glimpse-test-bus",
                    )
                    .stdin(Stdio::null())
                    .stdout(closed_stdout()?)
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?
                    .wait_with_output(),
            )
            .await??;
            assert!(
                output.status.success(),
                "{operation}, json={json}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(dir.join("org.example.ClosedPipe.desktop").is_file());
        let installed = root
            .path()
            .join("data/applications/org.example.ClosedPipe.desktop");
        assert!(fs::read_to_string(installed)?.contains("X-Glimpse-Installed-By=glimpsectl"));
    }
    Ok(())
}

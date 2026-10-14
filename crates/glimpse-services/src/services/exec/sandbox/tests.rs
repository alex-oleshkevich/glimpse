use std::ffi::OsStr;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

use super::{exec_confine, exec_state_dir, expanded, path_entries, require_abi};

#[test]
fn state_dir_is_private() {
    let base = tempfile::tempdir().expect("tempdir");
    let state = exec_state_dir(base.path(), "me.x.A").expect("state dir");
    assert_eq!(state, base.path().join("applets/me.x.A"));
    assert_eq!(
        std::fs::metadata(state)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn old_kernel_requires_unconfined() {
    assert_eq!(
        require_abi(9),
        Err("kernel lacks Landlock ABI 10; set sandbox.unconfined to run it".to_owned())
    );
    assert!(require_abi(10).is_ok());
}

#[test]
fn path_entries_exclude_home_and_ancestors() {
    assert_eq!(
        path_entries(
            OsStr::new("/:/home:/home/test:/home/test/bin:/usr/bin:/opt/bin:relative"),
            Path::new("/home/test")
        ),
        vec![PathBuf::from("/usr/bin"), PathBuf::from("/opt/bin")]
    );
}

#[test]
fn sandbox_grants_must_be_absolute() {
    assert!(expanded("relative/path").is_err());
}

#[test]
fn a_confined_child_inherits_no_descriptor_beyond_stdio() {
    let (_reader, writer) = std::io::pipe().expect("pipe");
    rustix::io::fcntl_setfd(&writer, rustix::io::FdFlags::empty()).expect("inheritable");
    let fd = writer.as_raw_fd();
    let target = std::fs::read_link(format!("/proc/self/fd/{fd}")).expect("pipe target");
    let parent = rustix::process::getpid();
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", &format!("readlink /proc/$$/fd/{fd} || true")]);
    unsafe {
        command.pre_exec(move || exec_confine(None, parent));
    }
    let output = command.output().expect("the child runs");

    assert_ne!(
        String::from_utf8_lossy(&output.stdout).trim(),
        target.to_string_lossy()
    );
}

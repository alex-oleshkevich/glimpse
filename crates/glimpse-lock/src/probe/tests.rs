use super::*;

#[test]
fn only_the_padded_identity_map_is_the_initial_namespace() {
    assert_eq!(uid_map("         0          0 4294967295\n"), Ok(()));
    assert!(uid_map("      1000       1000          1\n").is_err());
    assert!(uid_map("         0          0 4294967294\n").is_err());
    assert!(uid_map("0 0 4294967295\n0 0 1\n").is_err());
    assert!(uid_map("").is_err());
    assert!(uid_map("0 0 x\n").is_err());
}

#[test]
fn no_new_privs_must_read_zero() {
    let status = |value: &str| {
        format!(
            "Name:\tglimpse-lock\nUid:\t1000\t1000\t1000\t1000\nNoNewPrivs:\t{value}\nSeccomp:\t0\n"
        )
    };
    assert_eq!(no_new_privs(&status("0")), Ok(()));
    assert!(no_new_privs(&status("1")).is_err());
    assert!(no_new_privs("Name:\tx\n").is_err());
}

#[test]
fn unix_chkpwd_must_be_setuid_or_setgid_root_on_a_suid_filesystem() {
    assert_eq!(chkpwd(0, 0o106755, false), Ok(()));
    assert_eq!(chkpwd(0, 0o104755, false), Ok(()));
    assert_eq!(
        chkpwd(0, 0o102755, false),
        Ok(()),
        "Debian ships it 2755 root:shadow"
    );
    assert!(chkpwd(0, 0o100755, false).is_err());
    assert!(chkpwd(1000, 0o104755, false).is_err());
    assert!(chkpwd(65534, 0o106755, false).is_err());
    assert!(chkpwd(0, 0o106755, true).is_err());
    assert!(chkpwd(0, 0o102755, true).is_err());
}

#[test]
fn a_pam_file_must_exist_and_permit_nothing() {
    let shipped = include_str!("../../../../data/pam.d/glimpse-lock");
    assert_eq!(pam_file(Some(shipped)), Ok(()));
    let debian = include_str!("../../../../data/pam.d/debian/glimpse-lock");
    assert_eq!(pam_file(Some(debian)), Ok(()));
    assert!(pam_file(None).is_err());
    assert!(pam_file(Some("auth required pam_permit.so\n")).is_err());
    assert!(pam_file(Some("auth sufficient /usr/lib/security/pam_permit.so\n")).is_err());
    assert_eq!(pam_file(Some("#auth required pam_permit.so\n")), Ok(()));
    assert_eq!(
        pam_file(Some("auth include system-auth # pam_permit.so\n")),
        Ok(())
    );
}

#[test]
fn the_pam_stack_is_looked_up_in_etc_before_usr_lib() {
    let both = |_: &Path| true;
    assert_eq!(
        pam_path("glimpse-lock", &PAM_DIRS, both),
        Some(PathBuf::from("/etc/pam.d/glimpse-lock"))
    );
    let vendor = |path: &Path| path.starts_with("/usr/lib/pam.d");
    assert_eq!(
        pam_path("glimpse-lock", &PAM_DIRS, vendor),
        Some(PathBuf::from("/usr/lib/pam.d/glimpse-lock"))
    );
    assert_eq!(pam_path("glimpse-lock", &PAM_DIRS, |_| false), None);
}

#[test]
fn a_missing_pam_service_refuses_at_startup() {
    let missing = "glimpse-lock-no-such-service-for-tests";
    assert!(!failures(&startup(missing)).is_empty());
    assert!(!pam_stack(missing).passed());
}

#[test]
fn a_malformed_pam_service_is_refused_for_its_name_alone() {
    let shipped = include_str!("../../../../data/pam.d/glimpse-lock");
    let stack = |name: &str| pam_stack_in(name, &PAM_DIRS, |_| true, |_| Some(shipped.to_owned()));
    assert_eq!(
        stack("glimpse-lock").outcome,
        Ok(Some("/etc/pam.d/glimpse-lock".to_owned()))
    );
    for name in [".hidden", "a\0b", "..", ".", "/abs", "a/b", "../passwd", ""] {
        assert_eq!(
            stack(name).outcome,
            Err(NOT_A_SERVICE.to_owned()),
            "{name:?} must be refused as a name"
        );
    }
}

#[test]
fn every_include_substack_and_debian_include_is_found() {
    let shipped = include_str!("../../../../data/pam.d/glimpse-lock");
    assert_eq!(pam_includes(shipped), ["system-auth", "system-auth"]);
    let debian = include_str!("../../../../data/pam.d/debian/glimpse-lock");
    assert_eq!(pam_includes(debian), ["common-auth", "common-account"]);
    let fedora = include_str!("../../../../data/pam.d/fedora/glimpse-lock");
    assert_eq!(pam_includes(fedora), ["password-auth", "password-auth"]);
    assert_eq!(
        pam_includes(
            "-auth substack login\n\
                 auth [success=1 default=ignore] pam_unix.so\n\
                 # auth include commented\n\
                 auth required pam_deny.so # include trailing\n\
                 account include\n\
                 auth \\\n  include continued\n"
        ),
        ["login", "continued"]
    );
}

#[test]
fn a_missing_file_behind_a_nested_include_fails() {
    let dirs = ["/etc/pam.d"];
    let exists = |path: &Path| {
        path == Path::new("/etc/pam.d/glimpse-lock") || path == Path::new("/etc/pam.d/system-auth")
    };
    let read = |path: &Path| {
        Some(match path == Path::new("/etc/pam.d/system-auth") {
            true => "auth include gone\nauth include system-auth\n".to_owned(),
            false => "auth include system-auth\n".to_owned(),
        })
    };
    let outcome = pam_stack_in("glimpse-lock", &dirs, exists, read).outcome;
    assert!(
        outcome
            .as_ref()
            .is_err_and(|reason| reason.contains("\"gone\"")),
        "{outcome:?}"
    );
}

#[test]
fn a_stack_including_a_missing_file_fails() {
    let dirs = ["/etc/pam.d", "/usr/lib/pam.d"];
    let shipped = include_str!("../../../../data/pam.d/glimpse-lock");
    let read = |_: &Path| Some(shipped.to_owned());
    let only_ours = |path: &Path| path == Path::new("/etc/pam.d/glimpse-lock");
    let outcome = pam_stack_in("glimpse-lock", &dirs, only_ours, read).outcome;
    assert!(
        outcome
            .as_ref()
            .is_err_and(|reason| reason.contains("\"system-auth\"")),
        "{outcome:?}"
    );
    let vendor_auth = |path: &Path| {
        path == Path::new("/etc/pam.d/glimpse-lock")
            || path == Path::new("/usr/lib/pam.d/system-auth")
    };
    assert_eq!(
        pam_stack_in("glimpse-lock", &dirs, vendor_auth, read).outcome,
        Ok(Some("/etc/pam.d/glimpse-lock".to_owned())),
        "an include found in the second directory resolves"
    );
    let absolute = |path: &Path| path == Path::new("/etc/pam.d/glimpse-lock");
    let text = |_: &Path| Some("auth include /opt/pam/auth\n".to_owned());
    assert!(
        pam_stack_in("glimpse-lock", &dirs, absolute, text)
            .outcome
            .is_err()
    );
}

#[test]
fn a_report_is_one_line_per_check() {
    assert_eq!(Check::new("uid_map", Ok(None)).line(), "uid_map: ok");
    assert_eq!(
        Check::new("unix_chkpwd", Ok(Some("not installed; skipped".into()))).line(),
        "unix_chkpwd: ok (not installed; skipped)"
    );
    let failed = Check::new("NoNewPrivs", Err("NoNewPrivs is 1".into()));
    assert_eq!(failed.line(), "NoNewPrivs: fail (NoNewPrivs is 1)");
    assert_eq!(
        failures(&[Check::new("a", Ok(None)), failed]),
        vec!["NoNewPrivs: fail (NoNewPrivs is 1)".to_owned()]
    );
}

use std::env;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn find(
    glimpse_deno: Option<OsString>,
    path: Option<OsString>,
    home: Option<PathBuf>,
    is_executable: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(deno) = glimpse_deno.filter(|deno| !deno.is_empty()) {
        return Some(deno.into());
    }
    if let Some(path) = path {
        for dir in env::split_paths(&path) {
            let candidate = dir.join("deno");
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    home.map(|home| home.join(".deno/bin/deno"))
        .filter(|candidate| is_executable(candidate))
}

pub fn deno() -> Option<PathBuf> {
    find(
        env::var_os("GLIMPSE_DENO"),
        env::var_os("PATH"),
        env::var_os("HOME").map(PathBuf::from),
        |path| {
            path.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_respects_override_then_path_then_home() {
        let path = OsString::from("/first:/second");
        let home = PathBuf::from("/home/test");
        let executable = |candidate: &Path| {
            candidate == Path::new("/second/deno")
                || candidate == Path::new("/home/test/.deno/bin/deno")
        };

        assert_eq!(
            find(
                Some("/custom/deno".into()),
                Some(path.clone()),
                Some(home.clone()),
                executable
            ),
            Some(PathBuf::from("/custom/deno"))
        );
        assert_eq!(
            find(None, Some(path.clone()), Some(home.clone()), executable),
            Some(PathBuf::from("/second/deno"))
        );
        assert_eq!(
            find(None, Some(path.clone()), Some(home.clone()), |_| true),
            Some(PathBuf::from("/first/deno"))
        );
        assert_eq!(
            find(
                Some(OsString::new()),
                Some(path),
                Some(home.clone()),
                |candidate| candidate == Path::new("/home/test/.deno/bin/deno")
            ),
            Some(home.join(".deno/bin/deno"))
        );
        assert_eq!(find(None, None, None, |_| false), None);
    }
}

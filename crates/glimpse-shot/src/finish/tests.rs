use super::*;

struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_closed_stdout_is_not_a_failure_and_other_errors_are() {
    assert!(line(&mut Closed, "shot.png").is_ok());
    assert!(ignoring_broken_pipe(Err(io::ErrorKind::PermissionDenied.into())).is_err());
}

#[test]
fn the_default_directory_is_the_special_pictures_folder_under_screenshots() {
    assert_eq!(
        pictures_in(Some(PathBuf::from("/data/Bilder")), Path::new("/home/me")),
        PathBuf::from("/data/Bilder/Screenshots")
    );
}

#[test]
fn without_a_special_pictures_folder_the_default_is_home_pictures() {
    assert_eq!(
        pictures_in(None, Path::new("/home/me")),
        PathBuf::from("/home/me/Pictures/Screenshots")
    );
}

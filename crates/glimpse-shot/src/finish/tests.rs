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

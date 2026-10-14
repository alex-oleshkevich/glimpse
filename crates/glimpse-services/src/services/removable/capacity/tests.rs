use super::*;

#[test]
fn a_real_mount_point_yields_a_nonzero_total_with_available_no_greater_than_it() {
    let dir = tempfile::tempdir().expect("temporary mount point");
    let sampled = sample(dir.path()).expect("statvfs succeeds on a real path");

    assert!(sampled.total > 0);
    assert!(sampled.available <= sampled.total);
}

#[test]
fn a_path_that_does_not_exist_yields_nothing_rather_than_panicking() {
    assert!(sample(Path::new("/nonexistent-for-glimpse-removable-tests")).is_none());
}

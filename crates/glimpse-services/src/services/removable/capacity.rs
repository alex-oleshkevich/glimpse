use std::path::Path;

use super::Capacity;

pub fn sample(path: &Path) -> Option<Capacity> {
    let stat = rustix::fs::statvfs(path).ok()?;
    Some(Capacity {
        total: stat.f_blocks.checked_mul(stat.f_frsize)?,
        available: stat.f_bavail.checked_mul(stat.f_frsize)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_mount_point_yields_a_nonzero_total_with_available_no_greater_than_it() {
        let sampled = sample(&std::env::temp_dir()).expect("statvfs succeeds on a real path");

        assert!(sampled.total > 0);
        assert!(sampled.available <= sampled.total);
    }

    #[test]
    fn a_path_that_does_not_exist_yields_nothing_rather_than_panicking() {
        assert!(sample(Path::new("/nonexistent-for-glimpse-removable-tests")).is_none());
    }
}

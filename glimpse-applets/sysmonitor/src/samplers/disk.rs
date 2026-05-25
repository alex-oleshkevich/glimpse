use std::ffi::CString;
use std::mem::MaybeUninit;
use std::path::Path;

/// Per-mountpoint filesystem usage. Populated from `statvfs(3)`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DiskSample {
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_bytes: u64,
    /// `1.0 - (avail / total)` — uses the userspace-available count
    /// (`f_bavail`), not raw free blocks (`f_bfree`). The former excludes
    /// blocks reserved for root on ext-family filesystems, which is what
    /// `df` shows and what users compare against.
    pub util: f64,
}

#[derive(Debug, Default)]
pub struct DiskSampler;

impl DiskSampler {
    pub fn new() -> Self {
        Self
    }

    /// Reads filesystem usage for `mountpoint`. Returns `None` on any
    /// failure (path doesn't exist, statvfs syscall error). The caller
    /// renders that as a hidden indicator rather than misleading zeros.
    pub fn sample(&self, mountpoint: &Path) -> Option<DiskSample> {
        statvfs(mountpoint).map(|s| sample_from_statvfs(&s))
    }
}

fn sample_from_statvfs(s: &libc::statvfs) -> DiskSample {
    // f_frsize is the "fundamental file system block size" — what
    // f_blocks, f_bfree, f_bavail are measured in. f_bsize is the
    // "preferred block size" for I/O and is deliberately not used here.
    // On the platforms we ship for (x86_64 Linux) `fsblkcnt_t` is u64
    // so we treat these as native u64; on 32-bit hosts the same code
    // would need `as u64` casts back, which is out of scope.
    let frsize = s.f_frsize;
    let total_bytes = s.f_blocks * frsize;
    let avail_bytes = s.f_bavail * frsize;
    // Used = total - avail mirrors how `df` reports usage: it counts
    // the root-reserved chunk as "used" because non-root processes
    // can't allocate into it anyway.
    let used_bytes = total_bytes.saturating_sub(avail_bytes);
    let free_bytes = s.f_bfree * frsize;
    let util = if total_bytes == 0 {
        0.0
    } else {
        used_bytes as f64 / total_bytes as f64
    };
    DiskSample {
        used_bytes,
        free_bytes,
        total_bytes,
        util,
    }
}

fn statvfs(path: &Path) -> Option<libc::statvfs> {
    let c = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut buf: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    // SAFETY: c is a valid NUL-terminated C string for the duration of
    // this call; buf is a valid writable pointer; statvfs() initialises
    // the struct on success and we only call `assume_init` then.
    let rc = unsafe { libc::statvfs(c.as_ptr(), buf.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    Some(unsafe { buf.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure conversion test — synthesise a `statvfs` struct so the pure
    /// math is covered without depending on what `/` looks like on the
    /// host running the suite.
    #[test]
    fn used_equals_total_minus_avail_in_bytes() {
        let s = make_statvfs(4096, 1_000_000, 200_000, 100_000);
        let sample = sample_from_statvfs(&s);
        // total = 1_000_000 * 4096
        assert_eq!(sample.total_bytes, 1_000_000 * 4096);
        // avail = 100_000 blocks * 4096 = 409_600_000 bytes
        // used  = total - avail = 590_400_000 * (1) → in bytes
        assert_eq!(sample.used_bytes, (1_000_000 - 100_000) * 4096);
        assert_eq!(sample.free_bytes, 200_000 * 4096);
        // util uses *avail*, not free.
        let expected = (1_000_000 - 100_000) as f64 / 1_000_000.0;
        assert!((sample.util - expected).abs() < 1e-12);
    }

    /// Zero-sized fs (e.g. tmpfs mounted with size=0): no NaN, util=0.
    #[test]
    fn empty_filesystem_does_not_divide_by_zero() {
        let s = make_statvfs(4096, 0, 0, 0);
        let sample = sample_from_statvfs(&s);
        assert!(sample.util.abs() < f64::EPSILON);
        assert_eq!(sample.total_bytes, 0);
    }

    /// Reserved-for-root accounting: a fs where bfree > bavail still
    /// reports util based on the *user-visible* avail, matching `df`.
    /// Without this distinction the panel pill would say 92% on an ext4
    /// rootfs where `du` shows 87%, which is the kind of mismatch users
    /// open bug reports about.
    #[test]
    fn root_reserved_chunk_counted_as_used() {
        // 1000 total, 50 free, 30 avail. Root reserved 20 blocks.
        let s = make_statvfs(1024, 1000, 50, 30);
        let sample = sample_from_statvfs(&s);
        let expected_used = (1000 - 30) * 1024;
        assert_eq!(sample.used_bytes, expected_used);
        let expected_util = (1000 - 30) as f64 / 1000.0;
        assert!((sample.util - expected_util).abs() < 1e-12);
        // free still reports raw bfree so a renderer can show both.
        assert_eq!(sample.free_bytes, 50 * 1024);
    }

    fn make_statvfs(frsize: u64, blocks: u64, bfree: u64, bavail: u64) -> libc::statvfs {
        // SAFETY: zero-initialising a POD struct is fine; statvfs has no
        // pointer or NonZero fields. We then set the fields we exercise.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        s.f_frsize = frsize;
        s.f_blocks = blocks;
        s.f_bfree = bfree;
        s.f_bavail = bavail;
        s
    }
}

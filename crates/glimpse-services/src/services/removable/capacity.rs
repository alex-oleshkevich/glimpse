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
mod tests;

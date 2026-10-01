const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

pub fn bytes(n: u64) -> String {
    if n < 1000 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0;
    while unit < UNITS.len() - 1 && (value >= 1000.0 || (value * 10.0).round() >= 10000.0) {
        value /= 1000.0;
        unit += 1;
    }
    let tenths = (value * 10.0).round();
    if tenths % 10.0 == 0.0 {
        return format!("{} {}", tenths / 10.0, UNITS[unit]);
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests;

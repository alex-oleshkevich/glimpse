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
mod tests {
    use super::bytes;

    #[test]
    fn below_1000_has_no_decimal() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
    }

    #[test]
    fn si_units_use_base_1000() {
        assert_eq!(bytes(1000), "1 KB");
        assert_eq!(bytes(1_000_000_000), "1 GB");
    }

    #[test]
    fn rounding_at_a_unit_boundary_carries_into_the_next_unit() {
        assert_eq!(bytes(999_999), "1 MB");
    }

    #[test]
    fn a_round_value_drops_the_trailing_zero_and_a_fractional_one_keeps_it() {
        assert_eq!(bytes(64_000_000_000), "64 GB");
        assert_eq!(bytes(1_500), "1.5 KB");
    }

    #[test]
    fn u64_max_does_not_panic_or_overflow() {
        assert_eq!(bytes(u64::MAX), "18.4 EB");
    }
}

//! Numbers and times the way people say them.

use std::time::Duration;

/// `512 B`, `1.1 GB`, `34 MB` (powers of 1000, as disks and networks count).
pub fn bytes(n: u64) -> String {
    if n < 1000 {
        return format!("{n} B");
    }
    scaled(n, &[" KB", " MB", " GB", " TB", " PB"])
}

/// `340ms`, `4.2s`, `3m07s`, `1h02m`.
pub fn duration(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s < 1.0 {
        return format!("{}ms", d.as_millis());
    }
    // rounded before the unit is picked, so 59.96s reads 1m00s rather than 60.0s
    let tenths = (s * 10.0).round();
    if tenths < 600.0 {
        return format!("{:.1}s", tenths / 10.0);
    }
    let s = s.round() as u64;
    if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, s % 3600 / 60)
    }
}

/// `999`, `1.2k`, `34k`, `5.6M`.
pub fn count(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    scaled(n, &["k", "M", "B"])
}

/// `n` (at least 1000) in the first of `units`, each a thousand times the last, that keeps
/// it under 1000 once rounded: one decimal below 10, none from there.
fn scaled(n: u64, units: &[&str]) -> String {
    let mut v = n as f64;
    for (i, unit) in units.iter().enumerate() {
        v /= 1000.0;
        let short = format!("{v:.1}");
        let text = if short.parse::<f64>().is_ok_and(|r| r < 10.0) {
            short
        } else {
            format!("{v:.0}")
        };
        if (text != "1000" && v < 1000.0) || i == units.len() - 1 {
            return format!("{text}{unit}");
        }
    }
    unreachable!("scaled is given at least one unit")
}

/// `1 file`, `3 files`. For irregular words pass the plural: `plural_with(2, "child", "children")`.
pub fn plural(n: u64, word: &str) -> String {
    plural_with(n, word, &format!("{word}s"))
}

/// `1 child`, `3 children`: like [`plural`], for words that don't just add an s.
pub fn plural_with(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `just now`, `5m ago`, `3h ago`, `2d ago`.
pub fn ago(d: Duration) -> String {
    match d.as_secs() {
        0..10 => "just now".into(),
        s @ 10..60 => format!("{s}s ago"),
        s @ 60..3600 => format!("{}m ago", s / 60),
        s @ 3600..86400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_numbers_like_people() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(34_200_000), "34 MB");
        assert_eq!(bytes(1_100_000_000), "1.1 GB");
        assert_eq!(count(1_234), "1.2k");
        assert_eq!(count(999), "999");
        assert_eq!(plural(1, "file"), "1 file");
        assert_eq!(plural(3, "file"), "3 files");
    }

    #[test]
    fn says_times_like_people() {
        assert_eq!(duration(Duration::from_millis(340)), "340ms");
        assert_eq!(duration(Duration::from_secs(187)), "3m07s");
        assert_eq!(duration(Duration::from_secs(3720)), "1h02m");
        assert_eq!(ago(Duration::from_secs(300)), "5m ago");
    }

    #[test]
    fn every_size_of_number_and_age_reads_well() {
        assert_eq!(duration(Duration::from_millis(4200)), "4.2s");
        assert_eq!(count(34_000), "34k");
        assert_eq!(count(5_600_000), "5.6M");
        assert_eq!(count(7_000_000_000), "7.0B");
        assert_eq!(ago(Duration::from_secs(3)), "just now");
        assert_eq!(ago(Duration::from_secs(42)), "42s ago");
        assert_eq!(ago(Duration::from_secs(7200)), "2h ago");
        assert_eq!(ago(Duration::from_secs(3 * 86400)), "3d ago");
    }

    #[test]
    fn rounding_comes_before_the_unit() {
        assert_eq!(bytes(999_950), "1.0 MB");
        assert_eq!(bytes(999_499), "999 KB");
        assert_eq!(bytes(9_960), "10 KB");
        assert_eq!(bytes(9_940), "9.9 KB");
        assert_eq!(bytes(u64::MAX), "18447 PB");
        assert_eq!(count(999_999), "1.0M");
        assert_eq!(count(9_999), "10k");
        assert_eq!(count(999_999_999_999), "1000B");
        assert_eq!(duration(Duration::from_millis(59_960)), "1m00s");
        assert_eq!(duration(Duration::from_millis(59_940)), "59.9s");
        assert_eq!(duration(Duration::from_millis(119_600)), "2m00s");
        assert_eq!(duration(Duration::from_millis(3_599_600)), "1h00m");
    }
}

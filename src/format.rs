//! Numbers and times the way people say them.

use std::time::Duration;

/// `512 B`, `34.2 MB`, `1.1 GB` (powers of 1000, as disks and networks count).
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if n < 1000 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut unit = "B";
    for u in UNITS {
        if v < 1000.0 {
            break;
        }
        v /= 1000.0;
        unit = u;
    }
    if v < 10.0 {
        format!("{v:.1} {unit}")
    } else {
        format!("{v:.0} {unit}")
    }
}

/// `340ms`, `4.2s`, `3m07s`, `1h02m`.
pub fn duration(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s < 1.0 {
        format!("{}ms", d.as_millis())
    } else if s < 60.0 {
        format!("{s:.1}s")
    } else if s < 3600.0 {
        format!("{}m{:02}s", s as u64 / 60, s as u64 % 60)
    } else {
        format!("{}h{:02}m", s as u64 / 3600, s as u64 % 3600 / 60)
    }
}

/// `999`, `1.2k`, `34k`, `5.6M`.
pub fn count(n: u64) -> String {
    let (v, unit) = match n {
        0..1_000 => return n.to_string(),
        1_000..1_000_000 => (n as f64 / 1e3, "k"),
        1_000_000..1_000_000_000 => (n as f64 / 1e6, "M"),
        _ => (n as f64 / 1e9, "B"),
    };
    if v < 10.0 {
        format!("{v:.1}{unit}")
    } else {
        format!("{v:.0}{unit}")
    }
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
}

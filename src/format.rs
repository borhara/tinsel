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

/// `3 minutes ago`, `1 hour ago`, `2 weeks ago`: like [`ago`], in words, for a sentence
/// rather than a column. Under a minute is `just now`.
///
/// ```
/// use std::time::Duration;
///
/// assert_eq!(tinsel::format::ago_long(Duration::from_secs(7_200)), "2 hours ago");
/// ```
pub fn ago_long(d: Duration) -> String {
    let s = d.as_secs();
    let (n, unit) = match s {
        0..60 => return "just now".into(),
        60..3_600 => (s / 60, "minute"),
        3_600..86_400 => (s / 3_600, "hour"),
        86_400..604_800 => (s / 86_400, "day"),
        // a month and a year as their average lengths
        604_800..2_629_800 => (s / 604_800, "week"),
        2_629_800..31_557_600 => (s / 2_629_800, "month"),
        _ => (s / 31_557_600, "year"),
    };
    format!("{} ago", plural(n, unit))
}

/// `1,234,567`: a whole number with its thousands marked off, for counts read in full.
///
/// ```
/// assert_eq!(tinsel::format::grouped(1_234_567), "1,234,567");
/// assert_eq!(tinsel::format::grouped(999), "999");
/// ```
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The time of day on this computer's clock, in its time zone, as `21:04`: for a clock in
/// a status bar. Where the time zone can't be found (outside Unix), it's UTC.
///
/// ```
/// let now = tinsel::format::local_clock();
/// assert_eq!(now.len(), 5);
/// assert_eq!(&now[2..3], ":");
/// ```
pub fn local_clock() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    clock_at(now, local::utc_offset(now))
}

/// `secs` since 1970 as the time of day, `offset` seconds ahead of UTC.
fn clock_at(secs: i64, offset: i64) -> String {
    let mins = (secs + offset).rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

#[cfg(unix)]
mod local {
    /// How far ahead of UTC local time is at `secs` since 1970, from the C library's idea of
    /// the time zone.
    // c_long is i32 on 32-bit targets
    #[allow(clippy::useless_conversion)]
    pub(super) fn utc_offset(secs: i64) -> i64 {
        #[repr(C)]
        struct Tm {
            fields: [i32; 9],
            gmtoff: std::ffi::c_long,
            zone: *const std::ffi::c_char,
        }
        unsafe extern "C" {
            fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
        }
        let mut tm = Tm {
            fields: [0; 9],
            gmtoff: 0,
            zone: std::ptr::null(),
        };
        // SAFETY: localtime_r fills one struct tm, whose leading fields have this layout on
        // macOS and Linux; on failure it returns null and gmtoff stays 0
        unsafe { localtime_r(&secs, &mut tm) };
        i64::from(tm.gmtoff)
    }
}

#[cfg(not(unix))]
mod local {
    pub(super) fn utc_offset(_: i64) -> i64 {
        0
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

    #[test]
    fn long_ages_read_as_words() {
        let ago = |s| ago_long(Duration::from_secs(s));
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(7_200), "2 hours ago");
        assert_eq!(ago(86_400 * 3), "3 days ago");
        assert_eq!(ago(86_400 * 8), "1 week ago");
        assert_eq!(ago(86_400 * 70), "2 months ago");
        assert_eq!(ago(86_400 * 800), "2 years ago");
    }

    #[test]
    fn thousands_are_marked_off() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(123_456), "123,456");
        assert_eq!(grouped(u64::MAX), "18,446,744,073,709,551,615");
    }

    #[test]
    fn the_clock_reads_the_time_of_day_in_a_zone() {
        assert_eq!(clock_at(0, 0), "00:00");
        assert_eq!(clock_at(86_400 + 3_600 * 13 + 60 * 7 + 59, 0), "13:07");
        assert_eq!(clock_at(0, -3_600), "23:00", "the day before, an hour west");
        assert_eq!(clock_at(0, 5 * 3_600 + 1_800), "05:30");
        let offset = local::utc_offset(0);
        assert!(offset.abs() <= 14 * 3_600);
    }
}

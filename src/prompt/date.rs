//! Dates, picked from a calendar or typed.

use super::*;

/// A calendar date, as returned by [`date`]. Shown and parsed as `YYYY-MM-DD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// The year, e.g. 2026.
    pub year: i32,
    /// The month, 1-12.
    pub month: u8,
    /// The day of the month, from 1.
    pub day: u8,
}

pub(super) const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl Date {
    /// A date, if it exists (no February 30th).
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)).then_some(Date {
            year,
            month,
            day,
        })
    }

    /// Today, in the local time zone where that can be found (Unix), otherwise in UTC.
    pub fn today() -> Date {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Date::from_days((secs + crate::term::utc_offset_secs(secs)).div_euclid(86_400))
    }

    /// Days since 1970-01-01.
    pub(super) fn days(self) -> i64 {
        // Howard Hinnant's days_from_civil
        let y = self.year as i64 - (self.month <= 2) as i64;
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub(super) fn from_days(z: i64) -> Date {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        Date {
            year: (yoe + era * 400 + (month <= 2) as i64) as i32,
            month,
            day,
        }
    }

    fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }

    /// The same day `n` months on, or the month's last day if it's shorter.
    pub(super) fn add_months(self, n: i32) -> Date {
        let m = self.year * 12 + self.month as i32 - 1 + n;
        let (year, month) = (m.div_euclid(12), (m.rem_euclid(12) + 1) as u8);
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }

    /// 0 for Monday through 6 for Sunday.
    pub(super) fn weekday(self) -> usize {
        // 1970-01-01 was a Thursday
        (self.days() + 3).rem_euclid(7) as usize
    }
}

pub(super) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for Date {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Date, String> {
        let bad = || format!("“{s}” isn't a date like 2026-03-14");
        let mut parts = s.trim().splitn(3, '-');
        let mut next = || parts.next().and_then(|p| p.parse::<i32>().ok()).ok_or_else(bad);
        let (y, m, d) = (next()?, next()?, next()?);
        let (m, d) = (u8::try_from(m).map_err(|_| bad())?, u8::try_from(d).map_err(|_| bad())?);
        Date::new(y, m, d).ok_or_else(bad)
    }
}

/// A date picked from a calendar: ←→ move a day, ↑↓ a week, PgUp/PgDn (or `[` `]`) a month,
/// `t` jumps to today. Starts on `default`, or today.
///
/// ```no_run
/// let when = tinsel::prompt::date("Release on", None)?;
/// println!("{when}");   // 2026-11-12
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
///
/// # Errors
///
/// [`Error::Cancelled`] on Ctrl-C or Esc, [`Error::NotATerminal`] when there's no terminal to ask
/// on and no line to read, [`Error::Invalid`] when a line read from a pipe isn't an answer it
/// takes, and [`Error::Io`] when the terminal can't be used.
pub fn date(question: &str, default: Option<Date>) -> Result<Date> {
    let today = Date::today();
    let start = default.unwrap_or(today);
    if !interactive() {
        let line = read_piped_line()?;
        if line.trim().is_empty() {
            return Ok(start);
        }
        return line.parse().map_err(Error::Invalid);
    }
    let mut ui = Ui::open()?;
    let mut at = start;
    loop {
        let mut lines = vec![format!("{}  {}", ask(question), paint(BLUSH, &at.to_string()))];
        lines.extend(calendar(at, today));
        lines.push(keys("←→ day · ↑↓ week · pgup/pgdn month · t today · enter choose"));
        ui.draw(&lines);
        at = match ui.key()? {
            Key::Left | Key::Char('h') => at.add_days(-1),
            Key::Right | Key::Char('l') => at.add_days(1),
            Key::Up | Key::Char('k') => at.add_days(-7),
            Key::Down | Key::Char('j') => at.add_days(7),
            Key::PageUp | Key::Char('[') => at.add_months(-1),
            Key::PageDown | Key::Char(']') => at.add_months(1),
            Key::Char('t') => today,
            Key::Enter => break,
            Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
            _ => at,
        };
    }
    ui.draw(&[answered(question, &at.to_string())]);
    Ok(at)
}

/// A month laid out Monday to Sunday, the chosen day as a pill and today underlined.
pub(super) fn calendar(at: Date, today: Date) -> Vec<String> {
    let first = Date { day: 1, ..at };
    let title = format!("{} {}", MONTHS[at.month as usize - 1], at.year);
    let mut lines = vec![
        format!("       {}", paint(TEXT, &title)),
        format!("    {}", paint(MUTED, " Mo  Tu  We  Th  Fr  Sa  Su")),
    ];
    let mut row = "    ".to_string() + &"    ".repeat(first.weekday());
    for day in 1..=days_in_month(at.year, at.month) {
        let d = Date { day, ..at };
        let num = format!("{day:>2}");
        row += &if d == at {
            paint_on(ROSE, mix(TRACK, ROSE, 0.28), &format!(" {num} "))
        } else if d == today {
            format!(" {} ", underline(&paint(ROSE, &num)))
        } else {
            format!(" {} ", paint(SOFT, &num))
        };
        if d.weekday() == 6 {
            lines.push(std::mem::replace(&mut row, "    ".into()));
        }
    }
    if row.trim() != "" {
        lines.push(row);
    }
    lines
}

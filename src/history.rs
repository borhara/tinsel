//! How long steps took before, so they can say how long they'll take now.
//!
//! One small text file per program in the user's cache directory
//! (`$XDG_CACHE_HOME/tinsel` or `~/.cache/tinsel`), a line per step:
//! `deploy › Build package<TAB>41.2 39.8 44.0`. The last few runs are kept, and the
//! estimate is their median, so one unusually slow run doesn't throw it off.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use crate::block::Block;
use crate::message::Tone;
use crate::style::paint;
use crate::style::palette::{AMBER, MUTED, ROSE, SAGE};
use crate::table::{Align, Table};
use crate::{format, sparkline};

const KEEP: usize = 10;

// 0 = up to TINSEL_NO_HISTORY, 1 = on, 2 = off
static ENABLED: AtomicU8 = AtomicU8::new(0);

/// Turn remembered step timings on or off for every [`Live`](crate::Live) display: with it
/// off, nothing is read from or written to the cache, and steps show no estimates. Without
/// this, setting `TINSEL_NO_HISTORY` turns it off. [`Live::no_history`](crate::Live::no_history)
/// turns it off for one display.
///
/// ```
/// tinsel::set_history(false);
/// ```
pub fn set_history(on: bool) {
    ENABLED.store(if on { 1 } else { 2 }, Ordering::Relaxed);
}

pub(crate) struct History {
    path: PathBuf,
    runs: BTreeMap<String, Vec<f64>>,
    dirty: bool,
}

impl History {
    /// The history for a program, or `None` if there's nowhere to keep it or it's turned off.
    pub(crate) fn open(app: &str) -> Option<History> {
        let on = match ENABLED.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => std::env::var_os("TINSEL_NO_HISTORY").is_none(),
        };
        if !on {
            return None;
        }
        let path = cache_dir()?.join(format!("{}.tsv", sanitize(app)));
        let runs = std::fs::read_to_string(&path).map(|s| parse(&s)).unwrap_or_default();
        Some(History {
            path,
            runs,
            dirty: false,
        })
    }

    /// A history kept nowhere, for tests.
    #[cfg(test)]
    pub(crate) fn scratch() -> History {
        History {
            path: PathBuf::from("/nonexistent/x.tsv"),
            runs: BTreeMap::new(),
            dirty: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn runs(&self, key: &str) -> &[f64] {
        self.runs.get(key).map_or(&[], Vec::as_slice)
    }

    /// The program's own name, used when nothing else is given.
    pub(crate) fn default_app() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "tinsel".into())
    }

    /// The typical duration of a step, in seconds, from its recent runs.
    pub(crate) fn estimate(&self, key: &str) -> Option<f64> {
        History::median(self.runs.get(key)?)
    }

    fn median(runs: &[f64]) -> Option<f64> {
        let mut runs = runs.to_vec();
        if runs.is_empty() {
            return None;
        }
        runs.sort_by(|a, b| a.total_cmp(b));
        let mid = runs.len() / 2;
        Some(if runs.len() % 2 == 0 {
            (runs[mid - 1] + runs[mid]) / 2.0
        } else {
            runs[mid]
        })
    }

    pub(crate) fn record(&mut self, key: &str, secs: f64) {
        let runs = self.runs.entry(clean(key)).or_default();
        runs.push(secs);
        if runs.len() > KEEP {
            runs.drain(..runs.len() - KEEP);
        }
        self.dirty = true;
    }

    /// Write the history back, if anything was recorded. Written to a temporary file and
    /// renamed into place, so a crash never leaves half a file.
    pub(crate) fn save(&self) {
        if !self.dirty {
            return;
        }
        let Some(dir) = self.path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let body: String = self
            .runs
            .iter()
            .map(|(key, runs)| {
                let runs: Vec<String> = runs.iter().map(|r| format!("{r:.2}")).collect();
                format!("{key}\t{}\n", runs.join(" "))
            })
            .collect();
        let tmp = self.path.with_extension(format!("tsv.{}", std::process::id()));
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

/// What's been learned about step timings, as a table per program: recent runs as a
/// sparkline, the usual time, the last time, and whether the last run was unusually slow or fast.
pub(crate) fn report(app: Option<&str>) -> Block {
    let apps: Vec<String> = match app {
        Some(a) => vec![a.to_string()],
        None => cache_dir()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|entries| {
                let mut names: Vec<String> = entries
                    .filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".tsv").map(String::from))
                    .collect();
                names.sort();
                names
            })
            .unwrap_or_default(),
    };
    let mut out = Block::new();
    for app in &apps {
        let Some(history) = History::open(app).filter(|h| !h.runs.is_empty()) else {
            continue;
        };
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push(format!("  {}", paint(ROSE, app)));
        let mut table = Table::new(["step", "recent", "usual", "last", "trend"]).align(4, Align::Left);
        for (key, runs) in &history.runs {
            let (Some(&last), Some(usual)) = (runs.last(), history.estimate(key)) else {
                continue;
            };
            let earlier = History::median(&runs[..runs.len() - 1]);
            let trend = match earlier {
                Some(before) if last > before * 1.25 => {
                    paint(AMBER, &format!("{:.0}% slower", (last / before - 1.0) * 100.0))
                }
                Some(before) if last < before * 0.8 => {
                    paint(SAGE, &format!("{:.0}% faster", (1.0 - last / before) * 100.0))
                }
                Some(_) => paint(MUTED, "steady"),
                None => paint(MUTED, "first run"),
            };
            let secs = |s: f64| format::duration(Duration::from_secs_f64(s));
            table = table.row([
                key.split_once(" › ").map_or(key.as_str(), |(_, step)| step).to_string(),
                sparkline(runs),
                secs(usual),
                secs(last),
                trend,
            ]);
        }
        out.extend(table.lines());
    }
    if out.is_empty() {
        out.push(Tone::Hint.line("no step timings recorded yet: they're kept after a Live run on a terminal"));
    }
    out
}

/// The key a step's runs are kept under: its parent's key (or the display's title) and its label.
pub(crate) fn key(parent: &str, label: &str) -> String {
    clean(&format!("{parent} › {label}"))
}

fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("tinsel"))
}

/// Keys can't contain the file's separators.
fn clean(key: &str) -> String {
    key.replace(['\t', '\n', '\r'], " ")
}

fn sanitize(app: &str) -> String {
    let s: String = app
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with('.') {
        format!("app{s}")
    } else {
        s
    }
}

fn parse(body: &str) -> BTreeMap<String, Vec<f64>> {
    body.lines()
        .filter_map(|line| {
            let (key, runs) = line.split_once('\t')?;
            let runs: Vec<f64> = runs.split_whitespace().filter_map(|r| r.parse().ok()).collect();
            Some((key.to_string(), runs))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> History {
        History::scratch()
    }

    #[test]
    fn estimates_with_the_median() {
        let mut h = history();
        for secs in [40.0, 42.0, 300.0] {
            h.record("deploy › Build", secs);
        }
        assert_eq!(h.estimate("deploy › Build"), Some(42.0));
        h.record("deploy › Build", 44.0);
        assert_eq!(h.estimate("deploy › Build"), Some(43.0));
        assert_eq!(h.estimate("deploy › Upload"), None);
    }

    #[test]
    fn keeps_only_recent_runs() {
        let mut h = history();
        for i in 0..25 {
            h.record("k", i as f64);
        }
        assert_eq!(h.runs["k"].len(), KEEP);
        assert_eq!(h.runs["k"][0], 15.0);
    }

    #[test]
    fn reads_what_it_writes() {
        let parsed = parse("deploy › Build\t41.20 39.80\nbroken line\n");
        assert_eq!(parsed["deploy › Build"], vec![41.2, 39.8]);
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn can_be_turned_off_from_code() {
        set_history(false);
        assert!(History::open("tinsel-test").is_none());
        ENABLED.store(0, Ordering::Relaxed);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(sanitize("my app/../x"), "my_app_.._x");
        assert_eq!(sanitize(".hidden"), "app.hidden");
    }
}

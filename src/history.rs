//! How long steps took before, so they can say how long they'll take now.
//!
//! One small text file per program in the user's cache directory
//! (`$XDG_CACHE_HOME/tinsel` or `~/.cache/tinsel`), a line per step:
//! `deploy › Build package<TAB>41.2 39.8 44.0`. The last few runs are kept, and the
//! estimate is their median, so one unusually slow run doesn't throw it off.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
        Some(History::load(&cache_dir()?, app))
    }

    /// The history for a program, kept in `dir` rather than the cache, for tests.
    #[cfg(test)]
    pub(crate) fn open_in(dir: &Path, app: &str) -> Option<History> {
        Some(History::load(dir, app))
    }

    /// The history for a program, kept in `dir`.
    fn load(dir: &Path, app: &str) -> History {
        let path = dir.join(format!("{}.tsv", sanitize(app)));
        let runs = std::fs::read_to_string(&path).map(|s| parse(&s)).unwrap_or_default();
        History {
            path,
            runs,
            dirty: false,
        }
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
        let exe = std::env::current_exe().ok();
        let name = exe.as_deref().and_then(Path::file_stem);
        name.map_or("tinsel".into(), |s| s.to_string_lossy().into_owned())
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
    report_in(cache_dir().as_deref(), app)
}

/// [`report`] for the histories kept in `dir`.
fn report_in(dir: Option<&Path>, app: Option<&str>) -> Block {
    let apps: Vec<String> = match (dir, app) {
        (None, _) => Vec::new(),
        (Some(_), Some(a)) => vec![a.to_string()],
        (Some(d), None) => {
            let entries = std::fs::read_dir(d).into_iter().flatten().flatten();
            let mut names: Vec<String> = entries
                .filter_map(|e| e.file_name().to_str()?.strip_suffix(".tsv").map(String::from))
                .collect();
            names.sort();
            names
        }
    };
    let mut out = Block::new();
    for app in &apps {
        let history = dir.map(|d| History::load(d, app)).filter(|h| !h.runs.is_empty());
        let Some(history) = history else {
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

/// Where histories are kept, or `None` if there's nowhere or they're turned off.
fn cache_dir() -> Option<PathBuf> {
    let on = match ENABLED.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => std::env::var_os("TINSEL_NO_HISTORY").is_none(),
    };
    on.then(|| cache_dir_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME")))?
}

fn cache_dir_from(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".cache")))?;
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
        set_history(true);
        assert_eq!(cache_dir().is_some(), std::env::var_os("HOME").is_some());
        ENABLED.store(0, Ordering::Relaxed);
        let _ = cache_dir();
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(sanitize("my app/../x"), "my_app_.._x");
        assert_eq!(sanitize(".hidden"), "app.hidden");
    }
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tinsel-history-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cache_lives_under_xdg_or_home() {
        let (xdg, home) = (Some(OsString::from("/x")), Some(OsString::from("/h")));
        assert_eq!(cache_dir_from(xdg, home.clone()), Some(PathBuf::from("/x/tinsel")));
        assert_eq!(cache_dir_from(None, home), Some(PathBuf::from("/h/.cache/tinsel")));
        assert_eq!(cache_dir_from(None, None), None);
    }

    #[test]
    fn the_program_is_named_after_its_executable() {
        assert!(!History::default_app().is_empty());
    }

    #[test]
    fn saves_and_reports_what_it_learned() {
        crate::set_color_mode(crate::ColorMode::None);
        let dir = scratch_dir("report");
        let mut h = History::load(&dir, "my app");
        h.save();
        assert!(!dir.exists(), "nothing recorded, nothing written");
        for (key, runs) in [
            ("deploy › Build", &[40.0, 40.0, 80.0][..]),
            ("deploy › Upload", &[10.0, 10.0, 5.0]),
            ("deploy › Test", &[20.0, 21.0]),
            ("deploy › Lint", &[3.0]),
        ] {
            for &secs in runs {
                h.record(key, secs);
            }
        }
        h.runs.insert("deploy › Never".into(), Vec::new());
        h.save();
        History::load(&dir, "other").save();
        std::fs::write(dir.join("empty.tsv"), "").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a history").unwrap();

        let all = report_in(Some(&dir), None).plain();
        assert!(all.starts_with("  my_app\n"), "{all}");
        for want in ["100% slower", "50% faster", "steady", "first run"] {
            assert!(all.contains(want), "{want} in {all}");
        }
        assert!(
            !all.contains("Never") && !all.contains("empty") && !all.contains("notes"),
            "{all}"
        );
        assert_eq!(report_in(Some(&dir), Some("my_app")).plain(), all);

        let mut second = History::load(&dir, "second");
        second.record("x › y", 1.0);
        second.save();
        assert!(report_in(Some(&dir), None).plain().contains("\n\n  second\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn says_so_when_theres_nothing_to_report() {
        crate::set_color_mode(crate::ColorMode::None);
        let none = report_in(None, None).plain();
        assert!(none.contains("no step timings recorded yet"), "{none}");
        assert_eq!(report_in(Some(&scratch_dir("missing")), None).plain(), none);
        assert_eq!(crate::timings(Some("tinsel-never-ran")).plain(), none);
    }

    #[test]
    fn a_history_that_cant_be_written_is_let_go() {
        let mut h = History::load(Path::new("/dev/null/nowhere"), "x");
        h.record("k", 1.0);
        h.save();
    }

    #[test]
    fn a_history_in_a_read_only_place_is_let_go() {
        let dir = scratch_dir("read-only");
        std::fs::create_dir_all(&dir).unwrap();
        let mut perms = std::fs::metadata(&dir).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&dir, perms.clone()).unwrap();
        let mut h = History::load(&dir, "x");
        h.record("k", 1.0);
        h.save();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&dir, perms).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

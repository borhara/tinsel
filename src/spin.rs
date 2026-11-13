//! One-line live output for a single task: `spin` around a closure, or a standalone `Progress`.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::output::{self, Val};
use crate::style::palette::{MUTED, RED, ROSE, SOFT, TEXT};
use crate::style::{Rgb, mix, paint};
use crate::widgets::{BarStyle, Spinner, shimmer};
use crate::{format, term, text};

/// Redraws one line in place from a background thread until finished.
struct Ticker {
    stop: Arc<AtomicBool>,
    /// Frames a second; 0 for the global rate.
    fps: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
    prev_interrupt: Option<usize>,
}

impl Ticker {
    fn start(frame: impl Fn(f32) -> String + Send + 'static) -> Ticker {
        let prev_interrupt = term::guard_interrupt();
        // no cursor, and no wrapping: an over-long line is cut at the edge instead of breaking the redraw
        output::write("\x1b[?25l\x1b[?7l");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let fps = Arc::new(AtomicU32::new(0));
        let rate = fps.clone();
        let thread = thread::spawn(move || {
            let t0 = Instant::now();
            while !flag.load(Ordering::Relaxed) {
                output::write(&format!("\r{}\x1b[K", frame(t0.elapsed().as_secs_f32())));
                thread::sleep(crate::fps::frame(rate.load(Ordering::Relaxed)));
            }
        });
        Ticker {
            stop,
            fps,
            thread: Some(thread),
            prev_interrupt,
        }
    }

    fn finish(&mut self, last: &str) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.stop.store(true, Ordering::Relaxed);
        let _ = thread.join();
        output::write(&format!("\r{last}\x1b[K\n\x1b[?7h\x1b[?25h"));
        term::restore_interrupt(self.prev_interrupt.take());
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.finish("");
    }
}

/// `  ●  label` with the time at the right edge, in the same columns as a [`crate::Live`] step:
/// the mark sits in the middle of the spinner's column, so the label stays put.
fn finished_line(mark: &str, color: Rgb, label: &str, label_color: Rgb, time_color: Rgb, took: Duration) -> String {
    let gutter = Spinner::default().width();
    let (before, after) = ((gutter - 1) / 2, gutter - 1 - (gutter - 1) / 2);
    let left = format!(
        "  {}{}{}  {}",
        " ".repeat(before),
        paint(color, mark),
        " ".repeat(after),
        paint(label_color, label)
    );
    let took = format::duration(took);
    let pad = term::content_width().saturating_sub(text::width(&left) + took.chars().count() + 2);
    format!("{left}{}{}", " ".repeat(pad.max(2)), paint(time_color, &took))
}

fn done_line(label: &str, took: Duration) -> String {
    finished_line("●", ROSE, label, TEXT, MUTED, took)
}

/// Run `f` with a spinner beside `label`, then leave a `●  label` line with the time it took
/// on the right, lined up with [`Live`](crate::Live) steps.
pub fn spin<T>(label: &str, f: impl FnOnce() -> T) -> T {
    match try_spin(label, || Ok::<T, std::convert::Infallible>(f())) {
        Ok(v) => v,
        Err(never) => match never {},
    }
}

/// Like [`spin`], but an `Err` ends on `✕` with the error under it.
pub fn try_spin<T, E: Display>(label: &str, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let spinning = Spinning::start(label);
    let result = f();
    spinning.end(result.as_ref().err().map(ToString::to_string));
    result
}

/// One task under way for [`try_spin`]: a spinner on a terminal, nothing until it ends elsewhere.
struct Spinning<'a> {
    label: &'a str,
    start: Instant,
    ticker: Option<Ticker>,
}

impl<'a> Spinning<'a> {
    fn start(label: &'a str) -> Spinning<'a> {
        let drawn = !output::json() && !output::quiet() && output::is_terminal();
        let ticker = drawn.then(|| {
            let owned = label.to_string();
            Ticker::start(move |t| format!("  {}  {}", Spinner::default().frame(t), shimmer(&owned, t)))
        });
        Spinning {
            label,
            start: Instant::now(),
            ticker,
        }
    }

    /// Say how it went: done, or the error it failed with.
    fn end(self, error: Option<String>) {
        let (label, took) = (self.label, self.start.elapsed());
        if output::json() {
            let mut fields = vec![
                ("label", Val::Str(label)),
                ("outcome", Val::Str(if error.is_none() { "ok" } else { "fail" })),
                ("secs", Val::Num(took.as_secs_f64())),
            ];
            if let Some(e) = &error {
                fields.push(("error", Val::Str(e)));
            }
            return output::emit("task", &fields);
        }
        let Some(mut ticker) = self.ticker else {
            let took = format::duration(took);
            match error {
                Some(e) => eprintln!("  [FAIL] {label} ({took}) - {e}"),
                None if !output::quiet() => output::line(&format!("  [ok] {label} ({took})")),
                None => {}
            }
            return;
        };
        match error {
            None => ticker.finish(&done_line(label, took)),
            Some(e) => {
                ticker.finish(&finished_line("✕", RED, label, RED, RED, took));
                let indent = " ".repeat(2 + Spinner::default().width() + 2);
                output::line(&format!("{indent}{}", paint(mix(RED, SOFT, 0.3), &e)));
            }
        }
    }
}

struct Counts {
    done: u64,
    /// `None` when there's no telling how much work there is.
    total: Option<u64>,
    bytes: bool,
    bar: BarStyle,
}

/// A single progress line with rate and time left, for work that isn't part of a [`crate::Live`] list.
///
/// ```no_run
/// let p = tinsel::Progress::new("Downloading", 34_200_000).bytes();
/// p.inc(1_000_000);
/// p.finish();
/// ```
///
/// When the amount of work isn't known up front, use [`Progress::unknown`] (a count and a
/// rate, no bar), or grow the total as you find more with [`add_total`](Progress::add_total).
/// To track a loop, see [`ProgressIterator`]. Dropping a `Progress` finishes it.
pub struct Progress {
    counts: Arc<Mutex<Counts>>,
    label: String,
    start: Instant,
    ticker: Option<Ticker>,
    finished: bool,
}

impl Progress {
    /// Start a progress line for `total` units of work. It starts drawing straight away.
    pub fn new(label: impl Into<String>, total: u64) -> Self {
        Progress::start(label.into(), Some(total))
    }

    /// Start a progress line for an unknown amount of work: it counts and shows a rate, but
    /// has no bar or time left until [`set_total`](Progress::set_total) is called.
    pub fn unknown(label: impl Into<String>) -> Self {
        Progress::start(label.into(), None)
    }

    fn start(label: String, total: Option<u64>) -> Self {
        let counts = Arc::new(Mutex::new(Counts {
            done: 0,
            total,
            bytes: false,
            bar: BarStyle::default(),
        }));
        let start = Instant::now();
        let drawn = output::output() == output::Output::Pretty && output::is_terminal();
        let ticker = drawn.then(|| {
            let (counts, label) = (counts.clone(), label.clone());
            Ticker::start(move |t| progress_line(&label, &lock(&counts), start.elapsed(), t))
        });
        Progress {
            counts,
            label,
            start,
            ticker,
            finished: false,
        }
    }

    /// Draw this many frames a second, rather than [`fps`](crate::fps) for every display.
    pub fn fps(self, fps: u32) -> Self {
        if let Some(ticker) = &self.ticker {
            ticker.fps.store(fps.clamp(1, 240), Ordering::Relaxed);
        }
        self
    }

    /// Show amounts as bytes (`12 MB / 34 MB · 4.1 MB/s`) rather than plain counts.
    pub fn bytes(self) -> Self {
        lock(&self.counts).bytes = true;
        self
    }

    /// Choose how the bar is drawn (default: [`BarStyle::Line`]).
    ///
    /// ```no_run
    /// use tinsel::{BarStyle, Progress};
    ///
    /// let p = Progress::new("Encoding", 1_800).with_bar(BarStyle::Waveform);
    /// ```
    pub fn with_bar(self, bar: BarStyle) -> Self {
        lock(&self.counts).bar = bar;
        self
    }

    /// Record `n` more units done.
    pub fn inc(&self, n: u64) {
        let mut c = lock(&self.counts);
        c.done += n;
        if let Some(total) = c.total {
            c.done = c.done.min(total);
        }
    }

    /// Set how many units are done in total.
    pub fn set(&self, n: u64) {
        let mut c = lock(&self.counts);
        c.done = c.total.map_or(n, |total| n.min(total));
    }

    /// Say how much work there is after all, or that there's a different amount than first thought.
    pub fn set_total(&self, total: u64) {
        let mut c = lock(&self.counts);
        c.total = Some(total);
        c.done = c.done.min(total);
    }

    /// Add to the total, for work that's discovered as it goes (pages of results, nested
    /// folders). On an unknown total, this makes it known.
    pub fn add_total(&self, n: u64) {
        let mut c = lock(&self.counts);
        c.total = Some(c.total.unwrap_or(0) + n);
    }

    /// Stop drawing and leave a `●  label` line, with the time it took on the right.
    pub fn finish(mut self) {
        self.end();
    }

    fn end(&mut self) {
        if std::mem::replace(&mut self.finished, true) {
            return;
        }
        let took = self.start.elapsed();
        if let Some(t) = self.ticker.as_mut() {
            return t.finish(&done_line(&self.label, took));
        }
        if output::json() {
            let c = lock(&self.counts);
            let mut fields = vec![
                ("label", Val::Str(&self.label)),
                ("done", Val::Num(c.done as f64)),
                ("secs", Val::Num(took.as_secs_f64())),
            ];
            if let Some(total) = c.total {
                fields.push(("total", Val::Num(total as f64)));
            }
            output::emit("progress", &fields);
        } else if !output::quiet() {
            output::line(&format!("  [ok] {} ({})", self.label, format::duration(took)));
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.end();
    }
}

/// `.progress("label")` on any iterator: a [`Progress`] line that counts items as they go by
/// and finishes when the iterator runs out. Iterators that know their length get a bar.
///
/// ```no_run
/// use tinsel::ProgressIterator;
///
/// # fn index(_: &str) {}
/// let files = vec!["a.rs", "b.rs", "c.rs"];
/// for file in files.iter().progress("Indexing") {
///     index(file);
/// }
/// ```
pub trait ProgressIterator: Iterator + Sized {
    /// Wrap the iterator in a progress line labelled `label`.
    fn progress(self, label: impl Into<String>) -> WithProgress<Self>;
}

impl<I: Iterator> ProgressIterator for I {
    fn progress(self, label: impl Into<String>) -> WithProgress<Self> {
        let total = match self.size_hint() {
            (lo, Some(hi)) if lo == hi => Some(lo as u64),
            _ => None,
        };
        WithProgress {
            inner: self,
            bar: Some(Progress::start(label.into(), total)),
        }
    }
}

/// An iterator with a progress line: see [`ProgressIterator`].
pub struct WithProgress<I> {
    inner: I,
    bar: Option<Progress>,
}

impl<I> WithProgress<I> {
    /// Choose how the bar is drawn: `iter.progress("Encoding").with_bar(BarStyle::Waveform)`.
    pub fn with_bar(self, bar: BarStyle) -> Self {
        if let Some(p) = &self.bar {
            lock(&p.counts).bar = bar;
        }
        self
    }

    /// The progress line, to change its total or show amounts as bytes.
    pub fn bar(&self) -> Option<&Progress> {
        self.bar.as_ref()
    }
}

impl<I: Iterator> Iterator for WithProgress<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let item = self.inner.next();
        match &item {
            Some(_) => {
                if let Some(bar) = &self.bar {
                    bar.inc(1);
                }
            }
            // finished as soon as it runs out, not whenever the loop's iterator is dropped
            None => drop(self.bar.take()),
        }
        item
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

fn lock(counts: &Mutex<Counts>) -> std::sync::MutexGuard<'_, Counts> {
    counts.lock().unwrap_or_else(PoisonError::into_inner)
}

fn progress_line(label: &str, c: &Counts, elapsed: Duration, t: f32) -> String {
    let amount = |n: u64| {
        if c.bytes { format::bytes(n) } else { format::count(n) }
    };
    let rate = c.done as f64 / elapsed.as_secs_f64().max(0.001);
    let spinner = Spinner::default().frame(t);
    let Some(total) = c.total else {
        let mut stats = amount(c.done);
        if c.done > 0 {
            stats += &format!("  ·  {}/s", amount(rate as u64));
        }
        return format!("  {spinner}  {}  {}", shimmer(label, t), paint(SOFT, &stats));
    };
    let f = if total == 0 { 1.0 } else { c.done as f32 / total as f32 };
    let mut stats = format!("{:>3.0}%  {} / {}", f * 100.0, amount(c.done), amount(total));
    if c.done > 0 && rate > 0.0 {
        let left = Duration::from_secs_f64(total.saturating_sub(c.done) as f64 / rate);
        stats += &format!("  ·  {}/s  ·  {} left", amount(rate as u64), format::duration(left));
    }
    format!(
        "  {spinner}  {}  {}  {}",
        paint(TEXT, label),
        c.bar.draw(f, 24, t),
        paint(SOFT, &stats)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::{Output, capture, with_output, with_terminal};

    fn plain() {
        crate::set_color_mode(crate::ColorMode::None);
    }

    #[test]
    fn spinning_on_a_terminal_ends_on_a_mark() {
        plain();
        let out = capture(|| {
            with_terminal(true, || {
                assert_eq!(spin("s-ok", || std::thread::sleep(Duration::from_millis(30))), ());
                let failed: Result<(), &str> = try_spin("s-fail", || Err("it broke"));
                assert!(failed.is_err());
            })
        });
        assert!(out.contains("\x1b[?25l") && out.contains("●   s-ok"), "{out:?}");
        assert!(out.contains("✕   s-fail") && out.contains("it broke\n"), "{out:?}");
    }

    #[test]
    fn spinning_elsewhere_prints_a_line_an_event_or_nothing() {
        plain();
        let out = capture(|| {
            with_terminal(false, || {
                spin("p-ok", || ());
                let _ = try_spin("p-fail", || Err::<(), _>("nope"));
            })
        });
        assert!(out.contains("  [ok] p-ok (") && !out.contains("p-fail"), "{out:?}");
        let quiet = capture(|| with_output(Output::Quiet, || spin("q-ok", || ())));
        assert!(!quiet.contains("q-ok"));
        let json = capture(|| {
            with_output(Output::Json, || {
                spin("j-ok", || ());
                let _ = try_spin("j-fail", || Err::<(), _>("nope"));
            })
        });
        assert!(json.contains(r#""label":"j-ok","outcome":"ok""#), "{json}");
        assert!(
            json.contains(r#""label":"j-fail","outcome":"fail""#) && json.contains(r#""error":"nope""#),
            "{json}"
        );
    }

    #[test]
    fn progress_draws_on_a_terminal_and_finishes_on_a_mark() {
        plain();
        let out = capture(|| {
            with_terminal(true, || {
                let p = Progress::new("p-draw", 100)
                    .fps(240)
                    .bytes()
                    .with_bar(BarStyle::Waveform);
                p.inc(30);
                p.set(50);
                std::thread::sleep(Duration::from_millis(30));
                p.finish();
            })
        });
        assert!(out.contains("p-draw") && out.contains("●   p-draw"), "{out:?}");
    }

    #[test]
    fn progress_counts_within_its_total() {
        let p = with_terminal(false, || Progress::new("count", 10).fps(30));
        p.inc(25);
        assert_eq!(lock(&p.counts).done, 10);
        p.set(40);
        assert_eq!(lock(&p.counts).done, 10);
        p.set_total(5);
        assert_eq!(lock(&p.counts).done, 5);
        p.add_total(5);
        assert_eq!(lock(&p.counts).total, Some(10));
        capture(|| p.finish());

        let u = with_terminal(false, || Progress::unknown("unknown"));
        u.inc(7);
        u.set(9);
        assert_eq!(lock(&u.counts).done, 9);
        u.add_total(3);
        assert_eq!(lock(&u.counts).total, Some(3));
        assert!(capture(|| drop(u)).contains("[ok] unknown"));
    }

    #[test]
    fn progress_off_a_terminal_reports_once() {
        let json = capture(|| {
            with_output(Output::Json, || {
                let p = Progress::new("pj", 4);
                p.inc(2);
                p.finish();
                Progress::unknown("pu").finish();
            })
        });
        assert!(
            json.contains(r#"{"event":"progress","label":"pj","done":2,"#) && json.contains(r#""total":4}"#),
            "{json}"
        );
        assert!(json.contains(r#""label":"pu","done":0,"#), "{json}");
        let quiet = capture(|| with_output(Output::Quiet, || Progress::new("pq", 1).finish()));
        assert!(!quiet.contains("pq"));
    }

    #[test]
    fn iterators_count_as_they_go() {
        let out = capture(|| {
            with_terminal(false, || {
                let mut it = [1, 2, 3].iter().progress("known").with_bar(BarStyle::Line);
                assert_eq!(it.size_hint(), (3, Some(3)));
                assert_eq!(lock(&it.bar().unwrap().counts).total, Some(3));
                assert_eq!(it.by_ref().count(), 3);
                assert!(it.bar().is_none());
                assert!(it.with_bar(BarStyle::Line).next().is_none());
                let unknown = (0..10).filter(|n| n % 2 == 0).progress("filtered");
                assert_eq!(lock(&unknown.bar().unwrap().counts).total, None);
                assert_eq!(unknown.sum::<i32>(), 20);
            })
        });
        assert!(out.contains("[ok] known") && out.contains("[ok] filtered"), "{out:?}");
    }

    #[test]
    fn the_progress_line_shows_rate_and_time_left() {
        plain();
        let counts = |done, total| Counts {
            done,
            total,
            bytes: false,
            bar: BarStyle::Line,
        };
        let line = progress_line("work", &counts(50, Some(100)), Duration::from_secs(5), 0.0);
        assert!(
            line.contains(" 50%  50 / 100") && line.contains("10/s") && line.contains("5.0s left"),
            "{line}"
        );
        let line = progress_line("work", &counts(0, Some(0)), Duration::from_secs(1), 0.0);
        assert!(line.contains("100%  0 / 0") && !line.contains("/s"), "{line}");
        let line = progress_line("work", &counts(0, None), Duration::from_secs(1), 0.0);
        assert!(line.ends_with("  0"), "{line}");
        let bytes = Counts {
            bytes: true,
            ..counts(2048, None)
        };
        let line = progress_line("work", &bytes, Duration::from_secs(1), 0.0);
        assert!(line.contains("2.0 KB  ·  2.0 KB/s"), "{line}");
    }

    #[test]
    fn an_iterator_that_carries_on_after_running_out_isnt_counted_twice() {
        let mut calls = 0;
        let resumes = std::iter::from_fn(move || {
            calls += 1;
            (calls != 2).then_some(calls)
        });
        let out = capture(|| {
            with_terminal(false, || {
                let mut it = resumes.progress("resumes");
                assert_eq!((it.next(), it.next(), it.next()), (Some(1), None, Some(3)));
            })
        });
        assert_eq!(out.matches("[ok] resumes").count(), 1, "{out:?}");
    }
}

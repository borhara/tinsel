//! Clean terminal output for Rust CLIs, with a little sparkle.
//!
//! ```no_run
//! use tinsel::prelude::*;
//!
//! let live = Live::new("deploy", "my-app → production");
//! let build = live.step("Build package");
//! let upload = live.step("Upload bundle");
//!
//! build.start();
//! build.detail("bundling 412 files");
//! build.done();
//!
//! upload.start();
//! upload.progress(0.5, "17 / 34 MB");
//! upload.done();
//!
//! live.finish_with_card(Card::success("deployed").kv("url", "https://my-app.example.com"));
//! ```
//!
//! # What's here
//!
//! | | |
//! |---|---|
//! | **live work** | [`Live`] step lists with sub-steps, parallel steps and [`Live::log`]; a [`Region`] redrawing any block; commands run as steps ([`Step::run`], [`Step::command`]) and [`Step::retry`]; a [`Live::timeline`] of where the time went; [`spin`] / [`try_spin`] around one task; a standalone [`Progress`] bar or [`ProgressIterator`] around any loop, drawn as a line or a [`BarStyle::Waveform`]; seven [`Spinner`]s |
//! | **messages** | [`success`], [`info`], [`warn`], [`error`], [`hint`], [`counter`], [`title`], [`rule`], [`callout`], [`Report`] |
//! | **data** | [`Table`] (sorting, borders, wrapping, footers), [`Tree`], [`pairs`], [`columns`] |
//! | **charts** | [`sparkline`], [`bars`], [`meter`], [`histogram`], [`heatmap`], [`waveform`] |
//! | **code** | [`diff`], [`Code`] excerpts with syntax colouring ([`Lang`]) and carets under the problem |
//! | **documents** | [`markdown`], [`json`], [`Help`] (with an entrance), and [`Block::page`] for anything long |
//! | **inline** | [`badge`], [`link`], [`Card`], [`paint`], [`bold`] and friends |
//! | **apps** | [`app`]: full-screen dashboards and tools, with [`app::Panel`]s, [`app::List`]s, [`app::Tabs`], [`app::Plot`]s, [`app::Stat`]s and [`app::big`] text laid out in [`app::Rect`]s; [`app::snapshot`] to test them |
//! | **questions** | [`prompt`]: confirm, input (validation, suggestions, path completion), password, number, date, select and multiselect (filter as you type, headings, disabled options), a dropdown for long lists, editor, confirm by typing; `.back()` to go back a question |
//! | **helpers** | [`text`] (width-aware pad, truncate, wrap) and [`format`](mod@format) (bytes, durations, ago) |
//! | **setup** | [`install_panic_hook`], [`set_output`] for `--json` and `--quiet`, [`set_target`] to draw on stderr, [`set_accent`], [`set_theme`], [`set_color_mode`], [`set_focus`] and [`set_history`], [`timings`] |
//!
//! # How the pieces fit
//!
//! - **One-liners print.** [`success`], [`warn`] and the rest print straight away. Use a
//!   [`Tone`] when you want the line as a string instead.
//! - **Everything bigger is a [`Block`].** Tables, trees, cards, callouts, diffs: you can
//!   `.print()` them, `println!("{}")` them, or call `.plain()` for unstyled text.
//! - **Builders for anything with options.** [`Table`], [`Tree`], [`Card`], [`Report`] and
//!   [`Code`] are built up with chained methods, so call sites read like what they draw.
//! - **Questions return a [`prompt::Result`],** whose [`prompt::Error`] tells cancelling apart
//!   from having no terminal.
//!
//! # Output for programs, and quiet output
//!
//! [`set_output`] switches everything at once: [`Output::Json`] turns every message, step,
//! report and block into one JSON object per line, for tools reading your output, and
//! [`Output::Quiet`] keeps only what needs attention (warnings, errors, failures and anything
//! printed explicitly). `TINSEL_OUTPUT=json` or `quiet` does the same from outside.
//!
//! # Colour, terminals and tests
//!
//! tinsel draws in truecolor when `COLORTERM` says the terminal supports it, otherwise in
//! 256 colours. It respects `NO_COLOR`, and when stdout isn't a terminal (CI, a pipe, a log
//! file) it drops colour and animation and writes plain lines like `[ok] Build (1.2s)`.
//!
//! Everything is drawn on stdout unless [`set_target`] (or `TINSEL_TARGET=stderr`) moves it
//! to stderr, for tools whose stdout carries something else; then it's stderr that has to
//! be a terminal.
//!
//! Animations draw 60 frames a second. [`set_fps`] (or `TINSEL_FPS`) changes that for the
//! whole program, and a display can choose its own: `Live::fps`, `Progress::fps`, `App::fps`.
//!
//! [`set_color_mode`] overrides all of that, for a `--color` flag or to get predictable text
//! in tests:
//!
//! ```
//! tinsel::set_color_mode(tinsel::ColorMode::None);
//! let table = tinsel::Table::new(["app", "size"]).row(["core", "34 MB"]);
//! assert_eq!(table.lines().plain().lines().last(), Some("  core   34 MB"));
//! ```
//!
//! [`Live::headless`] gives you a display on a clock you move yourself, so you can check
//! exactly what a run looks like at each point with [`Live::snapshot`].

#![warn(missing_docs)]

pub mod app;
mod block;
mod chart;
pub mod format;
mod fps;
mod help;
mod highlight;
mod history;
mod json;
mod layout;
mod live;
mod markdown;
mod message;
mod notify;
mod output;
mod panic;
#[cfg(test)]
mod processes;
pub mod prompt;
mod region;
mod spin;
mod style;
mod table;
mod term;
pub mod text;
mod tree;
mod watch;
mod widgets;

pub use block::Block;
pub use chart::{bars, heatmap, histogram, meter, sparkline};
pub use fps::{DEFAULT_FPS, fps, set_fps};
pub use help::Help;
pub use highlight::Lang;
pub use history::set_history;
pub use json::{JsonError, json};
pub use layout::{Code, IntoLang, badge, columns, diff, link, pairs};
pub use live::{Live, RunError, Step, StepCommand};
pub use markdown::markdown;
pub use message::{Callout, Report, Tone, callout, counter, error, hint, info, rule, success, title, warn};
pub use notify::notify;
pub use output::{Output, Target, output, set_output, set_target, target};
pub use panic::install_panic_hook;
pub use region::Region;
pub use spin::{Progress, ProgressIterator, WithProgress, spin, try_spin};
pub use style::{
    Accent, ColorMode, Line, RAMP, Rgb, Theme, blink, bold, color_mode, dim, gradient, italic, mix, paint, paint_bg,
    palette, sample, set_accent, set_color_mode, set_theme, theme, underline,
};
pub use table::{Align, Border, Table};
pub use term::{CtrlC, content_width, set_ctrl_c, width};
pub use tree::Tree;
pub use watch::set_focus;
pub use widgets::{BarStyle, Card, Label, Spinner, bar, live_bar, live_waveform, pop, shimmer, waveform};

/// What [`Live`] displays have learned about how long steps take, as a table per program:
/// recent runs as a sparkline, the usual time, the last run, and whether it was unusually
/// slow or fast. Pass a name given to [`Live::remember_as`] (or a program's own name), or
/// `None` for every program with history.
///
/// ```no_run
/// tinsel::timings(Some("my-app")).print();
/// ```
///
/// ```text
///   my-app
///   step      recent        usual    last   trend
///   ─────────────────────────────────────────────────────
///   Build     ▃▃▄▃▃▄▃█      41.2s   1m12s   74% slower
///   Upload    ▅▄▅▅▄▅▅▄       8.1s    7.9s   steady
/// ```
pub fn timings(app: Option<&str>) -> Block {
    history::report(app)
}

/// The things most CLIs reach for, in one import: `use tinsel::prelude::*;`
pub mod prelude {
    pub use crate::{
        Block, Callout, Card, Code, Live, Progress, ProgressIterator, Report, Spinner, Step, Table, Tone, Tree, badge,
        callout, error, hint, info, pairs, palette, prompt, rule, spin, success, title, try_spin, warn,
    };
}

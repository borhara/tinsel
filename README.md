# tinsel

[![crates.io](https://img.shields.io/crates/v/tinsel)](https://crates.io/crates/tinsel)
[![docs.rs](https://img.shields.io/docsrs/tinsel)](https://docs.rs/tinsel)
[![License](https://img.shields.io/crates/l/tinsel)](LICENSE)
[![MSRV](https://img.shields.io/badge/rust-1.85%2B-ec7aa4)](Cargo.toml)

Clean, pink-hued terminal output for Rust CLIs, with a little sparkle. Live step lists,
spinners, tables, trees, prompts and error reports that all look like they belong
together. Zero dependencies.

```rust
use tinsel::prelude::*;

let live = Live::new("deploy", "my-app → production");
let build = live.step("Build");
let upload = live.step("Upload");

build.start();
build.done();
upload.start();
upload.progress(0.5, "6 / 12 MB");
upload.done();

live.finish_with_card(Card::success("deployed").kv("url", "https://my-app.example.com"));
```

```
  deploy  my-app → production

    ●    Build                                                        1.2s
    ●    Upload                                                       1.7s

  ╭────────────────────────────────────────────────────────────────────────╮
  │  ● deployed in 3.8s                                                    │
  │                                                                        │
  │  url       https://my-app.example.com                                  │
  ╰────────────────────────────────────────────────────────────────────────╯
```

---

## Why tinsel?

Most CLIs bolt together a spinner crate, a table crate, a prompt crate and a colour crate,
and end up looking like four different programs. tinsel is one small library with one
visual language: pink accents, quiet greys, eased animation, and a twinkle when something
finishes.

- **It learns:** steps remember how long they took, so next run a spinner becomes a real estimate: `usually 42s · 18s left`
- **It knows if you're watching:** switch windows during a long run and you get a desktop notification when it ends; come back and whatever finished meanwhile twinkles
- **It fits your terminal:** detects light and dark backgrounds and redraws the palette to suit, or in another accent colour for your brand
- **It speaks JSON too:** one switch turns every step, message and error into JSON lines for tools, or quiets everything but problems
- **Zero dependencies:** nothing in your lockfile but tinsel
- **One shape for everything:** one-liners print; anything bigger is a `Block` you can print, format, or test
- **Builders that read like the output:** `Card::success("deployed").kv("url", …)`
- **Errors you can match on:** prompts return `prompt::Error::Cancelled`, not a mystery `io::Error`
- **Behaves in CI:** no colour or animation when piped, `NO_COLOR` respected, plain `[ok] Build (1.2s)` lines in logs
- **Panics stay tidy:** a panic mid-run leaves the last frame on screen and prints a proper report, not a scrambled terminal
- **Testable:** `set_color_mode(ColorMode::None)` and `.plain()` give you exact text to assert on, and `Live::headless` runs a display on a clock you control

---

## Install

```bash
cargo add tinsel
```

---

## Quickstart

**One task with a spinner**, then a message:

```rust
let files = tinsel::spin("Counting files", || count_files());
tinsel::success(&format!("found {files} files"));
```

**A step list** for anything with stages: see the example at the top. Steps can run at the
same time from different threads, have sub-steps, and log lines above the display:

```rust
let build = live.step("Build");
let targets = [build.step("linux"), build.step("macos")];   // drawn indented, folded once they pass
build.start();
std::thread::scope(|s| {
    for t in &targets {
        s.spawn(move || { t.start(); compile(t); t.done(); });
    }
});
build.done();
live.log("signed with Developer ID");                          // printed above, kept on screen
```

Progress bars can be a thin line (the default) or a waveform that lights up as it plays
through, like an audio track: `Live::new(…).with_bar(BarStyle::Waveform)`.

**Run real commands as steps.** The latest line of output shows as the step's detail; if
it fails, its last lines stay under it:

```rust
use std::process::Command;

build.run(Command::new("cargo").args(["build", "--release"]))?;
push.command(Command::new("docker").args(["push", "my-app"]))
    .timeout(Duration::from_secs(120))
    .retries(2)
    .run()?;
fetch.retry(3, || download(url))?;      // any closure returning a Result
```

**See where the time went**, with `live.finish_with_timeline()`:

```
  Build      ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━                  1.7s
    linux    ━━━━━━━━━━━━━━━━━━━━━━━                           1.2s
    macos    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━                  1.7s
  Publish                                    ━━━━━━━━━━━━━     0.7s
             ──────────────────────────────────────────────
             0s                                          2.9s
```

**A loop with a progress bar:**

```rust
use tinsel::ProgressIterator;

for file in files.iter().progress("Indexing") {
    index(file);
}
```

**Ask questions**, and handle someone pressing Ctrl-C. Long lists filter as you type:

```rust
use tinsel::prompt;

match prompt::select("Deploy to", &["staging", "production"]) {
    Ok(i) => deploy(i),
    Err(prompt::Error::Cancelled) => std::process::exit(130),
    Err(e) => tinsel::Report::from_error("couldn't ask", &e).eprint(),
}
```

Text questions can validate, and ask again until the answer passes:

```rust
let port = prompt::Input::new("Port")
    .placeholder("8080")
    .validate(|s| s.parse::<u16>().map(|_| ()).map_err(|_| "a port is 1-65535".into()))
    .ask()?;
let replicas: u32 = prompt::number("Replicas", Some(3))?;
let launch = prompt::date("Launch on", None)?;                    // a calendar
let notes = prompt::editor("Release notes", "## What's new\n")?;   // opens $EDITOR
let region = prompt::Input::new("Region").suggestions(&regions).ask()?;  // Tab completes
let path = prompt::Input::new("Config").paths().ask()?;                  // like a shell
prompt::confirm_typing("Delete my-app?", "my-app")?;                       // type it to go on
```

**Help that looks like the rest of your CLI**, with a short entrance on a terminal: lines
fade in one after another while the name shimmers:

```rust
tinsel::Help::new("deploy", "ship an app to the cloud")
    .version(env!("CARGO_PKG_VERSION"))
    .usage("deploy [OPTIONS] <APP>")
    .command("status", "what's running where")
    .flag("--region <NAME>", "where to deploy")
    .example("deploy my-app --region us-east-1", "ship to Virginia")
    .animate();
```

**Documents:** `tinsel::markdown(changelog).page()` draws Markdown (headings, lists, tables,
code coloured by language) and pages it if it's long; `tinsel::json(body)?` indents and
colours JSON.

**Show data:**

```rust
use tinsel::{Border, Table, badge, palette};

Table::new(["service", "status", "p95"])
    .row(["api".to_string(), badge("healthy", palette::SAGE), "84ms".into()])
    .row(["auth".to_string(), badge("slow", palette::AMBER), "1.2s".into()])
    .sort_by_desc(2)           // by value: 1.2s before 84ms
    .border(Border::Rounded)
    .print();
```

**Full-screen apps:** dashboards, pickers, little toys. Write a struct, draw it in `view`,
change it in `event`; `app::run` takes over the screen and gives it back afterwards (even on
a panic). Panels shimmer when focused, lists scroll themselves, and numbers can be drawn big:

```rust
use tinsel::app::{self, App, Frame, Panel, Plot, Stat, Size::*};

struct Dash { latency: Vec<f64> }

impl App for Dash {
    fn view(&self, frame: &mut Frame) {
        let [stat, chart] = frame.area().columns_gap(1, [Fixed(20), Fill(1)]);
        frame.render(stat, Panel::new("p95").body(Stat::new("latency", "84ms")));
        frame.render(chart, Panel::new("latency").body(Plot::new(&self.latency)).focused(true));
    }
}

app::run(Dash { latency: vec![84.0, 90.0, 120.0, 340.0, 95.0] })?;   // q or Esc quits
```

Anything tinsel draws (tables, trees, cards, any `Block`) goes straight into a panel, next to
`List`, `Tabs`, `Plot`, `Stat`, `big` text, `hints` and twinkling `Sparkles`.
`app::snapshot(&app, 80, 24)` draws a frame without a terminal, for tests.

**Report an error** with its whole cause chain:

```rust
tinsel::Report::from_error("couldn't start", &err)
    .hint("create app.toml, or pass --config <path>")
    .eprint();
```

```
  ✕  couldn't start
     ╰─ couldn't read app.toml
        ╰─ No such file or directory (os error 2)
  →  create app.toml, or pass --config <path>
```

…and point at the problem:

```rust
Report::new("couldn't read app.toml")
    .cause("port must be a number")
    .code(Code::around(&config, 3, 1).path("app.toml").caret(8, 8, "not a number"))
    .eprint();
```

```
  ✕  couldn't read app.toml
     ╰─ port must be a number
       ╭─ app.toml:3
     2 │ host = "0.0.0.0"
     3 › port = "eighty"
       │        ^^^^^^^^ not a number
     4 │ workers = 4
```

**Panics**, drawn the same way, with the terminal put back first:

```rust
fn main() {
    tinsel::install_panic_hook();
    // …
}
```

**For scripts and tools**, one switch from a `--json` or `--quiet` flag:

```rust
tinsel::set_output(tinsel::Output::Json);   // or TINSEL_OUTPUT=json
```

```
{"event":"step","title":"deploy","step":"Build","path":["Build"],"state":"done","secs":1.204}
{"event":"error","title":"couldn't start","causes":["couldn't read app.toml"],"code":[],"hint":"…"}
```

---

## See it

```bash
cargo run -- showcase              # every component, then a live deploy
cargo run -- showcase table        # one section
cargo run -- showcase --accent blue  # in another accent; --theme light, --json and --quiet work too
cargo run -- demo --bar waveform     # the deploy with waveform progress bars
cargo run -- help                  # the CLI's own help, with its entrance
cargo run -- prompts               # the interactive questions
cargo run -- timings               # what Live has learned about your step durations
cargo run --example dashboard      # a live full-screen dashboard
cargo run --example deploy         # small, copyable examples: hello, deploy, parallel, status, questions, errors
```

---

## What's in it

| | |
|---|---|
| **live work** | `Live` step lists with sub-steps, parallel steps and `log` · commands as steps with timeouts and retries · `timeline` · `spin` / `try_spin` · `Progress` with rate and time left, unknown or growing totals · `.progress()` on any iterator · bars as a line or a `waveform` · seven `Spinner`s (`sparkle` twinkles, and is the default) |
| **messages** | `success` · `info` · `warn` · `error` · `hint` · `counter` · `title` · `rule` · `callout` · `Report` (with code excerpts) |
| **data** | `Table` (numbers right-align and sort by value, units and all; borders, wrapping, footers, highlighted rows) · `Tree` · `pairs` · `columns` |
| **charts** | `sparkline` · `bars` · `meter` · `histogram` · `heatmap` |
| **code** | `diff` · `Code` excerpts with syntax colouring (Rust, TOML, JSON, YAML, shell, Python, JS), a highlighted line and a caret under the problem |
| **documents** | `markdown` · `json` · `Help` with an animated entrance · `Block::page` |
| **inline** | `badge` · `link` (clickable) · `Card` · `paint`, `bold`, `dim`, `italic`, `underline`, `gradient` |
| **apps** | `app::run` full-screen apps · `Panel` · `List` · `Tabs` · `Plot` · `Stat` · `big` · `hints` · `Sparkles` · `Rect` layout with `rows` / `columns` · keys, mouse and resize events · `app::snapshot` for tests |
| **questions** | `prompt::confirm` · `input` / `Input` (validation, placeholders, line editing) · `password` · `number` · `date` · `select` · `multiselect` (filter as you type) · `editor` · suggestions and path completion · `confirm_typing` |
| **setup** | `install_panic_hook` · `set_output` (`Json`, `Quiet`) · `set_accent` · `set_theme` · `set_color_mode` · `timings` |
| **helpers** | `text::{width, pad, truncate, wrap, strip_ansi}` · `format::{bytes, duration, count, ago, plural}` |

Full reference with examples for every item: **[docs.rs/tinsel](https://docs.rs/tinsel)**.

---

## Testing what you print

Force plain output, then assert on exact text:

```rust
#[test]
fn status_table() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let table = tinsel::Table::new(["app", "size"]).row(["tube", "34 MB"]);
    assert_eq!(table.lines().plain().lines().last(), Some("  tube   34 MB"));
}
```

The colour mode is process-wide, so keep tests that need real colour codes in their own
file under `tests/` (Cargo runs each file as a separate process).

A `Live` display can run headless, on a clock that only moves when you say:

```rust
#[test]
fn deploy_shows_the_build_time() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let live = tinsel::Live::headless("deploy", "my-app");
    let build = live.step("Build");
    build.start();
    live.advance(std::time::Duration::from_millis(1200));
    build.done();
    assert!(live.snapshot().plain().ends_with("1.2s"));
}
```

---

## Things it does on its own

**Estimates from history.** `Live` steps remember their durations in `~/.cache/tinsel` (or
`$XDG_CACHE_HOME/tinsel`), one small file per program. A step with no progress of its own
gets an estimate bar from its recent median, and says `slower than usual` when it is. Only
successful steps on a terminal are recorded. `Live::remember_as("name")` picks the file,
`.no_history()` or `TINSEL_NO_HISTORY=1` turns it off. `tinsel::timings(Some("my-app"))`
shows what's been learned, and flags a step whose last run was unusually slow.

**Focus.** In terminals that report window focus (iTerm2, kitty, WezTerm, Ghostty,
Alacritty, foot, …), animation slows while you're away, a run of 10s or more that finishes
while you're away sends a notification (`.notify_after(…)`, `.no_notify()`), and returning
re-twinkles what finished. Keys typed during a run are held back from the display and handed
to the shell afterwards. `TINSEL_NO_FOCUS=1` turns all of this off. `tinsel::notify` sends a
notification yourself.

**Tall displays.** A `Live` display taller than the terminal leaves its top rows in
scrollback and keeps redrawing what fits, so long step lists don't scramble the screen.

**Theme.** Decided once from `TINSEL_THEME=light|dark`, then `COLORFGBG`, then by asking the
terminal for its background colour. Override with `tinsel::set_theme`.

**Accent.** Pink unless `TINSEL_ACCENT` (`violet`, `blue`, `teal`, `orange`, or a hue in
degrees) or `tinsel::set_accent` says otherwise. Every pink moves, including the faint tint
in the greys; red, amber and sage keep their meaning.

## Platform notes

Everything works on macOS, Linux and Windows. Prompts read single keypresses with `stty` on
Unix and the console API on Windows (Windows Terminal or a Windows 10+ console). Focus
awareness, terminal background detection and the Ctrl-C handler that restores the cursor are
Unix only; on Windows they quietly do nothing.

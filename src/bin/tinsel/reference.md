<!-- written by scripts/reference.mjs from tinsel's own source: don't edit it by hand -->

# Live work

## Live

`tinsel::Live` · struct

```rust
pub struct Live
```

A list of steps redrawn in place as they run.

Steps can run one after another or at the same time: `Step` handles are cheap to clone
and can be moved to other threads, and every active step animates on its own. Steps can
have sub-steps (`Step::step`), and `log` prints a line above the display
without disturbing it.

A step is pending (`○`) until it starts, then ends done (`●`),
with a warning (`▲`) or failed (`✕`). It can also be
skipped (`–`), cancelled (`⊘`), or left waiting
on someone (`◌`), and a finished step can start again.

Beyond spinners and timings, a `Live` display does three things on its own:

- **It learns.** Each step's duration is remembered between runs (see
  `remember_as`). Next time, a step that would only have had a spinner
  gets a progress bar and an estimate: `usually 42s · 18s left`, or `slower than usual`.
- **It notices when you look away.** In terminals that report window focus, animation
  slows while you're in another window. If a run of 10 seconds or more finishes while
  you're away, you get a desktop notification (`notify_after`), and
  when you come back, the steps that finished meanwhile twinkle once so you can see them.
- **It stays out of the way of typing.** Keys pressed during the run don't scribble over
  the display; they're handed back to the shell when it ends, where the system allows it.
  Linux 6.2 and later doesn't by default (`dev.tty.legacy_tiocsti`), so there they're lost.

It draws on stdout, or stderr after `set_target`. When that isn't a
terminal it does none of this and prints one plain line per finished
step, so CI logs stay readable. With `Output::Json` every change is
a JSON event instead, and with `Output::Quiet` only failures print.

**Methods**

```rust
pub fn new(title: impl Into<String>, subject: impl Into<String>) -> Self
```

A live display headed `title  subject`, e.g. `deploy  my-app → production`: what's
being done, and to what.

Nothing is drawn until the first `step` or `log`, so
everything chosen with the builder methods (`fps`,
`remember_as`, `spinner` and the rest) holds from
the first frame. Its clock starts here.

```rust
pub fn headless(title: impl Into<String>, subject: impl Into<String>) -> Self
```

A display that never draws, on a clock that only moves when you call
`advance`, for checking what a run looks like in tests. Take a
`snapshot` whenever you want to see it. It keeps no history and
sends no notifications.

```rust
use std::time::Duration;
use tinsel::Live;

tinsel::set_color_mode(tinsel::ColorMode::None);
let live = Live::headless("deploy", "my-app");
let build = live.step("Build");
build.start();
live.advance(Duration::from_millis(1200));
build.done();
assert!(live.snapshot().plain().contains("●   Build"));
assert!(live.snapshot().plain().ends_with("1.2s"));
```

```rust
pub fn fps(self, fps: u32) -> Self
```

Draw this many frames a second, rather than `fps` for every display.

```rust
let live = tinsel::Live::new("deploy", "my-app").fps(120);
```

```rust
pub fn no_focus(self) -> Self
```

Don't listen for focus reports: no calmer animation while you're away, no notification
when the run finishes, and stdin is left alone, for a tool that hands stdin to
something else. `set_focus` or the `TINSEL_NO_FOCUS` environment
variable does the same for every display.

```rust
let live = tinsel::Live::new("deploy", "my-app").no_focus();
```

```rust
pub fn step(&self, label: impl Into<String>) -> Step
```

Add a pending step. Declaring every step up front shows the whole plan from the start.

```rust
pub fn log(&self, text: impl AsRef<str>)
```

Print a line above the display, which carries on drawing underneath it: for output
that's worth keeping, like a warning from a compiler or a URL. Safe to call from any
thread. In plain output it's an ordinary line; as JSON, a `"log"` event.

```rust
let live = tinsel::Live::new("build", "my-app");
live.log("using cached dependencies from yesterday");
```

```rust
pub fn spinner(self, spinner: Spinner) -> Self
```

Choose how running steps animate (default: `Spinner::Sparkle`).

```rust
pub fn bar(self, bar: BarStyle) -> Self
```

Choose how progress bars and estimates are drawn (default: `BarStyle::Line`).

```rust
use tinsel::{BarStyle, Live};

let live = Live::new("render", "intro.mp4").bar(BarStyle::Waveform);
```

```rust
pub fn remember_as(mut self, name: impl Into<String>) -> Self
```

The name step timings are remembered under. Defaults to the program's own name, which
is right unless several programs share a name or one program wants separate histories.
Steps are told apart by this name, the display's title, and their label.

Timings live in `$XDG_CACHE_HOME/tinsel` (or `~/.cache/tinsel`), one small file per
name. Only runs drawn on a terminal are recorded, and only steps that succeed.
`timings` shows what's been learned.

```rust
pub fn no_history(mut self) -> Self
```

Don't remember step timings, or show estimates from earlier runs.
`set_history` or the `TINSEL_NO_HISTORY` environment variable
does the same for every display.

```rust
pub fn notify_after(mut self, after: Duration) -> Self
```

Send a desktop notification when the run finishes, if it took at least this long and
the terminal window wasn't focused at the time (default: 10 seconds).

```rust
pub fn no_notify(mut self) -> Self
```

Never send a notification when the run finishes.

```rust
pub fn elapsed(&self) -> Duration
```

How long since the display started, for a closing "done in 10.2s".

```rust
pub fn advance(&self, by: Duration)
```

Move a `headless` display's clock on. Does nothing to a display on
real time.

```rust
pub fn snapshot(&self) -> Block
```

The display as it looks right now, settled (no twinkles mid-way), with the header and
anything logged. Most useful with `headless` displays in tests.

```rust
pub fn timeline(&self) -> Block
```

When each step ran, as bars on one time axis, so you can see which steps overlapped
and where the time went.

```text
  Build                 ━━━━━━━━━━━━━━━━━━              2.3s
    linux-arm64         ━━━━━━━━━━━━━━                  1.8s
    linux-x64           ━━━━━━━━━━━━━━━━━━              2.3s
  Upload                                  ━━━━━━━━━━━━  1.6s
                        ──────────────────────────────
                        0s                          3.9s
```

```rust
pub fn finish_with_timeline(mut self)
```

Finish with a `timeline` under the steps.

```rust
pub fn finish(mut self)
```

Stop drawing and leave the final state on screen. Dropping a `Live` does the same.

```rust
pub fn finish_with_card(mut self, card: Card)
```

Finish with a summary `Card` under the steps.

```rust
use tinsel::{Card, Live};

let live = Live::new("deploy", "my-app");
let build = live.step("Build");
build.start();
build.done();
live.finish_with_card(Card::success("deployed").kv("url", "https://my-app.example.com"));
```

## RunError

`tinsel::RunError` · struct

```rust
pub struct RunError
```

Why a command run as a step didn't succeed. Shows as `cargo exited with status 101`.

**Methods**

```rust
pub fn program(&self) -> &str
```

The program that was run, as it was named: `cargo`, `./deploy.sh`.

```rust
pub fn kind(&self) -> &RunErrorKind
```

What went wrong.

```rust
pub fn code(&self) -> Option<i32>
```

The exit code, if the command ran to the end and exited with one (rather than being
stopped by a signal).

```rust
pub fn output(&self) -> &str
```

Everything the command printed before it stopped, stdout and stderr together.

```rust
pub fn status(&self) -> Option<ExitStatus>
```

How the command exited, if it ran to the end.

```rust
pub fn timed_out(&self) -> bool
```

Whether it was stopped for running past its `timeout`.

## RunErrorKind

`tinsel::RunErrorKind` · enum

```rust
pub enum RunErrorKind
```

What went wrong running a command, from `RunError::kind`.

**Variants**

```rust
Spawn(std::io::Error),
```

It couldn't be started: not installed, say, or not allowed to run.

```rust
Exit(ExitStatus),
```

It ran and failed, ending with this status.

```rust
TimedOut(Duration),
```

It was stopped for running past its `timeout`, this long.

## Step

`tinsel::Step` · struct

```rust
pub struct Step
```

A handle to one step of a `Live` display. Cheap to clone and safe to move to other threads.

**Methods**

```rust
pub fn step(&self, label: impl Into<String>) -> Step
```

Add a pending sub-step, drawn indented under this one. Once a step and all its
sub-steps have succeeded, the sub-steps fold away; if any warned or failed, they stay.

```rust
let live = tinsel::Live::new("release", "v2.1.0");
let build = live.step("Build");
let (linux, mac) = (build.step("linux"), build.step("macos"));
build.start();
for target in [&linux, &mac] {
    target.start();
    target.done();
}
build.done();
```

```rust
pub fn start(&self)
```

Mark the step as running: its spinner starts and its timer begins. If it has run
before, its usual duration is looked up so it can show an estimate.

A step that has finished, failed or been waiting can start again, like a job that's
retried: its timer starts over, and only its last run is remembered.

```rust
let live = tinsel::Live::new("deploy", "my-app");
let upload = live.step("Upload");
upload.start();
upload.fail("connection reset");
upload.start();
upload.done();
```

```rust
pub fn detail(&self, text: impl Into<String>)
```

A dim sub-line under the active step, e.g. what it's working on right now.

```rust
pub fn progress(&self, fraction: f32, text: impl Into<String>)
```

Show a progress bar under the active step; `text` sits beside the percentage. A step
that reports its own progress doesn't show an estimate. The fraction is kept between 0
and 1, and NaN (from dividing by a total of 0, say) counts as 0.

```rust
pub fn log(&self, text: impl AsRef<str>)
```

Print a line above the display, labelled with this step: see `Live::log`.

```rust
pub fn done(&self)
```

Mark the step as finished. Its mark twinkles, then settles into `●`.

```rust
pub fn warn(&self, note: impl Into<String>)
```

Mark the step as finished with something worth a look, shown as a note under it.

```rust
pub fn fail(&self, note: impl Into<String>)
```

Mark the step as failed, with the reason shown under it.

```rust
pub fn skip(&self, reason: impl Into<String>)
```

Mark the step as skipped: it didn't run and won't. The reason, if not empty, shows
under it. Skipped steps aren't trouble, so they fold away with their parent.

```rust
let live = tinsel::Live::new("check", "my-app");
live.step("Lint").skip("no source files changed");
```

```rust
pub fn cancel(&self, reason: impl Into<String>)
```

Mark the step as cancelled: stopped, or never started, because someone called it off
rather than because it failed. The reason, if not empty, shows under it.

```rust
let live = tinsel::Live::new("deploy", "my-app");
let upload = live.step("Upload");
upload.start();
upload.cancel("stopped by hand");
```

```rust
pub fn wait(&self, on: impl Into<String>)
```

Mark the step as waiting on someone or something, said under it, with how long it's
been waiting on the right. `start` it once it can go on. Waiting is
worth noticing, so it prints in quiet output too.

```rust
let live = tinsel::Live::new("deploy", "my-app → production");
let release = live.step("Release");
release.wait("waiting for approval");
// … once someone approves
release.start();
release.done();
```

```rust
pub fn try_run<T, E: std::fmt::Display>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<T, E>
```

Run `f` as this step: started if it hasn't been, then done on `Ok` or failed on `Err`,
with the error as its note.

```rust
let live = tinsel::Live::new("deploy", "my-app");
live.step("Load config").try_run(load_config)?;
```

**Errors**

Whatever `f` fails with, once the step is marked failed with it.

```rust
pub fn retry<T, E: std::fmt::Display>(&self, retries: u32, mut f: impl FnMut() -> Result<T, E>) -> Result<T, E>
```

Like `try_run`, but on `Err` tries again, up to `retries` more times,
waiting a little longer each time (0.5s, 1s, 2s, … up to 8s). The step's detail shows
which attempt it's on. Succeeding after a retry ends on a warning, so flaky steps
get noticed.

**Errors**

The last attempt's error, once every retry has failed too.

```rust
pub fn run<'c>(&self, command: impl BorrowMut<Command> + Send + Sync + 'c) -> Result<String, RunError>
```

Run a command as this step, showing its latest line of output as the step's detail.
Done if it exits successfully; otherwise failed, with its last lines of output under
it. Returns everything it printed (stdout and stderr together).

```rust
use std::process::Command;

let live = tinsel::Live::new("build", "my-app");
let output = live.step("Compile").run(Command::new("cargo").arg("build"))?;
```

The command can be handed over, or lent as `&mut Command` as above. It reads nothing
(see `StepCommand::stdin`), and its output is collected, so a lent command keeps
those settings afterwards.

For a timeout, retries or input, use `command`.

**Errors**

A `RunError` if the command can't be started or exits unsuccessfully.

```rust
pub fn command<'c>(&self, command: impl BorrowMut<Command> + Send + Sync + 'c) -> StepCommand<'_, 'c>
```

A command to run as this step, with options: see `StepCommand`.

## StepCommand

`tinsel::StepCommand` · struct

```rust
pub struct StepCommand<'a, 'c>
```

A command to run as a `Step`, from `Step::command`: set a timeout, retries or what it
reads, then `run` it.

```rust
use std::process::Command;
use std::time::Duration;

let live = tinsel::Live::new("deploy", "my-app");
live.step("Push image")
    .command(Command::new("docker").args(["push", "my-app:latest"]))
    .timeout(Duration::from_secs(120))
    .retries(2)
    .run()?;
```

**Methods**

```rust
pub fn timeout(mut self, limit: Duration) -> Self
```

Kill the command if it's still running after this long. Each retry gets the full time.

```rust
pub fn retries(mut self, n: u32) -> Self
```

Run it again, up to `n` more times, if it fails or times out, waiting a little longer
each time. Succeeding after a retry ends the step on a warning.

```rust
pub fn tail(mut self, lines: usize) -> Self
```

How many of the last lines of output to show under the step if it fails (default 12).

```rust
pub fn stdin(mut self, stdin: Stdio) -> Self
```

What the command reads as its input. Nothing unless this says otherwise, so a command
that stops to ask something fails rather than waiting unseen; `Stdio::inherit()` hands
it the terminal's input, for one that reads what's typed. Prompts written straight to
the terminal, as `sudo` and `ssh` ask for passwords, work either way.

```rust
use std::process::{Command, Stdio};

let live = tinsel::Live::new("setup", "my-app");
live.step("Install").command(Command::new("./install.sh")).stdin(Stdio::inherit()).run()?;
```

```rust
pub fn run(mut self) -> Result<String, RunError>
```

Run the command, marking the step done or failed. Returns everything the command
printed, stdout and stderr together.

**Errors**

A `RunError` if the command can't be started, exits unsuccessfully or runs past its timeout,
on its last attempt.

## Region

`tinsel::Region` · struct

```rust
pub struct Region
```

Any `Block` (a table, a tree, a few lines of status) redrawn in place at a steady rate
while work goes on, for displays a `Live` step list doesn't fit. The view
is a closure that builds the block afresh each time from whatever state it shares.

When it finishes it's cleared away, unless `keep` asks for the last frame
to stay. Lines logged while it runs print above it and stay.

```rust
use std::sync::{Arc, Mutex};
use tinsel::{Region, Table};

let done = Arc::new(Mutex::new(Vec::<String>::new()));
let shown = done.clone();
let region = Region::new(move || {
    let done = shown.lock().unwrap();
    let mut table = Table::new(["service", "status"]);
    for name in done.iter() {
        table = table.row([name.as_str(), "ready"]);
    }
    table.lines()
})
.keep();

for name in ["api", "web", "worker"] {
    // … start it
    done.lock().unwrap().push(name.to_string());
    region.log(format!("started {name}"));
}
region.finish();
```

On a terminal it redraws (on stdout, or the `Target`). Anywhere else it
prints logged lines as they come and nothing else while it runs, then the last frame once
if it's kept. With `Output::Json` logged lines are `"log"` events
and a kept last frame is one `"block"` event; with `Output::Quiet`
nothing prints.

**Methods**

```rust
pub fn new(view: impl FnMut() -> Block + Send + 'static) -> Self
```

Start a region drawing `view`, ten times a second unless `fps` says
otherwise. It starts straight away.

```rust
pub fn headless(view: impl FnMut() -> Block + Send + 'static) -> Self
```

A region that never draws, for checking what it shows in tests: take a
`snapshot` whenever you like.

```rust
use tinsel::{Block, Region};

tinsel::set_color_mode(tinsel::ColorMode::None);
let region = Region::headless(|| Block::from(vec!["  3 of 5 ready".to_string()]));
region.log("api is up");
assert_eq!(region.snapshot().plain(), "api is up\n  3 of 5 ready");
```

```rust
pub fn fps(self, fps: u32) -> Self
```

Redraw this many times a second (default 10), kept between 1 and 240. The view is
called once a frame, so a slow one wants a lower rate.

```rust
let region = tinsel::Region::new(|| tinsel::pairs(&[("queue", "12 jobs")])).fps(4);
```

```rust
pub fn keep(mut self) -> Self
```

Leave the last frame on screen when the region finishes, rather than clearing it
away. Off a terminal, the last frame is printed once at the end.

```rust
let region = tinsel::Region::new(|| tinsel::pairs(&[("status", "ready")])).keep();
```

```rust
pub fn log(&self, line: impl AsRef<str>)
```

Print a line above the region, which carries on drawing underneath it. The line is
printed as given, so style it as you like (a `Tone` line, say). Safe to
call from any thread.

```rust
let region = tinsel::Region::new(|| tinsel::pairs(&[("status", "starting")]));
region.log(tinsel::Tone::Success.line("api is up"));
```

```rust
pub fn refresh(&self)
```

Redraw now rather than at the next frame, after something changed that should show
straight away.

```rust
pub fn snapshot(&self) -> Block
```

What the region shows right now: anything logged, then the view. Most useful with
`headless` regions in tests.

```rust
pub fn finish(mut self)
```

Stop drawing: clear the region away, or leave its last frame if it's
kept. Dropping a `Region` does the same.

## Progress

`tinsel::Progress` · struct

```rust
pub struct Progress
```

A single progress line with rate and time left, for work that isn't part of a `crate::Live` list.

```rust
let p = tinsel::Progress::new("Downloading", 34_200_000).bytes();
p.inc(1_000_000);
p.finish();
```

When the amount of work isn't known up front, use `Progress::unknown` (a count and a
rate, no bar), or grow the total as you find more with `add_total`.
To track a loop, see `ProgressIterator`. Dropping a `Progress` finishes it.

**Methods**

```rust
pub fn new(label: impl Into<String>, total: u64) -> Self
```

A progress line for `total` units of work. It's drawn from the first update
(`inc`, `set` or a change of total), so everything
chosen with `bytes`, `bar` and `fps`
holds from its first frame; `set(0)` shows it before any work is done.

```rust
pub fn unknown(label: impl Into<String>) -> Self
```

Start a progress line for an unknown amount of work: it counts and shows a rate, but
has no bar or time left until `set_total` is called.

```rust
pub fn fps(mut self, fps: u32) -> Self
```

Draw this many frames a second, rather than `fps` for every display.

```rust
pub fn bytes(self) -> Self
```

Show amounts as bytes (`12 MB / 34 MB · 4.1 MB/s`) rather than plain counts.

```rust
pub fn bar(self, bar: BarStyle) -> Self
```

Choose how the bar is drawn (default: `BarStyle::Line`).

```rust
use tinsel::{BarStyle, Progress};

let p = Progress::new("Encoding", 1_800).bar(BarStyle::Waveform);
```

```rust
pub fn inc(&self, n: u64)
```

Record `n` more units done.

```rust
pub fn set(&self, n: u64)
```

Set how many units are done in total.

```rust
pub fn set_total(&self, total: u64)
```

Say how much work there is after all, or that there's a different amount than first thought.

```rust
pub fn add_total(&self, n: u64)
```

Add to the total, for work that's discovered as it goes (pages of results, nested
folders). On an unknown total, this makes it known.

```rust
pub fn finish(mut self)
```

Stop drawing and leave a `●  label` line, with the time it took on the right.

## ProgressIterator

`tinsel::ProgressIterator` · trait

```rust
pub trait ProgressIterator: Iterator + Sized
```

`.progress("label")` on any iterator: a `Progress` line that counts items as they go by
and finishes when the iterator runs out. Iterators that know their length get a bar.

```rust
use tinsel::ProgressIterator;

let files = vec!["a.rs", "b.rs", "c.rs"];
for file in files.iter().progress("Indexing") {
    index(file);
}
```

**Methods**

```rust
fn progress(self, label: impl Into<String>) -> WithProgress<Self>
```

Wrap the iterator in a progress line labelled `label`.

## WithProgress

`tinsel::WithProgress` · struct

```rust
pub struct WithProgress<I>
```

An iterator with a progress line: see `ProgressIterator`.

**Methods**

```rust
pub fn bar(self, bar: BarStyle) -> Self
```

Choose how the bar is drawn: `iter.progress("Encoding").bar(BarStyle::Waveform)`.

```rust
pub fn as_progress(&self) -> Option<&Progress>
```

The progress line behind this iterator, to change its total as more turns up.

## spin

`tinsel::spin` · fn

```rust
pub fn spin<T>(label: &str, f: impl FnOnce() -> T) -> T
```

Run `f` with a spinner beside `label`, then leave a `●  label` line with the time it took
on the right, lined up with `Live` steps.

## try_spin

`tinsel::try_spin` · fn

```rust
pub fn try_spin<T, E: Display>(label: &str, f: impl FnOnce() -> Result<T, E>) -> Result<T, E>
```

Like `spin`, but an `Err` ends on `✕` with the error under it.

**Errors**

Whatever `f` fails with, once the spinner has ended on its ✕ line.

## BarStyle

`tinsel::BarStyle` · enum

```rust
pub enum BarStyle
```

How a progress bar is drawn, for `Live::bar` and
`Progress::bar`.

**Variants**

```rust
Line,
```

A thin line with a glint running along it: `bar` and `anim::bar`. The default.

```rust
Waveform,
```

A row of bars like an audio track, lit up as it plays through: `waveform` and
`live_waveform`.

## Card

`tinsel::Card` · struct

```rust
pub struct Card
```

A rounded box with a thin border, for summing up a run: a headline, then details.

Printed, a card is as wide as what's inside it (never wider than the terminal), so a
one-line result gets a snug box; `fill` stretches it across the content
width instead, and `width` sets one. Drawn as a widget in an app, it fills
the area it's given unless it was given a width of its own or `fit`.

```rust
use tinsel::Card;

Card::success("deployed")
    .aside("in 10.0s")
    .kv("url", "https://my-app.example.com")
    .kv("commands", "14 registered")
    .print();
```

**Methods**

```rust
pub fn new() -> Self
```

An empty card. Most cards start from `Card::success` or `Card::failure` instead.

```rust
pub fn success(headline: &str) -> Self
```

A card headed `● headline` in rose.

```rust
pub fn failure(headline: &str) -> Self
```

A card headed `✕ headline` in red.

```rust
pub fn headline(mut self, line: Line) -> Self
```

Use any line as the headline. A blank line separates it from the rows below.

```rust
pub fn aside(mut self, text: &str) -> Self
```

Quieter text after the headline, like how long something took.

```rust
pub fn line(mut self, line: Line) -> Self
```

A row of any styled line.

```rust
pub fn blank(self) -> Self
```

An empty row.

```rust
pub fn kv(self, key: &str, value: &str) -> Self
```

A `key  value` row with the key muted.

```rust
pub fn kv_with(self, color: Rgb, key: &str, value: &str) -> Self
```

A `key  value` row with the value in a colour of your choosing.

```rust
pub fn width(mut self, cols: usize) -> Self
```

Columns wide, border included (default: as wide as what's inside, when printed).

```rust
pub fn title(mut self, title: impl Into<Label>) -> Self
```

Text set into the top border, at the left: `╭─ title ───╮`. Bold unless styled otherwise.

```rust
use tinsel::Card;

Card::new()
    .title("ready to ship")
    .footer("12s")
    .kv("target", "staging")
    .fit()
    .print();
```

```text
  ╭─ ready to ship ─────╮
  │  target    staging  │
  ╰─────────────── 12s ─╯
```

```rust
pub fn footer(mut self, footer: impl Into<Label>) -> Self
```

Text set into the bottom border, at the right: `╰─── 12s ─╯`. A time, a count down, a
note. Muted unless styled otherwise.

```rust
pub fn border(mut self, color: Rgb) -> Self
```

Draw the border in this colour, to say how things went at a glance.

```rust
pub fn fit(mut self) -> Self
```

Only as wide as what's inside. Printed cards are already; this is for a card drawn in
an app, which otherwise fills the area it's given.

```rust
pub fn fill(mut self) -> Self
```

As wide as the content width when printed, however little is inside: for a card that
should line up with the full-width output around it.

```rust
use tinsel::Card;

Card::success("deployed").kv("url", "https://my-app.example.com").fill().print();
```

```rust
pub fn lines(&self) -> Block
```

The card as lines, with a blank line above so it sits apart from what came before.

```rust
pub fn print(&self)
```

Print the card.

## Label

`tinsel::Label` · struct

```rust
pub struct Label(LabelText)
```

Text for a card's border, from a `&str`, a `String` or a `Line`. Plain text takes the
border's usual style; a `Line` keeps its own.

## Spinner

`tinsel::Spinner` · enum

```rust
pub enum Spinner
```

How a step looks while it's running. Every style keeps to the pink palette and
eases its brightness rather than snapping, so none of them feel mechanical.

**Variants**

```rust
Sparkle,
```

Three little stars twinkling out of step - the tinsel one.

```rust
Dots,
```

Three dots with a brightness wave rolling through them, like a typing indicator.

```rust
Comet,
```

A star gliding back and forth along a short track, leaving a fading trail.

```rust
Wave,
```

Five bars rising and falling like a live audio waveform, brighter as they peak.

```rust
Arc,
```

A single arc turning smoothly.

```rust
Orbit,
```

A gap travelling around a braille ring, its colour drifting berry → blush.

```rust
Pulse,
```

One dot slowly breathing in size and colour, flashing a star at its brightest.

**Methods**

```rust
pub const ALL: [Spinner; 7] = [
        Spinner::Sparkle,
        Spinner::Dots,
        Spinner::Comet,
        Spinner::Wave,
        Spinner::Arc,
        Spinner::Orbit,
        Spinner::Pulse,
    ]
```

Every spinner, in the order the showcase lists them.

```rust
pub fn name(self) -> &'static str
```

The spinner's lowercase name, as used by `--spinner` flags.

```rust
pub fn from_name(name: &str) -> Option<Spinner>
```

The spinner with this name, if there is one: handy for a `--spinner` flag.

```rust
pub fn width(self) -> usize
```

Columns the spinner occupies; step marks are centred in the same space so labels never shift.

```rust
pub fn frame(self, t: f64) -> Line
```

What the spinner looks like `t` seconds in. Call it every frame with the time so far.

## bar

`tinsel::bar` · fn

```rust
pub fn bar(fraction: f32, width: usize) -> Line
```

A thin line that fills berry → blush, its leading cell fading in as it fills so the line
grows smoothly and never breaks.

## waveform

`tinsel::waveform` · fn

```rust
pub fn waveform(fraction: f32, width: usize) -> Line
```

Progress as an audio waveform: played columns in berry → blush, the rest dim.

```rust
println!("{}", tinsel::waveform(0.4, 30));
```

## timings

`tinsel::timings` · fn

```rust
pub fn timings(app: Option<&str>) -> Block
```

What `Live` displays have learned about how long steps take, as a table per program:
recent runs as a sparkline, the usual time, the last run, and whether it was unusually
slow or fast. Pass a name given to `Live::remember_as` (or a program's own name), or
`None` for every program with history.

```rust
tinsel::timings(Some("my-app")).print();
```

```text
  my-app
  step      recent        usual    last   trend
  ─────────────────────────────────────────────────────
  Build     ▃▃▄▃▃▄▃█      41.2s   1m12s   74% slower
  Upload    ▅▄▅▅▄▅▅▄       8.1s    7.9s   steady
```

# Messages

## Callout

`tinsel::Callout` · enum

```rust
pub enum Callout
```

The tone of a `callout` block, which sets its colour.

**Variants**

```rust
Note,
```

Rose: worth knowing.

```rust
Tip,
```

Sage: a helpful suggestion.

```rust
Warning,
```

Amber: be careful.

```rust
Danger,
```

Red: something destructive or broken.

## Report

`tinsel::Report` · struct

```rust
pub struct Report
```

An error, what caused it (outermost first), and a hint at the fix.

```rust
use tinsel::Report;

let report = Report::new("couldn't deploy my-app")
    .cause("AccessDenied: iam:PassRole")
    .cause("the role has no trust policy for Lambda")
    .hint("attach one with `my-app role fix`");
report.eprint();
assert!(report.lines().plain().contains("╰─ AccessDenied"));
```

```text
  ✕  couldn't deploy my-app
     ╰─ AccessDenied: iam:PassRole
        ╰─ the role has no trust policy for Lambda
  →  attach one with `my-app role fix`
```

**Methods**

```rust
pub fn new(title: impl Into<String>) -> Self
```

A report headed by what went wrong.

```rust
pub fn from_error(title: impl Into<String>, err: &dyn std::error::Error) -> Self
```

A report for any error, with its whole `source()` chain as causes.

```rust
let err = std::io::Error::other("disk full");
let report = tinsel::Report::from_error("couldn't save", &err);
assert!(report.lines().plain().contains("disk full"));
```

```rust
pub fn cause(mut self, cause: impl Into<String>) -> Self
```

Add the next cause, one level deeper than the last.

```rust
pub fn code(mut self, code: crate::layout::Code) -> Self
```

Show where the problem is, as a `Code` excerpt under the causes.

```rust
use tinsel::{Code, Report};

let config = "[server]\nport = \"eighty\"\n";
Report::new("couldn't read app.toml")
    .cause("port must be a number")
    .code(Code::around(config, 2, 1).path("app.toml").caret(8, 8, "not a number"))
    .hint("use a port like 8080")
    .eprint();
```

```rust
pub fn hint(mut self, hint: impl Into<String>) -> Self
```

What to do about it.

```rust
pub fn lines(&self) -> Block
```

The report as lines.

```rust
pub fn eprint(&self)
```

Print the report to stderr, where errors belong. As JSON it's an `"error"` event
with `title`, `causes` and `hint`.

## Tone

`tinsel::Tone` · enum

```rust
pub enum Tone
```

The kind of a one-line message. The free functions (`success`, `info`, …) print
straight away; use a `Tone` when you want the line as a string instead.

```rust
use tinsel::Tone;

let line = Tone::Success.line("Deployed my-app");
assert!(tinsel::text::strip_ansi(&line).contains("Deployed my-app"));
```

**Variants**

```rust
Success,
```

`●` in rose: something finished.

```rust
Info,
```

`›` in blush: something worth knowing.

```rust
Warn,
```

`▲` in amber: something to look at.

```rust
Error,
```

`✕` in red: something went wrong.

```rust
Hint,
```

`→` in muted grey: what to do next.

**Methods**

```rust
pub fn line(self, msg: &str) -> String
```

The message as a styled line, without printing it.

```rust
pub fn print(self, msg: &str)
```

Print the message: errors to stderr, everything else to stdout (or the
`Target`). In
`Output::Quiet` only warnings and errors print.

## callout

`tinsel::callout` · fn

```rust
pub fn callout(kind: Callout, title: &str, body: &str) -> Block
```

A block set off by a coloured bar on the left, with a title and a body wrapped to fit.

```rust
use tinsel::{Callout, callout};

callout(Callout::Tip, "Faster rebuilds", "Pass --incremental to reuse the last build.").print();
```

## counter

`tinsel::counter` · fn

```rust
pub fn counter(n: usize, total: usize, msg: &str)
```

Prints `2/5  Building`, for scripts that announce steps as they go rather than drawing a
live list.

## error

`tinsel::error` · fn

```rust
pub fn error(msg: &str)
```

Prints `✕  msg` to stderr: something went wrong.

## hint

`tinsel::hint` · fn

```rust
pub fn hint(msg: &str)
```

Prints `→  msg`, quieter than the rest: what to do next.

## info

`tinsel::info` · fn

```rust
pub fn info(msg: &str)
```

Prints `›  msg`: something worth knowing.

## rule

`tinsel::rule` · fn

```rust
pub fn rule(label: &str) -> String
```

A thin divider across the content width, with an optional label in it.

```rust
let line = tinsel::rule("build");
assert!(tinsel::text::strip_ansi(&line).starts_with("  ── build ──"));
```

## success

`tinsel::success` · fn

```rust
pub fn success(msg: &str)
```

Prints `●  msg`: something finished.

## title

`tinsel::title` · fn

```rust
pub fn title(text: &str, subtitle: Option<&str>)
```

Prints a heading with an optional quieter subtitle beside it, with a blank line either side.

## warn

`tinsel::warn` · fn

```rust
pub fn warn(msg: &str)
```

Prints `▲  msg`: something to look at.

## notify

`tinsel::notify` · fn

```rust
pub fn notify(title: &str, body: &str)
```

Send a desktop notification. tinsel's `Live` does this for you when a long
run finishes while you're looking at another window; call it yourself for anything else.

Uses whatever the terminal understands (OSC 9, kitty's OSC 99, OSC 777, or Notification
Center for macOS Terminal), passed through tmux if needed, and falls back to the bell.
Does nothing when stdout (or the `Target`) isn't a terminal.

```rust
tinsel::notify("backup", "● finished in 12m04s");
```

## install_panic_hook

`tinsel::install_panic_hook` · fn

```rust
pub fn install_panic_hook()
```

Show panics as a tinsel `Report` instead of Rust's default message, and put the terminal
back first if a `Live` display or a `Region` was mid-draw: running steps are marked as
interrupted, the last frame is left on screen, and the cursor comes back. That's for a
panic that ends the program (one on the main thread, or any with `panic = "abort"`); one
on another thread, which the program may well survive, is reported above the displays and
they carry on.

Call it once, early in `main`; calling it again does nothing. A hook set before it still
runs, after the report. With `RUST_BACKTRACE` set the backtrace is printed under the
report, dimmed.

```rust
tinsel::install_panic_hook();
```

```text
  ✕  my-app panicked
     ╰─ index out of bounds: the len is 3 but the index is 7
        ╰─ at src/main.rs:42:13
  →  run with RUST_BACKTRACE=1 to see where it came from
```

# Tables, trees and charts

## Block

`tinsel::Block` · struct

```rust
pub struct Block
```

Several lines of output that travel together.

Every multi-line component in tinsel returns a `Block` (`Table`,
`Tree`, `Card`, `callout`, …), so they all
work the same way: print it, `println!` it, or take its lines and arrange them yourself.

```rust
use tinsel::{Block, pairs};

let block: Block = pairs(&[("region", "eu-west-2"), ("memory", "512 MB")]);
block.print();                    // print it
println!("{block}");              // or format it
assert_eq!(block.len(), 2);       // or look inside
```

**Methods**

```rust
pub fn new() -> Self
```

An empty block.

```rust
pub fn push(&mut self, line: impl Into<String>)
```

Add a line to the end.

```rust
pub fn extend(&mut self, other: Block)
```

Add every line of another block to the end.

```rust
pub fn lines(&self) -> &[String]
```

The lines, still styled.

```rust
pub fn into_lines(self) -> Vec<String>
```

The lines, owned.

```rust
pub fn len(&self) -> usize
```

How many lines there are.

```rust
pub fn is_empty(&self) -> bool
```

Whether there are no lines at all.

```rust
pub fn plain(&self) -> String
```

The text without any colour or styling, lines joined with `\n`. Handy in tests.

```rust
pub fn print(&self)
```

Print every line to stdout (or the `Target`). With `Output::Json` the block is
one `{"event":"block","lines":[…]}` event instead.

```rust
pub fn eprint(&self)
```

Print every line to stderr (or, as JSON, to stdout like everything else).

```rust
pub fn page(&self)
```

Print the block through a pager (`$PAGER`, or `less`) if it's taller than the
terminal, so it can be scrolled and searched; otherwise just print it. Colours are
kept (`less -R`).

```rust
tinsel::markdown(include_str!("../CHANGELOG.md")).page();
```

```rust
pub fn html(&self) -> String
```

The block as a web page that looks just like it does in the terminal, colours and all,
as `Frame::html` draws a frame. Draw the block in colour
(see `set_color_mode`) to see them.

```rust
let page = tinsel::pairs(&[("url", "https://example.com/?a=1&b=2")]).html();
assert!(page.contains("a=1&amp;b=2"));
```

```rust
pub fn html_pre(&self) -> String
```

The block as a `<pre>` element, styled inline so it can go anywhere in a page. See
`html`.

```rust
let pre = tinsel::Block::from(vec!["<b>".to_string()]).html_pre();
assert!(pre.ends_with("&lt;b&gt;</pre>"));
```

## bars

`tinsel::bars` · fn

```rust
pub fn bars(rows: &[(&str, f64)], width: usize) -> Block
```

A horizontal bar per label, scaled to the largest value, with the value at the end. A
value that isn't a number gets an empty bar and `–`, and an infinite one a full bar.

## heatmap

`tinsel::heatmap` · fn

```rust
pub fn heatmap(rows: &[(&str, &[f64])]) -> Block
```

A grid of values as shaded cells, darkest for the smallest: activity by day, load by hour.
Each row is a label and its values; all rows share one scale. A value that isn't a
finite number leaves its cell empty.

```rust
tinsel::heatmap(&[
    ("mon", &[0.0, 2.0, 5.0, 9.0, 4.0]),
    ("tue", &[1.0, 3.0, 8.0, 7.0, 2.0]),
]).print();
```

## histogram

`tinsel::histogram` · fn

```rust
pub fn histogram(values: &[f64], bins: usize, height: usize) -> Block
```

How values are spread: `bins` columns, `height` rows tall, with the range underneath.
Values that aren't finite numbers are left out.

```rust
let latencies = [12.0, 14.0, 15.0, 15.5, 16.0, 18.0, 22.0, 41.0];
tinsel::histogram(&latencies, 8, 4).print();
```

## meter

`tinsel::meter` · fn

```rust
pub fn meter(label: &str, value: f64, max: f64, width: usize) -> String
```

A labelled gauge that warms from rose to amber to red as it fills: disk, memory, quotas.
A value that isn't a number, or a `max` that isn't a finite number above zero, leaves it
empty, with `–` for the percentage.

## sparkline

`tinsel::sparkline` · fn

```rust
pub fn sparkline(values: &[f64]) -> String
```

Values as a row of rising blocks, taller ones brighter: `▁▂▄▇▅▃`. A value that isn't a
finite number (NaN, for a reading that's missing) leaves a gap.

```rust
tinsel::set_color_mode(tinsel::ColorMode::None);
assert_eq!(tinsel::sparkline(&[1.0, f64::NAN, 3.0]), "▁ █");
```

## Align

`tinsel::Align` · enum

```rust
pub enum Align
```

Which side of a column cells hug.

**Variants**

```rust
Left,
```

Text hugs the left edge: the default for words.

```rust
Right,
```

Text hugs the right edge: the default for numbers.

## Border

`tinsel::Border` · enum

```rust
pub enum Border
```

How a `Table` is framed.

**Variants**

```rust
None,
```

No frame: a dotted rule under the header and space between columns. The default.

```rust
Rounded,
```

A soft rounded frame around the whole table, with space (not lines) between columns.

```rust
Square,
```

A grid: square corners and lines between every column, for when cells need edges.

## Table

`tinsel::Table` · struct

```rust
pub struct Table
```

Rows under a header, columns sized to their widest cell. Columns of numbers right-align
on their own; cells can be styled (badges, coloured text) and still line up.

```rust
use tinsel::{Border, Table};

Table::new(["app", "status", "size"])
    .row(["core", "live", "34.2 MB"])
    .row(["queue", "building", "12.8 MB"])
    .sort_by(2)               // numerically, units and all
    .highlight(0)             // the first row, after sorting
    .footer(["2 services", "", "47.0 MB"])
    .border(Border::Rounded)
    .print();
```

**Methods**

```rust
pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Self
```

A table with these column headers.

```rust
pub fn row<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self
```

Add a row of cells. Cells can be plain or styled (a `badge`, say).
A row can have fewer cells than there are headers (the rest show `·`), but not more:
debug builds stop there and say so.

```rust
pub fn footer<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self
```

A closing row under a rule, for totals.

```rust
pub fn align(mut self, column: usize, align: Align) -> Self
```

Override the automatic alignment of one column.

```rust
pub fn max_width(mut self, column: usize, cols: usize) -> Self
```

Cut cells in a column to at most `cols` wide, ending in `…`.

```rust
pub fn wrap(mut self, column: usize, cols: usize) -> Self
```

Wrap cells in a column onto more lines once they're wider than `cols`.

```rust
pub fn border(mut self, border: Border) -> Self
```

Draw a frame around the table (default: `Border::None`).

```rust
pub fn title(mut self, title: impl Into<String>) -> Self
```

A title, set into the top border (or above the table, without one).

```rust
pub fn striped(mut self) -> Self
```

Tint every other row faintly, to help the eye along wide tables.

```rust
pub fn blank_cells(mut self) -> Self
```

Leave empty body cells blank, instead of marking each with a muted `·`.

```rust
use tinsel::Table;

tinsel::set_color_mode(tinsel::ColorMode::None);
let table = Table::new(["name", "note"]).row(["api", ""]).blank_cells();
assert_eq!(table.lines().plain(), "  name   note\n  ┈┈┈┈┈┈┈┈┈┈┈\n  api");
```

```rust
pub fn highlight(mut self, index: usize) -> Self
```

Pick out row `index` (counting rows added so far, from 0) in the accent colour, on a
soft tint, with a little `✦` beside it. Highlighted rows stay highlighted when sorted.

```rust
pub fn sort_by(mut self, column: usize) -> Self
```

Sort the rows added so far by a column, smallest first. Columns of numbers sort by
value, understanding units: `900 KB` comes before `1.2 MB`, `45s` before `2m`.

```rust
pub fn sort_by_desc(mut self, column: usize) -> Self
```

Like `sort_by`, largest first.

```rust
pub fn lines(&self) -> Block
```

The table as lines.

```rust
pub fn print(&self)
```

Print the table.

## Tree

`tinsel::Tree` · struct

```rust
pub struct Tree
```

A hierarchy drawn with thin guides. Branches are brighter than leaves.

```rust
use tinsel::Tree;
Tree::new("my-app")
    .child(Tree::new("core").leaf("src").leaf("tests"))
    .leaf("README.md")
    .print();
```

**Methods**

```rust
pub fn new(label: impl Into<String>) -> Self
```

A tree (or branch) with this label and no children yet.

```rust
pub fn child(mut self, tree: Tree) -> Self
```

Add a branch with children of its own.

```rust
pub fn leaf(self, label: impl Into<String>) -> Self
```

Add a child with no children.

```rust
pub fn lines(&self) -> Block
```

The tree as lines.

```rust
pub fn print(&self)
```

Print the tree.

# Text, code and JSON

## Help

`tinsel::Help` · struct

```rust
pub struct Help
```

A program's help: name, what it does, usage, then groups of commands, options and
examples, all aligned and wrapped to the terminal.

`animate` prints it with a short entrance (lines fade in one after
another while the name shimmers) when stdout is a terminal, and plainly otherwise. Either
way, the ✦ beside the name is left blinking, so it keeps twinkling in
terminals that support it after your program has exited.

```rust
use tinsel::Help;

Help::new("deploy", "ship an app to the cloud")
    .version("1.4.0")
    .usage("deploy [OPTIONS] <APP>")
    .command("status", "what's running where")
    .command("logs [--since 5m]", "recent logs")
    .flag("--region <NAME>", "where to deploy (default: eu-west-2)")
    .flag("-q, --quiet", "only print problems")
    .example("deploy my-app --region us-east-1", "ship to the US east region")
    .footer("docs: https://example.com/deploy")
    .print();
```

**Methods**

```rust
pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self
```

Help for `name`, which does `about`.

```rust
pub fn version(mut self, version: impl Into<String>) -> Self
```

The version, shown beside the name.

```rust
pub fn usage(mut self, line: impl Into<String>) -> Self
```

A usage line, like `deploy [OPTIONS] <APP>`. Call again for more than one.

```rust
pub fn group(mut self, title: impl Into<String>) -> Self
```

Start a new group of items under `title`. `item` adds to it.

```rust
pub fn item(mut self, left: impl Into<String>, right: impl Into<String>) -> Self
```

Add a row to the latest group: what to type on the left (`<args>` and `[optional]`
parts are dimmed), what it does on the right.

```rust
pub fn command(self, name: impl Into<String>, about: impl Into<String>) -> Self
```

Add a subcommand, to the "commands" group.

```rust
pub fn flag(self, flag: impl Into<String>, about: impl Into<String>) -> Self
```

Add a flag or option, to the "options" group.

```rust
pub fn example(self, command: impl Into<String>, about: impl Into<String>) -> Self
```

Add an example command line and what it does, to the "examples" group.

```rust
pub fn footer(mut self, text: impl Into<String>) -> Self
```

A closing line: where the docs are, how to report bugs.

```rust
pub fn lines(&self) -> Block
```

The help as lines.

```rust
pub fn print(&self)
```

Print the help.

```rust
pub fn animate(&self)
```

Print the help with a short entrance on a terminal (about a second); just print it
anywhere else.

## JsonError

`tinsel::JsonError` · struct

```rust
pub struct JsonError
```

Why `json` couldn't read its input, and where.

**Methods**

```rust
pub fn line(&self) -> usize
```

The line the problem is on, from 1.

```rust
pub fn column(&self) -> usize
```

The column, from 1, counted in characters.

```rust
pub fn message(&self) -> &str
```

What was wrong, without where: "expected a value", say.

## json

`tinsel::json` · fn

```rust
pub fn json(src: &str) -> Result<Block, JsonError>
```

JSON text indented and coloured: keys in blush, strings in sage, numbers in amber,
`true`/`false`/`null` in rose. Short arrays of plain values stay on one line. Key order
and the exact text of strings and numbers are kept.

```rust
let pretty = tinsel::json(r#"{"name":"my-app","ports":[80,443],"tls":true}"#)?;
pretty.print();
assert_eq!(pretty.plain().lines().nth(2), Some(r#"    "ports": [80, 443],"#));
```

**Errors**

A `JsonError` saying what's wrong and where, if `src` isn't valid JSON.

## Code

`tinsel::Code` · struct

```rust
pub struct Code
```

Source lines with numbers in a gutter, and optionally one line marked, with a caret under
the part that matters: the shape compilers use for errors.

```rust
use tinsel::Code;

let src = "fn main() {\n    run();\n}";
let excerpt = Code::new(src).first_line(12).highlight(13);
excerpt.print();
assert!(excerpt.lines().plain().contains("13 ›     run();"));
```

`Code::around` cuts the excerpt out of a whole file for you:

```rust
use tinsel::Code;

let config = "[server]\nport = \"eighty\"\nhost = \"0.0.0.0\"\n";
let excerpt = Code::around(config, 2, 1).path("app.toml").caret(8, 8, "expected a number");
assert!(excerpt.lines().plain().contains("^^^^^^^^ expected a number"));
```

**Methods**

```rust
pub fn new(src: impl Into<String>) -> Self
```

An excerpt of `src`, numbered from 1.

```rust
pub fn around(file: &str, line: usize, context: usize) -> Self
```

The lines around line `line` of a whole file (`context` lines either side), with
`line` marked.

```rust
pub fn first_line(mut self, n: usize) -> Self
```

Number the first line of the excerpt as `n`, as when showing lines from inside a file.

```rust
pub fn highlight(mut self, n: usize) -> Self
```

Mark line `n` (in the same numbering as the gutter).

```rust
pub fn path(mut self, path: impl Into<String>) -> Self
```

Name the file the excerpt comes from, shown above it with the marked line's number.
If its extension is one tinsel knows (`Lang`), the code is coloured as that language.

```rust
pub fn lang(mut self, lang: impl IntoLang) -> Self
```

Colour the code as a language: keywords, strings, numbers, comments and config keys.
Takes a `Lang` or a name like `"rust"` or `"toml"`; a name tinsel doesn't know
leaves the code plain.

```rust
tinsel::Code::new("let port = 8080; // default").lang("rust").print();
```

```rust
pub fn numbers(mut self, show: bool) -> Self
```

Leave out the line numbers, keeping a plain bar down the left (default: numbered).

```rust
pub fn caret(mut self, col: usize, len: usize, label: impl Into<String>) -> Self
```

Underline `len` characters of the marked line, starting at column `col` (from 1), with
a short label beside the carets.

```rust
pub fn lines(&self) -> Block
```

The excerpt as lines.

```rust
pub fn print(&self)
```

Print the excerpt.

## badge

`tinsel::badge` · fn

```rust
pub fn badge(color: Rgb, label: &str) -> String
```

A small pill: ` live ` on a soft tint of the colour, with the colour as its text.

```rust
use tinsel::{badge, palette};
println!("{} {}", badge(palette::ROSE, "live"), badge(palette::AMBER, "beta"));
```

## columns

`tinsel::columns` · fn

```rust
pub fn columns(items: &[&str]) -> Block
```

Items laid out in as many columns as fit, filling down then across, like `ls`.

```rust
tinsel::columns(&["Cargo.toml", "README.md", "src", "target"]).print();
```

## diff

`tinsel::diff` · fn

```rust
pub fn diff(old: &str, new: &str) -> Block
```

A line diff between two texts: removed lines in red, added in sage, unchanged dim.

```rust
let d = tinsel::diff("memory = 256\n", "memory = 512\n");
assert_eq!(d.plain(), "  -  memory = 256\n  +  memory = 512");
```

## link

`tinsel::link` · fn

```rust
pub fn link(label: &str, url: &str) -> String
```

A clickable link in terminals that support them (OSC 8); just the label elsewhere.

```rust
println!("docs: {}", tinsel::link("tinsel on docs.rs", "https://docs.rs/tinsel"));
```

## pairs

`tinsel::pairs` · fn

```rust
pub fn pairs(pairs: &[(&str, &str)]) -> Block
```

Keys in a muted column as wide as the longest, values beside them.

```rust
tinsel::pairs(&[("region", "eu-west-2"), ("memory", "512 MB")]).print();
```

# Command lines

## Args

`tinsel::Args` · struct

```rust
pub struct Args
```

The arguments a program was given, from `Cli::parse`. Flags, options and arguments are
looked up by name: a flag or option by its long or short name (`quiet`, `--quiet` or
`q`), an argument by the name in its spec (`app` for `<APP>`).

Asking for a name the `Cli` doesn't describe panics, so a typo shows up the first time
the code runs rather than as a flag that never seems to be set.

**Methods**

```rust
pub fn flag(&self, name: &str) -> bool
```

Whether a flag was given.

```rust
pub fn count(&self, name: &str) -> usize
```

How many times a flag was given, for `-v`, `-vv`, `-vvv`.

```rust
pub fn value(&self, name: &str) -> Option<&str>
```

An option's value, the last one given if it was given more than once.

```rust
pub fn values(&self, name: &str) -> Vec<&str>
```

Every value given for an option, in order, for options that can be given again.

```rust
pub fn value_as<T: FromStr>(&self, name: &str) -> Option<T>
```

An option's value as a number, a path or anything else that parses from text. A
value that doesn't parse is drawn as a `Report` on stderr, and the program exits
with status 2, as for any other mistake on the command line.

```rust
let cli = tinsel::Cli::new("serve", "serve files").flag("-p, --port <PORT>", "where to listen");
let args = cli.try_parse_from(["--port", "8080"])?;
assert_eq!(args.value_as::<u16>("port"), Some(8080));
```

```rust
pub fn try_value_as<T: FromStr>(&self, name: &str) -> Result<Option<T>, CliError>
```

An option's value parsed as `value_as` does, with a value that
doesn't parse as an error rather than an exit.

**Errors**

A `CliError` of kind `CliErrorKind::BadValue` when the value doesn't parse.

```rust
pub fn arg(&self, name: &str) -> Option<&str>
```

A positional argument, by the name in its spec: `app` for `<APP>`. For one that
takes the rest (`<FILES>...`), the first of them.

```rust
pub fn args(&self, name: &str) -> Vec<&str>
```

Every word a positional argument took: all of them for `<FILES>...`, one or none
for the others.

```rust
pub fn command(&self) -> Option<(&str, &Args)>
```

The subcommand that was given, by name, with its own arguments.

## Cli

`tinsel::Cli` · struct

```rust
pub struct Cli
```

A program's command line: its flags, options, arguments and subcommands, described once.
`parse` reads the arguments the program was started with, and the same
description draws `--help` as a `Help`.

Flags are written as they'd appear in help: `-q, --quiet` is a flag, and anything with a
value after it, `-r, --region <NAME>`, is an option that takes one. Arguments are `<APP>`
when they're needed, `[TAG]` when they're not, and `<FILES>...` for any number of them
(only the last can be). `-h, --help` comes free, and so does `-V, --version` once there's
a `version`.

```rust
use tinsel::Cli;

let cli = Cli::new("deploy", "ship an app to the cloud")
    .flag("-r, --region <NAME>", "where to deploy")
    .flag("-q, --quiet", "only print problems")
    .arg("<APP>", "what to ship");

let args = cli.try_parse_from(["my-app", "--region", "us-east-1"])?;
assert_eq!(args.arg("app"), Some("my-app"));
assert_eq!(args.value("region"), Some("us-east-1"));
assert!(!args.flag("quiet"));
```

A mistake is drawn as a `Report`, with the flag or command that was probably meant:

```text
  ✕  there's no --regon flag
  →  did you mean --region?
```

This covers what most programs need and stops there: there are no rules between flags,
no defaults from the environment, and subcommands go one level deep. A flag the
subcommand doesn't have but the program does is the program's, so `deploy status -q` works.

**Methods**

```rust
pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self
```

A program called `name`, which does `about`. Also how a subcommand starts, for
`command`.

```rust
pub fn version(mut self, version: impl Into<String>) -> Self
```

The version, shown in help and printed by `-V` or `--version`.

```rust
pub fn flag(mut self, spec: impl AsRef<str>, about: impl AsRef<str>) -> Self
```

A flag, `-q, --quiet`, or an option that takes a value, `--region <NAME>`. Either
name can be left out.

```rust
pub fn arg(mut self, spec: impl AsRef<str>, about: impl AsRef<str>) -> Self
```

A positional argument: `<APP>` is needed, `[TAG]` isn't, and `<FILES>...` takes the
rest. They're filled in the order they're added.

```rust
pub fn command(mut self, command: Cli) -> Self
```

A subcommand, described the same way.

```rust
use tinsel::Cli;

let cli = Cli::new("deploy", "ship an app")
    .flag("-q, --quiet", "only print problems")
    .command(Cli::new("logs", "recent logs").flag("--since <TIME>", "how far back"));

let args = cli.try_parse_from(["logs", "--since", "5m", "-q"])?;
let (name, logs) = args.command().unwrap();
assert_eq!((name, logs.value("since")), ("logs", Some("5m")));
assert!(args.flag("quiet"));
```

```rust
pub fn example(mut self, command: impl Into<String>, about: impl Into<String>) -> Self
```

An example command line for the help, and what it does.

```rust
pub fn footer(mut self, text: impl Into<String>) -> Self
```

A closing line for the help: where the docs are, how to report bugs.

```rust
pub fn help(&self) -> Help
```

The help this describes, with a usage line worked out from it.

```rust
pub fn parse(&self) -> Args
```

Read the arguments the program was started with. On `--help` or `--version` that's
printed and the program exits; on a mistake a `Report` says what's wrong on stderr
and the program exits with status 2.

```rust
pub fn try_parse_from<I, S>(&self, args: I) -> Result<Args, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
```

Read these arguments (without the program's name), saying what went wrong instead of
exiting. `--help` and `--version` come back as errors too, for `CliError::exit` to
print.

**Errors**

A `CliError` for a flag, command or argument that isn't known, an option without
its value, a needed argument left out, or `--help` and `--version`.

## CliError

`tinsel::CliError` · struct

```rust
pub struct CliError
```

Why the arguments couldn't be read, or that someone asked for `--help` or `--version`.
`exit` prints whichever it is and exits the way programs do.

**Methods**

```rust
pub fn kind(&self) -> &CliErrorKind
```

What happened.

```rust
pub fn as_report(&self) -> Option<&Report>
```

The mistake as a `Report`, for printing it your own way. `None` for `--help` and
`--version`, which aren't mistakes.

```rust
pub fn exit(&self) -> !
```

Print what this is and exit: the help (with its entrance on a terminal) or the
version on stdout with status 0, or a mistake on stderr with status 2.

## CliErrorKind

`tinsel::CliErrorKind` · enum

```rust
pub enum CliErrorKind
```

What `Cli::try_parse_from` ran into.

**Variants**

```rust
Help,
```

`-h` or `--help` was given: the help is what to print.

```rust
Version(String),
```

`-V` or `--version` was given, with the line to print.

```rust
UnknownFlag(String),
```

A flag no one described, as typed.

```rust
UnknownCommand(String),
```

A subcommand that doesn't exist, as typed.

```rust
MissingValue(String),
```

An option given without its value.

```rust
UnexpectedValue(String),
```

A flag given a value with `=`, though it doesn't take one.

```rust
MissingArg(String),
```

A needed argument that was left out, by name.

```rust
UnexpectedArg(String),
```

A word left over once every argument had one.

```rust
BadValue(String),
```

An option's value that didn't parse as what was asked for.

# Settings

## doctor

`tinsel::doctor` · fn

```rust
pub fn doctor() -> Block
```

What tinsel decided about the terminal it's drawing on, and why: the colour mode, theme,
size, output shape, target, accent, frame rate, glyphs, ambiguous width, pictures,
focus, history and notifications, each
with what decided it (`stdout isn't a terminal`, `COLORFGBG=15;0`, `set in code`). Then
a warning for each setting in the environment it can't use: a misspelt `TINSEL_` name,
or a value it doesn't understand, which would otherwise be ignored without a word.

`tinsel doctor` prints it. Print it from a hidden flag of your own too, so someone whose
colours look wrong can tell you why:

```rust
if std::env::args().any(|a| a == "--debug-terminal") {
    tinsel::doctor().print();
    return;
}
```

Finding the theme can mean asking the terminal, which takes a moment the first time.

## fps

`tinsel::fps` · fn

```rust
pub fn fps() -> u32
```

The frames a second animations draw at, as `set_fps` describes.

## set_fps

`tinsel::set_fps` · fn

```rust
pub fn set_fps(fps: u32)
```

Set how many frames a second every animation draws: spinners, progress bars, `Live`
displays and full-screen apps. 60 unless set, or `TINSEL_FPS` set; one display can still
choose its own (`Live::fps`, `Progress::fps`, `App::fps`). Animations are timed by
the clock, so this changes how smooth they look, not how fast they go. Kept between 1
and 240.

`Live`: crate::Live
`Live::fps`: crate::Live::fps
`Progress::fps`: crate::Progress::fps
`App::fps`: crate::app::App::fps

```rust
tinsel::set_fps(30);
assert_eq!(tinsel::fps(), 30);
```

## Glyphs

`tinsel::Glyphs` · enum

```rust
pub enum Glyphs
```

The characters tinsel draws its lines, blocks, dots and spinners with.

**Variants**

```rust
Unicode,
```

Unicode's: `╭─╮ ● ▁▄█ ✧`.

```rust
Ascii,
```

Plain ASCII, for a terminal whose font hasn't those: `+-+ * .:# +`.

## ambiguous_wide

`tinsel::ambiguous_wide` · fn

```rust
pub fn ambiguous_wide() -> bool
```

Whether the terminal draws Unicode's ambiguous characters two columns wide, as one set up
for Chinese, Japanese or Korean can (iTerm2's "Ambiguous characters are double-width",
GNOME Terminal's "Ambiguous-width characters: Wide"). Then tinsel measures text that has
them as wide, and draws its own symbols in ASCII. From `TINSEL_AMBIGUOUS` (`wide` or
`narrow`); narrow otherwise, as most terminals are.

## glyphs

`tinsel::glyphs` · fn

```rust
pub fn glyphs() -> Glyphs
```

The characters tinsel draws with. Decided once: from `TINSEL_GLYPHS` (`unicode` or
`ascii`) if set, and ASCII on the Linux console (`TERM=linux`), whose font has few
symbols. Unicode otherwise. Wide ambiguous characters (see `ambiguous_wide`) mean ASCII too.

## set_ambiguous_wide

`tinsel::set_ambiguous_wide` · fn

```rust
pub fn set_ambiguous_wide(wide: bool)
```

Say yourself whether the terminal draws ambiguous characters wide.

## set_glyphs

`tinsel::set_glyphs` · fn

```rust
pub fn set_glyphs(g: Glyphs)
```

Choose the characters yourself, for a `--ascii` flag say.

## with_ambiguous_wide

`tinsel::with_ambiguous_wide` · fn

```rust
pub fn with_ambiguous_wide<T>(wide: bool, f: impl FnOnce() -> T) -> T
```

Runs `f` as if the terminal drew ambiguous characters wide or not, on this thread only,
for tests.

## with_glyphs

`tinsel::with_glyphs` · fn

```rust
pub fn with_glyphs<T>(g: Glyphs, f: impl FnOnce() -> T) -> T
```

Runs `f` drawing with `g`, on this thread only, for tests.

## set_history

`tinsel::set_history` · fn

```rust
pub fn set_history(on: bool)
```

Turn remembered step timings on or off for every `Live` display: with it
off, nothing is read from or written to the cache, and steps show no estimates. Without
this, setting `TINSEL_NO_HISTORY` to anything but an empty value turns it off. `Live::no_history`
turns it off for one display.

```rust
tinsel::set_history(false);
```

## Output

`tinsel::Output` · enum

```rust
pub enum Output
```

What shape tinsel's output takes. Set it from a `--json` or `--quiet` flag with
`set_output`, or with `TINSEL_OUTPUT=json|quiet`.

```rust
tinsel::set_output(tinsel::Output::Json);
tinsel::success("deployed");    // {"event":"message","level":"success","text":"deployed"}
```

**Variants**

```rust
Pretty,
```

For people: colour and animation on a terminal, plain `[ok] Build (1.2s)` lines elsewhere.

```rust
Json,
```

For programs: one JSON object per line on stdout (or the `Target`), for every message, step, report and
block. Each has an `"event"` field saying what it is. Prompts still ask on the terminal.

```rust
Quiet,
```

Only what needs attention: warnings, errors, failed steps and anything printed
explicitly (tables, trees and other blocks). Progress, spinners and success messages
stay silent.

## Target

`tinsel::Target` · enum

```rust
pub enum Target
```

Where tinsel draws: stdout (the default) or stderr. Choose stderr when stdout belongs to
something else, like the output of a command your tool runs or data piped to another
program. Set it with `set_target` or `TINSEL_TARGET=stderr`.

```rust
tinsel::set_target(tinsel::Target::Stderr);
tinsel::success("checked");    // on stderr, coloured if stderr is a terminal
```

**Variants**

```rust
Stdout,
```

Standard output.

```rust
Stderr,
```

Standard error.

## capture

`tinsel::capture` · fn

```rust
pub fn capture(f: impl FnOnce()) -> String
```

What `f` prints through tinsel, instead of printing it: for testing your own program's
output, not just tinsel's pieces. Messages, steps, spinners, printed blocks and errors
are all caught, stdout and stderr together, in the order they were printed.

It's printed as it would be into a pipe: no animation, a `Live` display
as plain `[ok] Build (1.2s)` lines, and no colour, unless `f` runs inside
`with_color_mode`. `Output::Json` and `Output::Quiet` still
apply, so a `--json` flag can be tested too.

Prints from every thread are caught, so steps finished on worker threads are too. That
also means one capture runs at a time, and anything another test prints through tinsel
meanwhile lands in it as well: capture in every test that prints, or look for what you
expect rather than comparing the whole thing. A capture inside another, on the same
thread, gets what's printed inside it, and the outer one carries on afterwards.

```rust
fn deploy() {
    tinsel::success("deployed");
    tinsel::Table::new(["app", "size"]).row(["core", "34 MB"]).print();
}

let printed = tinsel::capture(deploy);
assert_eq!(printed, "  ●  deployed\n  app     size\n  ┈┈┈┈┈┈┈┈┈┈┈┈\n  core   34 MB\n");
```

## output

`tinsel::output` · fn

```rust
pub fn output() -> Output
```

The output shape in use. Decided once from `TINSEL_OUTPUT`, unless `set_output` has chosen one.

## set_output

`tinsel::set_output` · fn

```rust
pub fn set_output(out: Output)
```

Choose the output shape, for a `--json` or `--quiet` flag.

## set_target

`tinsel::set_target` · fn

```rust
pub fn set_target(target: Target)
```

Send everything tinsel draws to stdout or stderr: `Live` displays,
`Region`s, spinners and `Progress` bars, messages,
printed `Block`s and JSON events. Whether to draw in colour and animate
is then decided by whether that stream is a terminal. Errors and
`Report`s go to stderr either way, and prompts still ask on the terminal.

Call it early, before anything is drawn.

```rust
tinsel::set_target(tinsel::Target::Stderr);
tinsel::spin("Checking", || std::thread::sleep(std::time::Duration::from_millis(10)));
```

## target

`tinsel::target` · fn

```rust
pub fn target() -> Target
```

Where tinsel draws. Decided once from `TINSEL_TARGET` (`stderr` or `stdout`), unless
`set_target` has chosen.

## with_output

`tinsel::with_output` · fn

```rust
pub fn with_output<T>(out: Output, f: impl FnOnce() -> T) -> T
```

Run `f` with an output shape on this thread alone, leaving the rest of the program as it
was: for a library that wants quiet output from one call, or a test that wants JSON
without turning it on for the tests beside it. Displays started inside keep the shape
they started with; anything drawn on other threads meanwhile uses the program's.

```rust
use tinsel::{Output, capture, with_output};

let printed = capture(|| with_output(Output::Quiet, || tinsel::success("deployed")));
assert_eq!(printed, "");
```

## with_target

`tinsel::with_target` · fn

```rust
pub fn with_target<T>(target: Target, f: impl FnOnce() -> T) -> T
```

Run `f` drawing on `target` on this thread alone, as `with_output` does for the output
shape: messages and blocks printed inside go there, and the colour mode follows that
stream. Displays that draw from a thread of their own (`Live`,
`Region`, spinners and `Progress` bars) draw where
the program is set to, so set that with `set_target` for them.

```rust
use tinsel::{Target, with_target};

with_target(Target::Stderr, || tinsel::info("progress notes go to stderr"));
```

## terminal_width

`tinsel::terminal_width` · fn

```rust
pub fn width() -> usize
```

Columns in the terminal tinsel draws on (stdout, or the `Target`), falling
back to `$COLUMNS`, then 80. For the width of a piece of text, see
`text::width`.

## CtrlC

`tinsel::CtrlC` · enum

```rust
pub enum CtrlC
```

What Ctrl-C does while tinsel draws something live: a spinner, progress bar, steps, a
`Region` or the help pager. Set with `set_ctrl_c`.

On Unix, a Ctrl-C handler the program set before the drawing started (with the `ctrlc`
crate or `tokio::signal`, say) is left in place, whatever this says, as is a Ctrl-C the
program ignores; tinsel only steps in where Ctrl-C would otherwise end the program.

**Variants**

```rust
Exit,
```

Put the terminal back and end the program as Ctrl-C would have: on Unix by the signal
itself, so a shell sees it was interrupted (status 130) and stops a loop running it,
and on Windows with exit code 130. The default.

```rust
ExitWith(String),
```

The same, printing this line on the way out, so the program can say what was stopped.

```rust
Leave,
```

Leave Ctrl-C to the program's own handler, so it can stop its work its own way. The
terminal is put back when the drawing ends, as usual.

## set_ctrl_c

`tinsel::set_ctrl_c` · fn

```rust
pub fn set_ctrl_c(how: CtrlC)
```

Choose what Ctrl-C does while tinsel draws something live. Call it early in `main`.

```rust
tinsel::set_ctrl_c(tinsel::CtrlC::ExitWith("stopped: nothing was changed".into()));
```

## set_focus

`tinsel::set_focus` · fn

```rust
pub fn set_focus(on: bool)
```

Turn focus awareness on or off for every `Live` display: calmer animation
while you're in another window, a notification when a long run finishes unseen, and a
twinkle for what finished when you come back. Reading focus reports means reading stdin
while a display runs, so turn it off in tests, or in a tool that hands stdin to something
else. Without this, setting `TINSEL_NO_FOCUS` to anything but an empty value turns it off. `Live::no_focus`
turns it off for one display.

```rust
tinsel::set_focus(false);
```

# tinsel::anim

The pieces tinsel animates with, for drawing your own: a light passing over a label, the
twinkle a finished step shows, and bars still on the move. Each takes the seconds since
the animation started, so it can be drawn at any moment, as often as you like.

For values that move, a `Glide` eases from one to another, and a `Glider` glides
to wherever it's told to be next, asking an app's frames for more until it
lands.

```rust
use tinsel::anim;

let t = 1.5;
println!("{} uploading {}", tinsel::Spinner::Sparkle.frame(t), anim::bar(0.4, 20, t));
println!("{}", anim::shimmer("thinking", t));
```

## bar

`tinsel::anim::bar` · fn

```rust
pub fn live_bar(fraction: f32, width: usize, t: f64) -> Line
```

`tinsel::bar` with a soft glint running along the filled part, for bars
that are still moving.

## waveform

`tinsel::anim::waveform` · fn

```rust
pub fn live_waveform(fraction: f32, width: usize, t: f64) -> Line
```

`tinsel::waveform` still playing: the bars just behind the playhead bounce, and the playhead
itself glows.

## pop

`tinsel::anim::pop` · fn

```rust
pub fn pop(age: f64) -> Option<Line>
```

The mark a step shows in the moment after it finishes: a quick twinkle that settles into
`●`. `None` once settled.

## shimmer

`tinsel::anim::shimmer` · fn

```rust
pub fn shimmer(label: &str, t: f64) -> Line
```

A soft light passing slowly across the label, with a little glitter: letters near the
light now and then catch it and flash.

## Glide

`tinsel::anim::Glide` · struct

```rust
pub struct Glide
```

An eased move from one value to another, starting quickly and settling gently into
place: a scroll position, a column sliding over, a marker moving along a bar. Ask it where
the value is at any moment with `at`.

```rust
use tinsel::anim::Glide;

let glide = Glide::new(0.0, 10.0, 1.0, 0.5);
assert_eq!(glide.at(0.0), 0.0);
assert!(glide.at(1.25) > 5.0, "more than halfway by half time");
assert_eq!(glide.at(1.5), 10.0);
assert_eq!(glide.end(), 1.5);
```

**Methods**

```rust
pub fn new(from: f64, to: f64, start: f64, seconds: f64) -> Self
```

A glide from `from` to `to`, starting at `start` seconds and taking `seconds`.

```rust
pub fn at(&self, t: f64) -> f64
```

Where the value is at `t` seconds: `from` before it starts, `to` once it's over.

```rust
pub fn end(&self) -> f64
```

When it's over, in seconds.

```rust
pub fn moving(&self, t: f64) -> bool
```

Whether it's still on its way at `t` seconds.

```rust
pub fn from(&self) -> f64
```

Where it started from.

```rust
pub fn to(&self) -> f64
```

Where it's going.

```rust
pub fn towards(self, to: f64) -> Self
```

The same glide, on its way somewhere else instead: for a target that moves while
it's gliding, like a column sliding to wherever the text now starts.

```rust
use tinsel::anim::Glide;

let glide = Glide::new(0.0, 10.0, 0.0, 1.0).towards(20.0);
assert_eq!(glide.at(1.0), 20.0);
```

## Glider

`tinsel::anim::Glider` · struct

```rust
pub struct Glider
```

A value that glides to wherever it's told to be, rather than jumping: a list scrolling to
its new top, a marker following a cursor. Tell it where the value should be each frame
with `follow`; when that changes, it glides there from wherever it was
showing, even partway through another glide, and asks for frames until it lands.

It keeps what it last saw inside, so it can be kept in an app and followed from
`App::view`, which only borrows the app. A frame at time 0, as in
`snapshot` and most tests, always shows where it lands.

```rust
use tinsel::anim::Glider;
use tinsel::app::Frame;

let top = Glider::new().snap(2.0);
let mut frame = Frame::new(10, 10).at(1.0);
assert_eq!(top.follow(0.0, 10.0, &mut frame), 0.0);

// the list scrolls down a page: on its way there, and asking for frames until it lands
let mut frame = Frame::new(10, 10).at(1.05);
let shown = top.follow(10.0, 10.0, &mut frame);
assert!(shown > 0.0 && shown < 10.0 && frame.wants_redraw());

let mut frame = Frame::new(10, 10).at(2.0);
assert_eq!(top.follow(10.0, 10.0, &mut frame), 10.0);
assert!(!frame.wants_redraw());
```

**Methods**

```rust
pub fn new() -> Self
```

A glider with nothing seen yet, which glides any move, in 0.12 seconds for a short one
up to 0.18 for one as far as the span.

```rust
pub fn snap(mut self, distance: f64) -> Self
```

Jump straight to a new value no further away than `distance`, rather than gliding:
so holding a key down to scroll a line at a time never lags behind.

```rust
pub fn duration(mut self, short: f64, long: f64) -> Self
```

How long a glide takes: `short` seconds for the smallest move, up to `long` for one
as far as the span or further.

```rust
pub fn step(&self, to: f64, span: f64, t: f64) -> f64
```

Where the value shows at `t` seconds, now that it should be `to`. `span` is how far
counts as a long way, such as the rows a page shows: a glide over the whole span takes
the longest, and when the span itself changes (the screen was resized) the value
jumps instead.

```rust
use tinsel::anim::Glider;

let g = Glider::new();
assert_eq!(g.step(5.0, 20.0, 1.0), 5.0);
assert!(g.step(25.0, 20.0, 1.0) < 25.0);
assert!(g.moving(1.1));
assert_eq!(g.step(25.0, 20.0, 2.0), 25.0);
```

```rust
pub fn follow(&self, to: f64, span: f64, frame: &mut Frame) -> f64
```

Where the value shows in `frame`, now that it should be `to`, as `step`
says, asking the frame for another while it moves. At time 0 it shows where it
lands.

```rust
pub fn jump(&self, to: f64, span: f64) -> f64
```

Show `to` straight away, and glide from there next time it changes: for when gliding
is turned off.

```rust
pub fn reset(&self)
```

Forget what it's seen, so the next value shows straight away: for a list showing
something else entirely.

```rust
pub fn glide(&self) -> Option<Glide>
```

The glide it's on, if any, finished or not.

```rust
pub fn moving(&self, t: f64) -> bool
```

Whether it's still gliding at `t` seconds.

# tinsel::app

Full-screen terminal apps: dashboards, pickers, little toys, in the same pink as
everything else.

An app is a struct with your state, and two methods: `view` draws the state
into a `Frame`, and `event` changes it when a key is pressed, the mouse
moves or time passes. `run` takes over the screen, calls them in turn, and puts the
terminal back when you're done (or if anything panics).

```rust
use tinsel::app::{self, App, Event, Flow, Frame, Key, Keymap, List, Panel, Size::*, hints};

struct Picker {
    flavours: Vec<&'static str>,
    selected: usize,
}

impl App for Picker {
    fn view(&self, frame: &mut Frame) {
        let [main, footer] = frame.area().rows([Fill(1), Fixed(1)]);
        let list = List::new(self.flavours.clone()).selected(self.selected);
        frame.render(main, Panel::new("ice cream").body(list).focused(true));
        frame.render(footer.shrink(1, 0), hints(&[("↑↓", "choose"), ("enter", "done")]));
    }

    fn event(&mut self, event: &Event) -> Flow {
        app::navigate(&mut self.selected, self.flavours.len(), event, Keymap::Vim);
        match event {
            Event::Key(Key::Enter | Key::Esc | Key::Char('q')) => Flow::Quit,
            _ => Flow::Continue,
        }
    }
}

fn main() -> std::io::Result<()> {
    let picker = app::run(Picker { flavours: vec!["strawberry", "pistachio", "mint"], selected: 0 })?;
    println!("one scoop of {}", picker.flavours[picker.selected]);
    Ok(())
}
```

**Laying things out**

`Frame::area` is the whole screen as a `Rect`; `Rect::rows` and `Rect::columns`
cut it up by `Size`, and `Frame::render` draws a `Widget` into a piece. Everything
tinsel prints (`Table`, `Tree`, `Card`,
any `Block`) is a widget already; on top of those there's `Panel`, `List`,
`Grid` (a table with a cursor), `ScrollView` (lines to scroll through, or follow as they
grow), `TextInput` (a box to type into) and `TextArea` (several lines of it), `Tabs`, `Plot`, `Stat`, `Loading` (a spinner, and a bar once there's progress), `big` text, `hints`
for the footer and `Sparkles` for the background.

**Time**

`Frame::time` is the seconds since the app started, for anything animated, and
`time` the same clock read in `App::event`. Both are `f64`, so they stay precise
however long an app runs. Whatever animates from a moment is told that moment, rather
than reading the clock itself: `Modal::opened`, `Toasts::push` and
`Toasts::showing` all take it, so they can be drawn at any time in tests.

**Drawing only when needed**

An app is drawn when something happens: a key, a click, a paste, a resize, a `Waker`
waking it, or the terminal gaining or losing focus. Between those it sleeps, and costs
nothing, unless the last frame asked to be drawn again:

- `Frame::animate` while something moves, for the next frame at the app's full
  `fps`;
- `Frame::animate_at` for ambient animation (stars twinkling, a light running round a
  border) at a slower rate, which rests while the terminal is in the background and is
  timed by `Frame::ambient_time`, a clock that rests with it;
- `Frame::redraw_in` and `Frame::redraw_at` for something that changes at a known
  moment, like a clock on the screen at the turn of the minute.

Every frame asks afresh, so whatever stops asking stops costing. tinsel's own animated
widgets (`Panel` when focused, `Toasts`, `Modal::opened`, `Loading`,
`Sparkles`, a `Spinner` and `Grid` while it loads) ask for their
own frames. When a frame that was asked for comes due, the app is sent an `Event::Tick`
first, for polling whatever a dashboard shows. Work done on other threads can wake the
app the moment it's ready with a `Waker`, so it needn't poll at all; a `Job` runs
work on a thread of its own and wakes the app with its result.

**Typing**

The terminal's cursor is hidden while an app runs. A frame that wants it, to show where
typing goes, puts it somewhere with `Frame::set_cursor`, in a `CursorShape`. An app
that shouldn't end on Ctrl-C says so with `App::ctrl_c_quits`.

**Leaving for a moment**

`suspend` hands the terminal back while something else uses it (a sign-in command, an
editor, a shell), then takes it again and redraws.

**Tests**

`snapshot` draws one frame of an app at any size, without a terminal, so you can check
exactly what it shows. When stdout isn't a terminal, `run` prints that one frame and
returns.

## Edit

`tinsel::app::Edit` · enum

```rust
pub enum Edit
```

What a key or paste did to a `TextInput`, from `TextInput::edit`.

**Variants**

```rust
Changed,
```

The text changed.

```rust
Moved,
```

Only the cursor moved, or the key was one the box uses that had nothing to do
(Backspace at the start).

```rust
Submitted,
```

Enter was pressed: the text is ready.

```rust
Cancelled,
```

Esc was pressed: whatever it's for is off.

```rust
Ignored,
```

Not something the box uses, or it hasn't focus: the app's to handle.

## History

`tinsel::app::History` · struct

```rust
pub struct History
```

Lines entered before, to go back to with ↑ and ↓ in a `TextInput`: a command line's
history. A line entered again moves to the newest, blank lines aren't kept, and the
oldest go once there are more than its `limit`.

```rust
use tinsel::app::History;

let mut history = History::from(vec!["ls".to_string(), "git status".into()]);
history.push("ls");
assert_eq!(history.entries(), ["git status", "ls"]);
```

**Methods**

```rust
pub fn new() -> Self
```

No lines yet, and no limit.

```rust
pub fn limit(mut self, lines: usize) -> Self
```

Keep only the newest `lines`.

```rust
pub fn push(&mut self, line: &str)
```

Remember a line entered, as the newest.

```rust
pub fn entries(&self) -> &[String]
```

Every line, oldest first.

```rust
pub fn back(&mut self) -> Option<&str>
```

One line further back, or `None` with nothing to go back to: the newest at first,
then older ones, stopping at the oldest.

```rust
pub fn forward(&mut self) -> Option<&str>
```

One line nearer now, or `None` once past the newest, back where it started.

```rust
pub fn is_browsing(&self) -> bool
```

Whether it's partway back through its lines.

```rust
pub fn reset(&mut self)
```

Stop going back through its lines, so the next `back` starts from
the newest again.

## TextInput

`tinsel::app::TextInput` · struct

```rust
pub struct TextInput
```

A box to type into: a filter, a search, a name. Keep one in your app, pass it every
event while it has focus, and read what's typed with `value`.

```rust
use tinsel::app::{Event, Frame, TextInput, Key};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut filter = TextInput::new().prompt("/").placeholder("filter by name");
let mut frame = Frame::new(20, 1);
frame.render(frame.area(), &filter);
assert_eq!(frame.snapshot().plain(), "/ filter by name");

filter.focus(true);
for c in "api".chars() {
    filter.handle(&Event::Key(Key::Char(c)));
}
assert_eq!(filter.value(), "api");
```

**Methods**

```rust
pub fn new() -> Self
```

An empty box, without focus.

```rust
pub fn word(mut self, is_word: fn(char) -> bool) -> Self
```

Which characters make up a word, for Ctrl-W and moving a word at a time: anything
but a space unless changed. Deleting a word back takes the spaces before the cursor,
then a run of word characters, or else a run of the others.

```rust
use tinsel::app::{Event, Key, TextInput};

let mut path = TextInput::new().word(|c| c.is_alphanumeric());
path.focus(true);
path.set_value("src/app/main");
path.handle(&Event::Key(Key::Ctrl('w')));
assert_eq!(path.value(), "src/app/");
```

```rust
pub fn with_history(mut self, history: History) -> Self
```

Lines entered before, to go back through with ↑ and ↓. Enter adds the line to it.
Going forward past the newest brings back what was being typed.

```rust
pub fn history(&self) -> Option<&History>
```

The lines entered before, if it has a history, with
those entered since: to keep for next time.

```rust
pub fn terminal_cursor(mut self, on: bool) -> Self
```

Show the terminal's own cursor, a bar, where typing goes, rather than lighting up
the character there: it blinks if the terminal's does, and sits where screen readers
look.

```rust
pub fn prompt(mut self, prompt: impl Into<String>) -> Self
```

What sits before the text: `›` unless set. `/` suits a filter.

```rust
pub fn placeholder(mut self, text: impl Into<String>) -> Self
```

Quiet text shown while the box is empty, saying what goes in it.

```rust
pub fn value(&self) -> String
```

What's been typed.

```rust
pub fn set_value(&mut self, value: &str)
```

Replace what's typed, with the cursor at the end.

```rust
pub fn cursor(&self) -> usize
```

Where the cursor is, in characters from the start: 0 before the first, the length
after the last.

```rust
pub fn set_cursor(&mut self, at: usize)
```

Put the cursor before character `at` (counted in characters), or at the end if
there aren't that many.

```rust
use tinsel::app::TextInput;

let mut input = TextInput::new();
input.set_value("helo");
input.set_cursor(3);
input.insert_str("l");
assert_eq!((input.value().as_str(), input.cursor()), ("hello", 4));
```

```rust
pub fn insert_str(&mut self, text: &str)
```

Put text in at the cursor, as if typed, focused or not: line breaks and tabs become
spaces and other control characters are left out, so it stays one line.

```rust
pub fn clear(&mut self)
```

Empty the box.

```rust
pub fn focus(&mut self, focused: bool)
```

Whether this is where keys go. Only a focused box takes keys or shows its cursor.

```rust
pub fn is_focused(&self) -> bool
```

Whether the box has focus.

```rust
pub fn handle(&mut self, event: &Event) -> bool
```

Type, delete or move the cursor, if the box has focus and the event is a key it uses:
characters, Backspace and Delete, ←→, Home and End (or Ctrl-A and Ctrl-E), Ctrl-U
and Ctrl-K to clear before or after the cursor, Ctrl-W or Alt-Backspace to delete a
word, and Alt or Ctrl with ←→ to move by words. Pasted text goes in at the cursor, on one line: its line breaks and tabs become spaces.
Returns whether it used the event, so Enter, Esc and Tab are left for the app.

```rust
use tinsel::app::{Event, TextInput, Key};

let mut name = TextInput::new();
name.focus(true);
name.set_value("my-ap");
name.handle(&Event::Key(Key::Char('p')));
name.handle(&Event::Key(Key::Home));
name.handle(&Event::Key(Key::Delete));
assert_eq!(name.value(), "y-app");
assert!(!name.handle(&Event::Key(Key::Enter)));
```

```rust
pub fn edit(&mut self, event: &Event) -> Edit
```

Like `handle`, saying what happened: whether the text changed
(to filter again), only the cursor moved, Enter or Esc was pressed, or the event is
the app's. With a history, ↑ and ↓ go back through it,
and Enter adds the line to it.

```rust
use tinsel::app::{Edit, Event, Key, TextInput};

let mut search = TextInput::new();
search.focus(true);
assert_eq!(search.edit(&Event::Key(Key::Char('a'))), Edit::Changed);
assert_eq!(search.edit(&Event::Key(Key::Left)), Edit::Moved);
assert_eq!(search.edit(&Event::Key(Key::Enter)), Edit::Submitted);
assert_eq!(search.edit(&Event::Key(Key::Tab)), Edit::Ignored);
```

```rust
pub fn cursor_cell(&self, area: Rect) -> (usize, usize)
```

The column the cursor is in when the box is drawn at `area`, and its row: where
typing goes on the screen.

```rust
use tinsel::app::{Rect, TextInput};

let mut input = TextInput::new();
input.set_value("abc");
assert_eq!(input.cursor_cell(Rect::new(4, 2, 20, 1)), (9, 2));
```

## CursorShape

`tinsel::app::CursorShape` · enum

```rust
pub enum CursorShape
```

How the terminal's own cursor looks, when a frame shows it with
`set_cursor`.

**Variants**

```rust
Block,
```

A solid block over the cell, for moving around.

```rust
Bar,
```

A thin bar before the cell, for typing.

```rust
Underline,
```

A line under the cell, for replacing what's there.

## Frame

`tinsel::app::Frame` · struct

```rust
pub struct Frame
```

What one frame of an `App` looks like. `App::view`
is handed an empty one and fills it with `render`.

```rust
use tinsel::app::{Frame, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(20, 3);
frame.render(frame.area(), Panel::new("hi").body("hello"));
assert_eq!(frame.snapshot().plain(), "╭─ hi ─────────────╮\n│ hello            │\n╰──────────────────╯");
```

**Methods**

```rust
pub fn new(width: usize, height: usize) -> Self
```

An empty frame `width` columns by `height` rows. Apps are given theirs; this is for
tests and for drawing widgets on their own.

```rust
pub fn image(&mut self, area: Rect, image: &crate::image::Image)
```

Shows `image` in `area`, as big as fits without stretching it or making it bigger than
it is, in the middle, if the terminal can show it at full resolution; see
`image::support`. It sits above the text, so nothing should
be drawn under it that's meant to be seen.

```rust
pub fn images(&self) -> &[(Rect, crate::image::Image)]
```

The images this frame shows, and the areas they were given.

```rust
pub fn at(mut self, seconds: f64) -> Self
```

The frame as it is `seconds` after the app started, for testing anything animated.

```rust
use tinsel::app::{Frame, Panel};

let draw = |t| {
    let mut frame = Frame::new(12, 3).at(t);
    frame.render(frame.area(), Panel::new("hi").focused(true));
    frame.snapshot()
};
assert_eq!(draw(0.0).plain(), draw(1.0).plain());   // the same text…
```

```rust
pub fn ambient_at(mut self, seconds: f64) -> Self
```

The frame with its ambient clock reading `seconds`, whatever
`at` set: for testing how ambient animation looks after time in the
background, or holding it still while checking something else move.

```rust
use tinsel::app::Frame;

let frame = Frame::new(4, 1).at(9.0).ambient_at(0.0);
assert_eq!((frame.time(), frame.ambient_time()), (9.0, 0.0));
```

```rust
pub fn area(&self) -> Rect
```

The whole screen.

```rust
pub fn time(&self) -> f64
```

Seconds since the app started, for anything animated: pass it to
`Spinner::frame`, `anim::shimmer` and friends.

```rust
pub fn ambient_time(&self) -> f64
```

Seconds on the ambient clock, for animation that only decorates (twinkling stars, a
light running round a border): the app's clock, standing still while the terminal is
in the background. Ambient animation pauses then, as `animate_at`
says, and this clock lets it carry on from where it was when focus comes back rather
than jumping ahead. The same as `time` until focus first goes.

```rust
use tinsel::app::Frame;

assert_eq!(Frame::new(4, 1).at(2.5).ambient_time(), 2.5);
```

```rust
pub fn animate(&mut self)
```

Ask for the next frame as soon as the app's `fps` allows, because
something is moving: a glide, a scroll, a dialog rising into place. Ask in every frame
for as long as it moves; once a frame stops asking, the app waits for the next key.
Motion carries on while the terminal is in the background, so whatever is moving can
finish.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(4, 1);
assert!(!frame.wants_redraw());
frame.animate();
assert!(frame.wants_redraw());
```

```rust
pub fn animate_at(&mut self, fps: u32)
```

Ask for another frame `fps` times a second, for ambient animation: something that
drifts or twinkles while nothing else happens, which needn't be as smooth as motion.
It pauses while the terminal reports it's in the background
(`Event::Focus`) and carries on when focus comes back; time it
with `ambient_time` so it doesn't jump then. The fastest rate
asked for in a frame wins, and it's never faster than the app's
`fps`.

```rust
pub fn redraw_in(&mut self, seconds: f64)
```

Ask to be drawn again `seconds` from this frame's `time`, whether
anything happens or not: for something that changes on its own, like a countdown.
Unlike `animate_at`, it comes even while the terminal is in the
background. The earliest moment asked for in a frame wins.

```rust
pub fn redraw_at(&mut self, seconds: f64)
```

Ask to be drawn again at `seconds` on the app's clock (the clock of
`time`), whether anything happens or not: a clock on the screen asks
for the start of the next minute. Like `redraw_in`, it comes even
while the terminal is in the background.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(5, 1).at(61.5);
// the next whole minute
frame.redraw_at((frame.time() / 60.0).floor() * 60.0 + 60.0);
assert!(frame.wants_redraw());
```

```rust
pub fn wants_redraw(&self) -> bool
```

Whether anything drawn in this frame asked for another one, with
`animate`, `animate_at`,
`redraw_in` or `redraw_at`: for tests that
check an app rests when nothing changes.

```rust
pub fn render(&mut self, area: Rect, widget: impl Widget)
```

Draw a widget into part of the screen. Anything that doesn't fit is cut off.

```rust
pub fn print(&mut self, area: Rect, row: usize, text: &str) -> usize
```

Write one line of styled text into row `row` of `area`, cut off at its right edge.
Returns how many columns it took.

```rust
pub fn print_from(&mut self, area: Rect, row: usize, text: &str, skip: usize) -> usize
```

Like `print`, with the text's first `skip` columns left off: for
anything scrolled sideways. Half of a wide character left showing is a space.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(6, 1);
frame.print_from(frame.area(), 0, "a long line", 2);
assert_eq!(frame.snapshot().plain(), "long l");
```

```rust
pub fn print_right(&mut self, area: Rect, row: usize, text: &str) -> usize
```

Like `print`, against the right edge of `area` instead of the left.
Text too wide for it loses its start, so its end still meets the edge. Returns how
many columns it took.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(10, 1);
frame.print_right(frame.area(), 0, "12:04");
assert_eq!(frame.snapshot().plain(), "     12:04");
```

```rust
pub fn print_center(&mut self, area: Rect, row: usize, text: &str) -> usize
```

Like `print`, in the middle of `area`'s row, a column nearer the left
when it can't be exactly. Text too wide for it is cut off at the right. Returns how
many columns it took.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(9, 1);
frame.print_center(frame.area(), 0, "tvim");
assert_eq!(frame.snapshot().plain(), "  tvim");
```

```rust
pub fn lines(&mut self, area: Rect, block: &Block)
```

Write each line of a block into `area`, one per row, from the top.

```rust
pub fn set_cursor(&mut self, x: usize, y: usize, shape: CursorShape)
```

Show the terminal's cursor at column `x` of row `y`, counted from 0 at the top left, in
the given shape. Frames hide it unless this is called, or if the place is off the screen.

```rust
use tinsel::app::{CursorShape, Frame};

let mut frame = Frame::new(10, 2);
frame.set_cursor(3, 1, CursorShape::Bar);
let at = frame.cursor().unwrap();
assert_eq!((at.x, at.y, at.shape), (3, 1, CursorShape::Bar));
frame.set_cursor(10, 1, CursorShape::Bar);
assert_eq!(frame.cursor(), None);
```

```rust
pub fn cursor(&self) -> Option<TerminalCursor>
```

Where the terminal's cursor shows, and how, if `set_cursor` put
it on the screen.

```rust
pub fn clear(&mut self, area: Rect)
```

Blank out part of the screen, so whatever's drawn there next sits on nothing (or on
the frame's `background`, if it has one).

```rust
pub fn set_background(&mut self, rgb: Rgb)
```

Paint the whole frame in this colour instead of the terminal's. Tinted cells keep their
tint, `clear` goes back to it and `fade` fades towards it.
The terminal's window padding around the frame takes it too, as
`set_window_color` would, unless that says otherwise.
Call it first in `App::view`. Without colour it does nothing.

```rust
use tinsel::Rgb;
use tinsel::app::Frame;

let mut frame = Frame::new(4, 1);
frame.set_background(Rgb(30, 20, 30));
assert_eq!(frame.background().is_some(), tinsel::color_mode() != tinsel::ColorMode::None);
```

```rust
pub fn background(&self) -> Option<Rgb>
```

The colour `set_background` painted the frame, as drawn.

```rust
pub fn set_cursor_color(&mut self, rgb: Rgb)
```

Colour the terminal's cursor, where the terminal allows it. It goes back to the
terminal's own when the app ends.

```rust
use tinsel::Rgb;
use tinsel::app::{CursorShape, Frame};

let mut frame = Frame::new(4, 1);
frame.set_cursor(0, 0, CursorShape::Bar);
frame.set_cursor_color(Rgb(140, 200, 160));
```

```rust
pub fn set_window_color(&mut self, rgb: Rgb)
```

Colour the terminal's own background, where the terminal allows it: what shows in its
window padding around the frame, so bars at the edges can run into it. It's sent again
shortly after the app starts and whenever the terminal regains focus, for terminals
that lose it, and goes back to the terminal's own when the app ends.

```rust
use tinsel::Rgb;
use tinsel::app::Frame;

let mut frame = Frame::new(4, 1);
frame.set_window_color(Rgb(30, 20, 30));
```

```rust
pub fn transparent(&mut self, area: Rect)
```

Let the terminal's own background show behind part of the screen, keeping whatever's
written there: for an edge that should meet the terminal's window padding, which is
drawn in that colour and not the frame's `background`.

```rust
use tinsel::Rgb;
use tinsel::app::Frame;

let mut frame = Frame::new(4, 1);
frame.set_background(Rgb(30, 20, 30));
frame.transparent(frame.area());
frame.print(frame.area(), 0, "edge");
assert_eq!(frame.snapshot().plain(), "edge");
```

```rust
pub fn tint(&mut self, area: Rect, bg: Rgb)
```

Paint the cells behind part of the screen, keeping whatever's written there.

```rust
pub fn fade(&mut self, area: Rect)
```

Fade part of the screen into the background, so whatever's drawn on top of it (a
`Modal`, say) stands out, by mixing every colour towards the
background; `dim` is the terminal's own faint text instead. Without
colour it's left as it is.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(10, 1);
frame.print(frame.area(), 0, "behind");
frame.fade(frame.area());
assert_eq!(frame.snapshot().plain(), "behind");
```

```rust
pub fn fade_by(&mut self, area: Rect, amount: f32)
```

Like `fade`, as far as `amount` says: 0 leaves it alone and 1 is the
same as `fade`. For something that's only a little out of the way, or a fade.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(10, 1);
frame.print(frame.area(), 0, "aside");
frame.fade_by(frame.area(), 0.3);
assert_eq!(frame.snapshot().plain(), "aside");
```

```rust
pub fn shadow(&mut self, of: Rect)
```

The soft shadow a box at `of` casts, below it and to its right, as a
`Modal` has: what's there stays, faded, rather than being blanked out.
On a dark background that's all (a block of near-black over the terminal's own
background reads as a hole, not a shadow); on a light one the cells also turn a pale
grey, which is what a shadow on paper looks like.

```rust
use tinsel::app::{Frame, Rect};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(10, 4);
frame.print(frame.area(), 3, "under");
frame.shadow(Rect::new(0, 0, 6, 3));
assert_eq!(frame.snapshot().plain().lines().last(), Some("under"));
```

```rust
pub fn modal(&mut self, modal: &super::Modal<'_>) -> Rect
```

Draw a `Modal` over the whole screen and give back where what's
inside it goes, so it can be drawn there: inside the border of a
`Panel` it holds, say.

```rust
use tinsel::app::{Frame, Modal, Panel, Rect};

let mut frame = Frame::new(30, 10);
let inner = frame.modal(&Modal::new(Panel::new("pick")).at(Rect::new(5, 2, 20, 6)));
assert_eq!(inner, Rect::new(7, 3, 16, 4));
frame.print(inner, 0, "› first");
```

```rust
pub fn hline(&mut self, x: usize, y: usize, width: usize, color: Rgb)
```

A line `width` columns long from column `x` of row `y`, in `color`. Where it meets
or crosses a line already drawn (a panel's border, another rule) the two join up,
`├` or `┼`, rather than one cutting the other.

```rust
use tinsel::app::{Frame, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(8, 4);
frame.render(frame.area(), Panel::new(""));
frame.hline(0, 2, 8, tinsel::palette::LINE);
assert_eq!(frame.snapshot().plain().lines().nth(2), Some("├──────┤"));
```

```rust
pub fn vline(&mut self, x: usize, y: usize, height: usize, color: Rgb)
```

A line `height` rows tall from row `y` of column `x`, in `color`, joining up with
lines already drawn as `hline` does: a column rule drawn from one
row rule to another meets them with `┬` and `┴`.

```rust
use tinsel::app::Frame;

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(5, 3);
frame.hline(0, 0, 5, tinsel::palette::LINE);
frame.hline(0, 2, 5, tinsel::palette::LINE);
frame.vline(2, 0, 3, tinsel::palette::LINE);
assert_eq!(frame.snapshot().plain(), "──┬──\n  │\n──┴──");
```

```rust
pub fn fade_outside(&mut self, keep: Rect)
```

`fade` everything but `keep`, so it's what the eye goes to: a
column of text while the rest of the screen falls away.

```rust
use tinsel::app::{Frame, Rect};

let mut frame = Frame::new(10, 3);
frame.fade_outside(Rect::new(2, 1, 4, 1));
```

```rust
pub fn snapshot(&self) -> Block
```

The frame as text, a line per row, trailing blanks trimmed. `.plain()` it in tests.

```rust
pub fn html(&self) -> String
```

The frame as a web page that looks just like the terminal: a `<pre>` on tinsel's
background for the `theme` in use, and each run of cells in the same
style a `<span>` with its colours and its bold, dim, italic, underline or reverse. For
screenshots in a README, or to see how something looks without a terminal. The colours
are the ones the frame was drawn with, so draw it in colour (see
`set_color_mode`) to see them. `html_pre`
is just the `<pre>`, to put in a page of your own.

It keeps to the grid a terminal does, in any monospace font: lines and blocks
(`─│╭╮├┼━▁▄█░`) are drawn rather than left to the font, so they join across cells and
rows, and other symbols a font may lack, like a spinner's `✧`, are each held to their
cell, so one borrowed from another font can't push the rest of its row along. The
characters stay in the page, so copying it gives the text back.

```rust
use tinsel::app::{Frame, Panel};

let mut frame = Frame::new(20, 3);
frame.render(frame.area(), Panel::new("hi").body("1 < 2"));
let page = frame.html();
assert!(page.starts_with("<!DOCTYPE html>") && page.contains("1 &lt; 2"));
// std::fs::write("screenshot.html", page)?;
```

```rust
pub fn html_pre(&self) -> String
```

The frame as a `<pre>` element, styled inline so it can go anywhere in a page. See
`html`.

```rust
use tinsel::app::Frame;

let mut frame = Frame::new(4, 1);
frame.print(frame.area(), 0, "a&b");
let pre = frame.html_pre();
assert!(pre.starts_with("<pre style=") && pre.ends_with("a&amp;b </pre>"));
```

## TerminalCursor

`tinsel::app::TerminalCursor` · struct

```rust
pub struct TerminalCursor
```

Where a frame shows the terminal's cursor, and how: from `Frame::cursor`.

**Fields**

```rust
pub x: usize,
```

The column, from 0 at the left.

```rust
pub y: usize,
```

The row, from 0 at the top.

```rust
pub shape: CursorShape,
```

Its shape.

## Grid

`tinsel::app::Grid` · struct

```rust
pub struct Grid<'a>
```

A `Table` with a cursor: rows to move through with ↑↓, the chosen one lit with a bar
and a soft band, scrolling down to keep it in view and sideways when the table is wider
than its area, the header always on top. Cells are lined up exactly as a printed
`Table` lines them up: numbers right, words left, styled cells fine. Rows can be
gathered under `heading`s.

```rust
use tinsel::app::{GridState, Frame, Grid};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut cursor = GridState::new();
cursor.select(1);
let grid = Grid::new(["service", "pods"])
    .row(["api", "12"])
    .row(["worker", "3"])
    .state(&cursor);
let mut frame = Frame::new(20, 4);
frame.render(frame.area(), grid);
assert_eq!(frame.snapshot().plain(), "  service   pods\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  api         12\n› worker       3");
```

**Methods**

```rust
pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Self
```

A grid with these column headers.

```rust
pub fn fill(mut self, column: usize) -> Self
```

Give `column` whatever width the grid has to spare, so the grid fills its area
instead of leaving a gap at the right. Longer cells there are cut only when the
area is too narrow.

```rust
pub fn row<S: Into<String>>(mut self, cells: impl IntoIterator<Item = S>) -> Self
```

Add a row of cells. Styled cells (`badge`s, coloured dots) are fine.
As with `Table::row`, debug builds stop at a row with more cells than headers.

```rust
pub fn heading(mut self, text: impl Into<String>) -> Self
```

Start a group: a heading drawn on its own line above the rows added after it, up to
the next heading. Headings aren't rows: the cursor, `row_at` and
`GridState::handle`'s `len` count only the rows, so row 0 is the first row under the
first heading, and moving steps over headings. Scrolling up to a group's first rows
brings its heading into view too.

```rust
use tinsel::app::{GridState, Frame, Grid};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut cursor = GridState::new();
cursor.select(1);
let grid = Grid::new(["service", "pods"])
    .heading("frontend")
    .row(["web", "4"])
    .heading("backend")
    .row(["api", "12"])
    .state(&cursor);
let mut frame = Frame::new(20, 6);
frame.render(frame.area(), grid);
assert_eq!(
    frame.snapshot().plain(),
    "  service   pods\n  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n  frontend\n  web          4\n  backend\n› api         12"
);
```

```rust
pub fn align(mut self, column: usize, align: Align) -> Self
```

Override the automatic alignment of one column.

```rust
pub fn max_width(mut self, column: usize, cols: usize) -> Self
```

Cut cells in a column to at most `cols` wide, ending in `…`.

```rust
pub fn state(mut self, state: &'a GridState) -> Self
```

Where the cursor is and how far it's scrolled. Without one, nothing is chosen and the
grid starts at the top.

```rust
pub fn focused(mut self, focused: bool) -> Self
```

Whether keys go to this grid. When they don't, the cursor row stays marked, but
quietly. Focused unless set.

```rust
pub fn striped(mut self) -> Self
```

Tint every other row faintly, to help the eye along wide rows.

```rust
pub fn blank_cells(mut self) -> Self
```

Leave empty cells blank, instead of marking each with a muted `·`
(see `Table::blank_cells`).

```rust
pub fn sorted(mut self, column: usize) -> Self
```

Mark `column` as the one the rows are sorted by, smallest first: `▲` beside its
header. Sort the rows yourself, so the cursor's row is the one you think it is.

```rust
pub fn sorted_desc(mut self, column: usize) -> Self
```

Like `sorted`, largest first: `▼`.

```rust
pub fn loading(mut self, loading: bool) -> Self
```

Show that the rows are still on their way: shimmering placeholder rows under the
header, and a spinner.

```rust
pub fn error(mut self, message: impl Into<String>) -> Self
```

Show that the rows couldn't be had, instead of any rows.

```rust
pub fn empty(mut self, message: impl Into<String>) -> Self
```

What to say when there are no rows. "nothing here yet" unless set.

```rust
pub fn row_at(&self, area: Rect, x: usize, y: usize) -> Option<usize>
```

Which row is at `x`, `y`, if the grid is drawn at `area`: for clicks.

## GridState

`tinsel::app::GridState` · struct

```rust
pub struct GridState
```

Where the cursor row is in a `Grid`, and how far it's scrolled: the grid's state, as
opposed to the terminal's cursor. Keep one in your app for
each grid, move it in `event`, and hand it to the grid in
`view`.

```rust
use tinsel::app::{GridState, Event, Key};

let mut cursor = GridState::new();
cursor.handle(3, &Event::Key(Key::Down));
cursor.handle(3, &Event::Key(Key::End));
assert_eq!(cursor.row(), 2);
```

**Methods**

```rust
pub fn new() -> Self
```

A cursor on the first row, scrolled to the left edge.

```rust
pub fn row(&self) -> usize
```

The row the cursor is on, counted from 0.

```rust
pub fn select(&mut self, row: usize)
```

Put the cursor on `row`.

```rust
pub fn keymap(mut self, keymap: Keymap) -> Self
```

Which keys move the cursor: vim's j/k and g/G as well as the arrows unless this says
`Keymap::Arrows`.

```rust
pub fn handle(&mut self, len: usize, event: &Event) -> bool
```

Move the cursor through `len` rows in response to an event, as
`navigate` would with this state's `keymap`.
Returns whether it moved.

```rust
pub fn column(&self) -> usize
```

The first column showing, when the grid is scrolled sideways.

```rust
pub fn scroll_right(&mut self) -> bool
```

Scroll one column to the right, if anything's cut off there. Returns whether it moved.

```rust
use tinsel::app::{GridState, Frame, Grid};

let mut cursor = GridState::new();
let grid = |c| Grid::new(["name", "env", "owner"]).row(["api", "production", "platform"]).state(c);
let mut frame = Frame::new(16, 3);
frame.render(frame.area(), grid(&cursor));
assert!(cursor.scroll_right());
```

```rust
pub fn scroll_left(&mut self) -> bool
```

Scroll one column back to the left. Returns whether it moved, so a key that scrolls
can do something else (move to another panel, say) once it's at the edge.

## Key

`tinsel::app::Key` · enum

```rust
pub enum Key
```

A key someone pressed.

Keys pressed on their own are their own variants, so `Key::Up`, `Key::Char('x')` and
`Key::Ctrl('r')` match as you'd expect. A key held with modifiers comes as one of the
others:

- Alt (Option on a Mac, where the terminal sends it as Meta) and a character is
  `Key::Alt`: `Key::Alt('b')`.
- Any other key held with Shift, Alt or Ctrl is `Key::Mod`, the modifiers and the key:
  Ctrl-← is `Key::Mod(Mods::CTRL, NamedKey::Left)`, Shift-Alt-↑ is
  `Key::Mod(Mods::SHIFT | Mods::ALT, NamedKey::Up)`. Shift-Tab stays `Key::BackTab`.

`Key::mods` and `Key::unmodified` take a key apart, for an app that treats Ctrl-→
like → or wants to know whether Shift was down whatever the key.

```rust
use tinsel::app::{Key, Mods, NamedKey};

let key = Key::Mod(Mods::CTRL, NamedKey::Left);
assert!(matches!(key, Key::Mod(m, NamedKey::Left) if m.ctrl()));
assert_eq!(key.unmodified(), Key::Left);
assert_eq!(Key::modified(NamedKey::Up, Mods::NONE), Key::Up);
```

**Variants**

```rust
Char(char),
```

A printable character, space included.

```rust
Ctrl(char),
```

Ctrl and a letter, as the lowercase letter. Ctrl-C is `Ctrl('c')`.

```rust
Alt(char),
```

Alt (or Meta, or Option) and a character, as typed: Alt-Shift-x is `Alt('X')`.

```rust
Enter,
```

Return.

```rust
Tab,
```

Tab.

```rust
BackTab,
```

Shift-Tab.

```rust
Backspace,
```

Backspace.

```rust
Delete,
```

Forward delete.

```rust
Esc,
```

Escape, on its own.

```rust
Up,
```

↑

```rust
Down,
```

↓

```rust
Left,
```

←

```rust
Right,
```

→

```rust
Home,
```

Home.

```rust
End,
```

End.

```rust
PageUp,
```

Page Up.

```rust
PageDown,
```

Page Down.

```rust
F(u8),
```

A function key, F1 to F24.

```rust
Mod(Mods, NamedKey),
```

A key that isn't a character, held with modifiers (never `Mods::NONE`).

**Methods**

```rust
pub fn modified(key: NamedKey, mods: Mods) -> Key
```

`key` held with `mods`, as the terminal would send it: the plain variant with no
modifiers, `Key::BackTab` for Shift-Tab, and `Key::Mod` otherwise.

```rust
pub fn mods(self) -> Mods
```

The modifiers held with this key: Ctrl for `Key::Ctrl`, Alt for `Key::Alt`,
Shift for `Key::BackTab`, none for the rest.

```rust
pub fn unmodified(self) -> Key
```

The key with its modifiers let go: Ctrl-← is ←, Alt-b and Ctrl-B are `Char('b')`,
Shift-Tab is Tab.

```rust
pub fn parse_sequence(text: &str) -> Vec<Key>
```

Keys written one after another in vim's notation, as a mapping or a macro would be:
`dw<Esc>` is d, w and Esc. A `<` that doesn't start a key's name is the character.

```rust
use tinsel::app::Key;

assert_eq!(Key::parse_sequence("i<C-r>"), [Key::Char('i'), Key::Ctrl('r')]);
assert_eq!(Key::parse_sequence("a<b"), [Key::Char('a'), Key::Char('<'), Key::Char('b')]);
```

```rust
pub fn symbol(self) -> String
```

The key as the symbols a Mac's menus use, for showing which key does what: `⌃A`,
`⌥X`, `⇧⇥`, `⏎`, `⎋`, `↑`, and a character as itself (space as `␣`).

```rust
use tinsel::app::{Key, Mods, NamedKey};

assert_eq!(Key::Ctrl('a').symbol(), "⌃A");
assert_eq!(Key::Esc.symbol(), "⎋");
assert_eq!(Key::Mod(Mods::CTRL | Mods::SHIFT, NamedKey::Up).symbol(), "⌃⇧↑");
```

## Mods

`tinsel::app::Mods` · struct

```rust
pub struct Mods(u8)
```

Which of Shift, Alt and Ctrl were held, for `Key::Mod` and `Mouse::mods`. Combine
them with `|`; the constants work in patterns too.

```rust
use tinsel::app::Mods;

let held = Mods::CTRL | Mods::SHIFT;
assert!(held.ctrl() && held.shift() && !held.alt());
assert!(held.contains(Mods::CTRL));
```

**Methods**

```rust
pub const NONE: Mods = Mods(0)
```

Nothing held.

```rust
pub const SHIFT: Mods = Mods(1)
```

Shift.

```rust
pub const ALT: Mods = Mods(2)
```

Alt, Meta or Option: terminals don't tell them apart.

```rust
pub const CTRL: Mods = Mods(4)
```

Ctrl.

```rust
pub fn shift(self) -> bool
```

Whether Shift was held.

```rust
pub fn alt(self) -> bool
```

Whether Alt was held.

```rust
pub fn ctrl(self) -> bool
```

Whether Ctrl was held.

```rust
pub fn contains(self, other: Mods) -> bool
```

Whether everything in `other` was held.

```rust
pub fn is_empty(self) -> bool
```

Whether nothing was held.

## Mouse

`tinsel::app::Mouse` · struct

```rust
pub struct Mouse
```

A mouse event, at a cell counted from 0 at the top left. Check which panel it landed in
with `Rect::contains`.

**Fields**

```rust
pub kind: MouseKind,
```

What happened.

```rust
pub x: usize,
```

The column.

```rust
pub y: usize,
```

The row.

```rust
pub mods: Mods,
```

Which of Shift, Alt and Ctrl were held, where the terminal says (many keep
Shift-click for selecting text themselves).

**Methods**

```rust
pub fn new(kind: MouseKind, x: usize, y: usize) -> Self
```

A mouse event of this kind at column `x` and row `y`, with nothing held, as a test
might send one.

## MouseKind

`tinsel::app::MouseKind` · enum

```rust
pub enum MouseKind
```

What the mouse did. Only reported when `App::mouse` says so.

**Variants**

```rust
Click,
```

A left click.

```rust
MiddleClick,
```

A middle click (the wheel pressed).

```rust
RightClick,
```

A right click.

```rust
Release,
```

A button let go.

```rust
Drag,
```

The mouse moved with a button held down.

```rust
ScrollUp,
```

The wheel, rolled up.

```rust
ScrollDown,
```

The wheel, rolled down.

## NamedKey

`tinsel::app::NamedKey` · enum

```rust
pub enum NamedKey
```

Keys that aren't characters, for `Key::Mod`.

**Variants**

```rust
Enter,
```

Return.

```rust
Tab,
```

Tab.

```rust
Backspace,
```

Backspace.

```rust
Delete,
```

Forward delete.

```rust
Up,
```

↑

```rust
Down,
```

↓

```rust
Left,
```

←

```rust
Right,
```

→

```rust
Home,
```

Home.

```rust
End,
```

End.

```rust
PageUp,
```

Page Up.

```rust
PageDown,
```

Page Down.

```rust
F(u8),
```

A function key, F1 to F24.

## Job

`tinsel::app::Job` · struct

```rust
pub struct Job<T>
```

Slow work (running a command, reading a big file, asking a server) done on another
thread, so the app keeps drawing meanwhile. When the work is done the app is sent an
`Event::Wake`, and `poll` hands the result over.

To start the work again, make a new job in the old one's place: whatever the old one
finds is dropped with it, so a stale answer never turns up after a newer one.

```rust
use tinsel::app::{self, App, Event, Flow, Frame, Job};

struct Status {
    job: Option<Job<String>>,
    shown: String,
}

impl App for Status {
    fn view(&self, frame: &mut Frame) {
        frame.print(frame.area(), 0, &self.shown);
    }

    fn event(&mut self, event: &Event) -> Flow {
        if let Some(done) = self.job.as_mut().and_then(Job::poll) {
            self.shown = done;
            self.job = None;
        }
        match event {
            Event::Key(app::Key::Char('q')) => Flow::Quit,
            _ => Flow::Continue,
        }
    }
}

let job = Job::spawn(|| "all healthy".to_string());
app::run(Status { job: Some(job), shown: "checking…".into() })?;
```

**Methods**

```rust
pub fn spawn(work: impl FnOnce() -> T + Send + 'static) -> Self
```

Start `work` on a thread of its own.

```rust
pub fn poll(&mut self) -> Option<T>
```

The result, once the work is done: `Some` the first time it's asked for after that,
and `None` before and after.

```rust
pub fn is_finished(&self) -> bool
```

Whether the work has ended, with a result or by panicking.

```rust
pub fn wait(mut self) -> Option<T>
```

Wait for the work to end and take its result, if it gave one and it hasn't been
taken: for tests, and for an app that can't go on without it.

## Keys

`tinsel::app::Keys` · struct

```rust
pub struct Keys
```

Keys read one at a time in the normal terminal, outside a full-screen `run`:
for a loop that redraws now and then and reacts to a key in between.

While it's open, what's typed isn't shown and Ctrl-C arrives as `Key::Ctrl('c')` instead
of stopping the program. Dropping it puts the terminal back. While a
`prompt::script` runs on this thread, keys come from the script
instead, and once it has none left nothing is pressed.

```rust
use std::time::Duration;
use tinsel::app::{Key, Keys};

let mut keys = Keys::open()?;
loop {
    // show what changed
    match keys.next(Duration::from_secs(4))? {
        Some(Key::Char('q') | Key::Ctrl('c')) => break,
        _ => {}
    }
}
```

**Methods**

```rust
pub fn open() -> io::Result<Keys>
```

Start reading keys from the terminal on stdin. Fails without one.

**Errors**

An I/O error when there's no terminal on stdin, or it can't be put into raw mode.

```rust
pub fn next(&mut self, timeout: Duration) -> io::Result<Option<Key>>
```

The next key, waiting up to `timeout` (to within a tenth of a second) for one, or
`None` if nothing was pressed. Mouse reports and anything else that isn't a key are
skipped. Once the terminal has gone (its window closed), this is an error.

**Errors**

An I/O error once the terminal has gone.

## Rect

`tinsel::app::Rect` · struct

```rust
pub struct Rect
```

A rectangle of cells on the screen, counted from 0 at the top left.

Start from `Frame::area` and cut it up:

```rust
use tinsel::app::{Rect, Size::*};

let screen = Rect::new(0, 0, 80, 24);
let [header, body, footer] = screen.rows([Fixed(3), Fill(1), Fixed(1)]);
let [sidebar, main] = body.columns_gap(1, [Fixed(24), Fill(1)]);
assert_eq!((sidebar.width, main.x, main.width), (24, 25, 55));
assert_eq!((header.height, body.height, footer.y), (3, 20, 23));
```

**Fields**

```rust
pub x: usize,
```

The leftmost column.

```rust
pub y: usize,
```

The top row.

```rust
pub width: usize,
```

How many columns wide.

```rust
pub height: usize,
```

How many rows tall.

**Methods**

```rust
pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self
```

A rectangle at `x`, `y`, `width` columns by `height` rows.

```rust
pub fn rows<const N: usize>(self, sizes: [Size; N]) -> [Rect; N]
```

Cut into rows, top to bottom.

```rust
pub fn columns<const N: usize>(self, sizes: [Size; N]) -> [Rect; N]
```

Cut into columns, left to right.

```rust
pub fn rows_gap<const N: usize>(self, gap: usize, sizes: [Size; N]) -> [Rect; N]
```

Cut into rows with `gap` empty rows between them.

```rust
pub fn columns_gap<const N: usize>(self, gap: usize, sizes: [Size; N]) -> [Rect; N]
```

Cut into columns with `gap` empty columns between them.

```rust
pub fn shrink(self, x: usize, y: usize) -> Rect
```

The rectangle with `x` columns taken off each side and `y` rows off the top and bottom.

```rust
pub fn center(self, width: usize, height: usize) -> Rect
```

A `width` by `height` rectangle in the middle of this one, for dialogs and popups.
Clamped to fit.

```rust
pub fn split_left(self, width: usize) -> (Rect, Rect)
```

The rectangle cut in two down a column: the first `width` columns (or all of them,
if it's narrower), and the rest. For a gutter beside text, a sidebar beside a page.

```rust
use tinsel::app::Rect;

let (gutter, text) = Rect::new(0, 0, 80, 24).split_left(6);
assert_eq!((gutter, text), (Rect::new(0, 0, 6, 24), Rect::new(6, 0, 74, 24)));
```

```rust
pub fn split_right(self, width: usize) -> (Rect, Rect)
```

The rectangle cut in two down a column: the last `width` columns, and the rest
before them. For a scrollbar at the right edge.

```rust
use tinsel::app::Rect;

let (bar, rest) = Rect::new(0, 0, 10, 4).split_right(1);
assert_eq!((bar, rest), (Rect::new(9, 0, 1, 4), Rect::new(0, 0, 9, 4)));
```

```rust
pub fn split_top(self, height: usize) -> (Rect, Rect)
```

The rectangle cut in two across a row: the first `height` rows, and the rest.

```rust
use tinsel::app::Rect;

let (head, body) = Rect::new(0, 0, 10, 4).split_top(1);
assert_eq!((head, body), (Rect::new(0, 0, 10, 1), Rect::new(0, 1, 10, 3)));
```

```rust
pub fn split_bottom(self, height: usize) -> (Rect, Rect)
```

The rectangle cut in two across a row: the last `height` rows, and the rest above
them. For a status line at the bottom.

```rust
use tinsel::app::Rect;

let (status, page) = Rect::new(0, 0, 10, 4).split_bottom(1);
assert_eq!((status, page), (Rect::new(0, 3, 10, 1), Rect::new(0, 0, 10, 3)));
```

```rust
pub fn row(self, n: usize) -> Rect
```

One row of the rectangle, counted from 0. Empty if it's past the bottom.

```rust
pub fn contains(self, x: usize, y: usize) -> bool
```

Whether the cell at `x`, `y` is inside: which panel a click landed in.

```rust
pub fn is_empty(self) -> bool
```

Whether there's no room in it at all.

```rust
pub fn intersect(self, other: Rect) -> Rect
```

The part of this rectangle that's also inside `other`.
When they don't overlap, the result is empty and sits on `other`'s edge.

## Size

`tinsel::app::Size` · enum

```rust
pub enum Size
```

How much of a split one piece gets.

**Variants**

```rust
Fixed(usize),
```

Exactly this many cells, if there's room.

```rust
Percent(usize),
```

This percentage of the whole.

```rust
Fill(usize),
```

A share of whatever's left after the fixed and percentage pieces, by weight:
`Fill(2)` gets twice what `Fill(1)` does.

## ParseKeyError

`tinsel::app::ParseKeyError` · struct

```rust
pub struct ParseKeyError(String)
```

Why some text isn't a key, from `Key`'s `FromStr`: it holds the text.

## Modal

`tinsel::app::Modal` · struct

```rust
pub struct Modal<'a>
```

Something drawn in the middle of the screen over everything else, which fades back
behind it: a dialog, a picker, the help. Usually a focused `Panel`. Draw
it last, over the whole screen.

It's as big as what's inside wants to be (see `Widget::size`), or as
`size` says, and never bigger than the screen.

```rust
use tinsel::app::{Frame, Modal, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(20, 5);
frame.print(frame.area(), 2, "behind it all");
frame.render(frame.area(), Modal::new(Panel::new("sure?").body("y / n")));
assert_eq!(
    frame.snapshot().plain(),
    "\n    ╭─ sure? ──╮\nbehi│ y / n    │\n    ╰──────────╯\n"
);
```

**Methods**

```rust
pub fn new(body: impl Widget + 'a) -> Self
```

A modal holding this widget.

```rust
pub fn size(mut self, width: usize, height: usize) -> Self
```

Make it `width` by `height`, whatever's inside.

```rust
pub fn at(mut self, rect: Rect) -> Self
```

Put it exactly here, rather than in the middle of the screen: high up, say, so what
it's about shows below it. Kept on the screen.

```rust
use tinsel::app::{Modal, Rect};

let screen = Rect::new(0, 0, 40, 20);
let placed = Modal::new("hi").at(Rect::new(5, 2, 30, 6));
assert_eq!(placed.area(screen), Rect::new(5, 2, 30, 6));
```

```rust
pub fn opened(mut self, seconds: f64) -> Self
```

When it was opened, from `app::time` (or `Frame::time`), so it can
rise gently into place, with what's behind it fading as it comes, instead of
appearing all at once. It asks for the frames it needs to.

```rust
pub fn background(mut self, color: Rgb) -> Self
```

Fill it in this colour, as `Panel::background` does a
panel, so it stands apart from what's behind.

```rust
pub fn dim(mut self, dim: bool) -> Self
```

Whether what's behind fades back. Yes unless changed; a small popup that shouldn't
take over the screen says no.

```rust
pub fn shown(&self, t: f64) -> f32
```

How far it has come into place at `t` seconds, from 0 as it opens to 1 once settled:
1 throughout unless it was `opened`.

```rust
pub fn area(&self, screen: Rect) -> Rect
```

Where it goes when drawn over `screen`, once it's settled: for clicks.

## Toasts

`tinsel::app::Toasts` · struct

```rust
pub struct Toasts
```

Short messages that slide into a corner, stay a few seconds and go: "saved", "couldn't
connect". Keep one in your app, `push` to it from
`event`, and draw it last, over the area the toasts should sit in
the bottom right of. A line along each one's foot runs down until it goes.

```rust
use tinsel::Tone;
use tinsel::app::{Frame, Toasts};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut toasts = Toasts::new();
toasts.push(0.0, Tone::Success, "deployed my-app");
let mut frame = Frame::new(30, 3).at(1.0);
frame.render(frame.area(), &toasts);
assert_eq!(
    frame.snapshot().plain(),
    "       ╭─────────────────────╮\n       │ ●  deployed my-app  │\n       ╰━━━━━━━━━━━━━━━━─────╯"
);
```

**Methods**

```rust
pub fn new() -> Self
```

No toasts yet. Each stays for four seconds unless `life` says.

```rust
pub fn life(mut self, seconds: f64) -> Self
```

How many seconds each toast stays.

```rust
pub fn push(&mut self, now: f64, tone: Tone, text: impl Into<String>)
```

Show a message, marked and coloured by its tone, from `now`: `app::time`
in `App::event`, as for `Modal::opened`.

```rust
pub fn showing(&self, seconds: f64) -> bool
```

Whether any toast is still showing at `seconds`.

## Scroll

`tinsel::app::Scroll` · struct

```rust
pub struct Scroll
```

How far a `ScrollView` is scrolled. Keep one in your app for each view, pass it events
in `event`, and hand it to the view in `view`.

One made with `following` keeps the end in view as lines are
added, like `tail -f`. Scrolling up stops that, so what's being read stays put, and
scrolling back to the end starts it again.

```rust
use tinsel::app::{Event, Frame, Key, Scroll, ScrollView};

tinsel::set_color_mode(tinsel::ColorMode::None);
let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
let mut scroll = Scroll::new();
let mut frame = Frame::new(10, 3);
frame.render(frame.area(), ScrollView::new(&lines).scroll(&scroll));
scroll.handle(&Event::Key(Key::PageDown));
frame.render(frame.area(), ScrollView::new(&lines).scroll(&scroll));
assert_eq!(scroll.top(), 2);
assert!(frame.snapshot().plain().starts_with("line 3"));
```

**Methods**

```rust
pub fn new() -> Self
```

Scrolled to the top.

```rust
pub fn following() -> Self
```

Scrolled to the end, and kept there as lines are added, until scrolled up.

```rust
pub fn keymap(mut self, keymap: Keymap) -> Self
```

Which keys scroll it: vim's j/k and g/G as well as the arrows unless this says
`Keymap::Arrows`.

```rust
pub fn top(&self) -> usize
```

The first line showing, counted from 0.

```rust
pub fn is_following(&self) -> bool
```

Whether the end is being kept in view: made `following`, and
not scrolled up since. Good for a "following" mark in a panel's status.

```rust
pub fn scroll_to(&mut self, line: usize)
```

Scroll so `line` is the first showing (as far as there's text below it to fill the
view). Stops following the end.

```rust
pub fn scroll_to_end(&mut self)
```

Scroll to the end, and follow it if this scroll follows.

```rust
pub fn handle(&mut self, event: &Event) -> bool
```

Scroll in response to an event: ↑↓ (and j/k) a line, Page Up/Down and Space a screen,
Home/End (and g/G) to the ends, and the scroll wheel three lines, with the letters
only if its `keymap` is vim's. Returns whether it
moved, judging by how much the view showed when it was last drawn.

```rust
use tinsel::app::{Event, Key, Scroll};

let mut scroll = Scroll::following();
scroll.handle(&Event::Key(Key::Up));
assert!(!scroll.is_following());
```

## ScrollView

`tinsel::app::ScrollView` · struct

```rust
pub struct ScrollView<'a>
```

Lines to scroll through, with a scrollbar when there are more than fit: a log, a help
screen, a file. Lines are cut at the edge unless `wrap`ped, and
styled text is fine. Without a `Scroll`, it shows the top.

```rust
use tinsel::app::{Frame, Scroll, ScrollView};

tinsel::set_color_mode(tinsel::ColorMode::None);
let log = ["starting", "listening on :8080", "GET /health 200"];
let tail = Scroll::following();
let mut frame = Frame::new(20, 2);
frame.render(frame.area(), ScrollView::new(log).scroll(&tail));
assert_eq!(frame.snapshot().plain(), "listening on :8080 │\nGET /health 200    ┃");
```

**Methods**

```rust
pub fn new<S: Into<String>>(lines: impl IntoIterator<Item = S>) -> Self
```

A view of these lines. A `Block` works too, as `block.lines()`.

```rust
pub fn scroll(mut self, scroll: &'a Scroll) -> Self
```

How far it's scrolled.

```rust
pub fn wrap(mut self) -> Self
```

Word-wrap long lines to the width, instead of cutting them off.

```rust
pub fn empty(mut self, text: impl Into<String>) -> Self
```

What to say when there are no lines: "nothing here yet" unless changed.

## Position

`tinsel::app::Position` · struct

```rust
pub struct Position
```

A place in a `TextArea`'s text, from `TextArea::cursor`: the line, and the character
in it, both counted from 0. (Characters, not columns: a wide one counts once.)

**Fields**

```rust
pub row: usize,
```

The line.

```rust
pub column: usize,
```

The character in the line.

## TextArea

`tinsel::app::TextArea` · struct

```rust
pub struct TextArea
```

A box to type several lines into: a message, notes, a description. Like `TextInput`,
keep one in your app, pass it every event while it has focus, and read what's typed with
`value`. Enter starts a new line, pasted text keeps its line breaks,
long lines wrap, and the box scrolls to keep the cursor in view.

```rust
use tinsel::app::{Event, Frame, Key, Position, TextArea};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut notes = TextArea::new().placeholder("what changed?");
notes.focus(true);
notes.handle(&Event::Paste("fix the build\n".into()));
for c in "and the docs".chars() {
    notes.handle(&Event::Key(Key::Char(c)));
}
assert_eq!(notes.value(), "fix the build\nand the docs");
assert_eq!(notes.cursor(), Position { row: 1, column: 12 });
```

**Methods**

```rust
pub fn new() -> Self
```

An empty box, without focus.

```rust
pub fn placeholder(mut self, text: impl Into<String>) -> Self
```

Quiet text shown while the box is empty, saying what goes in it.

```rust
pub fn value(&self) -> String
```

What's been typed, its lines joined with `\n`.

```rust
pub fn set_value(&mut self, value: &str)
```

Replace what's typed, with the cursor at the end.

```rust
pub fn clear(&mut self)
```

Empty the box.

```rust
pub fn cursor(&self) -> Position
```

Where the cursor is: the line, and the character in it, both counted from 0.

```rust
pub fn focus(&mut self, focused: bool)
```

Whether this is where keys go. Only a focused box takes keys or shows its cursor.

```rust
pub fn is_focused(&self) -> bool
```

Whether the box has focus.

```rust
pub fn handle(&mut self, event: &Event) -> bool
```

Type, delete or move the cursor, if the box has focus and the event is one it uses:
characters and pastes, Enter for a new line, Backspace and Delete (joining lines at
their ends), ←→↑↓, Page Up/Down ten lines, Home and End (or Ctrl-A and Ctrl-E) for
the line's ends, Ctrl-U and Ctrl-K to clear the line before or after the cursor, and
Ctrl-W or Alt-Backspace to delete a word, and Alt or Ctrl with ←→ to move by words.
Returns whether it used the event, so Esc and Tab are left
for the app, to leave the box with.

```rust
use tinsel::app::{Event, Key, TextArea};

let mut text = TextArea::new();
text.focus(true);
text.set_value("one\ntwo");
text.handle(&Event::Key(Key::Home));
text.handle(&Event::Key(Key::Backspace));
assert_eq!(text.value(), "onetwo");
assert!(!text.handle(&Event::Key(Key::Esc)));
```

## Footer

`tinsel::app::Footer` · struct

```rust
pub struct Footer
```

The line along the bottom of an app: key hints on the left, in groups, and a status on
the right (when it last refreshed, a spinner while something loads).

When there isn't room for everything, it's fitted like this, so nothing is ever cut off
partway through a hint:

- The status always stays. Only if it's wider than the whole line is it cut short, with
  "…", and then there are no keys.
- The keys share what's left, less a two-column gap before the status. Groups are kept
  in the order they were added, so put the most important first: whole groups go from
  the end, and the last group that partly fits keeps as many of its first hints as fit.
  Nothing after a hint that didn't fit is shown, so later groups never jump ahead.

```rust
use tinsel::app::{Footer, Frame};

tinsel::set_color_mode(tinsel::ColorMode::None);
let footer = Footer::new()
    .keys(&[("↑↓", "move"), ("/", "filter")])
    .keys(&[("?", "help"), ("q", "quit")])
    .status("refreshed 12s ago");
let draw = |width| {
    let mut frame = Frame::new(width, 1);
    frame.render(frame.area(), &footer);
    frame.snapshot().plain()
};
assert_eq!(draw(60), "↑↓ move · / filter   ? help · q quit       refreshed 12s ago");
assert_eq!(draw(46), "↑↓ move · / filter   ? help  refreshed 12s ago");
assert_eq!(draw(30), "↑↓ move      refreshed 12s ago");
assert_eq!(draw(12), "refreshed 1…");
```

**Methods**

```rust
pub fn new() -> Self
```

An empty footer.

```rust
pub fn keys(mut self, keys: &[(&str, &str)]) -> Self
```

A group of keys and what they do, set a little apart from the others. Groups added
first stay longest when the footer is short of room.

```rust
pub fn status(mut self, status: impl Into<String>) -> Self
```

What's going on, at the right. Styled text is fine: a spinner, a coloured dot.

## List

`tinsel::app::List` · struct

```rust
pub struct List
```

A list to move through, with the chosen row lit up. It scrolls itself to keep that row in
view, and shows a scrollbar when there's more than fits. Keep the selected index in your
app and move it with `navigate`.

```rust
use tinsel::app::{Frame, List};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(12, 3);
frame.render(frame.area(), List::new(["api", "worker", "cron"]).selected(1));
assert_eq!(frame.snapshot().plain(), "  api\n› worker\n  cron");
```

**Methods**

```rust
pub fn new<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self
```

A list of these items. Styled text is fine.

```rust
pub fn selected(mut self, index: usize) -> Self
```

Which item is chosen.

## Loading

`tinsel::app::Loading` · struct

```rust
pub struct Loading
```

Something on its way: a spinner and a shimmering label, as a `Live` step
shows while it runs, with a live bar under it once there's `progress`
to show. It animates on the app's clock, so it needs nothing kept between frames. Put it
in a panel while its data loads, or in a footer while something saves.

```rust
use tinsel::app::{Frame, Loading};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(30, 2);
frame.render(frame.area(), Loading::new("fetching pods").progress(0.5, "6 / 12"));
let shown = frame.snapshot().plain();
assert!(shown.contains("fetching pods") && shown.contains(" 50%  6 / 12"));
```

**Methods**

```rust
pub fn new(label: impl Into<String>) -> Self
```

A spinner beside `label`, saying what's happening.

```rust
pub fn detail(mut self, text: impl Into<String>) -> Self
```

A quieter line under the label, saying more: what it's waiting on, how long it's been.
Left out once there's progress to show.

```rust
pub fn progress(mut self, fraction: f32, text: impl Into<String>) -> Self
```

How far along it is, from 0 to 1, shown as a bar and a percentage, with `text`
beside it: `"6 / 12"`, `"17 MB"`.

```rust
pub fn spinner(mut self, spinner: crate::Spinner) -> Self
```

Which spinner: `Sparkle` unless changed.

```rust
pub fn centered(mut self) -> Self
```

Sit in the middle of the area, as a panel's whole body does while it waits.

## Panel

`tinsel::app::Panel` · struct

```rust
pub struct Panel<'a>
```

A box with a rounded border and a title, holding any other widget. A focused panel's
border shimmers, so it's clear where keys will go.

Along the top border go the title, a `count` beside it and a
`status` at the right; along the bottom, a `footer`.
When the panel is too narrow for them all, the status is cut short and then left off,
then the count goes, and last the title is cut short. The border itself is never broken.

```rust
use tinsel::app::{Frame, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(24, 4);
frame.render(frame.area(), Panel::new("notes").body("buy glitter").footer("1/3"));
assert_eq!(
    frame.snapshot().plain(),
    "╭─ notes ──────────────╮\n│ buy glitter          │\n│                      │\n╰──────────────── 1/3 ─╯"
);
```

**Methods**

```rust
pub fn new(title: impl Into<String>) -> Self
```

A panel with this title (which can be empty). Plain text is drawn in the panel's own
quiet colour (pink when focused); styled text keeps its own colours, so a title can
carry a coloured mark or a `Line` of several colours.

```rust
use tinsel::app::{Frame, Panel};
use tinsel::{Line, palette};

tinsel::set_color_mode(tinsel::ColorMode::None);
let title = Line::new().push(palette::RED, "● ").push(palette::TEXT, "api");
let mut frame = Frame::new(14, 2);
frame.render(frame.area(), Panel::new(title.to_string()));
assert_eq!(frame.snapshot().plain(), "╭─ ● api ────╮\n╰────────────╯");
```

```rust
pub fn body(mut self, widget: impl Widget + 'a) -> Self
```

What goes inside.

```rust
pub fn count(mut self, count: impl std::fmt::Display) -> Self
```

A count beside the title, in a little pill: how many things are inside, or `3/12`
when some are filtered out.

```rust
use tinsel::app::{Frame, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(20, 2);
frame.render(frame.area(), Panel::new("services").count(12));
assert_eq!(frame.snapshot().plain(), "╭─ services 12 ────╮\n╰──────────────────╯");
```

```rust
pub fn status(mut self, text: impl Into<String>) -> Self
```

A little text on the top border, at the right: where the panel's data comes from,
when it was read. Styled text keeps its own colours; plain text is muted. When there
isn't room beside the title it's cut short, and left off once fewer than six columns
of it would show.

```rust
use tinsel::app::{Frame, Panel};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(36, 2);
frame.render(frame.area(), Panel::new("services").status("staging · read now"));
assert_eq!(
    frame.snapshot().plain(),
    "╭─ services ── staging · read now ─╮\n╰──────────────────────────────────╯"
);
```

```rust
pub fn footer(mut self, text: impl Into<String>) -> Self
```

A little text on the bottom border, at the right: a count, a hint.

```rust
pub fn focused(mut self, focused: bool) -> Self
```

Whether this is the panel keys go to. Its border shimmers and its title twinkles.

```rust
pub fn padding(mut self, x: usize, y: usize) -> Self
```

Space between the border and the body: `x` columns each side, `y` rows top and
bottom. `(1, 0)` unless set.

```rust
pub fn background(mut self, color: Rgb) -> Self
```

Fill the panel, border and all, with this colour, so it stands apart from what's
around it: a popup over text, say. What's drawn inside keeps any colour of its own.
Without colour it's left as it is.

```rust
use tinsel::Rgb;
use tinsel::app::{Frame, Panel};

let mut frame = Frame::new(10, 3);
frame.render(frame.area(), Panel::new("pick").background(Rgb(44, 33, 51)));
```

```rust
pub fn inner(&self, area: Rect) -> Rect
```

Where the body goes inside a panel drawn at `area`.

## Plot

`tinsel::app::Plot` · struct

```rust
pub struct Plot
```

An area chart of the latest values, as many as fit across, rising berry → blush. Good for
anything over time: latency, requests, temperature.

```rust
use tinsel::app::{Frame, Plot};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(6, 2);
frame.render(frame.area(), Plot::new(&[1.0, 2.0, 3.0, 4.0, 3.0, 2.0]));
assert_eq!(frame.snapshot().plain(), "  ▄█▄\n▄█████");
```

**Methods**

```rust
pub fn new(values: &[f64]) -> Self
```

A chart of these values, oldest first.

```rust
pub fn range(mut self, low: f64, high: f64) -> Self
```

The values at the bottom and top of the chart. Unless set, it runs from 0 (or the
lowest value, if that's negative) to the highest.

```rust
pub fn color(mut self, color: Rgb) -> Self
```

Fill in one colour, fading towards the bottom, instead of the pink ramp.

## Scrollbar

`tinsel::app::Scrollbar` · struct

```rust
pub struct Scrollbar
```

Where a view's rows are among all of them, as a thumb on a track down the first column
of its area: for anything that scrolls. Nothing is drawn while everything fits.

```rust
use tinsel::app::{Frame, Rect, Scrollbar};

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(1, 4);
// rows 4 to 7 of 16 showing
frame.render(frame.area(), Scrollbar::new(4, 16));
assert_eq!(frame.snapshot().plain(), "│\n┃\n│\n│");
```

**Methods**

```rust
pub fn new(top: usize, total: usize) -> Self
```

A scrollbar for a view showing from row `top` of `total`; it shows as many rows as
the area it's drawn in is tall.

```rust
pub fn color(mut self, color: Rgb) -> Self
```

The thumb's colour: tinsel's pink unless changed.

```rust
pub fn track(mut self, track: bool) -> Self
```

Whether the track shows behind the thumb. Yes unless changed; without it the thumb
is a half block, `▐`, which reads as a bar on its own.

## Sparkles

`tinsel::app::Sparkles` · struct

```rust
pub struct Sparkles
```

A scattering of stars twinkling quietly in the background: behind a header, an empty
state, a splash screen. Draw it first, then put things on top.

**Methods**

```rust
pub fn new() -> Self
```

One star for every forty or so cells.

```rust
pub fn wandering(mut self) -> Self
```

Each star fades out completely and comes back somewhere else, instead of twinkling in
one place.

```rust
use tinsel::app::{Frame, Sparkles};

let mut frame = Frame::new(40, 10);
frame.render(frame.area(), Sparkles::new().wandering());
```

```rust
pub fn density(mut self, density: f32) -> Self
```

The share of cells with a star, 0 to 1.

## Stat

`tinsel::app::Stat` · struct

```rust
pub struct Stat
```

One number that matters, with a label above it and a note below. With room for it, the
number is drawn large.

```rust
use tinsel::app::{Frame, Stat};
use tinsel::palette;

tinsel::set_color_mode(tinsel::ColorMode::None);
let mut frame = Frame::new(16, 3);
frame.render(frame.area(), Stat::new("uptime", "99.9%").note(palette::SAGE, "30 days"));
assert_eq!(frame.snapshot().plain(), "uptime\n99.9%\n30 days");
```

**Methods**

```rust
pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self
```

A stat called `label`, showing `value`.

```rust
pub fn note(mut self, color: Rgb, text: impl Into<String>) -> Self
```

A line under the number, in a colour: `▲ 12%` in sage, `3 failing` in red.

## Tabs

`tinsel::app::Tabs` · struct

```rust
pub struct Tabs
```

A row of tabs, the chosen one a soft pink pill. A tab can have a colour of its own, and
a little badge after its title: a count, a mark.

```rust
use tinsel::app::{Frame, Tabs};

tinsel::set_color_mode(tinsel::ColorMode::None);
let tabs = Tabs::new(["overview", "logs"]).selected(1);
let mut frame = Frame::new(30, 1);
frame.render(frame.area(), &tabs);
assert_eq!(frame.snapshot().plain(), " overview   logs");
assert_eq!(tabs.at(frame.area(), 13, 0), Some(1));
```

**Methods**

```rust
pub fn new<S: Into<String>>(titles: impl IntoIterator<Item = S>) -> Self
```

Tabs with these titles.

```rust
pub fn selected(mut self, index: usize) -> Self
```

Which tab is chosen.

```rust
pub fn color(mut self, index: usize, color: Rgb) -> Self
```

Draw tab `index`'s title in `color`, chosen or not: red for one with errors in it, say.
An index past the last tab is ignored.

```rust
use tinsel::app::Tabs;
use tinsel::palette;

let tabs = Tabs::new(["overview", "errors"]).color(1, palette::RED);
```

```rust
pub fn badge(mut self, index: usize, color: Rgb, text: impl Into<String>) -> Self
```

Put a little badge after tab `index`'s title, in `color`: a count, or a mark like
`✔`. Plain text without colour. An index past the last tab is ignored.

```rust
use tinsel::app::{Frame, Tabs};
use tinsel::palette;

tinsel::set_color_mode(tinsel::ColorMode::None);
let tabs = Tabs::new(["build", "deploy"]).badge(0, palette::SAGE, "✔").badge(1, palette::RED, "3");
let mut frame = Frame::new(30, 1);
frame.render(frame.area(), &tabs);
assert_eq!(frame.snapshot().plain(), " build ✔   deploy 3");
assert_eq!(tabs.at(frame.area(), 7, 0), Some(0));
```

```rust
pub fn at(&self, area: Rect, x: usize, y: usize) -> Option<usize>
```

Which tab is at `x`, `y`, if the tabs are drawn at `area`: for clicks.

## Widget

`tinsel::app::Widget` · trait

```rust
pub trait Widget
```

Something that can draw itself into a rectangle of a `Frame`.

Implement it for your own pieces; they can then go anywhere a built-in one can, including
inside a `Panel`.

```rust
use tinsel::app::{Frame, Rect, Widget};
use tinsel::{paint, palette};

struct Heart;

impl Widget for Heart {
    fn render(&self, area: Rect, frame: &mut Frame) {
        let heart = paint(palette::ROSE, "♥");
        for row in 0..area.height {
            frame.print(area, row, &heart.repeat(area.width));
        }
    }
}
```

**Methods**

```rust
fn render(&self, area: Rect, frame: &mut Frame)
```

Draw into `area` of `frame`. `area` is already clipped to the screen. A widget that
moves asks for the frames it needs, with `Frame::animate` or
`Frame::animate_at`, so whoever draws it needn't.

```rust
fn size(&self) -> Option<(usize, usize)>
```

How many columns and rows it would like, as `(width, height)` like every size in tinsel,
if it knows: what a `Modal`
is sized to. `None` unless implemented.

```rust
use tinsel::app::{Panel, Widget};

assert_eq!("two\nlines".size(), Some((5, 2)));
assert_eq!(Panel::new("hi").body("two\nlines").size(), Some((9, 4)));
```

```rust
fn inner(&self, area: Rect) -> Rect
```

Where what it holds goes when it's drawn at `area`: inside a `Panel`'s border and
padding, say. The whole area unless implemented; `Frame::modal` gives it back, so
what's in a modal can be drawn there.

```rust
use tinsel::app::{Panel, Rect, Widget};

let area = Rect::new(0, 0, 20, 5);
assert_eq!(Widget::inner(&Panel::new("hi"), area), Rect::new(2, 1, 16, 3));
assert_eq!("text".inner(area), area);
```

## big

`tinsel::app::big` · fn

```rust
pub fn big(text: &str) -> Block
```

Text three rows tall in thin rounded strokes, for clocks and numbers on a dashboard.
Digits and `. , : - + % /` are drawn large; anything else sits small on the bottom row.

```rust
tinsel::set_color_mode(tinsel::ColorMode::None);
assert_eq!(tinsel::app::big("12").plain(), "╶┐  ╶─╮\n │  ╭─╯\n╶┴╴ ╰─╴");
```

## hints

`tinsel::app::hints` · fn

```rust
pub fn hints(keys: &[(&str, &str)]) -> Line
```

Key hints for a footer: `q quit · ↑↓ move · tab next panel`.

```rust
tinsel::set_color_mode(tinsel::ColorMode::None);
let line = tinsel::app::hints(&[("q", "quit"), ("↑↓", "move")]);
assert_eq!(line.to_string(), "q quit · ↑↓ move");
```

## Event

`tinsel::app::Event` · enum

```rust
pub enum Event
```

Something that happened, handed to `App::event`.

**Variants**

```rust
Key(Key),
```

A key was pressed.

```rust
Paste(String),
```

Text was pasted, all at once, with its line breaks as `\n`: a newline in it is part
of the text, not a press of Enter. `TextInput` takes it as typed. Terminals that can't
say what's pasted (older ones, Windows consoles outside Windows Terminal) send it as
keys instead.

```rust
Mouse(Mouse),
```

The mouse was clicked or scrolled (only if `App::mouse` says so).

```rust
Tick,
```

Time passed: a frame asked for with `Frame::animate`, `Frame::animate_at`,
`Frame::redraw_in` or `Frame::redraw_at` is due, and is drawn straight after.
None arrive while nothing asks.

```rust
Resize
```

The terminal changed size; also sent once as the app starts, before its first frame.

```rust
Wake,
```

A `Waker` was woken: something another thread was doing is ready to look at.

```rust
Focus(bool),
```

The terminal window gained (`true`) or lost (`false`) focus, where the terminal says so:
a chance to pause animations nobody is watching.

## Flow

`tinsel::app::Flow` · enum

```rust
pub enum Flow
```

Whether to keep going after an event.

**Variants**

```rust
Continue,
```

Carry on.

```rust
Quit,
```

Put the terminal back and return from `run`.

## App

`tinsel::app::App` · trait

```rust
pub trait App
```

A full-screen app. Only `view` is needed: by default `q` and Esc quit,
it's drawn when something happens or a frame asks, and the mouse is left alone.

**Methods**

```rust
fn view(&self, frame: &mut Frame)
```

Draw the app as it is now. Called for every frame, so keep it quick, and keep slow
work (fetching, reading files) in `event` or another thread. Anything
that moves asks for the next frame, as [Drawing only when
needed](self#drawing-only-when-needed) says.

```rust
fn event(&mut self, event: &Event) -> Flow
```

Change the app in response to something happening. Ctrl-C comes here too, so the app
can note that it was stopped, and then quits, whatever this returns, unless
`ctrl_c_quits` says otherwise.

```rust
fn fps(&self) -> u32
```

The most frames a second to draw: the rate of `Frame::animate`, and the most any
`Frame::animate_at` gets. `tinsel::fps` unless changed. It's asked
again after every frame, so an app can change it as it goes.

```rust
fn title(&self) -> Option<String>
```

The terminal window's title while the app runs, such as the app's name and what it
shows. Left alone unless changed; the old title comes back when the app quits.

```rust
fn ctrl_c_quits(&self) -> bool
```

Whether Ctrl-C ends the app after `event` has seen it. Yes unless
changed; an app that says no gets it as an ordinary key.

```rust
fn mouse(&self) -> bool
```

Whether to report clicks, drags and the scroll wheel. Off unless changed, since while it's on
the terminal can't select text with the mouse.

## run

`tinsel::app::run` · fn

```rust
pub fn run<A: App>(mut app: A) -> io::Result<A>
```

Run an app full-screen until it quits, then hand it back, so you can read what was chosen.

The screen is put back as it was afterwards, whether the app quits, returns an error or
panics. If the terminal goes away (its window closed), that's the error. When stdout isn't a terminal, one frame is printed instead, at the terminal's
size (or 80×24), after the app is told that size with an `Event::Resize`, and the app is
returned straight away.

One app runs at a time: the screen and keyboard are the whole process's, so `run` called
while another app is running (from its `App::event`, or another thread) is an error, and
the app already running carries on as it was. To show something else for a while, draw
it in the same app, or hand the terminal over with `suspend`.

A panic on another thread while the app runs (in a `Job`'s work, say) leaves the app
running, and its message is kept until the app has left the screen, rather than written
over what the app shows.

**Errors**

An I/O error if the terminal can't be set up or drawn on, or once it goes away (its window
closed), or if another app is already running. The terminal is put back as it was either
way.

## Waker

`tinsel::app::Waker` · struct

```rust
pub struct Waker
```

Wakes the running app from another thread, so it can show what changed straight away,
without polling for it. Make one whenever you like (before `run` is fine), clone it
into every thread that needs it, and call `wake` when there's something
new: the app gets an `Event::Wake` at once, and is drawn again. So an app can sleep
until a result arrives and show it the moment it does.

The wake says only that something happened. To hand the result over, send it down your
own channel and read it on `Event::Wake`:

```rust
use std::sync::mpsc;
use tinsel::app::{self, App, Event, Flow, Frame, Waker};

struct Status {
    results: mpsc::Receiver<String>,
    latest: String,
}

impl App for Status {
    fn view(&self, frame: &mut Frame) {
        frame.print(frame.area(), 0, &self.latest);
    }

    fn event(&mut self, event: &Event) -> Flow {
        if *event == Event::Wake {
            self.latest = self.results.try_iter().last().unwrap_or_default();
        }
        Flow::Continue
    }
}

let (tx, rx) = mpsc::channel();
let waker = Waker::new();
std::thread::spawn(move || {
    tx.send("all healthy".to_string()).unwrap();
    waker.wake();
});
app::run(Status { results: rx, latest: "checking…".into() })?;
```

**Methods**

```rust
pub fn new() -> Self
```

A waker for whichever app is running, or the next one to start.

```rust
pub fn wake(&self)
```

Send the app an `Event::Wake`. Wakes that come quicker than the app handles them
arrive as one; a wake before the app starts arrives as it starts.

## size

`tinsel::app::size` · fn

```rust
pub fn size() -> (usize, usize)
```

The running app's screen, as `(width, height)` in columns and rows: the size its frames
are drawn at, for working out a page's height in `App::event`. Outside a running app,
the terminal's size (80×24 when there's no terminal).

```rust
let (width, height) = tinsel::app::size();
assert!(width > 0 && height > 0);
```

## redraw

`tinsel::app::redraw` · fn

```rust
pub fn redraw()
```

Send the whole screen again with the next frame, colours and cursor included, as after
`suspend`: for a terminal that may have lost track of what it shows. Works from any
thread: called from another, it wakes the app to draw. Does nothing when no app is running.

```rust
tinsel::app::redraw();
```

## time

`tinsel::app::time` · fn

```rust
pub fn time() -> f64
```

Seconds since the app started, on the same clock as `Frame::time`, so something that
happens in `App::event` can be animated from that moment in `App::view` (a
`Modal` rising into place). 0 when no app is running, as in tests.

```rust
assert_eq!(tinsel::app::time(), 0.0);
```

## snapshot

`tinsel::app::snapshot` · fn

```rust
pub fn snapshot<A: App + ?Sized>(app: &A, width: usize, height: usize) -> Block
```

One frame of an app at `width` by `height`, as text: for tests, and for printing what a
dashboard shows into a log.

```rust
use tinsel::app::{App, Frame, Panel};

struct Hello;

impl App for Hello {
    fn view(&self, frame: &mut Frame) {
        frame.render(frame.area(), Panel::new("hello").body("hi there"));
    }
}

tinsel::set_color_mode(tinsel::ColorMode::None);
let shown = tinsel::app::snapshot(&Hello, 16, 3).plain();
assert_eq!(shown.lines().nth(1), Some("│ hi there     │"));
```

## Keymap

`tinsel::app::Keymap` · enum

```rust
pub enum Keymap
```

Which keys move through a list or scroll a view, for `navigate`, `Scroll` and
`GridState`. Both have ↑↓ a step at a time, Page Up/Down a page, Home/End to the ends
and the scroll wheel.

**Variants**

```rust
Arrows,
```

The arrows and the keys above them, and nothing else, so letters are left free for
typing or the app's own shortcuts.

```rust
Vim,
```

Those, and vim's j/k a step at a time and g/G to the ends. The default for a
`Scroll` and a `GridState`.

```rust
Picker,
```

For a list under a box being typed into, as in a file picker: the arrows and the
keys above them, and Ctrl-N, Ctrl-J and Tab a step down, Ctrl-P, Ctrl-K and
Shift-Tab a step up, leaving every letter for the box.

## navigate

`tinsel::app::navigate` · fn

```rust
pub fn navigate(selected: &mut usize, len: usize, event: &Event, keymap: Keymap) -> bool
```

Move a selection through a list of `len` things in response to an event, with the keys
`keymap` names: ↑↓ one at a time, Page Up/Down ten, Home/End to the ends and the scroll
wheel, and j/k and g/G too with `Keymap::Vim`. Returns whether it moved. Made for
`List`, but works for anything with a position.

```rust
use tinsel::app::{Event, Key, Keymap, navigate};

let mut selected = 0;
navigate(&mut selected, 3, &Event::Key(Key::Down), Keymap::Arrows);
navigate(&mut selected, 3, &Event::Key(Key::Char('G')), Keymap::Vim);
assert_eq!(selected, 2);
assert!(!navigate(&mut selected, 3, &Event::Key(Key::Char('k')), Keymap::Arrows));
```

## keep_in_view

`tinsel::app::keep_in_view` · fn

```rust
pub fn keep_in_view(top: usize, selected: usize, rows: usize) -> usize
```

The first row to show of a list scrolled to `top`, `rows` tall, so that `selected` is
in view, scrolling as little as it can: for a list whose selection moves by more than
the keys `navigate` knows, or that changes under it as a filter is typed.

```rust
use tinsel::app::keep_in_view;

assert_eq!(keep_in_view(0, 3, 10), 0, "already showing");
assert_eq!(keep_in_view(0, 12, 10), 3, "just far enough to show it at the bottom");
assert_eq!(keep_in_view(8, 2, 10), 2, "or at the top");
```

## suspend

`tinsel::app::suspend` · fn

```rust
pub fn suspend<T>(f: impl FnOnce() -> T) -> io::Result<T>
```

Leave the full-screen app for a moment, to run something that needs the terminal as it
normally is: a command that asks for a password, opens an editor, or prints a sign-in
link and waits. The screen and keyboard are handed back, `f` runs, and then the app takes
them again and redraws from scratch. Call it from `App::event`.

While `f` runs, Ctrl-C goes to the commands it starts, not the app. If `f` panics, the
terminal is left as it should be. If the app can't take the terminal back afterwards,
that's the error, and the app should quit. Outside a running app (in tests, say) `f` simply runs.

```rust
use std::process::Command;
use tinsel::app::{self, App, Event, Flow, Frame, Key};

struct Account {
    signed_in: bool,
}

impl App for Account {
    fn view(&self, frame: &mut Frame) {
        let state = if self.signed_in { "signed in" } else { "s to sign in" };
        frame.print(frame.area(), 0, state);
    }

    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(Key::Char('s')) => {
                let login = app::suspend(|| Command::new("my-cli").arg("login").status());
                self.signed_in = matches!(login, Ok(Ok(status)) if status.success());
            }
            Event::Key(Key::Char('q')) => return Flow::Quit,
            _ => {}
        }
        Flow::Continue
    }
}
```

**Errors**

An I/O error if the app can't take the terminal back afterwards.

# tinsel::clipboard

The system clipboard: copying text to it and pasting from it, by whatever this computer
offers.

`copy` uses the system's own tool where there is one (`pbcopy` on a Mac, `wl-copy` on
Wayland, `xclip` or `xsel` on X, PowerShell on Windows and WSL), and otherwise asks the
terminal to copy with an OSC 52 escape, which works over ssh too. Over ssh the terminal is
asked first, since a tool on the far end would copy to the far end's clipboard.
`paste` uses the tools alone, as terminals rarely answer a request to read the
clipboard, and gives up on one that takes more than a couple of seconds.

```rust
if tinsel::clipboard::copy("https://example.com/build/412") {
    tinsel::success("link copied");
}
let pasted = tinsel::clipboard::paste().unwrap_or_default();
```

## copy

`tinsel::clipboard::copy` · fn

```rust
pub fn copy(text: &str) -> bool
```

Copy `text` to the clipboard. Gives whether it got there: a tool said it copied, or the
terminal was asked to (which it may decline, unseen). False when there's neither a tool
nor a terminal.

## paste

`tinsel::clipboard::paste` · fn

```rust
pub fn paste() -> Option<String>
```

The text on the clipboard, if a tool can read it, with Windows line endings made `\n`.
A tool that hasn't answered in two seconds is stopped, and the next one tried.

# tinsel::format

Numbers and times the way people say them.

## bytes

`tinsel::format::bytes` · fn

```rust
pub fn bytes(n: u64) -> String
```

`512 B`, `1.1 GB`, `34 MB` (powers of 1000, as disks and networks count).

## duration

`tinsel::format::duration` · fn

```rust
pub fn duration(d: Duration) -> String
```

`340ms`, `4.2s`, `3m07s`, `1h02m`.

## count

`tinsel::format::count` · fn

```rust
pub fn count(n: u64) -> String
```

`999`, `1.2k`, `34k`, `5.6M`.

## plural

`tinsel::format::plural` · fn

```rust
pub fn plural(n: u64, word: &str) -> String
```

`1 file`, `3 files`. For irregular words pass the plural: `plural_with(2, "child", "children")`.

## plural_with

`tinsel::format::plural_with` · fn

```rust
pub fn plural_with(n: u64, one: &str, many: &str) -> String
```

`1 child`, `3 children`: like `plural`, for words that don't just add an s.

## ago

`tinsel::format::ago` · fn

```rust
pub fn ago(d: Duration) -> String
```

`just now`, `5m ago`, `3h ago`, `2d ago`.

## ago_long

`tinsel::format::ago_long` · fn

```rust
pub fn ago_long(d: Duration) -> String
```

`3 minutes ago`, `1 hour ago`, `2 weeks ago`: like `ago`, in words, for a sentence
rather than a column. Under a minute is `just now`.

```rust
use std::time::Duration;

assert_eq!(tinsel::format::ago_long(Duration::from_secs(7_200)), "2 hours ago");
```

## grouped

`tinsel::format::grouped` · fn

```rust
pub fn grouped(n: u64) -> String
```

`1,234,567`: a whole number with its thousands marked off, for counts read in full.

```rust
assert_eq!(tinsel::format::grouped(1_234_567), "1,234,567");
assert_eq!(tinsel::format::grouped(999), "999");
```

## local_clock

`tinsel::format::local_clock` · fn

```rust
pub fn local_clock() -> String
```

The time of day on this computer's clock, in its time zone, as `21:04`: for a clock in
a status bar. Where the time zone can't be found (outside Unix), it's UTC.

```rust
let now = tinsel::format::local_clock();
assert_eq!(now.len(), 5);
assert_eq!(&now[2..3], ":");
```

# tinsel::highlight

Just enough syntax colouring for code excerpts: keywords, strings, numbers, comments, and
keys in config files. One line at a time, so a string or comment that spans lines is only
coloured on its first.

## IntoLang

`tinsel::highlight::IntoLang` · trait

```rust
pub trait IntoLang: sealed::Sealed
```

Anything `Code::lang` accepts: a `Lang`, or a language's name. Sealed: tinsel decides
what counts, so it can add more without breaking anything.

**Methods**

```rust
fn into_lang(self) -> Option<Lang>
```

The language, if it's one tinsel knows.

## Lang

`tinsel::highlight::Lang` · enum

```rust
pub enum Lang
```

A language `Code` excerpts know how to colour.

**Variants**

```rust
Rust,
```

Rust.

```rust
Toml,
```

TOML.

```rust
Json,
```

JSON.

```rust
Yaml,
```

YAML.

```rust
Shell,
```

Shell scripts and commands.

```rust
Python,
```

Python.

```rust
JavaScript,
```

JavaScript and TypeScript.

**Methods**

```rust
pub fn from_name(name: &str) -> Option<Lang>
```

The language with this name or file extension (`rust` or `rs`, `toml`, `json`,
`yaml` or `yml`, `sh`/`bash`/`zsh`/`shell`, `python` or `py`, `js`/`ts`/`javascript`).

```rust
pub fn from_path(path: &str) -> Option<Lang>
```

The language a file is written in, from its extension.

```rust
pub fn highlight(self, line: &str) -> String
```

One line of source coloured as this language: keywords, strings, numbers, comments,
and keys in config files. Each line is coloured on its own, so a string or comment that
spans lines is only coloured on its first. Without colour it comes back as it was.

```rust
use tinsel::Lang;

let line = Lang::Rust.highlight("let x = 1; // one");
assert_eq!(tinsel::text::strip_ansi(&line), "let x = 1; // one");
```

# tinsel::image

Pictures shown at the terminal's full resolution, through kitty's graphics protocol or
iTerm2's inline images. A terminal that speaks neither can't show them at all: there's
no blocky stand-in, so an app asks `support` first and says so instead.

An app puts a picture in an area of its frame with `Frame::image`; the run loop sends
it, fitted to the area and keeping its shape, and takes it away once a frame no longer
has it. A picture sits above the text, so nothing else should be drawn where it goes.

iTerm2 reads PNG, JPEG and GIF files itself. kitty's protocol takes PNG files as they
are, and JPEG and GIF as pixels, which the `decode` feature decodes in tinsel, shrunk to
no bigger than the screen.

`Frame::image`: crate::app::Frame::image

## Image

`tinsel::image::Image` · struct

```rust
pub struct Image
```

An image file's bytes, with its format and size read from its header. The terminal
decodes it, except where kitty is sent a JPEG or GIF, which the `decode` feature turns
into pixels first.

**Methods**

```rust
pub fn new(bytes: Vec<u8>) -> Result<Image, String>
```

The image in `bytes`, or why it isn't one tinsel knows.

**Errors**

Why `bytes` aren't a PNG, JPEG or GIF tinsel can read the size of, in words.

```rust
use tinsel::image::{Format, Image};

let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
png.extend([0, 0, 0, 3, 0, 0, 0, 2]);
let image = Image::new(png).unwrap();
assert_eq!((image.format(), image.width(), image.height()), (Format::Png, 3, 2));
assert!(Image::new(b"hello".to_vec()).is_err());
```

```rust
pub fn pending(&self) -> bool
```

Whether it's still being decoded on another thread, as a big JPEG or GIF is the first
time kitty is to show it. Until it's done the run loop holds it back, so an app can
show something else there meanwhile; something that moves, so the next frame comes.

```rust
pub fn looks_like(bytes: &[u8]) -> bool
```

Whether `bytes` start the way an image tinsel knows does, as a cheap test before
reading all of a file.

```rust
pub fn format(&self) -> Format
```

What kind of file it is.

```rust
pub fn width(&self) -> u32
```

How wide it is, in pixels, turned the way it's meant to be seen.

```rust
pub fn height(&self) -> u32
```

How tall it is, in pixels, turned the way it's meant to be seen.

## Format

`tinsel::image::Format` · enum

```rust
pub enum Format
```

The kinds of image file `Image::new` knows.

**Variants**

```rust
Png,
```

Portable Network Graphics.

```rust
Jpeg,
```

JPEG.

```rust
Gif,
```

GIF, of which terminals show the first frame.

**Methods**

```rust
pub fn name(self) -> &'static str
```

Its usual name, as in PNG.

## Protocol

`tinsel::image::Protocol` · enum

```rust
pub enum Protocol
```

How the terminal shows pictures, if it does.

**Variants**

```rust
Kitty,
```

kitty's graphics protocol: kitty, Ghostty and Konsole. It takes PNG as it is, and
JPEG and GIF as pixels with the `decode` feature.

```rust
Iterm,
```

iTerm2's inline images: iTerm2 and WezTerm. The terminal reads PNG, JPEG and GIF.

**Methods**

```rust
pub fn shows(self, format: Format) -> bool
```

Whether this protocol can show `format` without tinsel decoding it.

## Support

`tinsel::image::Support` · enum

```rust
pub enum Support
```

Whether, and how, this terminal can show pictures.

**Variants**

```rust
Yes(Protocol),
```

It can, this way.

```rust
No,
```

It can't: no protocol tinsel knows, or a multiplexer like tmux in the way.

**Methods**

```rust
pub fn check(self, image: &Image) -> Result<Protocol, String>
```

Whether it can show `image`, or why not, in words for the app to show instead.

**Errors**

Why not: the terminal shows no pictures, or not ones in this format.

```rust
use tinsel::image::{Protocol, Support};

assert!(Support::No.check_format(tinsel::image::Format::Png).unwrap_err().contains("can't show"));
assert!(Support::Yes(Protocol::Iterm).check_format(tinsel::image::Format::Jpeg).is_ok());
```

```rust
pub fn check_format(self, format: Format) -> Result<Protocol, String>
```

`check`, for a format.

**Errors**

As `check`'s.

## support

`tinsel::image::support` · fn

```rust
pub fn support() -> Support
```

Whether, and how, the terminal tinsel runs in can show pictures, going by its environment.
`TINSEL_IMAGES=kitty`, `iterm` or `none` decides it instead.

## with_support

`tinsel::image::with_support` · fn

```rust
pub fn with_support<T>(s: Support, f: impl FnOnce() -> T) -> T
```

Runs `f` as if the terminal's support were `s`, for tests.

# tinsel::markdown

Markdown drawn for the terminal, in tinsel's colours: for changelogs, long help text and
what's-new notes. `markdown` wraps it to the terminal, and
`wrapped` to a width of your own.

## markdown

`tinsel::markdown::markdown` · fn

```rust
pub fn markdown(src: &str) -> Block
```

Markdown as styled lines, wrapped to the terminal: headings, paragraphs, lists (nested,
numbered, and task lists), quotes, rules, tables, and fenced code blocks coloured by
language. Inline, it understands `**bold**`, `*italic*`, `` `code` `` and `links`.

```rust
tinsel::markdown("## What's new\n\n- **Faster** builds\n- A `--json` flag").print();
```

## wrapped

`tinsel::markdown::wrapped` · fn

```rust
pub fn wrapped(src: &str, width: usize) -> Block
```

Like `markdown`, wrapped to `width` columns rather than the
terminal's: for a pane beside something else.

```rust
let narrow = tinsel::markdown::wrapped("A paragraph long enough to wrap onto a second line.", 24);
assert!(narrow.lines().len() > 1);
```

# tinsel::prompt

Questions: yes/no, a line of text, a password, a number, a date, one of a list, several
of a list, or a longer answer in the user's editor.

Drawn on stderr, so stdout stays clean for piping. Each question collapses to a single
`●  question  answer` line once answered.

Every question returns a `Result`, whose error says what happened in a way you can
match on: `Error::Cancelled` when someone presses Ctrl-C or Esc, `Error::NotATerminal`
when there's no one to ask (no terminal on stdin, or none on stderr to show the question
on). Without a terminal on stdin, `confirm` and the text questions (`input`, `number`,
`date`) read a line, so scripts can still drive them (`echo yes | my-cli`); an answer
read that way that doesn't make sense or fails validation is an `Error::Invalid`.

```rust
use tinsel::prompt;

fn main() -> Result<(), prompt::Error> {
    let name = prompt::input("Project name", Some("my-app"))?;
    let target = prompt::select("Deploy to", &["staging", "production"])?;
    if prompt::confirm("Go?", true)? {
        tinsel::success(&format!("deploying {name} to target #{target}"));
    }
    Ok(())
}
```

For more control over a text question (a placeholder, validation), build an `Input`:

```rust
use tinsel::prompt::Input;

let port = Input::new("Port")
    .placeholder("8080")
    .validate(|s| match s.parse::<u16>() {
        Ok(p) if p >= 1024 => Ok(()),
        _ => Err("pick a port from 1024 to 65535".into()),
    })
    .ask()?;
```

Lists have builders too: `Select` and `MultiSelect` add groups under headings,
options that can't be picked (and why), a hint and validation, and `Dropdown` is for
long lists like versions, with notes and badges and room to type an answer of your own.
`Select` and `MultiSelect` answer with the numbers of the options chosen, so the same
label can appear twice; `Dropdown` answers with the text, since an answer typed in
has no number. All three start on the options given to `selected`.

**Testing a flow of questions**

`script` answers questions from a list of keys instead of the keyboard, on the current
thread, for as long as the `Script` it returns lives. Questions then draw nothing, and
`Script::answered` says what each one ended on, so a test can run the real code that
asks them.

```rust
use tinsel::app::Key;
use tinsel::prompt::{self, Select};

let keys = prompt::script([Key::Down, Key::Enter]);
let picked = Select::new("Deploy to").options(["staging", "production"]).ask()?;
assert_eq!(picked, 1);
assert_eq!(keys.answered(), ["●  Deploy to  production"]);
```

**Going back**

A `Form` asks several questions as one, going back and forth between them, and shows
all the answers at the end to change any of them.

For questions asked one after another, `.back()` lets ← go back: the question returns
`Error::Back` and you ask the one before it again. → always answers, as Enter does (in
typed answers, from the end of the line), so a flow can be stepped through with the arrows.

```rust
use tinsel::prompt::{Error, Input, Select};

let (mut name, mut target) = (String::new(), 0);
let mut step = 0;
while step < 2 {
    let asked = match step {
        0 => Input::new("Project name").ask().map(|n| name = n),
        _ => Select::new("Deploy to").options(["staging", "production"]).back().ask().map(|t| target = t),
    };
    match asked {
        Ok(()) => step += 1,
        Err(Error::Back) => step -= 1,
        Err(e) => return Err(e),
    }
}
```

## Error

`tinsel::prompt::Error` · enum

```rust
pub enum Error
```

Why a question didn't get an answer.

**Variants**

```rust
Cancelled,
```

The person pressed Ctrl-C or Esc: they want to stop altogether. Exiting with status
130 is the convention.

```rust
Back,
```

The person pressed ← to go back to the question before, to change their answer. Only
questions asked with `.back()` (on `Confirm`, `Input`, `Select`, `MultiSelect`
and `Dropdown`) return it, so the caller can ask the previous question again; the
question is cleared from the screen. Unlike `Error::Cancelled`, the person wants to
carry on.

```rust
NotATerminal,
```

There's no terminal to ask on (stdin is a pipe or a file, or stderr, where questions
are drawn, isn't a terminal), and this question has no sensible non-interactive
answer.

```rust
NoOptions,
```

A list question was given nothing to choose from.

```rust
Invalid(String),
```

An answer read without a terminal (from a pipe or file) didn't pass validation. On a
terminal the person is simply asked again.

```rust
Io(io::Error),
```

Reading the keyboard, setting up the terminal or running the editor failed.

## Result

`tinsel::prompt::Result` · type

```rust
pub type Result<T> = std::result::Result<T, Error>
```

What every question returns.

## Date

`tinsel::prompt::Date` · struct

```rust
pub struct Date
```

A calendar date, as returned by `date`. Shown and parsed as `YYYY-MM-DD`.

Always a day that exists: make one with `Date::new`, which turns down a 13th month or a
February 30th, or parse one from text.

```rust
use tinsel::prompt::Date;

let pi_day = Date::new(2026, 3, 14).unwrap();
assert_eq!((pi_day.year(), pi_day.month(), pi_day.day()), (2026, 3, 14));
assert_eq!("2026-03-14".parse(), Ok(pi_day));
assert_eq!(Date::new(2026, 2, 30), None);
```

**Methods**

```rust
pub fn new(year: i32, month: u8, day: u8) -> Option<Date>
```

A date, if it exists (no February 30th).

```rust
pub fn year(self) -> i32
```

The year, e.g. 2026.

```rust
pub fn month(self) -> u8
```

The month, 1-12.

```rust
pub fn day(self) -> u8
```

The day of the month, from 1.

```rust
pub fn today() -> Date
```

Today, in the local time zone where that can be found (Unix), otherwise in UTC.

## date

`tinsel::prompt::date` · fn

```rust
pub fn date(question: &str, default: Option<Date>) -> Result<Date>
```

A date picked from a calendar: ←→ move a day, ↑↓ a week, PgUp/PgDn (or `` ``) a month,
`t` jumps to today. Starts on `default`, or today.

```rust
let when = tinsel::prompt::date("Release on", None)?;
println!("{when}");   // 2026-11-12
```

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` when there's no terminal to ask
on and no line to read, `Error::Invalid` when a line read from a pipe isn't an answer it
takes, and `Error::Io` when the terminal can't be used.

## editor

`tinsel::prompt::editor` · fn

```rust
pub fn editor(question: &str, initial: &str) -> Result<String>
```

A longer answer written in the user's editor (`$VISUAL`, then `$EDITOR`, then `vi`, or
Notepad on Windows), starting from `initial`. Returns what was saved. For commit messages,
release notes, anything more than a line.

```rust
let notes = tinsel::prompt::editor("Release notes", "## What's new\n\n")?;
```

**Errors**

`Error::NotATerminal` without a terminal (or while a `script` answers the questions,
since an editor can't be scripted), `Error::Cancelled` if the editor exits with an error
(`:cq` in vim), and `Error::Io` if it can't be started or its file read.

## Question

`tinsel::prompt::Question` · enum

```rust
pub enum Question<'a>
```

One question in a `Form`: any of the builders, which turn into one with `into()`.

**Variants**

```rust
Confirm(Confirm),
```

Yes or no.

```rust
Input(Input<'a>),
```

A line of text.

```rust
Select(Select<'a>),
```

One of a list.

```rust
MultiSelect(MultiSelect<'a>),
```

Several of a list.

```rust
Dropdown(Dropdown<'a>),
```

One of a long list, or an answer typed in.

## Form

`tinsel::prompt::Form` · struct

```rust
pub struct Form<'a>
```

Questions asked one after another as a group: ← goes back to the one before (with its
answer still there to change), and once they're all answered, a last question shows the
answers and offers to change any of them before going on.

Each question is given a name to find its answer by. Without a terminal the questions are
answered as they would be on their own (lines read from stdin, or what's `selected`), and
there's nothing to look over.

```rust
use tinsel::prompt::{Confirm, Form, Input, MultiSelect, Select};

let answers = Form::new()
    .question("name", Input::new("Project name").default("my-app"))
    .question("target", Select::new("Deploy to").options(["staging", "production"]))
    .question("services", MultiSelect::new("Services").options(["api", "web", "worker"]))
    .question("tag", Confirm::new("Tag the release?").default(true))
    .ask()?;

let name = answers.text("name");
let target = answers.choice("target");
let services = answers.choices("services");
let tag = answers.yes("tag");
```

```text
  ●  Project name  my-app
  ●  Deploy to  production
  ●  Services  api, worker
  ●  Tag the release?  yes
  ?  All good?
    › yes, carry on
    change
      Project name                     my-app
      Deploy to                    production
      …
```

**Methods**

```rust
pub fn new() -> Self
```

A form with no questions yet.

```rust
pub fn question(mut self, name: impl Into<String>, question: impl Into<Question<'a>>) -> Self
```

Add a question, and the name its answer is found by in the `Answers`. Its own
`.back()` needn't be set: every question after the first can go back.

```rust
pub fn ask(mut self) -> Result<Answers>
```

Ask the questions, then offer to change any answer before returning them all.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc at any question, `Error::NoOptions` when a list has
nothing to choose, and otherwise whatever a question asked on its own would return
without a terminal (`Error::NotATerminal`, `Error::Invalid`) or when the terminal can't
be used (`Error::Io`).

## Answers

`tinsel::prompt::Answers` · struct

```rust
pub struct Answers
```

The answers to a `Form`, by the name each question was given.

Asking for a name the form doesn't have, or for the wrong kind of answer (`text` for a
yes or no question, say), panics: it's a mistake in the code, not something the person
answering could have done.

**Methods**

```rust
pub fn yes(&self, name: &str) -> bool
```

A `Confirm`'s answer.

```rust
pub fn text(&self, name: &str) -> &str
```

An `Input`'s or a `Dropdown`'s answer.

```rust
pub fn choice(&self, name: &str) -> usize
```

A `Select`'s answer: the number of the option chosen.

```rust
pub fn choices(&self, name: &str) -> &[usize]
```

A `MultiSelect`'s answer: the numbers of the options ticked, in order.

## confirm

`tinsel::prompt::confirm` · fn

```rust
pub fn confirm(question: &str, default: bool) -> Result<bool>
```

Yes or no. Enter takes the default. `Confirm` has more options.

Without a terminal it reads a line instead: `y`, `yes`, `n` or `no` in any case, and an
empty line (or the end of the input) takes the default. Anything else is an
`Error::Invalid`, unless the question is `lenient`.

```rust
if tinsel::prompt::confirm("Deploy to production?", false)? {
    tinsel::success("deploying");
}
```

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` when there's no terminal to ask
on and no line to read, `Error::Invalid` when a line read from a pipe isn't an answer it
takes, and `Error::Io` when the terminal can't be used.

## Confirm

`tinsel::prompt::Confirm` · struct

```rust
pub struct Confirm
```

A yes or no question with options. `confirm` is the shortcut. Answers no unless given
a `default`.

```rust
use tinsel::prompt::Confirm;

let tag = Confirm::new("Tag the release?").default(true).back().ask()?;
```

**Methods**

```rust
pub fn new(question: impl Into<String>) -> Self
```

A yes or no question.

```rust
pub fn default(mut self, yes: bool) -> Self
```

The answer when Enter or → is pressed (or an empty line is piped in).

```rust
pub fn back(mut self) -> Self
```

Let ← go back to the question before, as `Error::Back`.

```rust
pub fn end_of_input(mut self, yes: bool) -> Self
```

The answer when there's no terminal and the piped input has already ended, so a
script that runs out of answers can say no. The `default` unless set.

```rust
pub fn lenient(mut self) -> Self
```

Read a piped answer the way shell scripts often do: anything starting with `y` or `Y`
is yes, anything else is no, and an empty line takes the `default`.
Without it, only `y`, `yes`, `n` and `no` (in any case) are answers, and anything else
is an `Error::Invalid`. Keys typed at a terminal work the same either way.

```rust
use tinsel::prompt::Confirm;

// `echo yep | my-cli` says yes, `echo nope | my-cli` says no
let go = Confirm::new("Go?").lenient().ask()?;
```

```rust
pub fn ask(self) -> Result<bool>
```

Ask the question. Without a terminal, reads the answer from stdin as `confirm` does.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::Back` on ← when asked with `.back()`,
`Error::NotATerminal` when there's no terminal to ask on and no line to read,
`Error::Invalid` when a line read from a pipe isn't an answer it takes, and `Error::Io` when
the terminal can't be used.

## Input

`tinsel::prompt::Input` · struct

```rust
pub struct Input<'a>
```

A text question with options: a default, a placeholder, hidden input, and validation that
asks again until the answer passes. `input` and `password` are shortcuts for the
common cases.

The answer can be edited as you'd expect: ←→ move, Home/End (or Ctrl-A/Ctrl-E) jump,
Ctrl-U clears, Ctrl-W deletes a word. With `suggestions`,
`complete` or `paths`, the best match shows dim
after the cursor; Tab takes it, and pressing Tab again moves through the others.

**Methods**

```rust
pub fn new(question: impl Into<String>) -> Self
```

A text question.

```rust
pub fn suggestions<S: Into<String>>(self, options: impl IntoIterator<Item = S>) -> Self
```

Offer these answers as you type: those starting with what's typed (ignoring case).

```rust
let region = tinsel::prompt::Input::new("Region")
    .suggestions(["eu-west-1", "eu-west-2", "us-east-1"])
    .ask()?;
```

```rust
pub fn complete(mut self, complete: impl Fn(&str) -> Vec<String> + 'a) -> Self
```

Offer whatever `complete` returns for the text typed so far, best first. Each
suggestion is a whole answer, not just the rest of it.

```rust
pub fn paths(self) -> Self
```

Complete file and folder paths, like a shell: Tab fills in names from the folder
typed so far (`~` is your home folder), and folders end in `/`.

```rust
pub fn default(mut self, value: impl Into<String>) -> Self
```

The answer if Enter is pressed without typing anything, shown dim until you type.

```rust
pub fn initial(mut self, text: impl Into<String>) -> Self
```

Text already typed in when the question opens, with the cursor at its end, to edit or
take as it is: the current value, say. Without a terminal, an empty line takes it.

```rust
let name = tinsel::prompt::Input::new("Name").initial("api-gateway").ask()?;
```

```rust
pub fn placeholder(mut self, text: impl Into<String>) -> Self
```

Dim example text shown until you type. Unlike a default, it's never the answer.

```rust
pub fn password(mut self) -> Self
```

Show what's typed as dots. A password is never read from a pipe: without a terminal,
`ask` is an `Error::NotATerminal`. A `default` shows
as dots too, and how long it is stays hidden.

```rust
pub fn validate(mut self, check: impl Fn(&str) -> std::result::Result<(), String> + 'a) -> Self
```

Check the answer when Enter is pressed. An `Err` is shown under the question, which
is asked again. The default is checked too, if it's used.

```rust
pub fn back(mut self) -> Self
```

Let ← go back to the question before, as `Error::Back`, when the cursor is at the
start of the line (so ← still moves through what's typed).

```rust
use tinsel::prompt::{Error, Input};

match Input::new("Project name").back().ask() {
    Ok(name) => println!("{name}"),
    Err(Error::Back) => { /* ask the question before again */ }
    Err(e) => return Err(e),
}
```

```rust
pub fn ask(self) -> Result<String>
```

Ask the question.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::Back` on ← when asked with `.back()`,
`Error::NotATerminal` when there's no terminal to ask on and no line to read,
`Error::Invalid` when a line read from a pipe isn't an answer it takes, and `Error::Io` when
the terminal can't be used. A `password` is never read from a pipe, so
without a terminal it's an `Error::NotATerminal`.

## input

`tinsel::prompt::input` · fn

```rust
pub fn input(question: &str, default: Option<&str>) -> Result<String>
```

A line of text. An empty answer takes the default, shown dim until you type.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` when there's no terminal to ask
on and no line to read, `Error::Invalid` when a line read from a pipe isn't an answer it
takes, and `Error::Io` when the terminal can't be used.

## password

`tinsel::prompt::password` · fn

```rust
pub fn password(question: &str) -> Result<String>
```

A line of text shown as dots.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` without a terminal, since a
password is never read from a pipe, and `Error::Io` when the terminal can't be used.

## confirm_typing

`tinsel::prompt::confirm_typing` · fn

```rust
pub fn confirm_typing(question: &str, expected: &str) -> Result<()>
```

Make someone type `expected` exactly before going on: for things that can't be undone,
like deleting an app. Returns once they have; Esc or Ctrl-C is `Error::Cancelled`.

```rust
tinsel::prompt::confirm_typing("Delete my-app and all its data?", "my-app")?;
```

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` when there's no terminal to ask
on and no line to read, `Error::Invalid` when a line read from a pipe isn't an answer it
takes, and `Error::Io` when the terminal can't be used.

## number

`tinsel::prompt::number` · fn

```rust
pub fn number<T>(question: &str, default: Option<T>) -> Result<T>
where
    T: FromStr + fmt::Display,
```

A number of any type that parses from text (`u16`, `f64`, …). Anything that doesn't parse
is refused and asked again.

```rust
let replicas: u32 = tinsel::prompt::number("Replicas", Some(3))?;
```

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NotATerminal` when there's no terminal to ask
on and no line to read, `Error::Invalid` when a line read from a pipe isn't an answer it
takes, and `Error::Io` when the terminal can't be used.

## select

`tinsel::prompt::select` · fn

```rust
pub fn select(question: &str, options: &[&str]) -> Result<usize>
```

One of a list, moved through with ↑↓ (or j/k). Returns the chosen index. In lists longer
than eight, typing narrows the list to options containing those letters in order, and
Esc clears what's typed before it cancels. `Select` has more options.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NoOptions` when there's nothing to choose,
`Error::NotATerminal` when there's no terminal to ask on, and `Error::Io` when the
terminal can't be used.

## Select

`tinsel::prompt::Select` · struct

```rust
pub struct Select<'a>
```

One of a list, with options: where to start, groups under headings, options that can't be
picked (and why), notes and badges, a hint, validation and going back. `select` is the
shortcut. Returns the number of the chosen option, counting from 0 in the order they
were added.

↑↓ (or j/k) move, PgUp/PgDn move a page. Lists longer than `rows` filter
as you type, and Esc clears the filter before it cancels.

Without a terminal, the `selected` option is the answer, if there is one;
otherwise it's `Error::NotATerminal`.

```rust
use tinsel::prompt::Select;

let target = Select::new("Deploy to")
    .hint("production needs a second approval")
    .heading("Shared")
    .option("staging")
    .option("production")
    .disabled("legacy", "retired")
    .heading("Yours")
    .option("preview")
    .selected(0)
    .ask()?;
```

**Methods**

```rust
list_builders!(Select)
```

Add an option. Options are numbered from 0 in the order they're added,
disabled ones included, and the answer is given as those numbers.

```rust
list_builders!(Select)
```

Add several options.
```rust
let answer = tinsel::prompt::Select::new("Deploy to").options(["staging", "production"]).ask()?;
```

```rust
list_builders!(Select)
```

Add an option that can't be picked, drawn dim with the reason after it. The
cursor skips over it, but it still takes up a number.
```rust
let answer = tinsel::prompt::Select::new("Deploy to")
    .option("staging")
    .disabled("production", "frozen until Monday")
    .ask()?;
```

```rust
list_builders!(Select)
```

Start a group: a heading over the options added after it. It's not an option,
so it can't be picked and takes up no number.
```rust
let answer = tinsel::prompt::Select::new("Service")
    .heading("Frontend")
    .options(["web", "docs"])
    .heading("Backend")
    .options(["api", "worker"])
    .ask()?;
```

```rust
list_builders!(Select)
```

Muted text lined up on the right of the option added last, such as when it
was made.
```rust
let answer = tinsel::prompt::Select::new("Version")
    .option("v1.2.0").note("3 days ago").badge("latest")
    .option("v1.1.0").note("2 weeks ago")
    .ask()?;
```

```rust
list_builders!(Select)
```

A small badge after the option added last, such as `latest` or `current`.
Call it again for another.

```rust
list_builders!(Select)
```

Draw the option added last quieter than the rest, for ones that are less
likely to be wanted (a pre-release, say). It can still be picked.

```rust
list_builders!(Select)
```

A muted line under the question, to say more about what's being asked.

```rust
list_builders!(Select)
```

Let ← go back to the question before, as `Error::Back`.

```rust
list_builders!(Select)
```

How many rows to show at once (8 unless set); longer lists scroll, with a
count of what's above and below.

```rust
pub fn new(question: impl Into<String>) -> Self
```

A question with one answer from a list.

```rust
pub fn selected(mut self, index: usize) -> Self
```

Start on this option (by number), and answer with it when there's no terminal.

```rust
pub fn validate(mut self, check: impl Fn(usize) -> std::result::Result<(), String> + 'a) -> Self
```

Check the choice when Enter is pressed. An `Err` is shown under the question, which
stays open.

```rust
let size = tinsel::prompt::Select::new("Size")
    .options(["small", "large"])
    .validate(|i| if i == 1 { Err("large needs a paid plan".into()) } else { Ok(()) })
    .ask()?;
```

```rust
pub fn ask(self) -> Result<usize>
```

Ask the question. The answer is the chosen option's number, counted from 0 in the
order options were added (headings don't count).

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::Back` on ← when asked with `.back()`,
`Error::NoOptions` when there's nothing to choose, `Error::NotATerminal` when there's no
terminal to ask on (unless an option is `selected`), and `Error::Io` when
the terminal can't be used.

## multiselect

`tinsel::prompt::multiselect` · fn

```rust
pub fn multiselect(question: &str, options: &[&str], selected: &[usize]) -> Result<Vec<usize>>
```

Several of a list: space toggles, `a` toggles all, enter confirms. Returns the chosen
indices in order. In lists longer than eight, typing filters and Ctrl-A toggles all of
what's shown. `MultiSelect` has more options.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::NoOptions` when there's nothing to choose,
`Error::NotATerminal` when there's no terminal to ask on, and `Error::Io` when the
terminal can't be used.

## MultiSelect

`tinsel::prompt::MultiSelect` · struct

```rust
pub struct MultiSelect<'a>
```

Several of a list, with options: some ticked to start with, groups under headings,
options that can't be picked (and why), notes and badges, a hint, validation and going
back. `multiselect` is the shortcut. Returns the numbers of the chosen options in
order, counting from 0 in the order they were added.

Space toggles, `a` toggles all (Ctrl-A, all of what's shown, in lists that filter as you
type), Enter is done.

Without a terminal, the `selected` options are the answer, if any were
given; otherwise it's `Error::NotATerminal`.

```rust
use tinsel::prompt::MultiSelect;

let services = MultiSelect::new("Restart")
    .options(["api", "web", "worker"])
    .selected(&[0])
    .validate(|picked| if picked.is_empty() { Err("pick at least one".into()) } else { Ok(()) })
    .ask()?;
```

**Methods**

```rust
list_builders!(MultiSelect)
```

Add an option. Options are numbered from 0 in the order they're added,
disabled ones included, and the answer is given as those numbers.

```rust
list_builders!(MultiSelect)
```

Add several options.
```rust
let answer = tinsel::prompt::MultiSelect::new("Deploy to").options(["staging", "production"]).ask()?;
```

```rust
list_builders!(MultiSelect)
```

Add an option that can't be picked, drawn dim with the reason after it. The
cursor skips over it, but it still takes up a number.
```rust
let answer = tinsel::prompt::MultiSelect::new("Deploy to")
    .option("staging")
    .disabled("production", "frozen until Monday")
    .ask()?;
```

```rust
list_builders!(MultiSelect)
```

Start a group: a heading over the options added after it. It's not an option,
so it can't be picked and takes up no number.
```rust
let answer = tinsel::prompt::MultiSelect::new("Service")
    .heading("Frontend")
    .options(["web", "docs"])
    .heading("Backend")
    .options(["api", "worker"])
    .ask()?;
```

```rust
list_builders!(MultiSelect)
```

Muted text lined up on the right of the option added last, such as when it
was made.
```rust
let answer = tinsel::prompt::MultiSelect::new("Version")
    .option("v1.2.0").note("3 days ago").badge("latest")
    .option("v1.1.0").note("2 weeks ago")
    .ask()?;
```

```rust
list_builders!(MultiSelect)
```

A small badge after the option added last, such as `latest` or `current`.
Call it again for another.

```rust
list_builders!(MultiSelect)
```

Draw the option added last quieter than the rest, for ones that are less
likely to be wanted (a pre-release, say). It can still be picked.

```rust
list_builders!(MultiSelect)
```

A muted line under the question, to say more about what's being asked.

```rust
list_builders!(MultiSelect)
```

Let ← go back to the question before, as `Error::Back`.

```rust
list_builders!(MultiSelect)
```

How many rows to show at once (8 unless set); longer lists scroll, with a
count of what's above and below.

```rust
pub fn new(question: impl Into<String>) -> Self
```

A question with any number of answers from a list.

```rust
pub fn selected(mut self, indices: &[usize]) -> Self
```

Tick these options (by number) to start with, and answer with them when there's no
terminal. Options that can't be picked stay unticked.

```rust
pub fn validate(mut self, check: impl Fn(&[usize]) -> std::result::Result<(), String> + 'a) -> Self
```

Check the choices when Enter is pressed. An `Err` is shown under the question, which
stays open.

```rust
pub fn ask(self) -> Result<Vec<usize>>
```

Ask the question. The answer is the numbers of the ticked options, in order, counted
as `Select::ask` counts them.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::Back` on ← when asked with `.back()`,
`Error::NoOptions` when there's nothing to choose, `Error::NotATerminal` when there's no
terminal to ask on (unless options are `selected`), and `Error::Io`
when the terminal can't be used.

## Dropdown

`tinsel::prompt::Dropdown` · struct

```rust
pub struct Dropdown<'a>
```

One of a long list, such as versions or branches, as a dropdown: a window of rows that
scrolls, filtering as you type, with notes lined up on the right, badges, and options
drawn quieter than the rest. With `allow_custom`, what's typed
can be the answer too. Returns the chosen option's label, or the typed answer.

Without a terminal, the `selected` option is the answer, if there is one;
otherwise it's `Error::NotATerminal`.

```rust
use tinsel::prompt::Dropdown;

let version = Dropdown::new("Roll back to")
    .option("v1.2.0").note("3 days ago").badge("latest").badge("current")
    .option("v1.2.0-rc.1").note("5 days ago").dim()
    .option("v1.1.0").note("2 weeks ago")
    .option("v1.0.0").note("2 months ago")
    .selected(2)
    .allow_custom()
    .ask()?;
```

**Methods**

```rust
list_builders!(Dropdown)
```

Add an option. Options are numbered from 0 in the order they're added,
disabled ones included, and the answer is given as those numbers.

```rust
list_builders!(Dropdown)
```

Add several options.
```rust
let answer = tinsel::prompt::Dropdown::new("Deploy to").options(["staging", "production"]).ask()?;
```

```rust
list_builders!(Dropdown)
```

Add an option that can't be picked, drawn dim with the reason after it. The
cursor skips over it, but it still takes up a number.
```rust
let answer = tinsel::prompt::Dropdown::new("Deploy to")
    .option("staging")
    .disabled("production", "frozen until Monday")
    .ask()?;
```

```rust
list_builders!(Dropdown)
```

Start a group: a heading over the options added after it. It's not an option,
so it can't be picked and takes up no number.
```rust
let answer = tinsel::prompt::Dropdown::new("Service")
    .heading("Frontend")
    .options(["web", "docs"])
    .heading("Backend")
    .options(["api", "worker"])
    .ask()?;
```

```rust
list_builders!(Dropdown)
```

Muted text lined up on the right of the option added last, such as when it
was made.
```rust
let answer = tinsel::prompt::Dropdown::new("Version")
    .option("v1.2.0").note("3 days ago").badge("latest")
    .option("v1.1.0").note("2 weeks ago")
    .ask()?;
```

```rust
list_builders!(Dropdown)
```

A small badge after the option added last, such as `latest` or `current`.
Call it again for another.

```rust
list_builders!(Dropdown)
```

Draw the option added last quieter than the rest, for ones that are less
likely to be wanted (a pre-release, say). It can still be picked.

```rust
list_builders!(Dropdown)
```

A muted line under the question, to say more about what's being asked.

```rust
list_builders!(Dropdown)
```

Let ← go back to the question before, as `Error::Back`.

```rust
list_builders!(Dropdown)
```

How many rows to show at once (8 unless set); longer lists scroll, with a
count of what's above and below.

```rust
pub fn new(question: impl Into<String>) -> Self
```

A question with one answer from a long list.

```rust
pub fn selected(mut self, index: usize) -> Self
```

Start on this option (by number), and answer with it when there's no terminal.

```rust
pub fn allow_custom(mut self) -> Self
```

Let the answer be something that isn't in the list: once something's typed that
isn't an option, it's offered at the end of the list to choose.

```rust
pub fn validate(mut self, check: impl Fn(&str) -> std::result::Result<(), String> + 'a) -> Self
```

Check the answer when Enter is pressed, whether an option or typed. An `Err` is
shown under the question, which stays open.

```rust
let tag = tinsel::prompt::Dropdown::new("Tag")
    .options(["v1.1.0", "v1.0.0"])
    .allow_custom()
    .validate(|s| if s.starts_with('v') { Ok(()) } else { Err("tags start with v".into()) })
    .ask()?;
```

```rust
pub fn ask(self) -> Result<String>
```

Ask the question. The answer is the chosen option's text rather than its number,
since with `allow_custom` it may be typed and have none.

**Errors**

`Error::Cancelled` on Ctrl-C or Esc, `Error::Back` on ← when asked with `.back()`,
`Error::NoOptions` when there's nothing to choose, `Error::NotATerminal` when there's no
terminal to ask on (unless an option is `selected`), and `Error::Io`
when the terminal can't be used.

## script

`tinsel::prompt::script` · fn

```rust
pub fn script(keys: impl IntoIterator<Item = crate::app::Key>) -> Script
```

Answer the questions asked on this thread from `keys` instead of the keyboard, until the
returned `Script` is dropped. For tests of code that asks questions.

Questions behave as they do in a terminal (a piped stdin is ignored) but draw nothing.
A question that needs a key after the last one fails with an `Error::Io` of kind
`UnexpectedEof`. `editor` can't be scripted, so it's an `Error::NotATerminal`
meanwhile. `typed` turns text into keys.

```rust
use tinsel::app::Key;
use tinsel::prompt;

let mut keys = prompt::typed("v2");
keys.push(Key::Enter);
let _script = prompt::script(keys);
assert_eq!(prompt::input("Version", None)?, "v2");
```

## typed

`tinsel::prompt::typed` · fn

```rust
pub fn typed(text: &str) -> Vec<crate::app::Key>
```

The keys to type `text`, one per character.

## Script

`tinsel::prompt::Script` · struct

```rust
pub struct Script
```

Questions on this thread answered from a list of keys; made by `script`. Dropping it
gives the keyboard back.

**Methods**

```rust
pub fn answered(&self) -> Vec<String>
```

What each question showed when it was done, without colour or indent, in the order
they were asked: `●  Question  answer` for an answer, `✕  Question  cancelled`
for Esc or Ctrl-C. A question left with ← shows nothing, so it isn't listed.

```rust
pub fn remaining(&self) -> usize
```

How many keys haven't been read yet.

# tinsel::style

Colour and type: the palette, painting text, gradients, and how colour is chosen (the
terminal's colour support, a light or dark theme, an accent). The everyday parts are at
the crate root too: `paint`, `bold`, `palette`, `Rgb` and friends.

## Rgb

`tinsel::style::Rgb` · struct

```rust
pub struct Rgb(pub u8, pub u8, pub u8)
```

A colour as red, green and blue, 0-255 each.

## RAMP

`tinsel::style::RAMP` · const

```rust
pub const RAMP: &[Rgb] = &[palette::BERRY, palette::ROSE, palette::BLUSH]
```

Berry → rose → blush, for titles and filled progress.
A slice, so the stops can change without breaking code that uses it.

## ColorMode

`tinsel::style::ColorMode` · enum

```rust
pub enum ColorMode
```

How much colour tinsel draws with.

**Variants**

```rust
None,
```

Plain text, no escape codes at all: piped output, `NO_COLOR`, or tests.

```rust
Ansi16,
```

The 16 colours every terminal has, in whatever shades its theme gives them: for a
`TERM` without `256color` (the Linux console, plain `xterm`), or `FORCE_COLOR=1`.
Pinks become magenta and greys become black and white.

```rust
Ansi256,
```

The 256-colour palette, for terminals that don't advertise truecolor.

```rust
TrueColor,
```

Full 24-bit colour.

## color_mode

`tinsel::style::color_mode` · fn

```rust
pub fn color_mode() -> ColorMode
```

The colour mode in use. Decided once, from `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`,
whether stdout (or the `Target`) is a terminal, `TERM`, `CLICOLOR` and
`COLORTERM`, unless `set_color_mode` has chosen one.

## with_color_mode

`tinsel::style::with_color_mode` · fn

```rust
pub fn with_color_mode<T>(mode: ColorMode, f: impl FnOnce() -> T) -> T
```

Run `f` in a colour mode on this thread alone, leaving the rest of the program as it
was. Tests run side by side on threads of one process, so this is how a test that wants
real colour codes gets them without turning colour on, or off, for the tests beside it.
Anything drawn on other threads meanwhile uses the program's mode.

```rust
use tinsel::{ColorMode, palette, paint, with_color_mode};

let lit = with_color_mode(ColorMode::TrueColor, || paint(palette::ROSE, "hi"));
assert!(lit.starts_with("\x1b[38;2;"));
assert_eq!(with_color_mode(ColorMode::None, || paint(palette::ROSE, "hi")), "hi");
```

## with_theme

`tinsel::style::with_theme` · fn

```rust
pub fn with_theme<T>(theme: Theme, f: impl FnOnce() -> T) -> T
```

Run `f` in a theme on this thread alone, as `with_color_mode` does for the colour mode:
to check how something looks on a light background without changing it for anything else.

```rust
use tinsel::style::{ColorMode, Theme, with_color_mode, with_theme};
use tinsel::{paint, palette};

let dark = with_color_mode(ColorMode::TrueColor, || with_theme(Theme::Dark, || paint(palette::TEXT, "hi")));
let light = with_color_mode(ColorMode::TrueColor, || with_theme(Theme::Light, || paint(palette::TEXT, "hi")));
assert_ne!(dark, light);
```

## with_accent

`tinsel::style::with_accent` · fn

```rust
pub fn with_accent<T>(accent: Accent, f: impl FnOnce() -> T) -> T
```

Run `f` in an accent on this thread alone, as `with_color_mode` does for the colour
mode.

```rust
use tinsel::style::{Accent, ColorMode, with_accent, with_color_mode};
use tinsel::{paint, palette};

let pink = with_color_mode(ColorMode::TrueColor, || with_accent(Accent::Pink, || paint(palette::ROSE, "hi")));
let blue = with_color_mode(ColorMode::TrueColor, || with_accent(Accent::Blue, || paint(palette::ROSE, "hi")));
assert_ne!(pink, blue);
```

## set_color_mode

`tinsel::style::set_color_mode` · fn

```rust
pub fn set_color_mode(mode: ColorMode)
```

Choose the colour mode yourself, for a `--color` flag or to get plain text in tests.

```rust
tinsel::set_color_mode(tinsel::ColorMode::None);
assert_eq!(tinsel::paint(tinsel::palette::ROSE, "hi"), "hi");
```

## Theme

`tinsel::style::Theme` · enum

```rust
pub enum Theme
```

Whether the terminal's background is dark or light. tinsel's palette is designed for dark
backgrounds; on light ones every colour is redrawn darker, keeping its hue, so pinks stay
pink and text stays readable.

**Variants**

```rust
Dark,
```

Light text on a dark background.

```rust
Light,
```

Dark text on a light background.

## theme

`tinsel::style::theme` · fn

```rust
pub fn theme() -> Theme
```

The theme in use. Decided once: from `TINSEL_THEME` (`light` or `dark`) if set, then the
`COLORFGBG` variable some terminals export, then by asking the terminal for its background
colour. Falls back to `Theme::Dark`.

## set_theme

`tinsel::style::set_theme` · fn

```rust
pub fn set_theme(theme: Theme)
```

Choose the theme yourself, for a `--theme` flag or to preview the other one.

## mix

`tinsel::style::mix` · fn

```rust
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb
```

Blend two colours: `t = 0` is all `a`, `t = 1` is all `b`.

## sample

`tinsel::style::sample` · fn

```rust
pub fn sample(stops: &[Rgb], t: f32) -> Rgb
```

Sample a multi-stop gradient at t in 0..=1.

## Accent

`tinsel::style::Accent` · enum

```rust
pub enum Accent
```

The colour family tinsel's accents are drawn in. Pink is tinsel's own; the others redraw
every pink (accents, the spinner, the faint tint in the greys) in another hue, so a CLI can
match its brand. Red, amber and sage keep their meaning whatever the accent.

```rust
tinsel::set_accent(tinsel::Accent::Blue);
```

**Variants**

```rust
Pink,
```

tinsel's own: rose, blush and berry.

```rust
Violet,
```

Lavender and plum.

```rust
Blue,
```

Cornflower and navy.

```rust
Teal,
```

Aqua and deep teal.

```rust
Orange,
```

Apricot and rust.

```rust
Hue(f32),
```

Any hue, in degrees around the colour wheel (pink is about 338°).

**Methods**

```rust
pub fn from_name(name: &str) -> Option<Accent>
```

The accent with this name (`pink`, `violet`, `blue`, `teal`, `orange`), or a hue in
degrees. Case and surrounding spaces don't matter.

## set_accent

`tinsel::style::set_accent` · fn

```rust
pub fn set_accent(accent: Accent)
```

Choose the accent colour, for a brand or a `--accent` flag.

## paint

`tinsel::style::paint` · fn

```rust
pub fn paint(rgb: Rgb, s: &str) -> String
```

Colour the text. Resets only the foreground, so it nests inside bold, backgrounds and so on.

Colours come before the text they colour throughout tinsel: `paint(ROSE, "hi")`,
`paint_on`, `paint_bg`, `badge`, `Line::push`.

## paint_on

`tinsel::style::paint_on` · fn

```rust
pub fn paint_on(fg: Rgb, bg: Rgb, s: &str) -> String
```

Colour both the text and the cells behind it: `fg` text on `bg`.

```rust
use tinsel::{Rgb, paint_on};

let lit = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || paint_on(Rgb(0, 0, 0), Rgb(255, 255, 255), "hi"));
assert_eq!(lit, "\x1b[38;2;0;0;0;48;2;255;255;255mhi\x1b[39;49m");
```

## paint_bg

`tinsel::style::paint_bg` · fn

```rust
pub fn paint_bg(bg: Rgb, s: &str) -> String
```

Tint the cells behind the text, leaving its own colours alone.

```rust
use tinsel::{Rgb, paint_bg};

let lit = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || paint_bg(Rgb(40, 30, 36), "hi"));
assert_eq!(lit, "\x1b[48;2;40;30;36mhi\x1b[49m");
```

## bold

`tinsel::style::bold` · fn

```rust
pub fn bold(s: &str) -> String
```

Bold text.

## dim

`tinsel::style::dim` · fn

```rust
pub fn dim(s: &str) -> String
```

Faint text.

## italic

`tinsel::style::italic` · fn

```rust
pub fn italic(s: &str) -> String
```

Italic text, where the terminal's font has it.

## underline

`tinsel::style::underline` · fn

```rust
pub fn underline(s: &str) -> String
```

Underlined text.

## blink

`tinsel::style::blink` · fn

```rust
pub fn blink(s: &str) -> String
```

Blinking text, animated by the terminal itself, so it keeps blinking after your program
has exited. Most terminals support it (iTerm2, kitty, WezTerm, Ghostty, GNOME Terminal,
macOS Terminal); where it's turned off or unsupported, the text simply stays still. Best
kept to a single mark: a whole line blinking is hard to read.

```rust
println!("  {} ready", tinsel::style::blink(&tinsel::paint(tinsel::palette::ROSE, "✦")));
```

## gradient

`tinsel::style::gradient` · fn

```rust
pub fn gradient(s: &str, stops: &[Rgb], shift: f32) -> String
```

Colour s across the stops; a non-zero shift slides the gradient back and forth.

## Line

`tinsel::style::Line` · struct

```rust
pub struct Line
```

A painted string that remembers its visible width, so it can be padded and aligned.

**Methods**

```rust
pub fn new() -> Self
```

An empty line.

```rust
pub fn push(mut self, rgb: Rgb, s: &str) -> Self
```

Add text in a colour.

```rust
pub fn raw(mut self, styled: &str) -> Self
```

Append text that's already styled (a badge, a link); its width is measured without the codes.

```rust
pub fn append(mut self, other: Line) -> Self
```

Add another line's text to the end of this one.

```rust
pub fn space(mut self, n: usize) -> Self
```

Add `n` spaces.

```rust
pub fn kv(color: Rgb, key: &str, value: &str) -> Self
```

A muted, aligned key followed by a value - the row shape cards are built from.

```rust
pub fn width(&self) -> usize
```

How many columns the line takes up on screen.

```rust
pub fn wrap(&self, cols: usize) -> Vec<Line>
```

The line cut into pieces no wider than `cols`, its colours and links carried across
each cut. It breaks at a space where it can and mid-word where it must (a long URL), and
a `kv` line hangs the pieces after the first under its value.

```rust
use tinsel::{Line, palette};

let line = Line::kv(palette::TEXT, "url", "https://example.com/a/rather/long/path");
assert!(line.wrap(30).iter().all(|piece| piece.width() <= 30));
```

# tinsel::style::palette

The palette everything in tinsel is drawn from: pinks for accent, quiet greys for the rest.

## BERRY

`tinsel::style::palette::BERRY` · const

```rust
pub const BERRY: Rgb = Rgb(190, 80, 124)
```

Deep pink: the dark end of the ramp.

## ROSE

`tinsel::style::palette::ROSE` · const

```rust
pub const ROSE: Rgb = Rgb(236, 122, 164)
```

The accent: done marks, headings, the spinner's rest colour.

## BLUSH

`tinsel::style::palette::BLUSH` · const

```rust
pub const BLUSH: Rgb = Rgb(246, 172, 200)
```

Light pink: secondary accents and values.

## PETAL

`tinsel::style::palette::PETAL` · const

```rust
pub const PETAL: Rgb = Rgb(252, 218, 231)
```

Palest pink: the peak of the shimmer.

## SHINE

`tinsel::style::palette::SHINE` · const

```rust
pub const SHINE: Rgb = Rgb(255, 242, 248)
```

The brightest point of a glint or a twinkle - near white, pinked.

## TEXT

`tinsel::style::palette::TEXT` · const

```rust
pub const TEXT: Rgb = Rgb(236, 230, 234)
```

Main text: a warm, pinkish white.

## SOFT

`tinsel::style::palette::SOFT` · const

```rust
pub const SOFT: Rgb = Rgb(172, 160, 168)
```

Secondary text: details and notes.

## MUTED

`tinsel::style::palette::MUTED` · const

```rust
pub const MUTED: Rgb = Rgb(118, 106, 114)
```

Quiet text: timings, keys, hints.

## LINE

`tinsel::style::palette::LINE` · const

```rust
pub const LINE: Rgb = Rgb(120, 92, 106)
```

Card borders: a grey with a hint of rose.

## TRACK

`tinsel::style::palette::TRACK` · const

```rust
pub const TRACK: Rgb = Rgb(66, 58, 64)
```

Unfilled progress track.

## SAGE

`tinsel::style::palette::SAGE` · const

```rust
pub const SAGE: Rgb = Rgb(140, 200, 160)
```

Green, for additions and tips.

## AMBER

`tinsel::style::palette::AMBER` · const

```rust
pub const AMBER: Rgb = Rgb(240, 184, 100)
```

Amber, for warnings.

## RED

`tinsel::style::palette::RED` · const

```rust
pub const RED: Rgb = Rgb(240, 96, 96)
```

Red, for errors and removals.

# tinsel::text

Measuring and shaping text by what the terminal shows, not by bytes or chars:
escape codes count for nothing, CJK and emoji count for two columns, and a character
with the marks, variation selectors and joiners that build on it counts as one.

## Fuzzy

`tinsel::text::Fuzzy` · struct

```rust
pub struct Fuzzy
```

How well some text matched what was typed, from `fuzzy`: a score to rank matches by,
higher being better, and which characters matched, to highlight them.

**Fields**

```rust
pub score: i64,
```

Higher is better. Only worth comparing with other scores for the same query.

```rust
pub hits: Vec<usize>,
```

Which characters matched, counted in `char`s from 0, in order.

## fuzzy

`tinsel::text::fuzzy` · fn

```rust
pub fn fuzzy(text: &str, query: &str) -> Option<Fuzzy>
```

Whether `text` holds the letters of `query` in order, ignoring case and any spaces in
the query, and how well. Matches score higher where the letters run together, start
words (after `/`, `_`, `-`, `.` or a space, or a capital after a small letter), and fall
in the last part of a path, the file's own name; shorter texts win ties. An empty
query matches everything, the shortest first.

```rust
use tinsel::text::fuzzy;

let hit = fuzzy("src/app/main.rs", "main").unwrap();
assert_eq!(hit.hits, [8, 9, 10, 11]);
assert!(fuzzy("src/app/main.rs", "mian").is_none());
// the file's own name beats a folder's
assert!(hit.score > fuzzy("main/src/app.rs", "main").unwrap().score);
```

## highlight

`tinsel::text::highlight` · fn

```rust
pub fn highlight(
    text: &str,
    hits: &[usize],
    hit: impl Fn(&str) -> String,
    other: impl Fn(usize, &str) -> String,
) -> String
```

`text` with the characters at `hits` (counted in `char`s from 0, as `fuzzy` gives
them) drawn by `hit` and every other one by `other`, which is told where it is too: to
show what matched a search.

```rust
use tinsel::{bold, paint, palette};
use tinsel::text::{fuzzy, highlight, strip_ansi};

let name = "main.rs";
let found = fuzzy(name, "mrs").unwrap();
let shown = highlight(name, &found.hits, |s| bold(&paint(palette::ROSE, s)), |_, s| s.to_string());
assert_eq!(strip_ansi(&shown), "main.rs");
```

## content_width

`tinsel::text::content_width` · fn

```rust
pub fn content_width() -> usize
```

The width tinsel lays text out in: the terminal's, capped at 76 columns so lines stay
readable. What `markdown`, cards and messages wrap to.

## strip_ansi

`tinsel::text::strip_ansi` · fn

```rust
pub fn strip_ansi(s: &str) -> String
```

The text with every terminal escape (colours, cursor moves, hyperlinks) removed.

## char_width

`tinsel::text::char_width` · fn

```rust
pub fn char_width(c: char) -> usize
```

Columns a single character takes up, by Unicode 17's tables: two for wide (CJK and
emoji), none for combining marks, joiners, variation selectors and control characters,
one for the rest, a tab included (it's drawn as a space). Ambiguous characters take two
where the terminal draws them wide (see `ambiguous_wide`), but
for tinsel's own symbols, which go to such a terminal as ASCII. Text is
better measured with `width`, which knows that a character and what builds on it, such
as a flag's two letters or a family joined into one emoji, are drawn as one.

## width

`tinsel::text::width` · fn

```rust
pub fn width(s: &str) -> usize
```

Columns the text takes up on screen, ignoring escape codes.

## pad

`tinsel::text::pad` · fn

```rust
pub fn pad(s: &str, cols: usize) -> String
```

Pad on the right to `cols` visible columns. Styled text is fine.

## pad_left

`tinsel::text::pad_left` · fn

```rust
pub fn pad_left(s: &str, cols: usize) -> String
```

Pad on the left to `cols` visible columns, for right-aligned numbers.

## truncate

`tinsel::text::truncate` · fn

```rust
pub fn truncate(s: &str, cols: usize) -> String
```

Cut text to at most `cols` columns, ending in "…" when anything was cut. Styled text is
fine: every escape is kept, so colours opened before the cut are closed after it.

## truncate_start

`tinsel::text::truncate_start` · fn

```rust
pub fn truncate_start(s: &str, cols: usize) -> String
```

Cut text to at most `cols` columns from the front, starting with "…" when anything was
cut: for a path, whose end says the most. Styled text is fine, as for `truncate`.

```rust
use tinsel::text::truncate_start;

assert_eq!(truncate_start("src/app/main.rs", 10), "…p/main.rs");
assert_eq!(truncate_start("main.rs", 10), "main.rs");
```

## char_width_at

`tinsel::text::char_width_at` · fn

```rust
pub fn char_width_at(c: char, column: usize, tab_stop: usize) -> usize
```

Columns a character takes up when it's drawn at `column` (counted from 0): a tab reaches
the next tab stop, every `tab_stop` columns, and anything else is as wide as
`char_width` says.

```rust
use tinsel::text::char_width_at;

assert_eq!(char_width_at('\t', 0, 4), 4);
assert_eq!(char_width_at('\t', 6, 4), 2);
assert_eq!(char_width_at('漢', 6, 4), 2);
```

## width_with_tabs

`tinsel::text::width_with_tabs` · fn

```rust
pub fn width_with_tabs(s: &str, tab_stop: usize) -> usize
```

Columns the text takes up with its tabs reaching the next tab stop, every `tab_stop`
columns, as an editor shows them. Escape codes count for nothing, as for `width`.

```rust
assert_eq!(tinsel::text::width_with_tabs("a\tb", 4), 5);
assert_eq!(tinsel::text::width_with_tabs("\t\t", 8), 16);
```

## wrap

`tinsel::text::wrap` · fn

```rust
pub fn wrap(s: &str, cols: usize) -> Vec<String>
```

Word-wrap plain text to `cols` columns. Words longer than a line are broken, between
characters and never inside one.

# tinsel::web

Full-screen apps on a web page, with the `web` feature: the app built for wasm, and drawn
in the browser as `Frame::html_pre` draws it, the keys, the
mouse and the window's size passed in. `web_app!` names the app, in a
crate built as a `cdylib`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
tinsel = { version = "1.2", features = ["web"] }
```

```rust
tinsel::web_app!(Counter::default());
```

Built with `cargo build --release --target wasm32-wasip1`, it's run in a page by
`SCRIPT`, which a page imports:

```html
<div id="app" style="height: 100vh"></div>
<script type="module">
  import { run } from './tinsel.js';
  run(document.getElementById('app'), 'counter.wasm');
</script>
```

The app runs as it would in a terminal: told its size first, sent `Event::Tick` when a
frame it asked for comes due, and drawn only when something changes, with
`app::time` and `app::size` as they'd be there.
What a page can't do, it doesn't: there are no threads, so no `Job` or
`Waker`; no files, processes or clipboard; and pictures aren't shown.

## SCRIPT

`tinsel::web::SCRIPT` · const

```rust
pub const SCRIPT: &str = include_str!("web.js")
```

The page's side, as an ES module: it loads the app's wasm, gives it the little of WASI it
needs (a clock, randomness and an empty environment), passes in the keys, the mouse, the
window's size and whether it's in view, and puts each frame in place, a row at a time.
Write it next to the page, as `tinsel.js` say, and call its `run(element, wasm, options)`;
the options are `light` for tinsel's light colours, `fontSize` in pixels, and `onQuit`,
called with the browser's name for the key that quit the app (`"q"`, `"Escape"`). The app
follows the element's size, however it changes, and `run` gives back `stop()` to end it.

## touch

`tinsel::web::touch` · fn

```rust
pub fn touch() -> bool
```

Whether the page is on a touch screen, with no keyboard to speak of: for an app to leave
out the keys' hints a touch screen has no use for. Known once `web_app!`
is starting the app, so it can be asked while the app is made.

## web_app!

`tinsel::web_app!` · macro

```rust
web_app!(…)
```

Run `app` on a web page: makes the functions the page's `SCRIPT` calls, in a crate built
as a `cdylib` for wasm. `app` is made as the page starts, after `touch` is known.

```rust
tinsel::web_app!(Counter::default());
```

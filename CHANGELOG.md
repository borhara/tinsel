# Changelog

## Unreleased

First release.

- `app`: full-screen terminal apps. An `App` trait (`view`, `event`, `fps`, `mouse`) run by
  `app::run`, with keys, mouse, ticks and resizes as `Event`s, only changed rows redrawn, and
  the screen given back on quit or panic; `Rect` layout (`rows`, `columns`, gaps, `center`);
  `Panel` (shimmering when focused), `List`, `Tabs`, `Plot`, `Stat`, `big`, `hints`,
  `Sparkles`, and every `Block`-drawing type as a `Widget`; `navigate`; `app::snapshot` for
  tests; `examples/dashboard.rs`
- `Live` steps that learn their usual duration and show an estimate next run
- Focus awareness: calmer animation while you're away, a notification when a long run finishes
  unnoticed, a twinkle for what finished when you return; `tinsel::notify`
- Light/dark background detection (`Theme`, `set_theme`) with the palette redrawn for light terminals
- `Live` step lists with seven spinners (`sparkle` by default), a twinkle when a step finishes,
  progress bars with a running glint, and a closing `Card`
- `spin` / `try_spin` and a standalone `Progress` bar
- Messages (`success`, `info`, `warn`, `error`, `hint`, `counter`, `title`, `rule`), `callout`
  blocks and `Report`s with cause chains (`Report::from_error`)
- `Table`, `Tree`, `pairs`, `columns`, `sparkline`, `bars`, `meter`, `diff`, `Code`, `badge`, `link`
- `prompt::{confirm, input, password, select, multiselect}` with a typed `prompt::Error`
- `Block` as the one return type for multi-line output, with `.plain()` for tests
- `set_color_mode` to force a colour mode; `NO_COLOR` and non-terminal output respected
- `text` and `format` helpers
- `Live` sub-steps (`Step::step`), folded away once they all pass; steps can run in parallel
  from any thread; `Live::log` / `Step::log` print above the display
- `Live::headless`, `advance` and `snapshot` for testing exactly what a run looks like
- `install_panic_hook`: panics as a `Report`, with a mid-draw `Live` left tidy
- `set_output` with `Output::Json` (JSON lines for every message, step, report and block) and
  `Output::Quiet`; `TINSEL_OUTPUT`
- `set_accent` / `Accent` and `TINSEL_ACCENT`, to redraw the pinks in another hue
- `Table`: `sort_by` / `sort_by_desc` (by value, understanding units), `border`, `wrap`,
  `max_width`, `footer`, `highlight`
- `Progress::unknown`, `set_total`, `add_total`; finishes on drop; `ProgressIterator`
- `Code::around`, `path`, `caret`; `Report::code`
- `histogram` and `heatmap`
- `waveform` / `live_waveform` progress bars, and `BarStyle` to use them in `Live`
  (`with_bar`) and `Progress` (`with_bar`, also on `.progress()` iterators)
- `prompt::Input` (validation, placeholders, line editing), `number`, `date` (`prompt::Date`),
  `editor`; type-to-filter in long `select` / `multiselect` lists; `prompt::Error::Invalid`
- Prompts on Windows, through the console API
- `timings`, and `tinsel timings` in the CLI, to show learned step durations
- `Step::run` and `Step::command` (`StepCommand`: `timeout`, `retries`, `tail`) run commands
  as steps; `RunError`; `Step::try_run` and `Step::retry` for closures
- `Live::timeline` and `finish_with_timeline`
- `Live` displays taller than the terminal keep working, leaving their top in scrollback
- Step notes can span several lines
- Syntax colouring in `Code` (`Code::lang`, `Lang`, picked from `Code::path` too) and
  `Code::numbers(false)`
- `Help`, with `animate` for an entrance on a terminal; `tinsel help` uses it
- `markdown`, `json` (`JsonError`) and `Block::page`
- `set_target(Target::Stderr)` and `TINSEL_TARGET=stderr` draw everything on stderr, for tools
  whose stdout carries something else; colour and animation then follow whether stderr is a
  terminal
- `prompt::Input::suggestions`, `complete` and `paths` (Tab completes, → takes the
  suggestion); `prompt::confirm_typing`

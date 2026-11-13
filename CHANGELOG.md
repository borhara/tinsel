# Changelog

## [Unreleased]

## [0.2.0](https://github.com/borhara/tinsel/compare/v0.1.0...v0.2.0) - 2026-11-13

### Added

- keys with a timeout, Ctrl-C choices, pre-filled input and the right arrow to answer
- *(prompt)* answer questions from a script of keys, for tests
- *(app)* counts on panel titles and a footer with a status
- *(app)* toasts that slide into a corner
- *(app)* suspend to run something in the normal terminal
- *(app)* wake a running app from other threads
- *(app)* modals over a faded screen
- *(app)* add modal overlays
- *(app)* a box to type into
- *(app)* a table with a cursor
- *(app)* add a grid with a cursor row and scrolling
- *(prompt)* bold the option under the cursor and summarise several answers by name
- *(demo)* ask the new list questions as steps that go back
- *(prompt)* add Select, MultiSelect and Dropdown builders
- *(prompt)* go back a question with the left arrow in confirm and input
- *(prompt)* read a piped answer in confirm
- *(live)* show time left beside step progress
- *(showcase)* show skipped, cancelled and waiting steps, a region, and --stderr
- *(region)* export Region and add tests
- *(region)* add Region for redrawing any block in place
- *(live)* turn focus and history off from code with no_focus, set_focus and set_history
- *(live)* skipped, cancelled and waiting steps, and steps that run again
- *(output)* draw on stderr with set_target

### Fixed

- *(app)* scroll grids to whole columns and mark what's off to either side
- *(spin)* line finished spinners up with Live steps

### Other

- release from main with release-plz
- split the changelog at 0.1.0
- use generic names in the demos, examples and tests
- *(app)* draw every piece together in plain text
- *(app)* add a services dashboard example
- *(app)* cover the grid
- document the list builders
- *(prompt)* read keys through a source tests can script
- document Region
- document the new step states
- *(live)* cover skipped, cancelled, waiting and repeated steps

## [0.1.0] - 2026-11-12

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
- `prompt::Input::suggestions`, `complete` and `paths` (Tab completes, → takes the
  suggestion); `prompt::confirm_typing`

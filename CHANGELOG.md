# Changelog

## Unreleased

First release.

- `prompt::confirm` reads a piped answer (`y`, `yes`, `n`, `no`, or an empty line for the
  default) when stdin isn't a terminal, so scripts can answer it
- Going back in multi-question flows: questions asked with `.back()` return
  `prompt::Error::Back` when ← is pressed (in typed answers, from the start of the line);
  `prompt::Confirm` builder and `Input::back`
- `prompt::Select` and `prompt::MultiSelect` builders: a default, group headings, options
  that can't be picked (shown with the reason), notes and badges, a hint under the
  question, validation and `.back()`; the default is the answer without a terminal
- `prompt::Dropdown` for long lists such as versions: a scrolling window with counts above
  and below, notes lined up on the right, badges, quieter options, and `allow_custom` to
  type an answer that isn't listed
- Filtered lists show how many options match; the option under the cursor is bold, as in
  app lists; several answers collapse to whole names and how many more (`api, web and 3 more`)

- `app`: full-screen terminal apps. An `App` trait (`view`, `event`, `fps`, `mouse`) run by
  `app::run`, with keys, mouse, ticks and resizes as `Event`s, only changed rows redrawn, and
  the screen given back on quit or panic; `Rect` layout (`rows`, `columns`, gaps, `center`);
  `Panel` (shimmering when focused), `List`, `Tabs`, `Plot`, `Stat`, `big`, `hints`,
  `Sparkles`, and every `Block`-drawing type as a `Widget`; `navigate`; `app::snapshot` for
  tests; `examples/dashboard.rs`
- `app::Grid`: a table with a cursor, kept in view as it moves, scrolling sideways a column
  at a time with the header always showing; striped rows, a sort mark, and loading, empty
  and error states. Its place is kept in an `app::Cursor`
- `app::Input`: a box to type into inside an app, for filters and searches, with a cursor,
  the usual editing keys, a prompt and a placeholder
- `app::Modal`: a dialog in the middle of the screen, sized to what's inside it, rising into
  place over a faded backdrop with a soft shadow; `Frame::dim`, `Widget::size` and
  `app::time`
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
- `Step::skip`, `Step::cancel` and `Step::wait` (on someone, like an approval), drawn as `–`,
  `⊘` and `◌`, as `[skip]`, `[cancelled]` and `[wait]` lines, and as `skipped`, `cancelled`
  and `waiting` JSON states; a finished or failed step can `start` again, timed afresh and
  remembered once
- `Live` progress bars say how long is left at the rate so far
- `spin` and `Progress` finish in the same columns as `Live` steps, the time at the right edge
- `Region`: any `Block` redrawn in place at a set rate (`fps`), cleared when it finishes
  unless `keep`, with `log` printing above it; off a terminal only logged lines and a kept
  last frame print; `Region::headless` and `snapshot` for tests
- `Live::no_focus`, `set_focus` and `set_history` turn focus awareness and remembered
  timings off from code, alongside `TINSEL_NO_FOCUS` and `TINSEL_NO_HISTORY`
- `set_target(Target::Stderr)` and `TINSEL_TARGET=stderr` draw everything on stderr, for tools
  whose stdout carries something else; colour and animation then follow whether stderr is a
  terminal
- `prompt::Input::suggestions`, `complete` and `paths` (Tab completes, → takes the
  suggestion); `prompt::confirm_typing`

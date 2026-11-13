# Changelog

## [Unreleased]

## [0.3.1](https://github.com/borhara/tinsel/compare/v0.3.0...v0.3.1) - 2026-11-13

### Fixed

- *(app)* let a grid fill a column that has no header
- *(highlight)* treat the word after a spaced pipe as a command
- *(highlight)* colour shell flags like --release as flags
- *(card)* keep a space after a key of ten or more characters

### Other

- *(history)* keep the scratch history off disk, so Windows doesn't save it to /nonexistent
- set TERM for the terminal test, so less draws on CI
- *(app)* pin the accent in the web page tests
- *(app)* pin the accent in the colour tests
- check help width with stdout piped, so COLUMNS decides
- run terminal, pipe and process-wide tests in child processes
- *(prompt)* cover errors, keys, dates, the editor and every list
- *(prompt)* make path completion, the editor and key reading testable
- *(app)* run, drive, read keys and suspend through non-generic cores
- *(live)* cover every mode, commands, estimates and panics
- *(live)* keep step decisions out of generic functions
- *(help)* cover loose items, footers, faded examples and animation
- *(help)* animate through the output stream
- *(spin)* cover spinners, progress lines and iterators
- *(spin)* keep try_spin's decisions out of the generic function
- *(region)* cover drawing, logging and panics mid-draw
- *(region)* join the drawing thread without an impossible branch
- *(app)* cover wrapping escapes, sizes, panels and lists
- *(app)* cover grid alignment, headings, stripes and loading
- *(app)* draw a grid's header and rows without impossible branches
- *(app)* split lengths outside the generic function
- *(widgets)* cover waveform bars, card titles and fitted cards
- *(widgets)* style labels through a reference
- *(markdown)* cover headings, rules, task lists and inline markup
- *(markdown)* drop two checks that were always true
- *(block)* cover printing, events and paging
- *(block)* page through a pager passed in
- *(panic)* build the report without branches that never run
- *(notify)* cover every kind of notification
- *(notify)* choose and send notifications from values passed in
- *(message)* cover every message, counter, title and report
- *(table)* cover sorting, units, titles, stripes and footers
- *(highlight)* cover language names, sections and shell variables
- *(layout)* cover links, columns and code excerpts
- *(app)* cover codes frames drop and dimming in either theme
- *(app)* cover deleting forwards and the caret in colour
- *(app)* cover narrow modals and every toast mark
- *(app)* cover four-byte characters
- *(tree)* cover printing and formatting
- *(chart)* cover bars, meters, histograms and heatmaps
- *(json)* cover empty objects, long arrays and every mistake
- *(format)* cover every size of number and age
- *(text)* cover every escape and long words starting a line
- *(history)* cover saving, reporting and unwritable folders
- *(history)* load and report timings from any folder
- *(watch)* cover focus reports, typing and coming back
- *(watch)* read focus reports from any reader, at any time
- *(term)* cover stty failures, saved settings and size fallbacks
- *(term)* share the size fallback and hand keys back to any terminal
- *(fps)* cover the frame rate from the environment
- *(fps)* parse TINSEL_FPS in a function of its own
- *(style)* cover colour depth, themes, accents and 256 colours
- *(style)* decide colour depth and theme from values passed in
- *(output)* cover output shapes, JSON values and capturing
- *(output)* let unit tests pretend a terminal and catch what's written
- keep the highlighter's colour test from turning colour on for the tests beside it
- rewrap the tour's help flags after the shorter tagline
- describe tinsel without pink or zero-deps in the tagline

## [0.3.0](https://github.com/borhara/tinsel/compare/v0.2.0...v0.3.0) - 2026-11-13

### Added

- *(app)* tabs with colours and badges of their own
- *(app)* a status on a panel's top border, and titles that keep their own colours
- *(app)* Frame::html and Block::html draw what the terminal shows as a web page
- *(prompt)* Confirm::lenient takes any piped answer starting with y as yes
- *(app)* group a Grid's rows under headings the cursor steps over
- *(table)* blank_cells leaves empty cells blank instead of dotted, on Table and Grid
- *(help)* use the terminal's full width for commands, options and examples
- 60fps by default, with set_fps, TINSEL_FPS and per-display rates
- *(app)* a grid column that fills the spare width
- *(app)* set the window title while an app runs
- *(card)* set a title and footer into the border, colour it and fit it to its rows

### Fixed

- *(app)* faded text in the default colour fades for real and is never bold
- *(app)* a narrow footer drops whole hints from the end and keeps its status
- *(app)* a modal's shadow fades what's under it instead of blacking it out
- *(region)* keep its frame rate between 1 and 240, like everything else
- *(app)* keep a tint under text drawn over it

### Other

- changelog entries come from commit messages, and rewrap a long line
- README pictures of the dashboard and services examples, drawn with tinsel
- *(examples)* show panel statuses, tab badges, grouped grid rows and window titles

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

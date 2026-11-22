# Changelog

## [Unreleased]

## [1.0.2](https://github.com/borhara/tinsel/compare/v1.0.1...v1.0.2) - 2026-11-22

### Other

- list every example in the readme, and add contributing and license sections
- "docs: tidy MSRV wording in readme"
- tidy MSRV wording in readme

## [1.0.1](https://github.com/borhara/tinsel/compare/v1.0.0...v1.0.1) - 2026-11-18

### Fixed

- a progress bar is one unbroken line, its tip fading in rather than leaving a half-cell gap

### Other

- mention full-screen apps in crate metadata

## [1.0.0](https://github.com/borhara/tinsel/compare/v0.5.0...v1.0.0) - 2026-11-17

tinsel is stable: from here on, breaking changes wait for 2.0. Enums that may grow are
`#[non_exhaustive]`, so new variants arrive in minor releases.

### Breaking

- `prompt::Date`'s fields are private: build one with `Date::new`, which checks it's a real date, and read it with `year()`, `month()` and `day()`
- `style::RAMP` is a slice, `&[Rgb]`, rather than an array of three
- Ctrl-C while tinsel draws ends the program by the signal itself, so shell loops stop too, rather than exiting with status 130 (Windows still exits with 130)
- a Ctrl-C handler the program set up itself, or an ignored Ctrl-C, is left in place on Unix rather than replaced
- `prompt::password` and other secret questions are never read from a pipe; without a terminal they return `NotATerminal`
- `TINSEL_NO_FOCUS` and `TINSEL_NO_HISTORY` set but empty leave things on, as an empty `NO_COLOR` does


### Added

- *(app)* Keymap::Picker moves through a list with Ctrl-N, Ctrl-P and Tab, leaving the letters for the box above it
- *(app)* TextInput says what each key did with Edit, keeps a History, takes its own idea of a word, and its cursor can be read, moved, typed at and shown as the terminal's
- *(app)* keep_in_view scrolls a list as little as it can to show what's selected
- *(text)* fuzzy matches typed letters against names and paths with a score and the places they hit, and highlight draws those places their own way
- *(app)* Scrollbar is a widget of its own, in any colour, with or without its track
- *(app)* hline and vline draw rules that join up with the lines they meet
- *(app)* print_right and print_center line text up in a row, fade_outside fades all but one rectangle, and a Rect splits off its edges
- *(app)* a Modal can be placed, filled and left undimmed, fades what's behind in as it opens, and Frame::modal gives back where its inside goes, beside a public Frame::shadow
- *(app)* Panel::background fills a panel with a colour of its own, for popups over text
- *(app)* Job runs work on a thread of its own, wakes the app when it's done and hands the result over when polled
- *(clipboard)* copy and paste with the system's own tools, asking the terminal with OSC 52 where there are none and over ssh
- *(format)* grouped marks off thousands, ago_long says how long ago in words, and local_clock gives the time of day in the local zone
- *(text)* truncate_start keeps a path's end, and char_width_at and width_with_tabs measure tabs to the next stop
- *(app)* keys read and write vim's notation, <C-a> and <Esc>, with Key::parse_sequence for several, and Key::symbol shows them as ⌃A
- *(app)* Frame::ambient_at sets the ambient clock on its own, for testing ambient animation apart from motion
- *(app)* [**breaking**] an app is told its size with a Resize before its first frame, and app::size gives the screen's size at any time
- *(app)* set_background colours the window margin too, and the window's colour is sent again once the app settles and whenever focus comes back
- *(anim)* Glide eases a value from one place to another, and Glider glides to wherever it's told, asking for frames until it lands
- *(app)* [**breaking**] apps are drawn only when something happens or a frame asks, with animate, animate_at and redraw_at, and ambient animation rests while the terminal is in the background
- every public type can be printed with {:?}, small ones hash and compare, and builders and displays warn when dropped unused
- *(style)* [**breaking**] ColorMode::Ansi16 draws in the 16 basic colours where TERM promises no more, keeping pinks magenta, and doctor says so
- *(live)* [**breaking**] a step's command can be handed over or lent, chooses what it reads with stdin, and RunError says the program, exit code and what went wrong
- *(app)* [**breaking**] time is f64 seconds, so animations stay smooth however long an app runs, and toasts are told when they were pushed like modals are
- [**breaking**] with_theme, with_accent, with_output and with_target scope a setting to one closure like with_color_mode, and every TINSEL_ variable ignores case and spaces
- *(app)* [**breaking**] keys carry their modifiers, so alt-b and ctrl-left arrive as themselves, and the mouse reports drags, releases and the middle button
- [**breaking**] enums that may grow are non_exhaustive, and JsonError says where and what through methods
- *(app)* Event::Focus says when the terminal window gains or loses focus
- *(app)* app::redraw sends the whole screen again with the next frame
- *(app)* Frame::set_window_color colours the terminal's own background, padding included
- *(app)* Frame::bare lets the terminal's own background show behind part of the screen
- *(app)* Frame::dim_by dims part of the screen by a chosen amount
- markdown_in draws markdown wrapped to a given width, for a pane beside something else
- *(app)* App::fps is asked after every frame, so an app can speed up while something moves
- *(sparkles)* wandering stars fade out and light up again somewhere else
- *(app)* App::ctrl_c_quits lets an app take Ctrl-C as an ordinary key
- *(app)* Frame::set_background paints the frame in its own colour, kept through clear and dim
- *(app)* Frame::set_cursor_color colours the terminal's cursor
- *(app)* a frame can show the terminal's own cursor, as a block, bar or underline
- *(app)* Frame::print_from is public, for text scrolled sideways
- *(highlight)* Lang::highlight colours one line of source

### Fixed

- *(live)* a step's command can be set up on one thread and run on another again, as StepCommand is Send and Sync
- TINSEL_NO_FOCUS and TINSEL_NO_HISTORY set but empty leave things on, as an empty NO_COLOR does, and doctor says so
- *(app)* Grid::loading(false) takes back an earlier loading(true), and function keys are documented up to F24 as they're read
- *(clipboard)* paste gives up on a tool that hangs, and Windows copies and pastes in UTF-8
- *(app)* one app runs at a time, other threads' panics wait until the screen is left, piped apps get their first Resize, a failed suspend ends the loop, and redraw works from any thread
- *(live)* a question holds off the focus watch, only a panic that ends the program stops the displays, progress left short says so, line breaks count as rows, and history is never kept in the current directory
- *(prompt)* [**breaking**] a Date is always a real one, a paste never answers a question, secrets stay secret, the editor runs as git runs it in a file only you can read, and a killed question puts the terminal back
- *(text)* links can't carry escapes out of themselves, markdown tables and long unclosed marks are safe, code carets line up after tabs, kv keys line up by columns, and charts take infinities and bad maximums
- *(term)* Ctrl-C ends by the signal itself and leaves a program's own handler alone, and displays that overlap put the terminal back only when the last one ends
- *(live)* [**breaking**] a live display and a progress line start drawing at their first step or update, so every builder setting holds from the first frame
- *(prompt)* a question counts the rows its lines wrap onto and draws only what fits, so a long answer leaves nothing behind
- *(prompt)* Alt and Ctrl with an arrow move by words instead of cancelling or eating keys, and a paste is typed in whole
- exiting mid-run with std::process::exit puts the terminal back, and keys typed meanwhile are said to be lost where Linux won't take them back
- *(term)* the background colour is asked on the stream tinsel draws on, so TINSEL_TARGET=stderr gets its theme
- *(prompt)* with stderr sent elsewhere a question is NotATerminal rather than waiting unseen
- *(region)* a view that panicked isn't called again on the way out, and the cursor always comes back
- questions and editors hold every display still, and messages print above a display rather than under its next frame
- *(live)* step times read as format::duration says them, 300ms rather than 0.3s, like a spinner's
- *(live)* a line break in a label, detail or progress text no longer makes the display creep down
- TERM=dumb gets plain lines rather than displays redrawn in place
- *(prompt)* typing ~ into a path no longer panics, and completes to the home folder
- *(output)* a capture inside a capture gets its own output instead of waiting forever
- *(term)* a background colour reply with too many digits a channel is ignored instead of overflowing
- *(history)* times in the cache that can't be durations are left out, instead of panicking later
- *(app)* a hangup or kill puts the terminal back before the app ends, unless the program handles it itself
- *(app)* each frame reuses the cells of the one before last, and escape codes are written straight into the output
- *(app)* a resized window is redrawn within a tenth of a second, however slowly the app ticks
- *(app)* an input scrolls back from its cursor, so a long value draws quickly and a wide letter under the caret stays in view
- *(app)* a grid lays out only the rows that show, so a long table draws as fast as a short one
- *(app)* accents written as separate characters stay on their letter instead of vanishing
- *(app)* a rectangle reaching past usize::MAX is cut down instead of overflowing
- *(app)* the panic hook is put back after an app, even one that panicked, and a hook set meanwhile is kept
- *(app)* an app ends with an error when its terminal goes away, instead of spinning with no keyboard
- *(app)* a wake as the app starts is never lost, so later wakes keep arriving
- *(app)* a paste that never ends is handed over once it goes quiet, so keys and ctrl-c work again
- *(app)* a big paste is read in one pass, not searched again after every byte
- *(app)* an escape sequence cut short by another is read as Esc, not a stray letter
- *(app)* ctrl-s, ctrl-q, ctrl-v and ctrl-o reach the app instead of the terminal
- *(cli)* flags work before the command, --version says which tinsel this is, unknown flags, stray words and a bad --bar are usage errors, and JSON from doctor and timings comes without blank lines
- *(panic)* the panic hook passes a panic on to any hook set before it, and installing it twice does nothing
- *(layout)* diff finds its edits by Myers' algorithm split at the middle, so long texts take memory in proportion to their length rather than its square
- *(layout)* a caret is as wide as the characters it underlines, so ^^^^ sits under all of 名前
- *(chart)* values that aren't finite are left out of a chart's range and its histogram bins, and a waveform of NaN has played nothing
- *(format)* sizes, counts and times are rounded before their unit is picked, so 999,999 reads 1.0M rather than 1000k
- *(table)* a tab is measured as the one column frames draw it as, and a cell's tabs reach the next stop and its newlines start new lines
- *(markdown)* numbered lists count per level from their first number, items keep their continuation lines, a lone * is a star, and narrow widths are kept to
- *(text)* widths come from Unicode 17 and count emoji sequences, flags and marks as one; wrap and truncate go by columns and keep escapes
- *(style)* 256-colour terminals get the nearest palette colour, from xterm's real cube levels or its grey ramp
- *(style)* only one thread ever asks the terminal for its background, and a theme set in code always wins
- *(style)* colour is decided for stdout and stderr apart, follows FORCE_COLOR, CLICOLOR_FORCE, CLICOLOR and TERM=dumb, and a mode set in code always wins
- *(doctor)* an environment variable that isn't UTF-8 is read lossily instead of stopping doctor
- *(highlight)* a long line is coloured in linear time instead of copying the rest of it at every token
- *(json)* deep nesting is an error instead of a stack overflow, and strings can't carry control characters to the terminal
- *(toasts)* a toast is drawn as its rounded outline over what's behind, with no square fill around it
- *(toasts)* the countdown along a toast's bottom edge fades its last cell, so it shrinks smoothly
- *(app)* colour changes go out before the frame they belong to, so the terminal repaints its padding with them

### Other

- *(app)* match the panic's place with file!(), which Windows spells with backslashes
- *(live)* remember_as takes any string, owned or borrowed, as the other names kept do
- *(style)* [**breaking**] RAMP is a slice, so its stops can change without breaking code that uses it
- *(app)* a changed row sends only the cells that changed, with runs close together sent as one
- the README says what stays stable and what each platform needs, and its examples run as doc tests
- every function that can fail says how, under Errors
- *(highlight)* [**breaking**] IntoLang is sealed, and takes a String as well as a &str or a Lang
- *(app)* [**breaking**] Event::Resize names its width and height, Frame::cursor gives a TerminalCursor and TextArea::cursor a Position, so no tuple has to be remembered
- [**breaking**] gradient, sample, RAMP and blink live in style alone, as the crate root was meant to leave them
- [**breaking**] the crate root keeps what most programs use, with gradients and the like in style, animation pieces in anim and IntoLang in highlight
- [**breaking**] colours come before the text they colour everywhere, as in paint, and paint_on and paint_bg colour text on a background or the background alone
- *(prompt)* [**breaking**] lists start on what selected() says, suggestions take any strings like options do, and each list says what its answer is
- [**breaking**] builders choose a spinner and bar with spinner() and bar(), like their other options, and an iterator's progress line is as_progress()
- [**breaking**] markdown at a chosen width is markdown::wrapped, the terminal's width is terminal_width and the width text wraps to is text::content_width
- *(app)* [**breaking**] widgets take events with handle, and navigate, Scroll and GridState say whether vim's letters move them with a Keymap
- *(app)* [**breaking**] Frame::fade and fade_by mix towards the background and Frame::transparent shows the terminal's own, so neither is mistaken for faint text
- *(app)* [**breaking**] a grid's cursor row and scroll are app::GridState, handed over with Grid::state, so it isn't mistaken for the terminal's cursor
- *(app)* [**breaking**] the one-line text box is app::TextInput, so it isn't mistaken for prompt::Input
- *(term)* run the keyboard check all ten times in a plain loop
- *(live)* split into modules for commands, drawing, the timeline and tests
- *(prompt)* split into modules for the shared parts, text, lists, dates, the editor, scripts and tests
- *(processes)* a child left running by a failed test is killed, and finishing waits a minute at most
- *(live)* expect step times under a second in milliseconds, as they're now shown
- live displays, regions and spinners share one Mode and one lock helper
- *(spin)* check what spin hands back instead of comparing unit values
- *(app)* lists and scrolling views move by one table of keys
- *(app)* an input and a text area share one set of line-editing keys
- *(chart)* charts draw with the widgets' block levels instead of a copy of their own
- *(style)* Line::wrap reads its text with the shared tokenizer, so a character and its marks stay together
- leave the formatting config and contributing guide out of the package, and build docs.rs once
- link the screenshots in full, so they show on crates.io too

## [0.5.0](https://github.com/borhara/tinsel/compare/v0.4.1...v0.5.0) - 2026-11-16

### Added

- *(card)* printed cards fit what is inside by default, with fill for the full width

### Fixed

- *(card)* a wrapped row keeps two clear columns on the right, as on the left, instead of running into the border
- a question asked under a live display holds it still, so the display stops drawing over it and stops reading the keys typed into it

## [0.4.1](https://github.com/borhara/tinsel/compare/v0.4.0...v0.4.1) - 2026-11-16

### Fixed

- a card row too wide for its box wraps inside it, under its value, instead of pushing the right border out

## [0.4.0](https://github.com/borhara/tinsel/compare/v0.3.1...v0.4.0) - 2026-11-14

### Added

- debug builds stop at a table or grid row with more cells than headers
- doctor says what tinsel decided about the terminal and why, and flags settings it can't use
- capture what your own code prints, and with_color_mode for one closure on one thread
- Ctrl-C, focus and background detection on Windows, as on Unix
- *(app)* Loading, a spinner with a shimmering label and a live bar once there's progress
- *(app)* TextArea, a box for several lines that wraps, scrolls and keeps pasted line breaks
- *(app)* ScrollView and Scroll, for logs and long text, following the end as it grows
- *(app)* [**breaking**] pastes arrive whole as Event::Paste, so a pasted newline isn't Enter

### Fixed

- NaN reads as missing in charts and as no progress in bars, never as NaN%

### Other

- CI runs the tests on every platform, not just the build
- run the piped, settings, panic and Ctrl-C scenarios on Windows too

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

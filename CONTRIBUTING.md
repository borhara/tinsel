# Contributing to tinsel

Thanks for wanting to help. tinsel is small on purpose, so the bar for new things is "does
it look like it belongs, and would most CLIs want it?"

## Ground rules

- **Zero dependencies.** Not even dev-dependencies. If something needs a platform call, call
  it directly (see `src/term.rs`) and say why in a comment.
- **One visual language.** New components draw from `palette`, use the same two-space
  indent, and sit in the same columns as everything else. Run `cargo run -- showcase` and
  check yours doesn't stand out.
- **Every component degrades.** Without colour (`NO_COLOR`, pipes) it should still read well
  as plain text; with `Output::Json` anything that prints should emit an event; with
  `Output::Quiet` only things that need attention print.
- **Builders read like the output.** `Card::success("deployed").kv("url", …)`, not option
  structs.
- **Docs for every public item.** `#![warn(missing_docs)]` is on; CI treats warnings as errors.
  Prefer a short runnable example over a long explanation.

## Working on it

```bash
cargo test --all-targets && cargo test --doc   # everything CI runs
cargo clippy --all-targets
cargo fmt
cargo run -- showcase [SECTION]               # see what you changed
cargo run -- showcase --theme light            # and on a light background
cargo run -- prompts                           # the interactive questions
```

Tests compare plain text: assert on `.plain()`, or on what `tinsel::capture` caught.
Tests that need real escape codes wrap themselves in `tinsel::with_color_mode`, which sets
the mode for that thread alone (see `tests/capture.rs`). For `Live`, use `Live::headless`
and `advance` its clock, then compare `snapshot()`s (see `tests/live.rs`).

Prompts read keys through a `Ui`, so their tests type a script of keys with `Ui::script` (or
the public `prompt::script`) and check what was drawn last (see the tests in `src/prompt.rs`).
The live redraw can't be unit-tested in a pipe. Try both in a real terminal, and if you can,
on macOS, Linux and Windows. CI runs the tests on Linux alone and only builds and lints the
macOS and Windows code, so those two are tested by hand. On Windows, the tests on a real
terminal (`src/processes.rs`) don't run, since they need a pseudo-terminal.

## Recording the demo

`demo.tape` records `demo.gif` with [VHS](https://github.com/charmbracelet/vhs):

```bash
cargo build --release && vhs demo.tape
```

## Redrawing the README's pictures

`docs/dashboard.png` and `docs/services.png` are drawn by tinsel itself: the `dashboard` and
`services` examples write one frame as a web page with `--html COLSxROWS`, and headless
Chrome turns each page into a PNG:

```bash
scripts/screenshots.sh
```

## The docs on the web

[tinsel.shiv.mov](https://tinsel.shiv.mov) runs `tinsel docs` in a browser, as the `web_docs`
example: the reader run with `web_app!` (`src/web.rs`), and drawn in the page by `src/web.js`.

```bash
rustup target add wasm32-wasip1
cargo build --release --example web_docs --features web --target wasm32-wasip1
```

The page lives in the [shiv.mov](https://shiv.mov) site, whose `npm run tinsel-wasm` runs this
and copies in `target/wasm32-wasip1/release/examples/web_docs.wasm` and `src/web.js`. Run it
again after a release, so the page has the new guides and version.

## Changes

Don't edit `CHANGELOG.md`: release-plz writes it from commit messages when it prepares a
release. Write each commit as a one-line conventional commit (`feat(app): …`, `fix(table): …`,
`docs: …`) that says what a user would notice; `feat` and `fix` commits are what end up in it.

## Releasing

Releases are cut by hand, on an up-to-date `main` once CI has passed:

```bash
scripts/release.sh
```

It shows the changelog release-plz wrote and asks before it commits, tags, publishes to
crates.io, pushes and makes a GitHub release. It needs `brew install release-plz`, `cargo login`
and `gh auth login`.

# Testing what you print

Catch everything your own code prints through tinsel, as it would come out into a pipe:
messages, steps, spinners, tables and errors, in order, without colour.

```rust
#[test]
fn deploy_says_where_it_went() {
    let printed = tinsel::capture(|| my_cli::deploy("production"));
    assert!(printed.contains("●  deployed to production"));
}
```

## One piece on its own

No capturing needed: `.plain()` gives any block's text.

```rust
#[test]
fn status_table() {
    let table = tinsel::Table::new(["app", "size"]).row(["core", "34 MB"]);
    assert_eq!(table.lines().plain().lines().last(), Some("  core   34 MB"));
}
```

For the colour codes themselves, `tinsel::with_color_mode(ColorMode::TrueColor, || …)`
turns colour on for that closure, on that thread alone, so tests beside it aren't affected.

## A display on a clock you control

A `Live` display can run headless, on a clock that only moves when you say. The step list
in these guides runs just like this:

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

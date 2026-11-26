# Messages and errors

## One line each

```rust
tinsel::success("deployed to production");
tinsel::info("3 files changed");
tinsel::warn("the cache is nearly full");
tinsel::error("couldn't reach the registry");
tinsel::hint("pass --offline to skip it");
```

`title`, `rule`, `counter` and `callout` round them out, and a `Card` sums a run up:
`Card::success("deployed").kv("url", …)`.

## Errors with their causes

`Report` shows an error with its whole cause chain, and what to do about it:

```rust
tinsel::Report::from_error("couldn't start", &err)
    .hint("create app.toml, or pass --config <path>")
    .eprint();
```

```tinsel report
```

## Point at the problem

```rust
Report::new("couldn't read app.toml")
    .cause("port must be a number")
    .code(Code::around(&config, 3, 1).path("app.toml").caret(8, 8, "not a number"))
    .eprint();
```

## Panics

Drawn the same way, with the terminal put back first:

```rust
fn main() {
    tinsel::install_panic_hook();
    // …
}
```

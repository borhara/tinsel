# Spinners and progress

## One task, one spinner

```rust
let files = tinsel::spin("Counting files", || count_files());
```

`try_spin` does the same for work that returns a `Result`, ending on a ✕ line if it fails.
There are seven spinners; `sparkle` twinkles, and is the default:

```tinsel spinners
```

## A loop with a progress bar

```rust
use tinsel::ProgressIterator;

for file in files.iter().progress("Indexing") {
    index(file);
}
```

`Progress` on its own takes a total, shows the rate and the time left, and copes with a
total that's unknown or still growing.

```tinsel progress
```

## Anything else, redrawn in place

A `Region` calls your closure a few times a second and draws the `Block` it returns, a
table of services say, with lines logged above it. It's cleared when it finishes, unless
you `keep()` the last frame:

```rust
let region = tinsel::Region::new(move || status_table(&state.lock().unwrap())).keep();
region.log("api is up");
region.finish();
```

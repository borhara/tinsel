# Quickstart

```bash
cargo add tinsel
```

## One task with a spinner

Then a message saying how it went:

```rust
let files = tinsel::spin("Counting files", || count_files());
tinsel::success(&format!("found {files} files"));
```

## A step list

For anything with stages: each step starts, finishes, and says how long it took.

```rust
use tinsel::prelude::*;

let live = Live::new("deploy", "my-app → production");
let build = live.step("Build");
let upload = live.step("Upload");

build.start();
build.done();
upload.start();
upload.progress(0.5, "6 / 12 MB");
upload.done();

live.finish_with_card(Card::success("deployed").kv("url", "https://my-app.example.com"));
```

```tinsel deploy
```

## Where next

The next guides go through each part in turn: step lists, spinners and progress,
messages and errors, tables and charts, questions, the command line, full-screen apps and
pictures. The last two say how to test what you print, and which settings tinsel reads.

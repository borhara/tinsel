# Live step lists

For anything with stages. Steps can run at the same time from different threads, have
sub-steps, and log lines above the display.

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
```

What it prints, running here on the page:

```tinsel deploy
```

## Steps inside steps

```rust
let build = live.step("Build");
let targets = [build.step("linux"), build.step("macos")];   // drawn indented, folded once they pass
build.start();
std::thread::scope(|s| {
    for t in &targets {
        s.spawn(move || { t.start(); compile(t); t.done(); });
    }
});
build.done();
live.log("signed with Developer ID");                          // printed above, kept on screen
```

## Steps that don't run

Steps that don't run can say why: `lint.skip("nothing changed")`,
`upload.cancel("stopped by hand")`, or `release.wait("waiting for approval")` until someone
says go, then `release.start()`. A failed step can `start()` again, and its timer starts
over.

## Run real commands as steps

The latest line of output shows as the step's detail; if it fails, its last lines stay
under it:

```rust
use std::process::Command;

build.run(Command::new("cargo").args(["build", "--release"]))?;
push.command(Command::new("docker").args(["push", "my-app"]))
    .timeout(Duration::from_secs(120))
    .retries(2)
    .run()?;
fetch.retry(3, || download(url))?;      // any closure returning a Result
```

## Where the time went

`live.finish_with_timeline()` ends on a timeline of every step, and progress bars can be
a thin line (the default) or a waveform that lights up as it plays through:
`Live::new(…).bar(BarStyle::Waveform)`.

## It learns

Steps remember their durations in `~/.cache/tinsel`, one small file per program. A step
with no progress of its own gets an estimate bar from its recent median, and says
`slower than usual` when it is. `tinsel timings my-app` shows what's been learned.

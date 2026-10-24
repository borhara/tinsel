//! A live step list: declare the plan, run each step, close with a card.
//!
//!   cargo run --example deploy
//!   cargo run --example deploy -- --fail

use std::thread::sleep;
use std::time::Duration;

use tinsel::prelude::*;

fn main() {
    let fail = std::env::args().any(|a| a == "--fail");

    // declaring every step up front shows the whole plan from the first frame
    let live = Live::new("deploy", "my-app → production");
    let build = live.step("Build");
    let upload = live.step("Upload");
    let release = live.step("Release");

    build.start();
    for file in ["main.rs", "config.rs", "assets"] {
        build.detail(format!("compiling {file}"));
        sleep(Duration::from_millis(400));
    }
    build.done();

    upload.start();
    for i in 1..=40 {
        let f = i as f32 / 40.0;
        upload.progress(f, format!("{:.1} / 12.0 MB", f * 12.0));
        sleep(Duration::from_millis(40));
    }
    upload.done();

    release.start();
    sleep(Duration::from_millis(800));
    if fail {
        release.fail("the health check timed out after 30s");
        live.finish_with_card(Card::failure("release failed").kv("logs", "my-app logs --since 5m"));
        std::process::exit(1);
    }
    release.done();

    let took = format!("in {:.1}s", live.elapsed().as_secs_f32());
    live.finish_with_card(
        Card::success("deployed")
            .aside(&took)
            .kv("url", "https://my-app.example.com"),
    );
}

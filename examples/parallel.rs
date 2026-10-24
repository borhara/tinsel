//! Steps running at the same time, with sub-steps, a log line, and a panic hook in case
//! something goes badly wrong.
//!
//!   cargo run --example parallel
//!   cargo run --example parallel -- --json     the same run as JSON events

use std::thread::{scope, sleep};
use std::time::Duration;

use tinsel::prelude::*;

fn main() {
    tinsel::install_panic_hook();
    if std::env::args().any(|a| a == "--json") {
        tinsel::set_output(tinsel::Output::Json);
    }

    let live = Live::new("release", "v2.1.0");
    let build = live.step("Build");
    // sub-steps are drawn under their parent, and fold away once they've all passed
    let targets: Vec<Step> = ["linux-x64", "linux-arm64", "macos-arm64"]
        .into_iter()
        .map(|t| build.step(t))
        .collect();
    let publish = live.step("Publish");

    build.start();
    // Step handles are cheap to clone and can move to other threads
    scope(|s| {
        for (i, target) in targets.iter().enumerate() {
            s.spawn(move || {
                target.start();
                for stage in ["compiling", "linking", "stripping"] {
                    target.detail(stage);
                    sleep(Duration::from_millis(350 + 150 * i as u64));
                }
                if i == 2 {
                    // a line worth keeping, printed above the display without disturbing it
                    target.log("signed with Developer ID");
                }
                target.done();
            });
        }
    });
    build.done();

    publish.start();
    for i in 1..=30 {
        let f = i as f32 / 30.0;
        publish.progress(f, format!("{} of 3 archives", (f * 3.0).ceil()));
        sleep(Duration::from_millis(40));
    }
    publish.done();

    live.finish_with_card(Card::success("released").kv("tag", "v2.1.0"));
}

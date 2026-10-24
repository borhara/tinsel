//! The smallest useful tinsel program: a spinner around some work, then a message.
//!
//!   cargo run --example hello

use std::thread::sleep;
use std::time::Duration;

fn main() {
    let files = tinsel::spin("Counting files", || {
        sleep(Duration::from_millis(1200));
        412
    });
    tinsel::success(&format!("found {files} files"));
    tinsel::hint("run with --verbose to list them");
}

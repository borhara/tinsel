//! A read-only status command: a table, key/value details, a chart and a tree.
//!
//!   cargo run --example status
//!   cargo run --example status | cat     # the same, as plain text

use tinsel::prelude::*;
use tinsel::{meter, sparkline};

fn main() {
    title("my-app", Some("production · eu-west-2"));

    Table::new(["service", "status", "p95", "requests"])
        .row([
            "api".to_string(),
            badge("healthy", palette::SAGE),
            "84ms".into(),
            "1.2k/s".into(),
        ])
        .row([
            "worker".to_string(),
            badge("degraded", palette::AMBER),
            "1.9s".into(),
            "40/s".into(),
        ])
        .row([
            "cron".to_string(),
            badge("idle", palette::SOFT),
            "-".into(),
            "0/s".into(),
        ])
        .print();
    println!();

    pairs(&[
        ("version", "v2.14.0"),
        ("deployed", "3h ago by ci"),
        ("region", "eu-west-2"),
    ])
    .print();
    println!();

    let latency = [84.0, 90.0, 88.0, 120.0, 340.0, 210.0, 95.0, 87.0, 86.0, 91.0];
    println!("  latency  {}", sparkline(&latency));
    println!("{}", meter("memory ", 61.0, 100.0, 24));
    println!("{}", meter("disk   ", 88.0, 100.0, 24));
    println!();

    Tree::new("my-app")
        .child(Tree::new("api").leaf("eu-west-2a").leaf("eu-west-2b"))
        .child(Tree::new("worker").leaf("eu-west-2a"))
        .print();
}

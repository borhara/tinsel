//! Help rows use the whole terminal. The width comes from `COLUMNS` only when stdout isn't a
//! terminal (otherwise the terminal's own size wins), so the help is drawn by this test binary
//! run again as a child, with its output piped and `COLUMNS` set.

use std::process::Command;

use tinsel::{ColorMode, Help, set_color_mode};

const ABOUT: &str = "what's deployed in each environment, from AWS, with the deployment CLI";

/// The child's half: prints the help, at whatever width `COLUMNS` says.
#[test]
#[ignore = "run by the test below, in a child process"]
fn print_help() {
    set_color_mode(ColorMode::None);
    println!(
        "{}",
        Help::new("tool", "does things")
            .command("status", ABOUT)
            .lines()
            .plain()
    );
}

/// The help as printed `columns` wide.
fn help_at(columns: &str) -> String {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["print_help", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env("COLUMNS", columns)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn help_rows_only_wrap_when_the_terminal_is_too_narrow() {
    let wide = help_at("160");
    assert!(wide.contains(&format!("status  {ABOUT}")), "{wide}");
    let narrow = help_at("60");
    assert!(narrow.contains("status") && !narrow.contains(ABOUT), "{narrow}");
}

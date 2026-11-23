//! What `Cli::parse` and `CliError::exit` print, and the status they exit with. They end the
//! process, so each is run in this test binary run again as a child.

use std::process::Command;

use tinsel::Cli;

const MODE: &str = "TINSEL_CLI_EXIT";

fn cli() -> Cli {
    Cli::new("tool", "does things").version("2.1")
}

/// The child's half: parses or exits as `TINSEL_CLI_EXIT` says.
#[test]
#[ignore = "run by the tests below, in a child process"]
fn exit_child() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    match std::env::var(MODE).as_deref() {
        // its own arguments are the test harness's, which the tool doesn't take
        Ok("parse") => drop(cli().parse()),
        Ok("value") => {
            let cli = cli().flag("--port <PORT>", "where to listen");
            let args = cli.try_parse_from(["--port", "eighty"]).unwrap();
            args.value_as::<u16>("port");
        }
        Ok("unversioned") => Cli::new("tool", "x").try_parse_from(["-h"]).unwrap_err().exit(),
        Ok(other) => cli().try_parse_from([other]).unwrap_err().exit(),
        Err(_) => {}
    }
}

/// The child's status, stdout and stderr, run in `mode`.
fn run(mode: &str) -> (i32, String, String) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["exit_child", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(MODE, mode)
        .output()
        .unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (out.status.code().unwrap(), text(out.stdout), text(out.stderr))
}

#[test]
fn a_mistake_is_reported_on_stderr_with_status_2() {
    let (status, _, err) = run("parse");
    assert_eq!(status, 2, "{err}");
    assert!(err.contains("✕  there's no --exact flag"), "{err}");
    let (status, _, err) = run("value");
    assert_eq!(status, 2);
    assert!(err.contains("--port can't be “eighty”"), "{err}");
    let (status, _, err) = run("--colour");
    assert_eq!(status, 2);
    assert!(err.contains("there's no --colour flag"), "{err}");
}

#[test]
fn help_and_version_print_on_stdout_and_succeed() {
    let (status, out, _) = run("--help");
    assert_eq!(status, 0);
    assert!(out.contains("usage  tool") && out.contains("-V, --version"), "{out}");
    let (status, out, _) = run("unversioned");
    assert_eq!(status, 0);
    assert!(out.contains("-h, --help") && !out.contains("--version"), "{out}");
    let (status, out, _) = run("--version");
    assert_eq!(status, 0);
    assert!(out.contains("tool 2.1\n"), "{out}");
}

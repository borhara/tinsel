//! The README's examples that can't be doc tests: fragments, leaning on helpers and values
//! the README leaves out. Each is copied here between `// readme` and `// readme end`, with
//! what it needs around it, so it has to compile, and `copies_match_the_readme` checks that
//! the copies are still the README's own, in order.

// the fragments bind values the README doesn't go on to use, and keep its way of writing things
#![allow(unused, clippy::redundant_closure)]

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinsel::{Block, Live, Step};

type Result = std::result::Result<(), Box<dyn Error>>;

#[rustfmt::skip]
fn spinner() -> Result {
    fn count_files() -> usize {
        3
    }
    // readme
    let files = tinsel::spin("Counting files", || count_files());
    tinsel::success(&format!("found {files} files"));
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn step_list() -> Result {
    fn compile(_: &Step) {}
    let live = Live::new("deploy", "my-app");
    // readme
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
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn commands() -> Result {
    fn download(_: &str) -> std::io::Result<()> {
        Ok(())
    }
    let live = Live::new("deploy", "my-app");
    let (build, push, fetch) = (live.step("Build"), live.step("Push"), live.step("Fetch"));
    let url = "https://example.com";
    // readme
    use std::process::Command;

    build.run(Command::new("cargo").args(["build", "--release"]))?;
    push.command(Command::new("docker").args(["push", "my-app"]))
        .timeout(Duration::from_secs(120))
        .retries(2)
        .run()?;
    fetch.retry(3, || download(url))?;      // any closure returning a Result
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn region() -> Result {
    struct State;
    fn status_table(_: &State) -> Block {
        Block::new()
    }
    let state = Arc::new(Mutex::new(State));
    // readme
    let region = tinsel::Region::new(move || status_table(&state.lock().unwrap())).keep();
    region.log("api is up");
    region.finish();
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn progress_bar() -> Result {
    fn index(_: &str) {}
    let files = ["a.rs"];
    // readme
    use tinsel::ProgressIterator;

    for file in files.iter().progress("Indexing") {
        index(file);
    }
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn select() -> Result {
    fn deploy(_: usize) {}
    // readme
    use tinsel::prompt;

    match prompt::select("Deploy to", &["staging", "production"]) {
        Ok(i) => deploy(i),
        Err(prompt::Error::Cancelled) => std::process::exit(130),
        Err(e) => tinsel::Report::from_error("couldn't ask", &e).eprint(),
    }
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn text_questions() -> Result {
    use tinsel::prompt;
    let regions = ["eu-west-2", "us-east-1"];
    // readme
    let port = prompt::Input::new("Port")
        .placeholder("8080")
        .validate(|s| s.parse::<u16>().map(|_| ()).map_err(|_| "a port is 1-65535".into()))
        .ask()?;
    let replicas: u32 = prompt::number("Replicas", Some(3))?;
    let live_on = prompt::date("Go live on", None)?;                  // a calendar
    let notes = prompt::editor("Release notes", "## What's new\n")?;   // opens $EDITOR
    let region = prompt::Input::new("Region").suggestions(regions).ask()?;  // Tab completes
    let path = prompt::Input::new("Config").paths().ask()?;                  // like a shell
    prompt::confirm_typing("Delete my-app?", "my-app")?;                       // type it to go on
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn form() -> Result {
    fn deploy(_: &str, _: usize, _: bool) {}
    // readme
    use tinsel::prompt::{Confirm, Form, Input, Select};

    let answers = Form::new()
        .question("name", Input::new("Project name").default("my-app"))
        .question("target", Select::new("Deploy to").options(["staging", "production"]))
        .question("tag", Confirm::new("Tag the release?").default(true))
        .ask()?;
    deploy(answers.text("name"), answers.choice("target"), answers.yes("tag"));
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn grouped_lists() -> Result {
    use tinsel::prompt;
    // readme
    let service = prompt::Select::new("Service")
        .heading("Frontend").option("web").disabled("docs", "deploys paused")
        .heading("Backend").options(["api", "worker"])
        .back()                                       // ← returns prompt::Error::Back
        .ask()?;
    let version = prompt::Dropdown::new("Roll back to")
        .option("v1.2.0").note("3 days ago").badge("latest")
        .option("v1.1.0").note("2 weeks ago")
        .allow_custom()                               // or type a version that isn't listed
        .ask()?;
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn command_line() -> Result {
    // readme
    let cli = tinsel::Cli::new("deploy", "ship an app to the cloud")
        .version(env!("CARGO_PKG_VERSION"))
        .flag("-r, --region <NAME>", "where to deploy")
        .flag("-q, --quiet", "only print problems")
        .flag("-n, --replicas <N>", "how many to run")
        .arg("<APP>", "what to ship")
        .command(tinsel::Cli::new("status", "what's running where"));
    let args = cli.parse();               // --help, --version and mistakes print and exit

    let region = args.value("region").unwrap_or("eu-west-2");
    let replicas: u32 = args.value_as("replicas").unwrap_or(1);
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn full_screen() -> Result {
    // readme
    use tinsel::app::{self, App, Frame, Panel, Plot, Stat, Size::*};

    struct Dash { latency: Vec<f64> }

    impl App for Dash {
        fn view(&self, frame: &mut Frame) {
            let [stat, chart] = frame.area().columns_gap(1, [Fixed(20), Fill(1)]);
            frame.render(stat, Panel::new("p95").body(Stat::new("latency", "84ms")));
            frame.render(chart, Panel::new("latency").body(Plot::new(&self.latency)).focused(true));
        }
    }

    app::run(Dash { latency: vec![84.0, 90.0, 120.0, 340.0, 95.0] })?;   // q or Esc quits
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn report() -> Result {
    let err = std::io::Error::other("app.toml not found");
    // readme
    tinsel::Report::from_error("couldn't start", &err)
        .hint("create app.toml, or pass --config <path>")
        .eprint();
    // readme end
    Ok(())
}

#[rustfmt::skip]
fn report_with_code() -> Result {
    use tinsel::{Code, Report};
    let config = "[server]\nhost = \"0.0.0.0\"\nport = eighty\n".to_string();
    // readme
    Report::new("couldn't read app.toml")
        .cause("port must be a number")
        .code(Code::around(&config, 3, 1).path("app.toml").caret(8, 8, "not a number"))
        .eprint();
    // readme end
    Ok(())
}

mod my_cli {
    pub fn deploy(to: &str) {
        tinsel::success(&format!("deployed to {to}"));
    }
}

#[rustfmt::skip]
// readme
#[test]
fn deploy_says_where_it_went() {
    let printed = tinsel::capture(|| my_cli::deploy("production"));
    assert!(printed.contains("●  deployed to production"));
}
// readme end

/// Each block between the markers in `source`, without the indent its marker has.
fn copies(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut open: Option<(usize, Vec<&str>)> = None;
    for line in source.lines() {
        match (line.trim(), &mut open) {
            ("// readme", None) => open = Some((line.len() - line.trim_start().len(), Vec::new())),
            ("// readme end", Some((_, lines))) => {
                found.push(lines.join("\n"));
                open = None;
            }
            (_, Some((indent, lines))) => lines.push(line.get(*indent..).unwrap_or("")),
            _ => {}
        }
    }
    found
}

/// Each code block in the README that its doc tests skip.
fn fragments(readme: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in readme.lines() {
        match (line, &mut open) {
            ("```rust,ignore", None) => open = Some(Vec::new()),
            ("```", Some(lines)) => {
                found.push(lines.join("\n"));
                open = None;
            }
            (_, Some(lines)) => lines.push(line),
            _ => {}
        }
    }
    found
}

#[test]
fn copies_match_the_readme() {
    let readme = fragments(include_str!("../README.md"));
    let here = copies(include_str!("readme.rs"));
    assert!(!readme.is_empty());
    for (i, (theirs, ours)) in readme.iter().zip(&here).enumerate() {
        assert_eq!(
            ours,
            theirs,
            "example {} in the README has changed: copy it here again",
            i + 1
        );
    }
    assert_eq!(
        here.len(),
        readme.len(),
        "the README has {} examples it can't run, and tests/readme.rs copies {}",
        readme.len(),
        here.len()
    );
}

//! The tinsel CLI: a guided tour of the library, a simulated deploy and a prompt demo.
//! Nothing real happens.
//!
//!   tinsel showcase [SECTION] [--spinner NAME]   every component (or one section), then the deploy
//!   tinsel demo [--fail] [--spinner NAME]        just the deploy
//!   tinsel prompts                               the interactive questions
//!   tinsel timings [APP]                         what Live has learned about step durations
//!   tinsel help                                  all of this, with a little entrance
//!
//! Add `--theme light|dark` or `--accent pink|violet|blue|teal|orange` to any of them to
//! preview a look, `--bar waveform` to see the waveform progress bar in the deploy, and
//! `--json` or `--quiet` to see the other output modes, and `--stderr` to draw on stderr.
//!
//! Spinner names: sparkle, dots, comet, wave, arc, orbit, pulse.

use std::io::{IsTerminal, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tinsel::{
    Accent, Border, Callout, Card, Code, Help, Live, Output, Progress, ProgressIterator, RAMP, Region, Report, Spinner,
    Step, Table, Target, Tone, Tree, badge, bars, callout, columns, counter, diff, error, format, gradient, heatmap,
    hint, histogram, info, link, live_bar, live_waveform, meter, paint, pairs, palette, prompt, rule, shimmer,
    sparkline, spin, success, title, try_spin, warn,
};

/// What `tinsel help` shows, with an entrance when it's on a terminal.
fn help() -> Help {
    let sections: Vec<&str> = SECTIONS.iter().map(|(n, _, _)| *n).collect();
    Help::new(
        "tinsel",
        "clean terminal output for Rust CLIs, with a little sparkle. This is its tour: nothing real happens.",
    )
    .version(env!("CARGO_PKG_VERSION"))
    .usage("tinsel <COMMAND> [OPTIONS]")
    .command(
        "showcase [SECTION]",
        format!("every component, then a live deploy. Sections: {}", sections.join(", ")),
    )
    .command("demo [--fail]", "just the deploy, or one that fails part way")
    .command("prompts", "the interactive questions")
    .command("timings [APP]", "what Live has learned about how long steps take")
    .command("help", "this")
    .flag("--spinner <NAME>", "sparkle, dots, comet, wave, arc, orbit or pulse")
    .flag("--theme <light|dark>", "preview the other theme")
    .flag(
        "--accent <NAME>",
        "pink, violet, blue, teal, orange, or a hue in degrees",
    )
    .flag("--bar <line|waveform>", "how the deploy draws its progress bars")
    .flag("--json", "every message, step and error as a line of JSON")
    .flag("--quiet", "only what needs attention")
    .flag("--stderr", "draw on stderr, leaving stdout for something else")
    .example("tinsel showcase table", "just the tables")
    .example(
        "tinsel demo --bar waveform --accent teal",
        "the deploy, in teal, with waveform progress",
    )
    .example(
        "tinsel demo --json | jq -r .event",
        "what a program reading tinsel's JSON sees",
    )
    .footer("docs and examples: https://docs.rs/tinsel")
}

/// Say what was wrong with the command line, point at help, and stop.
fn usage_error(problem: &str) -> ! {
    error(problem);
    hint("tinsel help shows what it can do");
    std::process::exit(2);
}

/// Flags that take a value, so the value isn't mistaken for a section or app name.
const VALUE_FLAGS: [&str; 4] = ["--spinner", "--theme", "--accent", "--bar"];

type Section = (&'static str, &'static str, fn());

/// Every showcase section: name, one-line blurb, and what it draws.
const SECTIONS: &[Section] = &[
    (
        "palette",
        "pinks for accent, quiet greys for everything else",
        palette_section,
    ),
    (
        "accents",
        "the same sparkle in another hue, for a CLI with its own brand",
        accents_section,
    ),
    (
        "spinners",
        "pick one with --spinner; sparkle is the tinsel one",
        spinners_section,
    ),
    (
        "shimmer",
        "a soft light with a little glitter, across the running step",
        shimmer_section,
    ),
    (
        "bar",
        "a thin line with a glint running along it, or a waveform that plays through",
        bar_section,
    ),
    (
        "messages",
        "one-liners for the things a CLI says most",
        messages_section,
    ),
    ("rules", "headings and dividers", rules_section),
    ("callouts", "blocks set off by a coloured bar", callouts_section),
    ("report", "an error with what caused it, and the fix", report_section),
    ("badges", "small pills, and links you can click", badges_section),
    (
        "table",
        "columns sized to fit; numbers right-align themselves",
        table_section,
    ),
    ("tree", "a hierarchy with thin guides", tree_section),
    ("pairs", "aligned keys and values, and ls-style columns", pairs_section),
    (
        "charts",
        "sparklines, bars, meters, histograms and heatmaps",
        charts_section,
    ),
    ("diff", "what changed, line by line", diff_section),
    (
        "code",
        "an excerpt with the line that matters marked, and a caret under the problem",
        code_section,
    ),
    ("format", "numbers and times the way people say them", format_section),
    ("spin", "one task with a spinner, then a single line", spin_section),
    (
        "progress",
        "a standalone bar with rate and time left, or around any loop",
        progress_section,
    ),
    ("card", "a summary that closes the run", card_section),
    (
        "commands",
        "real commands as steps: their latest output as the detail, the tail if they fail",
        commands_section,
    ),
    (
        "timeline",
        "when each step ran, on one axis: what overlapped, and where the time went",
        timeline_section,
    ),
    (
        "states",
        "steps that are skipped, cancelled, waiting on someone, or run again",
        states_section,
    ),
    (
        "region",
        "any block redrawn in place, with lines logged above it",
        region_section,
    ),
    (
        "markdown",
        "changelogs and long help, drawn in the same colours",
        markdown_section,
    ),
    ("json", "API responses and config, indented and coloured", json_section),
];

fn main() {
    tinsel::install_panic_hook();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `cargo run tinsel help` hands the program its own name first; skip it
    if args.first().is_some_and(|a| a == "tinsel") {
        args.remove(0);
    }
    let spinner = spinner_arg(&args);
    if let Some(name) = flag_value(&args, "--accent") {
        match Accent::from_name(&name) {
            Some(accent) => tinsel::set_accent(accent),
            None => usage_error(&format!("there's no accent called {name}")),
        }
    }
    if args.iter().any(|a| a == "--stderr") {
        tinsel::set_target(Target::Stderr);
    }
    if args.iter().any(|a| a == "--json") {
        tinsel::set_output(Output::Json);
    } else if args.iter().any(|a| a == "--quiet") {
        tinsel::set_output(Output::Quiet);
    }
    match flag_value(&args, "--theme").as_deref() {
        None => {}
        Some("light") => tinsel::set_theme(tinsel::Theme::Light),
        Some("dark") => tinsel::set_theme(tinsel::Theme::Dark),
        Some(other) => usage_error(&format!("the theme is light or dark, not {other}")),
    }
    // the first word after the command that isn't a flag or a flag's value
    let operand = args
        .iter()
        .enumerate()
        .skip(1)
        .find(|(i, a)| !a.starts_with("--") && !VALUE_FLAGS.contains(&args[i - 1].as_str()))
        .map(|(_, a)| a.as_str());
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return help().animate();
    }
    match args.first().map(String::as_str) {
        None | Some("help") => help().animate(),
        Some("showcase") => showcase(operand, spinner),
        Some("demo") => demo(args.iter().any(|a| a == "--fail"), spinner),
        Some("prompts") => prompts(),
        Some("timings") => {
            println!();
            tinsel::timings(operand).print();
            println!();
        }
        Some(other) => usage_error(&format!("there's no command called {other}")),
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    Some(args.get(i + 1).cloned().unwrap_or_default())
}

fn spinner_arg(args: &[String]) -> Spinner {
    let Some(i) = args.iter().position(|a| a == "--spinner") else {
        return Spinner::default();
    };
    let name = args.get(i + 1).map_or("", String::as_str);
    match Spinner::from_name(name) {
        Some(spinner) => spinner,
        None => usage_error(&format!("there's no spinner called {name}")),
    }
}

/// Redraw a block of lines in place for `secs`, then leave its final frame behind.
fn animate(secs: f32, frame: impl Fn(f32) -> String) {
    if !std::io::stdout().is_terminal() {
        println!("{}", frame(secs));
        return;
    }
    let start = Instant::now();
    let mut out = std::io::stdout();
    let mut drawn = 0;
    let mut draw = |text: String, out: &mut std::io::Stdout| {
        let up = if drawn > 0 {
            format!("\x1b[{drawn}F")
        } else {
            String::new()
        };
        let body: String = text.lines().map(|l| format!("{l}\x1b[K\n")).collect();
        drawn = text.lines().count();
        let _ = write!(out, "{up}{body}");
        let _ = out.flush();
    };
    let _ = write!(out, "\x1b[?25l");
    while start.elapsed().as_secs_f32() < secs {
        draw(frame(start.elapsed().as_secs_f32()), &mut out);
        sleep(Duration::from_millis(33));
    }
    draw(frame(secs), &mut out);
    let _ = write!(out, "\x1b[?25h");
    let _ = out.flush();
}

fn heading(title: &str, blurb: &str) {
    println!();
    println!("  {}  {}", paint(palette::ROSE, title), paint(palette::MUTED, blurb));
    println!();
}

fn showcase(only: Option<&str>, spinner: Spinner) {
    if let Some(name) = only {
        let Some((name, blurb, draw)) = SECTIONS.iter().find(|(n, _, _)| *n == name) else {
            let names: Vec<&str> = SECTIONS.iter().map(|(n, _, _)| *n).collect();
            usage_error(&format!(
                "there's no section called {name}. Try one of: {}",
                names.join(", ")
            ));
        };
        heading(name, blurb);
        draw();
        return;
    }
    animate(1.6, |t| {
        format!(
            "\n  {}  {}",
            shimmer("tinsel", t * 1.5),
            paint(palette::MUTED, "clean terminal output")
        )
    });
    for (name, blurb, draw) in SECTIONS {
        heading(name, blurb);
        draw();
    }
    heading("live", "all of it together");
    sleep(Duration::from_millis(600));
    demo(false, spinner);
}

fn palette_section() {
    let accents = [
        ("berry", palette::BERRY),
        ("rose", palette::ROSE),
        ("blush", palette::BLUSH),
        ("petal", palette::PETAL),
    ];
    let neutrals = [
        ("text", palette::TEXT),
        ("soft", palette::SOFT),
        ("muted", palette::MUTED),
        ("line", palette::LINE),
    ];
    let signals = [
        ("sage", palette::SAGE),
        ("amber", palette::AMBER),
        ("red", palette::RED),
    ];
    for row in [&accents[..], &neutrals[..], &signals[..]] {
        let cells: Vec<String> = row
            .iter()
            .map(|(name, c)| format!("{} {}", paint(*c, "●"), paint(palette::SOFT, &format!("{name:<9}"))))
            .collect();
        println!("  {}", cells.join(""));
    }
    println!();
    println!("  {}", gradient(&"━".repeat(36), &RAMP, 0.0));
}

fn accents_section() {
    let accents = [
        ("pink", Accent::Pink),
        ("violet", Accent::Violet),
        ("blue", Accent::Blue),
        ("teal", Accent::Teal),
        ("orange", Accent::Orange),
    ];
    // colours are decided when a string is painted, so each line is built under its accent
    let lines: Vec<String> = accents
        .iter()
        .map(|(name, accent)| {
            tinsel::set_accent(*accent);
            format!(
                "  {}  {}  {}  {}",
                paint(palette::MUTED, &format!("{name:<7}")),
                Spinner::Sparkle.frame(1.3),
                gradient("tinsel", &RAMP, 0.0),
                badge("live", palette::ROSE)
            )
        })
        .collect();
    tinsel::set_accent(Accent::Pink);
    for line in lines {
        println!("{line}");
    }
    println!();
    hint("tinsel::set_accent(Accent::Blue), or TINSEL_ACCENT=blue");
}

fn spinners_section() {
    animate(4.0, |t| {
        Spinner::ALL
            .iter()
            .map(|sp| {
                let pad = " ".repeat(6 - sp.width());
                let default = if *sp == Spinner::default() { "  (default)" } else { "" };
                format!(
                    "  {}{pad}{}{}",
                    sp.frame(t),
                    paint(palette::TEXT, &format!("{:<7}", sp.name())),
                    paint(palette::MUTED, default)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
}

fn shimmer_section() {
    animate(2.5, |t| format!("  {}", shimmer("Uploading bundle to production", t)));
}

fn bar_section() {
    animate(2.5, |t| {
        let p = (t / 2.3).min(1.0);
        let pct = paint(palette::SOFT, &format!("{:>3.0}%", p * 100.0));
        // the waveform plays through like an audio track
        format!(
            "  {}  {pct}\n\n  {}  {pct}",
            live_bar(p, 36, t),
            live_waveform(p, 36, t)
        )
    });
    println!();
    hint("Live::with_bar(BarStyle::Waveform), or Progress::with_bar for a standalone bar");
}

fn messages_section() {
    success("Deployed my-app to production");
    info("Using the eu-west-2 region");
    warn("app.toml has a key tinsel doesn't know: retries");
    error("Couldn't reach the registry");
    hint("run my-app login first");
    println!();
    for (i, step) in ["Fetching", "Building", "Signing"].iter().enumerate() {
        counter(i + 1, 3, step);
    }
}

fn rules_section() {
    title("Deploying the stack", Some("3 services · staging and production"));
    println!("{}", rule("build"));
    println!("{}", rule(""));
}

fn callouts_section() {
    for (kind, head, body) in [
        (
            Callout::Note,
            "Tokens last 7 days",
            "Access tokens expire after a week, so sign in again weekly.",
        ),
        (
            Callout::Tip,
            "Faster rebuilds",
            "Pass --incremental to reuse the last build's cache.",
        ),
        (
            Callout::Warning,
            "No staging database",
            "The migration step was skipped. db/README.md says how to set one up.",
        ),
        (
            Callout::Danger,
            "This deletes data",
            "Resetting the profile removes every saved session on this machine.",
        ),
    ] {
        callout(kind, head, body).print();
        println!();
    }
}

fn report_section() {
    Report::new("couldn't deploy my-app")
        .cause("AccessDenied: iam:PassRole on my-app-role")
        .cause("the role has no trust policy for Lambda")
        .hint("attach one with: my-app role fix")
        .lines()
        .print();
    println!();
    let config = "[server]\nhost = \"0.0.0.0\"\nport = \"eighty\"\nworkers = 4\n";
    Report::new("couldn't read app.toml")
        .cause("port must be a number from 1 to 65535")
        .code(Code::around(config, 3, 1).path("app.toml").caret(8, 8, "not a number"))
        .hint("try port = 8080")
        .lines()
        .print();
}

fn badges_section() {
    println!(
        "  {}  {}  {}  {}  {}",
        badge("live", palette::ROSE),
        badge("passing", palette::SAGE),
        badge("beta", palette::AMBER),
        badge("failed", palette::RED),
        badge("v0.1.0", palette::SOFT)
    );
    println!();
    println!(
        "  {}  {}",
        paint(palette::MUTED, "docs"),
        link("tinsel on crates.io", "https://crates.io/crates/tinsel")
    );
}

fn table_section() {
    Table::new(["service", "status", "environments", "size", "deploys"])
        .row([
            "core".to_string(),
            badge("live", palette::ROSE),
            "dev, staging, prod".into(),
            "34.2 MB".into(),
            "128".into(),
        ])
        .row([
            "queue".to_string(),
            badge("building", palette::AMBER),
            "dev, staging".into(),
            "12.8 MB".into(),
            "64".into(),
        ])
        .row([
            "worker".to_string(),
            badge("failed", palette::RED),
            "dev".into(),
            "8.1 MB".into(),
            "9".into(),
        ])
        .striped()
        .print();
    println!();
    Table::new(["region", "requests", "p95", "notes"])
        .row(["eu-west-2", "1.2M", "84ms", "primary"])
        .row([
            "us-east-1",
            "880k",
            "112ms",
            "failover, and the busiest at US peak hours",
        ])
        .row(["ap-south-1", "2.4M", "1.3s", "cold starts"])
        .sort_by_desc(1)
        .highlight(0)
        .wrap(3, 22)
        .footer(["3 regions", "4.5M", "", ""])
        .title("traffic")
        .border(Border::Rounded)
        .print();
}

fn tree_section() {
    Tree::new("my-app")
        .child(Tree::new("api").leaf("src").leaf("tests").leaf("Dockerfile"))
        .child(Tree::new("web").leaf("src").leaf("public"))
        .child(Tree::new("tools").child(Tree::new("scripts").leaf("setup.sh").leaf("release.sh")))
        .leaf("README.md")
        .print();
}

fn pairs_section() {
    pairs(&[
        ("region", "eu-west-2"),
        ("runtime", "python3.14 · arm64"),
        ("memory", "512 MB"),
        ("function url", "https://my-app.example.com"),
    ])
    .print();
    println!();
    columns(&[
        "Cargo.toml",
        "README.md",
        "src",
        "target",
        "chart.rs",
        "format.rs",
        "layout.rs",
        "live.rs",
        "message.rs",
        "prompt.rs",
        "spin.rs",
        "style.rs",
        "table.rs",
        "term.rs",
        "text.rs",
        "tree.rs",
        "widgets.rs",
    ])
    .print();
}

fn charts_section() {
    let week = [3.0, 5.0, 4.0, 8.0, 12.0, 9.0, 14.0, 11.0, 7.0, 10.0, 15.0, 13.0];
    println!(
        "  {}  {}  {}",
        paint(palette::TEXT, "builds"),
        sparkline(&week),
        paint(palette::MUTED, "last 12 days")
    );
    println!();
    bars(&[("core", 34.2), ("queue", 12.8), ("worker", 8.1), ("server", 3.4)], 28).print();
    println!();
    println!("{}", meter("disk  ", 41.0, 100.0, 28));
    println!("{}", meter("memory", 72.0, 100.0, 28));
    println!("{}", meter("quota ", 93.0, 100.0, 28));
    println!();
    let latencies: Vec<f64> = (0..400)
        .map(|i| {
            let x = (i as f64 * 0.618_034).fract();
            40.0 + 60.0 * x * x + 25.0 * ((i as f64) * 1.7).sin().abs()
        })
        .collect();
    println!("  {}", paint(palette::MUTED, "response times, ms"));
    histogram(&latencies, 16, 4).print();
    println!();
    println!("  {}", paint(palette::MUTED, "deploys by hour, last week"));
    let day = |seed: f64| -> Vec<f64> {
        (0..12)
            .map(|h| ((h as f64 - 5.5).abs() * -0.4 + 3.0 + (seed * h as f64).sin()).max(0.0))
            .collect()
    };
    let (mon, tue, wed, thu, fri) = (day(1.1), day(2.3), day(0.7), day(3.1), day(1.9));
    heatmap(&[
        ("mon", &mon),
        ("tue", &tue),
        ("wed", &wed),
        ("thu", &thu),
        ("fri", &fri),
    ])
    .print();
}

fn diff_section() {
    diff(
        "region = \"us-east-1\"\nmemory = 256\nruntime = \"python3.12\"",
        "region = \"eu-west-2\"\nmemory = 256\nruntime = \"python3.14\"\nkeep-warm = true",
    )
    .print();
}

fn code_section() {
    let src = "fn main() {\n    let live = Live::new(\"deploy\", target);\n    let build = live.step(\"Build\");\n    build.start();\n}";
    Code::new(src).first_line(12).highlight(14).lang("rust").print();
    println!();
    let config = "[deploy]\nregion = \"eu-west-2\"\nmemory = 5l2\n";
    Code::around(config, 3, 1)
        .path("app.toml")
        .caret(10, 3, "expected a number")
        .print();
}

fn format_section() {
    pairs(&[
        (
            "bytes",
            &format!(
                "{}  ·  {}  ·  {}",
                format::bytes(512),
                format::bytes(34_200_000),
                format::bytes(1_100_000_000)
            ),
        ),
        (
            "durations",
            &format!(
                "{}  ·  {}  ·  {}",
                format::duration(Duration::from_millis(340)),
                format::duration(Duration::from_secs(187)),
                format::duration(Duration::from_secs(3720))
            ),
        ),
        (
            "counts",
            &format!(
                "{}  ·  {}  ·  {}",
                format::count(999),
                format::count(1_234),
                format::count(5_600_000)
            ),
        ),
        (
            "ago",
            &format!(
                "{}  ·  {}  ·  {}",
                format::ago(Duration::from_secs(3)),
                format::ago(Duration::from_secs(300)),
                format::ago(Duration::from_secs(200_000))
            ),
        ),
        (
            "plurals",
            &format!(
                "{}  ·  {}",
                format::plural(1, "file"),
                format::plural_with(3, "child", "children")
            ),
        ),
    ])
    .print();
}

fn spin_section() {
    spin("Resolving packages", || sleep(Duration::from_millis(1400)));
    let _ = try_spin("Contacting registry", || {
        sleep(Duration::from_millis(1100));
        Err::<(), _>("connection refused (registry.example.com:443)")
    });
}

fn progress_section() {
    let p = Progress::new("Downloading", 48_000_000).bytes();
    for _ in 0..60 {
        p.inc(800_000);
        sleep(Duration::from_millis(40));
    }
    p.finish();
    let files: Vec<u32> = (0..240).collect();
    for _ in files.iter().progress("Indexing files") {
        sleep(Duration::from_millis(8));
    }
    let crawl = Progress::unknown("Crawling links");
    for page in 0..50 {
        crawl.inc(1);
        if page == 30 {
            // found out how many there are part way through
            crawl.set_total(50);
        }
        sleep(Duration::from_millis(30));
    }
    crawl.finish();
}

fn card_section() {
    Card::success("all set")
        .kv("key", "value, aligned for you")
        .kv_with("link", "https://example.com", palette::BLUSH)
        .width(52)
        .print();
}

fn commands_section() {
    let live = Live::new("check", "my-app").no_history().no_notify();
    // sh stands in for real tools here; in a real CLI this would be cargo, docker, npm, …
    let script = |s: &str| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", s]);
        c
    };
    let lint = live.step("Lint");
    let test = live.step("Test");
    let audit = live.step("Audit");
    let _ = lint.run(&mut script(
        "for f in main config live prompt table; do echo \"checking src/$f.rs\"; sleep 0.25; done",
    ));
    let _ = test.run(&mut script(
        "for i in $(seq 1 24); do echo \"test case_$i ... ok\"; sleep 0.06; done; \
         echo 'test parses_ports ... FAILED'; echo; echo 'thread panicked: expected 8080, got 80'; exit 101",
    ));
    let _ = audit
        .command(&mut script("echo 'fetching advisory database'; sleep 5"))
        .timeout(Duration::from_millis(1500))
        .run();
    live.finish();
}

fn timeline_section() {
    let live = Live::new("release", "v2.1.0").no_history().no_notify();
    let fetch = live.step("Fetch");
    let build = live.step("Build");
    let targets = [build.step("linux"), build.step("macos"), build.step("windows")];
    let publish = live.step("Publish");
    run(&fetch, 0.5, &[]);
    fetch.done();
    build.start();
    std::thread::scope(|s| {
        for (t, secs) in targets.iter().zip([1.2, 1.7, 0.9]) {
            s.spawn(move || {
                run(t, secs, &[]);
                t.done();
            });
        }
    });
    build.done();
    run(&publish, 0.7, &[]);
    publish.done();
    live.finish_with_timeline();
}

fn states_section() {
    let live = Live::new("release", "my-app").no_history().no_notify();
    let [lint, test, upload, approve, ship, announce] =
        ["Lint", "Test", "Upload", "Approve", "Ship", "Announce"].map(|label| live.step(label));
    lint.skip("no source files changed");
    run(&test, 0.8, &["running 48 tests"]);
    test.done();
    run(&upload, 0.7, &["sending bundle"]);
    upload.fail("connection reset");
    sleep(Duration::from_millis(500));
    // the same step again, timed afresh
    run(&upload, 0.9, &["sending bundle"]);
    upload.done();
    approve.wait("waiting for approval");
    sleep(Duration::from_millis(1800));
    run(&approve, 0.3, &[]);
    approve.done();
    run(&ship, 0.8, &["rolling out"]);
    ship.done();
    announce.cancel("called off by hand");
    live.finish();
}

fn region_section() {
    use std::sync::{Arc, Mutex};
    let services = ["api", "web", "worker", "scheduler"];
    let ready = Arc::new(Mutex::new(0));
    let shown = ready.clone();
    let started = Instant::now();
    let region = Region::new(move || {
        let ready = *shown.lock().unwrap_or_else(|e| e.into_inner());
        let t = started.elapsed().as_secs_f32();
        let mut table = Table::new(["service", "status"]);
        for (i, name) in services.iter().enumerate() {
            let status = match i.cmp(&ready) {
                std::cmp::Ordering::Less => badge("ready", palette::SAGE),
                std::cmp::Ordering::Equal => {
                    format!("{} {}", Spinner::default().frame(t), paint(palette::SOFT, "starting"))
                }
                std::cmp::Ordering::Greater => paint(palette::MUTED, "queued"),
            };
            table = table.row([name.to_string(), status]);
        }
        table.lines()
    })
    .fps(30)
    .keep();
    for (i, name) in services.iter().enumerate() {
        sleep(Duration::from_millis(700));
        *ready.lock().unwrap_or_else(|e| e.into_inner()) = i + 1;
        region.log(Tone::Success.line(&format!("{name} is up")));
    }
    region.finish();
}

fn markdown_section() {
    tinsel::markdown(
        "## v2.1.0\n\
         \n\
         A **faster** release, with a new `--json` flag. See [the docs](https://docs.rs/tinsel).\n\
         \n\
         - Builds reuse their cache\n\
         - Prompts *filter as you type*\n\
         \x20 - in lists longer than eight\n\
         - [x] Windows support\n\
         - [ ] Recorded demo\n\
         \n\
         > Upgrading from 1.x? Run the migration first.\n\
         \n\
         ```toml\n\
         [deploy]\n\
         region = \"eu-west-2\"   # the default\n\
         memory = 512\n\
         ```\n\
         \n\
         | target | size | time |\n\
         |---|---|---|\n\
         | linux | 4.1 MB | 38s |\n\
         | macos | 3.8 MB | 52s |\n",
    )
    .print();
}

fn json_section() {
    let response = r#"{"app":"my-app","status":"live","regions":["eu-west-2","us-east-1"],"replicas":3,"tls":true,"owner":null,"limits":{"memory_mb":512,"timeout_s":30}}"#;
    match tinsel::json(response) {
        Ok(pretty) => pretty.print(),
        Err(e) => error(&e.to_string()),
    }
    println!();
    if let Err(e) = tinsel::json("{\"port\": 80,, \"host\": \"x\"}") {
        warn(&format!("and when it isn't valid: {e}"));
    }
}

fn prompts() {
    let quit = |e: prompt::Error| -> ! {
        if let prompt::Error::Cancelled = e {
            std::process::exit(130);
        }
        tinsel::error(&e.to_string());
        std::process::exit(1);
    };
    println!();
    let name = prompt::Input::new("Project name")
        .placeholder("my-app")
        .validate(|s| {
            if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                Err("letters, numbers and dashes only".into())
            } else {
                Ok(())
            }
        })
        .ask()
        .unwrap_or_else(|e| quit(e));
    let _token = prompt::password("API token").unwrap_or_else(|e| quit(e));
    let replicas: u32 = prompt::number("Replicas", Some(2)).unwrap_or_else(|e| quit(e));
    let spinners: Vec<&str> = Spinner::ALL.iter().map(|s| s.name()).collect();
    let pick = prompt::select("Spinner", &spinners).unwrap_or_else(|e| quit(e));
    let regions = [
        "us-east-1",
        "us-east-2",
        "us-west-1",
        "us-west-2",
        "eu-west-1",
        "eu-west-2",
        "eu-central-1",
        "ap-south-1",
        "ap-southeast-1",
        "ap-northeast-1",
        "sa-east-1",
        "ca-central-1",
    ];
    let region = prompt::select("Region (type to filter)", &regions).unwrap_or_else(|e| quit(e));
    let environments = ["dev", "staging", "production", "preview", "sandbox"];
    let chosen = prompt::multiselect("Environments", &environments, &[0, 1]).unwrap_or_else(|e| quit(e));
    let live_on = prompt::date("Go live on", None).unwrap_or_else(|e| quit(e));
    let runtime = prompt::Input::new("Runtime (tab completes)")
        .suggestions(&["node", "python", "go", "rust", "java"])
        .ask()
        .unwrap_or_else(|e| quit(e));
    let config = prompt::Input::new("Config file (tab completes paths)")
        .paths()
        .default("app.toml")
        .ask()
        .unwrap_or_else(|e| quit(e));
    // three questions in a row, where ← goes back to the one before
    let services = ["web", "docs", "api", "worker"];
    let (mut service, mut restart, mut version) = (0, Vec::new(), String::new());
    let mut step = 0;
    while step < 3 {
        let asked = match step {
            0 => prompt::Select::new("Service")
                .heading("Frontend")
                .option(services[0])
                .disabled(services[1], "deploys paused")
                .heading("Backend")
                .options([services[2], services[3]])
                .default(2)
                .ask()
                .map(|i| service = i),
            1 => prompt::MultiSelect::new("Restart after")
                .hint("the ones that read its config")
                .options(services)
                .validate(|picked| {
                    if picked.is_empty() {
                        Err("pick at least one".into())
                    } else {
                        Ok(())
                    }
                })
                .back()
                .ask()
                .map(|picked| restart = picked),
            _ => {
                let mut tags = prompt::Dropdown::new("Version")
                    .option("v1.12.0")
                    .note("today")
                    .badge("latest")
                    .option("v1.12.0-rc.2")
                    .note("yesterday")
                    .dim()
                    .option("v1.11.1")
                    .note("6 days ago")
                    .badge("current");
                for minor in (0..11).rev() {
                    tags = tags
                        .option(format!("v1.{minor}.0"))
                        .note(format!("{} weeks ago", 12 - minor));
                }
                tags.default(2).allow_custom().back().ask().map(|v| version = v)
            }
        };
        match asked {
            Ok(()) => step += 1,
            Err(prompt::Error::Back) => step -= 1,
            Err(e) => quit(e),
        }
    }
    if prompt::confirm("Write release notes in your editor?", false).unwrap_or_else(|e| quit(e)) {
        prompt::editor("Release notes", "## What's new\n\n").unwrap_or_else(|e| quit(e));
    }
    let go = prompt::confirm("Create it?", true).unwrap_or_else(|e| quit(e));
    if go {
        prompt::confirm_typing("It replaces the existing app. Sure?", &name).unwrap_or_else(|e| quit(e));
    }
    println!();
    if go {
        let names: Vec<&str> = chosen.iter().map(|&i| environments[i]).collect();
        success(&format!(
            "Created {name} ({} in {}, {runtime}, from {config}) with the {} spinner for {}, going live {live_on}",
            format::plural(replicas as u64, "replica"),
            regions[region],
            spinners[pick],
            names.join(", ")
        ));
        let restarted: Vec<&str> = restart.iter().map(|&i| services[i]).collect();
        info(&format!(
            "{} on {version}, restarting {}",
            services[service],
            restarted.join(", ")
        ));
    } else {
        hint("nothing created");
    }
}

fn run(step: &Step, secs: f32, details: &[&str]) {
    step.start();
    if details.is_empty() {
        sleep(Duration::from_secs_f32(secs));
        return;
    }
    for d in details {
        step.detail(*d);
        sleep(Duration::from_secs_f32(secs / details.len() as f32));
    }
}

fn demo(fail: bool, spinner: Spinner) {
    // a lower notification threshold than the default 10s, so switching away during the
    // ~10s demo reliably shows one
    let bar = match flag_value(&std::env::args().collect::<Vec<_>>(), "--bar").as_deref() {
        Some("waveform") => tinsel::BarStyle::Waveform,
        _ => tinsel::BarStyle::Line,
    };
    let live = Live::new("deploy", "my-app → production · eu-west-2")
        .with_spinner(spinner)
        .with_bar(bar)
        .notify_after(Duration::from_secs(5));
    let [config, build, upload, role, function, url, register, schedule] = [
        "Resolve config",
        "Build package",
        "Upload bundle",
        "IAM role",
        "Lambda function",
        "Function URL",
        "Register commands",
        "Keep-warm schedule",
    ]
    .map(|label| live.step(label));

    // two architectures built side by side, as sub-steps on their own threads
    let targets = [build.step("linux-arm64"), build.step("linux-x64")];

    run(&config, 0.6, &["app.toml", ".env · 4 keys"]);
    config.done();
    live.log("reusing 3 cached packages from the last deploy");

    build.start();
    std::thread::scope(|scope| {
        for (target, secs) in targets.iter().zip([1.8, 2.3]) {
            scope.spawn(move || {
                run(target, secs, &["compiling", "bundling 412 files"]);
                target.done();
            });
        }
    });
    build.done();

    upload.start();
    for i in 1..=80 {
        // ease-out, so it reads like a real transfer settling
        let p = 1.0 - (1.0 - i as f32 / 80.0).powi(2);
        upload.progress(p, format!("{:4.1} / 34.2 MB", p * 34.2));
        sleep(Duration::from_millis(32));
    }
    upload.done();

    run(&role, 0.9, &["my-app-role", "attaching 2 policies"]);
    role.done();

    if fail {
        run(&function, 1.3, &["updating code", "waiting for active"]);
        function.fail("AccessDenied: iam:PassRole on my-app-role");
        live.finish_with_card(
            Card::failure("deploy failed at Lambda function")
                .kv("why", "your user can't hand the role to the function")
                .kv("fix", "allow iam:PassRole, then deploy again")
                .kv_with("check", "my-app doctor", palette::ROSE),
        );
        std::process::exit(1);
    }

    run(
        &function,
        1.6,
        &["updating code", "updating configuration", "waiting for active"],
    );
    function.done();

    run(&url, 0.5, &[]);
    url.done();

    run(&register, 1.1, &["diffing 14 commands", "uploading changes"]);
    register.done();

    run(&schedule, 0.6, &[]);
    schedule.warn("rule already existed - reused, not recreated");

    let total = format!("in {:.1}s", live.elapsed().as_secs_f32());
    live.finish_with_card(
        Card::success("deployed")
            .aside(&total)
            .kv_with("url", "https://my-app.example.com", palette::BLUSH)
            .kv("commands", "14 registered")
            .kv_with("logs", "my-app logs --since 5m", palette::SOFT),
    );
}

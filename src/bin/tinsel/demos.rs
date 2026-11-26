//! The examples in the guides, drawn by tinsel as they'd really look, at any moment `t`
//! seconds in, so the docs can play them as you read. Each has a fixed height, so the page
//! doesn't jump as it plays.

use std::time::Duration;

use tinsel::{Border, Card, Live, Report, Spinner, Table, badge, bars, meter, paint, palette, sparkline};

/// The example called `name`, `width` columns wide, as it looks `t` seconds in; `None` if
/// there's no example by that name.
pub fn lines(name: &str, width: usize, t: f64) -> Option<Vec<String>> {
    let lines = match name {
        "deploy" => deploy(t),
        "spinners" => spinners(t),
        "progress" => progress(width, t),
        "table" => table(),
        "charts" => charts(width, t),
        "report" => report(),
        _ => return None,
    };
    Some(lines)
}

// the README's deploy, over and over: Build, then Upload filling up, then the card
fn deploy(t: f64) -> Vec<String> {
    const CYCLE: f64 = 7.0;
    let t = t % CYCLE;
    let live = Live::headless("deploy", "my-app → production");
    let build = live.step("Build");
    let upload = live.step("Upload");
    let mut out: Vec<String> = Vec::new();
    build.start();
    if t < 1.2 {
        live.advance(Duration::from_secs_f64(t));
    } else {
        live.advance(Duration::from_millis(1200));
        build.done();
        upload.start();
        let up = (t - 1.2).min(1.7);
        live.advance(Duration::from_secs_f64(up));
        if t < 2.9 {
            let done = up / 1.7;
            upload.progress(done as f32, format!("{} / 12 MB", (done * 12.0).round()));
        } else {
            upload.done();
        }
    }
    out.extend(live.snapshot().into_lines());
    if t >= 2.9 {
        out.push(String::new());
        let card = Card::success("deployed in 2.9s").kv("url", "https://my-app.example.com");
        out.extend(card.lines().into_lines());
    }
    pad(out, 12)
}

fn spinners(t: f64) -> Vec<String> {
    Spinner::ALL
        .iter()
        .map(|s| format!("  {}  {}", s.frame(t), paint(palette::SOFT, s.name())))
        .collect()
}

fn progress(width: usize, t: f64) -> Vec<String> {
    let done = (t % 6.0 / 5.0).min(1.0);
    let w = width.saturating_sub(30).clamp(10, 40);
    vec![
        meter("Indexing", done * 240.0, 240.0, w),
        meter("Uploading", (done * 1.6).min(1.0) * 12.0, 12.0, w),
    ]
}

fn table() -> Vec<String> {
    Table::new(["service", "status", "p95"])
        .row(["api".to_string(), badge(palette::SAGE, "healthy"), "84ms".into()])
        .row(["auth".to_string(), badge(palette::AMBER, "slow"), "1.2s".into()])
        .row(["web".to_string(), badge(palette::SAGE, "healthy"), "120ms".into()])
        .sort_by_desc(2)
        .border(Border::Rounded)
        .lines()
        .into_lines()
}

fn charts(width: usize, t: f64) -> Vec<String> {
    // a latency trace that drifts along as the page plays
    let values: Vec<f64> = (0..32)
        .map(|i| {
            let x = i as f64 * 0.45 + t * 1.5;
            90.0 + 40.0 * x.sin() + 25.0 * (x * 2.3).cos()
        })
        .collect();
    let mut out = vec![format!("  p95  {}", paint(palette::ROSE, &sparkline(&values)))];
    out.push(String::new());
    let w = width.saturating_sub(16).clamp(10, 40);
    out.extend(bars(&[("api", 84.0), ("auth", 1200.0), ("web", 120.0)], w).into_lines());
    out
}

fn report() -> Vec<String> {
    Report::new("couldn't start")
        .cause("couldn't read app.toml")
        .cause("No such file or directory (os error 2)")
        .hint("create app.toml, or pass --config <path>")
        .lines()
        .into_lines()
}

// the same height whatever moment it is, so the page below doesn't jump
fn pad(mut lines: Vec<String>, height: usize) -> Vec<String> {
    lines.resize(height.max(lines.len()), String::new());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_draws_and_keeps_its_height() {
        for name in ["deploy", "spinners", "progress", "table", "charts", "report"] {
            let heights: Vec<usize> = [0.0, 0.5, 1.5, 2.5, 4.0, 6.5]
                .iter()
                .map(|&t| lines(name, 80, t).unwrap().len())
                .collect();
            assert!(heights[0] > 0, "{name} draws nothing");
            assert!(
                heights.windows(2).all(|w| w[0] == w[1]),
                "{name} changes height: {heights:?}"
            );
        }
        assert!(lines("nonsense", 80, 0.0).is_none());
    }
}

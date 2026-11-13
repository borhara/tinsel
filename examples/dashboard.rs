//! A live dashboard: stats, a service list, a chart that follows the selection, and logs.
//!
//!   cargo run --example dashboard
//!
//! ←→ or 1/2 switch tabs, ↑↓ choose a service, ? for help, q to quit. Clicks and the scroll
//! wheel work too.
//!
//! To draw one frame as a web page instead of running, for a screenshot:
//!
//!   cargo run --example dashboard -- --html 120x36 > dashboard.html

use tinsel::app::{
    self, App, Event, Flow, Footer, Frame, Key, List, MouseKind, Panel, Plot, Size::*, Sparkles, Stat, Tabs,
};
use tinsel::{ColorMode, Line, Table, Theme, badge, meter, paint, palette, shimmer};

struct Service {
    name: &'static str,
    base: f64,
    latency: Vec<f64>,
}

struct Dashboard {
    services: Vec<Service>,
    selected: usize,
    tab: usize,
    help: bool,
    logs: Vec<String>,
    ticks: u64,
    frames: u64,
    rng: u64,
}

const TABS: [&str; 2] = ["overview", "logs"];

impl Dashboard {
    fn new() -> Self {
        let mut d = Dashboard {
            services: [
                ("api", 84.0),
                ("auth", 140.0),
                ("worker", 420.0),
                ("search", 61.0),
                ("cron", 12.0),
            ]
            .into_iter()
            .map(|(name, base)| Service {
                name,
                base,
                latency: Vec::new(),
            })
            .collect(),
            selected: 0,
            tab: 0,
            help: false,
            logs: Vec::new(),
            ticks: 0,
            frames: 0,
            rng: 0x2545_F491_4F6C_DD1D,
        };
        for _ in 0..200 {
            d.sample();
        }
        d
    }

    /// A pseudo-random 0..1, so the example needs nothing outside tinsel.
    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f64 / 10_000.0
    }

    /// New latency for every service: a slow wave, jitter, and now and then a spike.
    fn sample(&mut self) {
        self.ticks += 1;
        for i in 0..self.services.len() {
            let wave = ((self.ticks as f64 / 18.0) + i as f64).sin() * 0.25;
            let spike = if self.random() > 0.97 { 2.5 } else { 0.0 };
            let jitter = self.random() * 0.3;
            let s = &mut self.services[i];
            let v = s.base * (1.0 + wave + jitter + spike);
            s.latency.push(v);
            if s.latency.len() > 400 {
                s.latency.remove(0);
            }
            if spike > 0.0 {
                let line = format!(
                    "{}  {}  p95 {}",
                    paint(palette::MUTED, &format!("t+{:>4}", self.ticks)),
                    badge("slow", palette::AMBER),
                    paint(palette::TEXT, &format!("{} took {v:.0}ms", self.services[i].name)),
                );
                self.logs.push(line);
            }
        }
        if self.ticks % 7 == 0 {
            let n = self.random();
            let line = format!(
                "{}  {}  {}",
                paint(palette::MUTED, &format!("t+{:>4}", self.ticks)),
                badge("ok", palette::SAGE),
                paint(palette::SOFT, &format!("deployed build #{}", 1200 + (n * 99.0) as u32)),
            );
            self.logs.push(line);
        }
    }

    fn latest(&self, s: &Service) -> f64 {
        *s.latency.last().unwrap_or(&0.0)
    }

    fn slow(&self, s: &Service) -> bool {
        self.latest(s) > s.base * 2.0
    }

    /// The tabs, marked with how things are: ✔ while all is calm, a count of slow services
    /// when not, and how many lines the logs have.
    fn tabs(&self) -> Tabs {
        let slow = self.services.iter().filter(|s| self.slow(s)).count();
        let tabs = Tabs::new(TABS).selected(self.tab);
        let tabs = if slow == 0 {
            tabs.badge(0, "✔", palette::SAGE)
        } else {
            tabs.badge(0, slow.to_string(), palette::AMBER)
        };
        tabs.badge(1, self.logs.len().to_string(), palette::MUTED)
    }

    /// A dot for a service: green while it's quick, amber while it's slow.
    fn dot(&self, s: &Service) -> String {
        paint(if self.slow(s) { palette::AMBER } else { palette::SAGE }, "●")
    }

    fn overview(&self, frame: &mut Frame, area: app::Rect) {
        let [stats, body] = area.rows_gap(0, [Fixed(7), Fill(1)]);
        let cells = stats.columns_gap(1, [Fill(1), Fill(1), Fill(1), Fill(1)]);
        let total: f64 = self.services.iter().map(|s| 12_000.0 / self.latest(s).max(1.0)).sum();
        let p95 = self.latest(&self.services[self.selected]);
        let slow = self.services.iter().filter(|s| self.slow(s)).count();
        let secs = frame.time() as u64;
        let stat = |label: &str, value: String, note: &str, color| {
            Panel::new("").body(Stat::new(label, value).note(note, color))
        };
        frame.render(
            cells[0],
            stat(
                "requests / s",
                format!("{:.1}k", total / 1000.0),
                "▲ 12% today",
                palette::SAGE,
            ),
        );
        frame.render(
            cells[1],
            stat("p95 latency", format!("{p95:.0}"), "milliseconds", palette::SOFT),
        );
        frame.render(
            cells[2],
            stat(
                "running slow",
                slow.to_string(),
                if slow == 0 { "all calm ✿" } else { "keep an eye out" },
                if slow == 0 { palette::SAGE } else { palette::AMBER },
            ),
        );
        frame.render(
            cells[3],
            stat(
                "watching for",
                format!("{:02}:{:02}", secs / 60, secs % 60),
                "minutes : seconds",
                palette::SOFT,
            ),
        );

        let [side, main] = body.columns_gap(1, [Fixed(26), Fill(1)]);
        let names = self.services.iter().map(|s| format!("{} {}", self.dot(s), s.name));
        frame.render(
            side,
            Panel::new("services")
                .body(List::new(names.collect::<Vec<_>>()).selected(self.selected))
                .footer(format!("{}/{}", self.selected + 1, self.services.len()))
                .focused(true),
        );

        let [chart, details] = main.rows([Fill(3), Fixed(8)]);
        let s = &self.services[self.selected];
        frame.render(
            chart,
            Panel::new(format!(
                "{} {}",
                self.dot(s),
                paint(palette::TEXT, &format!("{} latency", s.name))
            ))
            .status(format!("last {} readings", s.latency.len()))
            .body(Plot::new(&s.latency))
            .footer(format!("now {:.0}ms", self.latest(s))),
        );

        let [gauges, table] = details.columns_gap(1, [Fill(1), Fill(1)]);
        let wobble = |seed: f64| 50.0 + 30.0 * (frame.time() as f64 / 3.0 + seed).sin();
        let lines: tinsel::Block = [("cpu   ", wobble(0.0)), ("memory", wobble(2.0)), ("disk  ", 88.0)]
            .iter()
            .map(|(label, v)| meter(label, *v, 100.0, 14))
            .collect();
        frame.render(gauges, Panel::new("resources").body(lines).padding(1, 1));
        let regions = Table::new(["region", "pods"])
            .row(["eu-west-2", "12"])
            .row(["us-east-1", "8"])
            .row(["ap-south-1", "3"]);
        frame.render(table, Panel::new("regions").status("23 pods").body(regions));
    }
}

impl App for Dashboard {
    fn view(&self, frame: &mut Frame) {
        let t = frame.time();
        let [header, body, footer] = frame.area().rows([Fixed(3), Fill(1), Fixed(1)]);

        // a header of quiet stars, with the name catching the light now and then
        frame.render(header, Sparkles::new().density(0.04));
        let [name, tabs] = header.shrink(2, 1).columns([Fixed(22), Fill(1)]);
        let title = Line::new()
            .push("✦ ", palette::ROSE)
            .append(shimmer("tinsel status", t));
        frame.render(name, title);
        frame.render(tabs, self.tabs());

        let body = body.shrink(1, 0);
        if self.tab == 0 {
            self.overview(frame, body);
        } else {
            let shown = body.height.saturating_sub(2);
            let start = self.logs.len().saturating_sub(shown);
            frame.render(
                body,
                Panel::new("logs")
                    .body(tinsel::Block::from(self.logs[start..].to_vec()))
                    .footer(format!("{} lines", self.logs.len()))
                    .focused(true),
            );
        }

        // the keys that matter most first: a narrow footer keeps those
        let footer_keys = Footer::new()
            .keys(&[("↑↓", "service"), ("←→", "tabs")])
            .keys(&[("?", "help"), ("q", "quit")])
            .status(format!("{} services", self.services.len()));
        frame.render(footer.shrink(2, 0), footer_keys);

        if self.help {
            let help = Table::new(["key", "does"])
                .row(["← → 1 2", "switch tab"])
                .row(["↑ ↓ j k", "choose a service"])
                .row(["g G", "first, last"])
                .row(["?", "close this"])
                .row(["q", "quit"]);
            let area = frame.area().center(36, 10);
            frame.render(area, Panel::new("help").body(help).padding(2, 1).focused(true));
        }
    }

    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Tick => {
                // draw at 20 frames a second, but take a new reading 5 times a second
                self.frames += 1;
                if self.frames % 4 == 0 {
                    self.sample();
                }
            }
            Event::Key(Key::Char('q')) => return Flow::Quit,
            Event::Key(Key::Esc) if self.help => self.help = false,
            Event::Key(Key::Esc) => return Flow::Quit,
            Event::Key(Key::Char('?')) => self.help = !self.help,
            Event::Key(Key::Left | Key::BackTab) => self.tab = self.tab.saturating_sub(1),
            Event::Key(Key::Right | Key::Tab) => self.tab = (self.tab + 1).min(TABS.len() - 1),
            Event::Key(Key::Char(c @ '1'..='2')) => self.tab = *c as usize - '1' as usize,
            Event::Mouse(m) if m.kind == MouseKind::Click => {
                // where view puts the tabs: two in from the edge, after the 22-column name
                let tabs = app::Rect::new(24, 1, 60, 1);
                if let Some(i) = self.tabs().at(tabs, m.x, m.y) {
                    self.tab = i;
                }
            }
            other => {
                app::navigate(&mut self.selected, self.services.len(), other);
            }
        }
        Flow::Continue
    }

    fn fps(&self) -> u32 {
        20
    }

    fn mouse(&self) -> bool {
        true
    }

    fn title(&self) -> Option<String> {
        Some(format!("tinsel status · {}", TABS[self.tab]))
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, size] = args.as_slice() {
        if flag == "--html" {
            let (w, h) = size.split_once('x').expect("a size like 120x36");
            print!(
                "{}",
                screenshot(w.parse().expect("a width"), h.parse().expect("a height"))
            );
            return Ok(());
        }
    }
    app::run(Dashboard::new())?;
    Ok(())
}

/// One frame as a web page, in colour on a dark background, a few minutes in with a
/// service chosen: for the README.
fn screenshot(width: usize, height: usize) -> String {
    tinsel::set_color_mode(ColorMode::TrueColor);
    tinsel::set_theme(Theme::Dark);
    let mut dash = Dashboard::new();
    dash.selected = 2;
    let mut frame = Frame::new(width, height).at(754.0);
    dash.view(&mut frame);
    frame.html()
}

//! A services dashboard: groups on the left, a table of services with a column per
//! environment, details for the one under the cursor, a filter, dialogs and toasts.
//!
//!   cargo run --example services
//!
//! ↑↓ or j/k move, ←→ or h/l move between panels and scroll the table sideways, tab
//! switches panel, 1-4 pick a group, / filters, d shows only drifted services, x restarts,
//! r refreshes, e picks the environment the details show, s signs in (leaving the dashboard
//! while a login command runs), ? for help, q to quit.
//!
//! To draw one frame as a web page instead of running, for a screenshot:
//!
//!   cargo run --example services -- --html 120x30 > services.html

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tinsel::app::{
    self, App, Cursor, Event, Flow, Footer, Frame, Grid, Input, Key, List, Modal, Panel, Size::*, Tabs, Toasts, Waker,
};
use tinsel::{ColorMode, Line, Spinner, Table, Theme, Tone, format, paint, palette, shimmer};

#[derive(Clone)]
struct Service {
    name: &'static str,
    group: &'static str,
    staging: String,
    production: String,
    health: Health,
    replicas: u32,
    updated_mins: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Health {
    Healthy,
    Slow,
    Down,
}

impl Service {
    fn drifted(&self) -> bool {
        self.staging != self.production
    }

    /// A coloured mark and a word for how it's doing.
    fn health(&self) -> (String, &'static str) {
        match self.health {
            Health::Healthy => (paint(palette::SAGE, "●"), "healthy"),
            Health::Slow => (paint(palette::AMBER, "▲"), "slow"),
            Health::Down => (paint(palette::RED, "✕"), "down"),
        }
    }
}

const GROUPS: [&str; 4] = ["all", "core", "data", "edge"];
const ENVS: [&str; 2] = ["staging", "production"];

/// What the pretend backend says, after a pretend wait.
fn fetch(round: u32) -> Vec<Service> {
    thread::sleep(Duration::from_millis(1200));
    let services = [
        ("api", "core"),
        ("auth", "core"),
        ("billing", "core"),
        ("worker", "core"),
        ("scheduler", "core"),
        ("db", "data"),
        ("cache", "data"),
        ("search", "data"),
        ("queue", "data"),
        ("web", "edge"),
        ("gateway", "edge"),
        ("cdn-purger", "edge"),
    ];
    services
        .iter()
        .enumerate()
        .map(|(i, &(name, group))| {
            let n = i as u32 + round;
            let minor = 4 + n % 5;
            Service {
                name,
                group,
                staging: format!("v2.{minor}.{}", n % 3),
                production: format!("v2.{}.{}", if i % 4 == 1 { minor - 1 } else { minor }, n % 3),
                health: match n % 7 {
                    3 => Health::Slow,
                    5 if round > 0 => Health::Down,
                    _ => Health::Healthy,
                },
                replicas: 1 + n % 6,
                updated_mins: (n as u64 * 37) % 600,
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Groups,
    Table,
    Filter,
}

enum Dialog {
    Help,
    Environment(usize),
    Restart(&'static str),
}

struct Dashboard {
    services: Vec<Service>,
    group: usize,
    pane: Pane,
    cursor: Cursor,
    filter: Input,
    drifted_only: bool,
    env: usize,
    dialog: Option<(Dialog, f32)>,
    toasts: Toasts,
    loading: bool,
    refreshed: Option<Instant>,
    results: mpsc::Receiver<Vec<Service>>,
    send: mpsc::Sender<Vec<Service>>,
    waker: Waker,
    round: u32,
}

impl Dashboard {
    fn new() -> Self {
        let (send, results) = mpsc::channel();
        let mut d = Dashboard {
            services: Vec::new(),
            group: 0,
            pane: Pane::Table,
            cursor: Cursor::new(),
            filter: Input::new().prompt("/").placeholder("filter services by name"),
            drifted_only: false,
            env: 1,
            dialog: None,
            toasts: Toasts::new(),
            loading: false,
            refreshed: None,
            results,
            send,
            waker: Waker::new(),
            round: 0,
        };
        d.refresh();
        d
    }

    /// A dashboard with the data already in, without the wait: for screenshots.
    fn loaded(services: Vec<Service>) -> Self {
        let (send, results) = mpsc::channel();
        Dashboard {
            services,
            group: 0,
            pane: Pane::Table,
            cursor: Cursor::new(),
            filter: Input::new().prompt("/").placeholder("filter services by name"),
            drifted_only: false,
            env: 1,
            dialog: None,
            toasts: Toasts::new(),
            loading: false,
            refreshed: Instant::now().checked_sub(Duration::from_secs(40)),
            results,
            send,
            waker: Waker::new(),
            round: 1,
        }
    }

    /// Load in the background; the dashboard is woken the moment it's done.
    fn refresh(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        let (send, waker, round) = (self.send.clone(), self.waker.clone(), self.round);
        thread::spawn(move || {
            let _ = send.send(fetch(round));
            waker.wake();
        });
        self.round += 1;
    }

    /// The services in the table, in its order: by group, then by name.
    fn shown(&self) -> Vec<&Service> {
        let filter = self.filter.value().to_lowercase();
        let mut shown: Vec<&Service> = self
            .services
            .iter()
            .filter(|s| self.group == 0 || s.group == GROUPS[self.group])
            .filter(|s| s.name.contains(&filter))
            .filter(|s| !self.drifted_only || s.drifted())
            .collect();
        shown.sort_by_key(|s| (s.group, s.name));
        shown
    }

    fn chosen(&self) -> Option<&Service> {
        let shown = self.shown();
        shown.get(self.cursor.row().min(shown.len().saturating_sub(1))).copied()
    }

    fn focus(&mut self, pane: Pane) {
        self.pane = pane;
        self.filter.focus(pane == Pane::Filter);
    }

    fn open(&mut self, dialog: Dialog) {
        self.dialog = Some((dialog, app::time()));
    }

    fn sign_in(&mut self) {
        let ran = app::suspend(|| {
            println!("\n  signing in to {}\n", ENVS[self.env]);
            login()
        });
        match ran {
            Ok(Ok(status)) if status.success() => {
                self.toasts
                    .push(Tone::Success, format!("signed in to {}", ENVS[self.env]));
                self.refresh();
            }
            Ok(Ok(status)) => self.toasts.push(Tone::Error, format!("sign-in failed ({status})")),
            Ok(Err(e)) => self
                .toasts
                .push(Tone::Error, format!("couldn't run the login command: {e}")),
            Err(e) => self.toasts.push(Tone::Error, e.to_string()),
        }
    }

    // MARK: drawing

    fn header(&self, frame: &mut Frame, area: app::Rect) {
        let t = frame.time();
        let title = Line::new().push("✦ ", palette::ROSE).append(shimmer("services", t));
        frame.render(area, title);
        let tabs = self.environments();
        // each tab is its title, a mark and a space either side, with a space between tabs
        let w = ENVS.iter().map(|e| e.len() + 4).sum::<usize>() + ENVS.len() - 1;
        frame.render(
            app::Rect::new(area.x + area.width.saturating_sub(w), area.y, w, 1),
            tabs,
        );
    }

    /// The environments as tabs, the one the details show chosen. Production is red, so it's
    /// never picked by mistake, and each has a mark: ○ while loading, ✖ when a service is down.
    fn environments(&self) -> Tabs {
        let down = self.services.iter().any(|s| s.health == Health::Down);
        let mut tabs = Tabs::new(ENVS).selected(self.env).color(1, palette::RED);
        for i in 0..ENVS.len() {
            tabs = if self.services.is_empty() {
                tabs.badge(i, "○", palette::MUTED)
            } else if down && i == 1 {
                tabs.badge(i, "✖", palette::RED)
            } else {
                tabs.badge(i, "✔", palette::SAGE)
            };
        }
        tabs
    }

    fn groups(&self, frame: &mut Frame, area: app::Rect) {
        let names: Vec<String> = GROUPS
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let n = self.services.iter().filter(|s| i == 0 || s.group == *g).count();
                let count = paint(palette::MUTED, &format!("{n:>3}"));
                format!("{}  {:<8}{count}", paint(palette::MUTED, &(i + 1).to_string()), g)
            })
            .collect();
        frame.render(
            area,
            Panel::new("groups")
                .body(List::new(names).selected(self.group))
                .focused(self.pane == Pane::Groups),
        );
    }

    fn table(&self, frame: &mut Frame, area: app::Rect) {
        let shown = self.shown();
        let mut grid = Grid::new(["service", "staging", "production", "health", "replicas", "updated", ""])
            .cursor(&self.cursor)
            .focused(self.pane == Pane::Table)
            .striped()
            .blank_cells()
            .fill(6)
            .sorted(0)
            .empty(if self.services.is_empty() {
                "nothing here yet"
            } else {
                "no services match"
            })
            .loading(self.loading && self.services.is_empty());
        for (i, s) in shown.iter().enumerate() {
            // in the "all" view, services sit under their group's name
            if self.group == 0 && (i == 0 || shown[i - 1].group != s.group) {
                grid = grid.heading(paint(palette::PETAL, s.group));
            }
            let production = if s.drifted() {
                paint(palette::AMBER, &s.production)
            } else {
                s.production.clone()
            };
            let (mark, word) = s.health();
            let health = format!("{mark} {word}");
            let drift = if s.drifted() {
                paint(palette::AMBER, "drifted")
            } else {
                String::new()
            };
            grid = grid.row([
                s.name.to_string(),
                s.staging.clone(),
                production,
                health,
                s.replicas.to_string(),
                format!("{}m ago", s.updated_mins),
                drift,
            ]);
        }
        let count = if shown.len() == self.services.len() {
            shown.len().to_string()
        } else {
            format!("{}/{}", shown.len(), self.services.len())
        };
        let position = if shown.is_empty() {
            String::new()
        } else {
            format!("{} of {}", self.cursor.row().min(shown.len() - 1) + 1, shown.len())
        };

        let status = match self.refreshed {
            Some(at) if !self.loading => format!("{} · refreshed {}", ENVS[self.env], format::ago(at.elapsed())),
            _ => ENVS[self.env].to_string(),
        };
        let filtering = self.pane == Pane::Filter || !self.filter.value().is_empty();
        let panel = Panel::new(GROUPS[self.group])
            .count(count)
            .status(status)
            .footer(position)
            .focused(self.pane != Pane::Groups);
        let inner = panel.inner(area);
        frame.render(area, panel);
        let rows = if filtering {
            let [input, _, rest] = inner.rows([Fixed(1), Fixed(1), Fill(1)]);
            frame.render(input, &self.filter);
            rest
        } else {
            inner
        };
        frame.render(rows, grid);
    }

    fn details(&self, frame: &mut Frame, area: app::Rect) {
        let Some(s) = self.chosen() else {
            frame.render(area, Panel::new("details"));
            return;
        };
        let version = if self.env == 0 { &s.staging } else { &s.production };
        let mut table = Table::new(["", ""])
            .row(["version", version.as_str()])
            .row(["replicas", &s.replicas.to_string()])
            .row(["updated", &format!("{} minutes ago", s.updated_mins)]);
        if s.drifted() {
            table = table.row([
                "drift".to_string(),
                paint(palette::AMBER, &format!("staging has {}", s.staging)),
            ]);
        }
        let lines = table.lines();
        // drop the empty header and its rule: just the pairs
        let pairs: tinsel::Block = lines.lines().iter().skip(2).cloned().collect();
        let (mark, _) = s.health();
        let title = format!(
            "{mark} {}",
            paint(palette::TEXT, &format!("{} in {}", s.name, ENVS[self.env]))
        );
        frame.render(area, Panel::new(title).body(pairs));
    }

    fn footer(&self, frame: &mut Frame, area: app::Rect) {
        let status = if self.loading {
            format!("{} refreshing", Spinner::Sparkle.frame(frame.time()))
        } else {
            String::new()
        };
        // the keys that matter most first: a narrow footer keeps those
        let footer = Footer::new()
            .keys(&[("↑↓", "move"), ("/", "filter"), ("?", "help")])
            .keys(&[("x", "restart"), ("r", "refresh"), ("e", "env")])
            .keys(&[("tab", "panel"), ("d", "drifted"), ("s", "sign in"), ("q", "quit")])
            .status(status);
        frame.render(area, footer);
    }

    fn dialog(&self, frame: &mut Frame) {
        let Some((dialog, opened)) = &self.dialog else { return };
        let modal = match dialog {
            Dialog::Help => {
                let help = Table::new(["key", "does"])
                    .row(["↑ ↓  j k", "move"])
                    .row(["← → h l", "panels, scroll sideways"])
                    .row(["tab", "next panel"])
                    .row(["1-4", "pick a group"])
                    .row(["/", "filter by name"])
                    .row(["d", "only drifted services"])
                    .row(["x", "restart the service"])
                    .row(["r", "refresh"])
                    .row(["e", "pick the environment"])
                    .row(["s", "sign in"])
                    .row(["q", "quit"]);
                Modal::new(Panel::new("keys").body(help).padding(2, 1).focused(true))
            }
            Dialog::Environment(at) => Modal::new(
                Panel::new("environment")
                    .body(List::new(ENVS).selected(*at))
                    .footer("enter")
                    .padding(2, 1)
                    .focused(true),
            ),
            Dialog::Restart(name) => {
                let body = tinsel::Block::from(vec![
                    format!(
                        "Restart {} in {}?",
                        paint(palette::PETAL, name),
                        paint(palette::PETAL, ENVS[self.env])
                    ),
                    String::new(),
                    app::hints(&[("y", "restart"), ("n", "not now")]).to_string(),
                ]);
                Modal::new(Panel::new("restart").body(body).padding(2, 1).focused(true))
            }
        };
        frame.render(frame.area(), modal.opened(*opened));
    }

    // MARK: keys

    fn dialog_key(&mut self, key: Key) {
        let Some((dialog, _)) = &mut self.dialog else { return };
        match (dialog, key) {
            (Dialog::Environment(at), Key::Up | Key::Down | Key::Char('j' | 'k')) => {
                app::navigate(at, ENVS.len(), &Event::Key(key));
            }
            (Dialog::Environment(at), Key::Enter) => {
                self.env = *at;
                self.dialog = None;
            }
            (Dialog::Restart(name), Key::Char('y') | Key::Enter) => {
                let msg = format!("restarting {name} in {}", ENVS[self.env]);
                self.dialog = None;
                self.toasts.push(Tone::Success, msg);
            }
            (Dialog::Help, _) | (_, Key::Esc | Key::Char('n' | 'q')) => self.dialog = None,
            _ => {}
        }
    }
}

impl App for Dashboard {
    fn view(&self, frame: &mut Frame) {
        let [header, body, footer] = frame.area().shrink(1, 0).rows([Fixed(2), Fill(1), Fixed(1)]);
        self.header(frame, header.shrink(1, 0).row(0));
        let [side, main] = body.columns_gap(1, [Fixed(22), Fill(1)]);
        let [table, details] = main.rows([Fill(1), Fixed(7)]);
        self.groups(frame, side);
        self.table(frame, table);
        self.details(frame, details);
        self.footer(frame, footer.shrink(1, 0));
        frame.render(body.shrink(3, 2), &self.toasts);
        self.dialog(frame);
    }

    fn event(&mut self, event: &Event) -> Flow {
        if *event == Event::Wake {
            if let Some(services) = self.results.try_iter().last() {
                let first = self.services.is_empty();
                self.services = services;
                self.loading = false;
                self.refreshed = Some(Instant::now());
                let down = self.services.iter().filter(|s| s.health == Health::Down).count();
                if down > 0 {
                    self.toasts
                        .push(Tone::Warn, format!("{} down", format::plural(down as u64, "service")));
                } else if !first {
                    self.toasts.push(Tone::Info, "all services up to date");
                }
            }
            return Flow::Continue;
        }
        let Event::Key(key) = *event else {
            return Flow::Continue;
        };
        if self.dialog.is_some() {
            self.dialog_key(key);
            return Flow::Continue;
        }
        if self.pane == Pane::Filter {
            if !self.filter.event(event) {
                match key {
                    Key::Esc => {
                        self.filter.clear();
                        self.focus(Pane::Table);
                    }
                    Key::Enter | Key::Tab | Key::Down => self.focus(Pane::Table),
                    _ => {}
                }
            }
            self.cursor.select(0);
            return Flow::Continue;
        }
        let rows = self.shown().len();
        match key {
            Key::Char('q') => return Flow::Quit,
            Key::Esc if !self.filter.value().is_empty() => self.filter.clear(),
            Key::Esc => return Flow::Quit,
            Key::Char('?') => self.open(Dialog::Help),
            Key::Char('/') => self.focus(Pane::Filter),
            Key::Char('e') => self.open(Dialog::Environment(self.env)),
            Key::Char('r') => self.refresh(),
            Key::Char('s') => self.sign_in(),
            Key::Char('d') => {
                self.drifted_only = !self.drifted_only;
                self.cursor.select(0);
            }
            Key::Char(c @ '1'..='4') => {
                self.group = c as usize - '1' as usize;
                self.cursor.select(0);
            }
            Key::Tab | Key::BackTab => self.focus(if self.pane == Pane::Table {
                Pane::Groups
            } else {
                Pane::Table
            }),
            Key::Char('x') => {
                if let Some(s) = self.chosen() {
                    let name = s.name;
                    self.open(Dialog::Restart(name));
                }
            }
            _ if self.pane == Pane::Groups => match key {
                Key::Right | Key::Char('l') | Key::Enter => self.focus(Pane::Table),
                _ => {
                    if app::navigate(&mut self.group, GROUPS.len(), event) {
                        self.cursor.select(0);
                    }
                }
            },
            Key::Left | Key::Char('h') => {
                if !self.cursor.scroll_left() {
                    self.focus(Pane::Groups);
                }
            }
            Key::Right | Key::Char('l') => {
                self.cursor.scroll_right();
            }
            _ => {
                self.cursor.navigate(rows, event);
            }
        }
        Flow::Continue
    }

    fn fps(&self) -> u32 {
        10
    }

    fn title(&self) -> Option<String> {
        Some(format!("services · {} · {}", GROUPS[self.group], ENVS[self.env]))
    }
}

/// A stand-in for a real login command, which would open a browser or ask for a code.
fn login() -> std::io::Result<std::process::ExitStatus> {
    let script = "printf '  open https://example.com/device and enter code WXYZ-1234\\n\\n  press enter once you have… '; read _";
    if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "pause"]).status()
    } else {
        std::process::Command::new("sh").args(["-c", script]).status()
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, size] = args.as_slice() {
        if flag == "--html" {
            let (w, h) = size.split_once('x').expect("a size like 120x30");
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

/// One frame as a web page, in colour on a dark background, with the services loaded and
/// one chosen: for the README.
fn screenshot(width: usize, height: usize) -> String {
    tinsel::set_color_mode(ColorMode::TrueColor);
    tinsel::set_theme(Theme::Dark);
    let mut dash = Dashboard::loaded(fetch(1));
    dash.cursor.select(5);
    let mut frame = Frame::new(width, height).at(2.0);
    dash.view(&mut frame);
    frame.html()
}

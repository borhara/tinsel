//! `tinsel docs`: the guides in a full-screen reader, drawn by tinsel itself, with their
//! examples running as you read. Piped, a guide prints as plain text instead.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::IsTerminal;

use tinsel::app::{self, App, Event, Flow, Footer, Frame, Key, List, MouseKind, Panel, Rect, Scrollbar, Tabs, Widget};
use tinsel::{bold, paint, palette};

use crate::demos;

/// A guide: what `tinsel docs <slug>` opens, its title, and its Markdown.
pub struct Guide {
    pub slug: &'static str,
    pub title: &'static str,
    pub src: &'static str,
}

macro_rules! guide {
    ($slug:literal, $title:literal) => {
        Guide {
            slug: $slug,
            title: $title,
            src: include_str!(concat!("guides/", $slug, ".md")),
        }
    };
}

pub const GUIDES: &[Guide] = &[
    guide!("introduction", "Introduction"),
    guide!("quickstart", "Quickstart"),
    guide!("live", "Live step lists"),
    guide!("spinners", "Spinners and progress"),
    guide!("messages", "Messages and errors"),
    guide!("data", "Tables, trees and charts"),
    guide!("questions", "Asking questions"),
    guide!("cli", "Command lines and help"),
    guide!("apps", "Full-screen apps"),
    guide!("pictures", "Pictures"),
    guide!("testing", "Testing what you print"),
    guide!("settings", "Settings and terminals"),
];

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// A page as its parts: runs of Markdown, each `##` heading starting a new one, and the
/// examples between them, which are drawn fresh every frame.
struct Page {
    pieces: Vec<Piece>,
    // each heading: the piece it starts, and its title
    headings: Vec<(usize, String)>,
}

enum Piece {
    Text(String),
    Example(String),
}

fn parse(src: &str) -> Page {
    let mut page = Page {
        pieces: Vec::new(),
        headings: Vec::new(),
    };
    let mut text = String::new();
    let mut lines = src.lines();
    let mut fenced = false;
    let flush = |text: &mut String, page: &mut Page| {
        if !text.trim().is_empty() {
            page.pieces.push(Piece::Text(std::mem::take(text)));
        }
        text.clear();
    };
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !fenced {
            if let Some(name) = trimmed.strip_prefix("```tinsel ") {
                flush(&mut text, &mut page);
                page.pieces.push(Piece::Example(name.trim().to_string()));
                lines
                    .by_ref()
                    .take_while(|l| !l.trim_start().starts_with("```"))
                    .for_each(drop);
                continue;
            }
            if let Some(title) = line.strip_prefix("## ") {
                flush(&mut text, &mut page);
                page.headings.push((page.pieces.len(), heading_title(title)));
            }
        }
        if trimmed.starts_with("```") {
            fenced = !fenced;
        }
        text.push_str(line);
        text.push('\n');
    }
    flush(&mut text, &mut page);
    page
}

// a heading as its words, without a link round them, as the changelog's versions are
fn heading_title(title: &str) -> String {
    match (title.find('['), title.find(']')) {
        (Some(a), Some(b)) if a < b => {
            format!("{}{}", &title[a + 1..b], title[b + 1..].split(')').nth(1).unwrap_or(""))
        }
        _ => title.to_string(),
    }
    .trim()
    .to_string()
}

// headings with the links taken out of them, as the changelog's versions have, which a
// heading would show as they are; and none with nothing under it, as Unreleased has
// between releases
fn plain_headings(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        match l.strip_prefix("## ") {
            Some(title) => {
                let empty = lines[i + 1..]
                    .iter()
                    .take_while(|l| !l.starts_with("## "))
                    .all(|l| l.trim().is_empty());
                if !empty {
                    out.push(format!("## {}", heading_title(title)));
                }
            }
            None => out.push(l.to_string()),
        }
    }
    out.join("\n")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Guides,
    Changelog,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    List,
    Page,
    Headings,
}

/// Where things were drawn last, for the mouse and for paging.
#[derive(Default, Clone, Copy)]
struct Drawn {
    tabs: Rect,
    list: Option<Rect>,
    page: Rect,
    headings: Option<Rect>,
    // rows the page has, and how many show at once
    total: usize,
    shown: usize,
}

pub struct Docs {
    tab: Tab,
    guide: usize,
    pane: Pane,
    scroll: usize,
    heading: usize,
    // the search being typed, and the one applied to the list
    search: Option<String>,
    filter: String,
    pages: Vec<Page>,
    changelog: Page,
    // Markdown wrapped to a width, by page, piece and width
    wrapped: RefCell<HashMap<(usize, usize, usize), Vec<String>>>,
    drawn: RefCell<Drawn>,
    // whether the footer says which keys do what, which a touch screen has no use for
    pub(crate) hints: bool,
}

impl Docs {
    pub(crate) fn new(tab: Tab, guide: usize) -> Docs {
        Docs {
            tab,
            guide,
            pane: Pane::Page,
            scroll: 0,
            heading: 0,
            search: None,
            filter: String::new(),
            pages: GUIDES.iter().map(|g| parse(g.src)).collect(),
            changelog: parse(&plain_headings(CHANGELOG)),
            wrapped: RefCell::default(),
            drawn: RefCell::default(),
            hints: true,
        }
    }

    fn page(&self) -> (usize, &Page) {
        match self.tab {
            Tab::Guides => (self.guide, &self.pages[self.guide]),
            Tab::Changelog => (usize::MAX, &self.changelog),
        }
    }

    /// The guides the search lets through, as their places in [`GUIDES`].
    fn listed(&self) -> Vec<usize> {
        let q = self.search.as_deref().unwrap_or(&self.filter).to_lowercase();
        (0..GUIDES.len())
            .filter(|&i| {
                q.is_empty() || GUIDES[i].title.to_lowercase().contains(&q) || GUIDES[i].src.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// The page's rows at `width`, `t` seconds in, and the row each heading starts on.
    fn rows(&self, width: usize, t: f64) -> (Vec<String>, Vec<usize>) {
        let (id, page) = self.page();
        let mut rows = Vec::new();
        let mut starts = Vec::new();
        let mut heads = page.headings.iter().peekable();
        for (i, piece) in page.pieces.iter().enumerate() {
            if !rows.is_empty() {
                rows.push(String::new());
            }
            while heads.next_if(|(at, _)| *at == i).is_some() {
                starts.push(rows.len());
            }
            match piece {
                Piece::Text(src) => {
                    let mut cache = self.wrapped.borrow_mut();
                    let lines = cache
                        .entry((id, i, width))
                        .or_insert_with(|| tinsel::markdown::wrapped(src, width).into_lines());
                    rows.extend(lines.iter().cloned());
                }
                Piece::Example(name) => rows.extend(
                    demos::lines(name, width, t)
                        .unwrap_or_else(|| vec![paint(palette::MUTED, &format!("  (no example called {name})"))]),
                ),
            }
        }
        (rows, starts)
    }

    fn has_examples(&self) -> bool {
        self.page().1.pieces.iter().any(|p| matches!(p, Piece::Example(_)))
    }

    fn panes(&self) -> Vec<Pane> {
        let d = self.drawn.borrow();
        let mut panes = Vec::new();
        if d.list.is_some() {
            panes.push(Pane::List);
        }
        panes.push(Pane::Page);
        if d.headings.is_some() && !self.page().1.headings.is_empty() {
            panes.push(Pane::Headings);
        }
        panes
    }

    fn max_scroll(&self) -> usize {
        let d = self.drawn.borrow();
        d.total.saturating_sub(d.shown)
    }

    fn scroll_by(&mut self, by: isize) {
        self.scroll = self.scroll.saturating_add_signed(by).min(self.max_scroll());
    }

    fn open(&mut self, guide: usize) {
        self.tab = Tab::Guides;
        self.guide = guide;
        self.scroll = 0;
        self.heading = 0;
    }

    fn step_guide(&mut self, by: isize) {
        let listed = self.listed();
        if listed.is_empty() {
            return;
        }
        let at = listed.iter().position(|&g| g == self.guide).unwrap_or(0);
        let next = (at as isize + by).clamp(0, listed.len() as isize - 1) as usize;
        self.open(listed[next]);
    }

    // "on this page": the page's title, for its top, then its headings, with the row each starts on
    fn contents(&self) -> (Vec<String>, Vec<usize>) {
        let (_, starts) = self.rows(text_width(self.drawn.borrow().page), 0.0);
        let title = match self.tab {
            Tab::Guides => GUIDES[self.guide].title,
            Tab::Changelog => "Changelog",
        };
        let names = std::iter::once(title.to_string())
            .chain(self.page().1.headings.iter().map(|(_, t)| t.clone()))
            .collect();
        (names, std::iter::once(0).chain(starts).collect())
    }

    fn jump_to_heading(&mut self, i: usize) {
        let (_, rows) = self.contents();
        if let Some(&row) = rows.get(i) {
            self.heading = i;
            self.scroll = row.min(self.max_scroll());
        }
    }

    fn switch_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.scroll = 0;
            self.heading = 0;
            self.pane = Pane::Page;
        }
    }

    // where the page is scrolled to in "on this page": the last entry at or above the top
    fn current_heading(&self, rows: &[usize]) -> usize {
        rows.iter().rposition(|&row| row <= self.scroll + 1).unwrap_or(0)
    }
}

// how wide the page's text is inside its panel: clear of the scrollbar, with a gap
fn text_width(inner: Rect) -> usize {
    inner.width.saturating_sub(2)
}

impl App for Docs {
    fn view(&self, frame: &mut Frame) {
        let area = frame.area();
        let (w, h) = (area.width, area.height);
        if w < 40 || h < 10 {
            frame.print(area, 0, "  make the window bigger to read the docs");
            return;
        }
        let t = frame.time();

        // the top line: name, tabs, and search
        let top = Rect::new(0, 0, w, 1);
        frame.print(
            top,
            0,
            &format!(
                "  {} {}",
                paint(palette::ROSE, "✶"),
                bold(&paint(palette::TEXT, "tinsel docs"))
            ),
        );
        let tabs = Rect::new(20, 0, 30, 1);
        let tab = if self.tab == Tab::Guides { 0 } else { 1 };
        Tabs::new(["Guides", "Changelog"]).selected(tab).render(tabs, frame);
        let right = match &self.search {
            Some(q) => format!("{} {}{}  ", paint(palette::ROSE, "/"), q, paint(palette::ROSE, "▏")),
            None if !self.filter.is_empty() => {
                format!(
                    "{} {}  ",
                    paint(palette::MUTED, "showing"),
                    paint(palette::ROSE, &self.filter)
                )
            }
            None => format!("{}  {}  ", paint(palette::MUTED, "search"), paint(palette::ROSE, "/")),
        };
        frame.print_right(top, 0, &right);

        let body = Rect::new(0, 2, w, h - 4);
        let show_list = self.tab == Tab::Guides && w >= 72;
        let show_headings = w >= 110;
        let list = show_list.then(|| Rect::new(1, body.y, 30, body.height));
        let headings = show_headings.then(|| Rect::new(w - 33, body.y, 32, body.height));
        let left = list.map_or(1, |l| l.x + l.width + 1);
        let right_edge = headings.map_or(w - 1, |r| r.x - 1);
        let main = Rect::new(left, body.y, right_edge - left, body.height);

        let listed = self.listed();
        if let Some(rect) = list {
            let titles: Vec<&str> = listed.iter().map(|&g| GUIDES[g].title).collect();
            let at = listed.iter().position(|&g| g == self.guide).unwrap_or(0);
            let body = List::new(titles).selected(at);
            frame.render(
                rect,
                Panel::new("guides")
                    .count(listed.len())
                    .body(body)
                    .focused(self.pane == Pane::List),
            );
        }

        let (title, status) = match self.tab {
            Tab::Guides => (
                GUIDES[self.guide].title,
                format!("{} of {}", self.guide + 1, GUIDES.len()),
            ),
            Tab::Changelog => ("Changelog", format!("tinsel {}", env!("CARGO_PKG_VERSION"))),
        };
        let panel = Panel::new(title)
            .status(status)
            .focused(self.pane == Pane::Page)
            .padding(2, 1);
        let inner = panel.inner(main);
        frame.render(main, panel);
        let width = text_width(inner);
        let (rows, starts) = self.rows(width, t);
        let shown = inner.height;
        let scroll = self.scroll.min(rows.len().saturating_sub(shown));
        for (i, row) in rows.iter().skip(scroll).take(shown).enumerate() {
            frame.print(Rect::new(inner.x, inner.y + i, width, 1), 0, row);
        }
        if rows.len() > shown {
            let bar = Rect::new(inner.x + inner.width - 1, inner.y, 1, shown);
            frame.render(bar, Scrollbar::new(scroll, rows.len()));
        }

        if let Some(rect) = headings {
            let title = match self.tab {
                Tab::Guides => GUIDES[self.guide].title,
                Tab::Changelog => "Changelog",
            };
            let names: Vec<&str> = std::iter::once(title)
                .chain(self.page().1.headings.iter().map(|(_, t)| t.as_str()))
                .collect();
            let rows: Vec<usize> = std::iter::once(0).chain(starts.iter().copied()).collect();
            let at = if self.pane == Pane::Headings {
                self.heading
            } else {
                self.current_heading(&rows)
            };
            let body = List::new(names).selected(at.min(rows.len() - 1));
            frame.render(
                rect,
                Panel::new("on this page")
                    .body(body)
                    .focused(self.pane == Pane::Headings),
            );
        }

        let keys: &[(&str, &str)] = match (&self.search, self.pane) {
            (Some(_), _) => &[("type", "to search"), ("enter", "keep it"), ("esc", "clear it")],
            (None, Pane::List) => &[
                ("j k", "move"),
                ("enter", "read"),
                ("tab", "next pane"),
                ("/", "search"),
                ("q", "quit"),
            ],
            (None, Pane::Headings) => &[
                ("j k", "move"),
                ("enter", "go there"),
                ("tab", "next pane"),
                ("q", "quit"),
            ],
            (None, Pane::Page) => &[
                ("j k", "scroll"),
                ("space", "page"),
                ("n N", "next guide"),
                ("tab", "next pane"),
                ("1 2", "guides, changelog"),
                ("q", "quit"),
            ],
        };
        let version = format!("tinsel {}", env!("CARGO_PKG_VERSION"));
        // on a web page, where it isn't plain that a terminal library drew it
        let status = if cfg!(all(feature = "web", target_family = "wasm")) {
            format!(
                "{} {} {}",
                paint(palette::MUTED, "these docs were built with tinsel"),
                paint(palette::MUTED, "·"),
                version.replace("tinsel ", "tinsel v")
            )
        } else {
            version
        };
        Footer::new()
            .keys(if self.hints { keys } else { &[] })
            .status(status)
            // in from the edges as far as the top line is
            .render(Rect::new(2, h - 1, w - 4, 1), frame);

        if self.has_examples() {
            frame.animate();
        }
        *self.drawn.borrow_mut() = Drawn {
            tabs,
            list,
            page: inner,
            headings,
            total: rows.len(),
            shown,
        };
    }

    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(key) => return self.key(*key),
            Event::Mouse(m) => {
                let d = *self.drawn.borrow();
                let within = |r: Rect| m.x >= r.x && m.x < r.x + r.width && m.y >= r.y && m.y < r.y + r.height;
                match m.kind {
                    MouseKind::ScrollUp => self.scroll_by(-3),
                    MouseKind::ScrollDown => self.scroll_by(3),
                    MouseKind::Click if within(d.tabs) => {
                        let tabs = Tabs::new(["Guides", "Changelog"]);
                        match tabs.at(d.tabs, m.x, m.y) {
                            Some(0) => self.switch_tab(Tab::Guides),
                            Some(1) => self.switch_tab(Tab::Changelog),
                            _ => {}
                        }
                    }
                    MouseKind::Click if d.list.is_some_and(within) => {
                        let row = m.y.saturating_sub(d.list.unwrap().y + 1);
                        if let Some(&g) = self.listed().get(row) {
                            self.open(g);
                            self.pane = Pane::List;
                        }
                    }
                    MouseKind::Click if d.headings.is_some_and(within) => {
                        let row = m.y.saturating_sub(d.headings.unwrap().y + 1);
                        self.jump_to_heading(row);
                        self.pane = Pane::Headings;
                    }
                    MouseKind::Click if within(d.page) => self.pane = Pane::Page,
                    _ => {}
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn title(&self) -> Option<String> {
        Some(match self.tab {
            Tab::Guides => format!("{} · tinsel docs", GUIDES[self.guide].title),
            Tab::Changelog => "Changelog · tinsel docs".into(),
        })
    }

    fn mouse(&self) -> bool {
        true
    }
}

impl Docs {
    fn key(&mut self, key: Key) -> Flow {
        if let Some(q) = &mut self.search {
            match key {
                Key::Char(c) => q.push(c),
                Key::Backspace => {
                    q.pop();
                }
                Key::Enter => {
                    self.filter = self.search.take().unwrap_or_default();
                    if let Some(&first) = self.listed().first() {
                        self.open(first);
                    }
                    self.pane = Pane::List;
                }
                Key::Esc => {
                    self.search = None;
                    self.filter.clear();
                }
                _ => {}
            }
            return Flow::Continue;
        }
        let page = self.drawn.borrow().shown.max(1) as isize;
        match key {
            Key::Char('q') => return Flow::Quit,
            Key::Esc if !self.filter.is_empty() => self.filter.clear(),
            Key::Esc => return Flow::Quit,
            Key::Char('/') => self.search = Some(self.filter.clone()),
            Key::Char('1') => self.switch_tab(Tab::Guides),
            Key::Char('2') => self.switch_tab(Tab::Changelog),
            Key::Tab | Key::BackTab => {
                let panes = self.panes();
                let at = panes.iter().position(|&p| p == self.pane).unwrap_or(0);
                let by = if key == Key::Tab { 1 } else { panes.len() - 1 };
                self.pane = panes[(at + by) % panes.len()];
                if self.pane == Pane::Headings {
                    let (_, rows) = self.contents();
                    self.heading = self.current_heading(&rows);
                }
            }
            Key::Char('n') if self.tab == Tab::Guides => self.step_guide(1),
            Key::Char('N') if self.tab == Tab::Guides => self.step_guide(-1),
            Key::Char('j') | Key::Down => match self.pane {
                Pane::List => self.step_guide(1),
                Pane::Page => self.scroll_by(1),
                Pane::Headings => self.jump_to_heading((self.heading + 1).min(self.contents().1.len() - 1)),
            },
            Key::Char('k') | Key::Up => match self.pane {
                Pane::List => self.step_guide(-1),
                Pane::Page => self.scroll_by(-1),
                Pane::Headings => self.jump_to_heading(self.heading.saturating_sub(1)),
            },
            Key::Enter => self.pane = Pane::Page,
            Key::Char(' ') | Key::PageDown | Key::Ctrl('f') => self.scroll_by(page - 2),
            Key::Char('b') | Key::PageUp | Key::Ctrl('b') => self.scroll_by(2 - page),
            Key::Ctrl('d') => self.scroll_by(page / 2),
            Key::Ctrl('u') => self.scroll_by(-page / 2),
            Key::Char('g') | Key::Home => self.scroll = 0,
            Key::Char('G') | Key::End => self.scroll = self.max_scroll(),
            _ => {}
        }
        Flow::Continue
    }
}

/// The guide `name` names: its slug, or its title or the start of either, any case.
fn find(name: &str) -> Option<usize> {
    let name = name.trim().to_lowercase();
    let starts = |s: &str| s.to_lowercase().starts_with(&name);
    GUIDES
        .iter()
        .position(|g| g.slug == name)
        .or_else(|| GUIDES.iter().position(|g| g.title.to_lowercase() == name))
        .or_else(|| GUIDES.iter().position(|g| starts(g.slug) || starts(g.title)))
}

/// `tinsel docs [PAGE]`: the reader on a terminal, or the guide as plain text in a pipe.
pub fn run(page: Option<&str>) {
    let (tab, guide) = match page {
        None => (Tab::Guides, 0),
        Some(p) if p.eq_ignore_ascii_case("changelog") => (Tab::Changelog, 0),
        Some(p) => match find(p) {
            Some(g) => (Tab::Guides, g),
            None => {
                let names: Vec<&str> = GUIDES.iter().map(|g| g.slug).collect();
                crate::usage_error(&format!(
                    "there's no guide called {p}: try {}, or changelog",
                    names.join(", ")
                ));
            }
        },
    };
    let docs = Docs::new(tab, guide);
    if !std::io::stdout().is_terminal() {
        // the examples as they look a few seconds in, when there's something to see
        let (rows, _) = docs.rows(tinsel::terminal_width(), 4.0);
        for row in rows {
            println!("{row}");
        }
        return;
    }
    if let Err(e) = app::run(docs) {
        tinsel::error(&format!("couldn't open the docs: {e}"));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guide_parses_and_names_its_headings() {
        for g in GUIDES {
            let page = parse(g.src);
            assert!(!page.pieces.is_empty(), "{}", g.slug);
            assert!(
                g.src.starts_with(&format!("# {}", g.title)),
                "{} starts with its title",
                g.slug
            );
            for p in &page.pieces {
                if let Piece::Example(name) = p {
                    assert!(
                        demos::lines(name, 80, 0.0).is_some(),
                        "{} uses an example that isn't there: {name}",
                        g.slug
                    );
                }
            }
        }
        let live = parse(GUIDES[2].src);
        let names: Vec<&str> = live.headings.iter().map(|(_, t)| t.as_str()).collect();
        assert!(
            names.contains(&"Steps inside steps") && names.contains(&"Run real commands as steps"),
            "{names:?}"
        );
    }

    #[test]
    fn code_isnt_mistaken_for_headings() {
        let page = parse("# T\n\n```rust\n## not a heading\n```\n\n## Real\n\ntext\n");
        let names: Vec<&str> = page.headings.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }

    #[test]
    fn the_changelog_lists_its_versions_without_their_links() {
        let docs = Docs::new(Tab::Changelog, 0);
        assert!(!docs.changelog.headings.is_empty());
        assert!(
            docs.changelog
                .headings
                .iter()
                .all(|(_, t)| !t.contains('[') && !t.contains("]("))
        );
        let (rows, _) = docs.rows(100, 0.0);
        let page = tinsel::text::strip_ansi(&rows.join("\n"));
        assert!(!page.contains("](https://"), "the page shows them plain too");
        assert!(page.contains("1.1.0 - 2026-11-25"), "{page}");
    }

    #[test]
    fn a_version_with_nothing_in_it_isnt_shown() {
        let plain = plain_headings(
            "# Changelog\n\n## [Unreleased]\n\n## [1.2.0](https://x) - 2026-11-26\n\n### Added\n\n- pictures\n",
        );
        assert!(!plain.contains("Unreleased"), "{plain}");
        assert!(plain.contains("## 1.2.0 - 2026-11-26"));
        let with = plain_headings("## [Unreleased]\n\n### Fixed\n\n- a thing\n\n## [1.2.0](https://x) - 2026-11-26\n");
        assert!(with.starts_with("## Unreleased"), "kept once it has something: {with}");
    }

    #[test]
    fn on_this_page_starts_at_the_top() {
        let mut docs = Docs::new(Tab::Guides, 2);
        let (names, rows) = docs.contents();
        assert_eq!((names[0].as_str(), rows[0]), ("Live step lists", 0));
        assert_eq!(names.len(), docs.pages[2].headings.len() + 1);
        assert_eq!(docs.current_heading(&rows), 0, "at the top, the top");
        docs.scroll = rows[2];
        assert_eq!(docs.current_heading(&rows), 2);
    }

    #[test]
    fn guides_are_found_by_slug_title_or_start() {
        assert_eq!(find("live"), Some(2));
        assert_eq!(find("Asking questions"), Some(6));
        assert_eq!(find("pic"), Some(9));
        assert_eq!(find("nothing like it"), None);
    }

    #[test]
    fn headings_say_where_they_start() {
        let docs = Docs::new(Tab::Guides, 2);
        let (rows, starts) = docs.rows(80, 0.0);
        assert_eq!(starts.len(), docs.pages[2].headings.len());
        for (&row, (_, title)) in starts.iter().zip(&docs.pages[2].headings) {
            let plain = tinsel::text::strip_ansi(&rows[row]);
            assert!(plain.contains(title.as_str()), "{title} at row {row}: {plain:?}");
        }
    }
}

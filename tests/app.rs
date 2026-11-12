use tinsel::app::{self, App, Cursor, Event, Flow, Frame, Grid, Key, List, Panel, Size::*, Tabs, hints};

struct Picker {
    tab: usize,
    selected: usize,
}

const FLAVOURS: [&str; 3] = ["strawberry", "pistachio", "mint"];

impl App for Picker {
    fn view(&self, frame: &mut Frame) {
        let [tabs, main, footer] = frame.area().rows([Fixed(1), Fill(1), Fixed(1)]);
        frame.render(tabs, Tabs::new(["cones", "cups"]).selected(self.tab));
        let list = List::new(FLAVOURS).selected(self.selected);
        frame.render(main, Panel::new("flavours").body(list).footer("3"));
        frame.render(footer, hints(&[("q", "quit")]));
    }

    fn event(&mut self, event: &Event) -> Flow {
        app::navigate(&mut self.selected, FLAVOURS.len(), event);
        match event {
            Event::Key(Key::Tab) => self.tab = 1 - self.tab,
            Event::Key(Key::Enter) => return Flow::Quit,
            _ => {}
        }
        Flow::Continue
    }
}

#[test]
fn an_app_draws_and_moves() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let mut picker = Picker { tab: 0, selected: 0 };
    for key in [Key::Down, Key::Down, Key::Down, Key::Up, Key::Tab] {
        assert_eq!(picker.event(&Event::Key(key)), Flow::Continue);
    }
    assert_eq!(picker.event(&Event::Key(Key::Enter)), Flow::Quit);
    assert_eq!(
        app::snapshot(&picker, 20, 7).plain(),
        [
            " cones   cups",
            "╭─ flavours ───────╮",
            "│   strawberry     │",
            "│ › pistachio      │",
            "│   mint           │",
            "╰────────────── 3 ─╯",
            "q quit",
        ]
        .join("\n")
    );
}

#[test]
fn widgets_stay_inside_their_area() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let mut frame = Frame::new(12, 3);
    let [left, right] = frame.area().columns([Fill(1), Fill(1)]);
    frame.render(left, "a sentence that wraps");
    frame.render(right, Panel::new("a very long title"));
    assert_eq!(frame.snapshot().plain(), "a     ╭─ … ╮\nsenten│    │\nce    ╰────╯");
}

fn services(n: usize) -> Vec<[String; 3]> {
    (0..n)
        .map(|i| [format!("service-{i}"), format!("v1.{i}"), "production".to_string()])
        .collect()
}

fn grid<'a>(rows: &[[String; 3]], cursor: &'a Cursor) -> Grid<'a> {
    rows.iter()
        .fold(Grid::new(["service", "version", "env"]), |g, r| {
            g.row(r.iter().cloned())
        })
        .cursor(cursor)
}

#[test]
fn a_grid_keeps_the_cursor_and_header_in_view() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let rows = services(8);
    let mut cursor = Cursor::new();
    let draw = |cursor: &Cursor| {
        let mut frame = Frame::new(36, 5);
        frame.render(frame.area(), grid(&rows, cursor));
        frame.snapshot().plain()
    };
    for _ in 0..4 {
        cursor.navigate(rows.len(), &Event::Key(Key::Down));
    }
    // three rows fit: moving down scrolls just enough to keep the cursor on the last one
    assert_eq!(
        draw(&cursor),
        [
            "  service     version   env",
            "  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈",
            "  service-2   v1.2      production ┃",
            "  service-3   v1.3      production │",
            "› service-4   v1.4      production │",
        ]
        .join("\n")
    );
    let shown = draw(&cursor);
    assert!(shown.starts_with("  service     version   env"), "{shown}");
    assert!(shown.lines().last().unwrap().starts_with("› service-4"), "{shown}");
    assert!(!shown.contains("service-1 "), "{shown}");
    // moving back up only scrolls once the cursor reaches the top
    cursor.navigate(rows.len(), &Event::Key(Key::Up));
    cursor.navigate(rows.len(), &Event::Key(Key::Up));
    let shown = draw(&cursor);
    assert!(shown.lines().nth(2).unwrap().starts_with("› service-2"), "{shown}");
    cursor.navigate(rows.len(), &Event::Key(Key::Up));
    let shown = draw(&cursor);
    assert!(shown.lines().nth(2).unwrap().starts_with("› service-1"), "{shown}");
}

#[test]
fn a_grid_scrolls_sideways_a_column_at_a_time() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let rows = services(2);
    let mut cursor = Cursor::new();
    let draw = |cursor: &Cursor| {
        let mut frame = Frame::new(20, 4);
        frame.render(frame.area(), grid(&rows, cursor));
        frame.snapshot().plain()
    };
    assert!(!cursor.scroll_left());
    assert_eq!(draw(&cursor).lines().next(), Some("  service     versio"));
    assert_eq!(
        draw(&cursor).lines().nth(1),
        Some("  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈›"),
        "more to the right"
    );
    assert!(cursor.scroll_right());
    assert_eq!(draw(&cursor).lines().next(), Some("  version   env"));
    assert_eq!(draw(&cursor).lines().nth(1), Some(" ‹┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈›"));
    assert_eq!(draw(&cursor).lines().nth(2), Some("› v1.0      producti"));
    // everything from the last column on fits, so there's nowhere further to go
    assert!(cursor.scroll_right());
    assert_eq!(draw(&cursor).lines().next(), Some("  env"));
    assert_eq!(draw(&cursor).lines().nth(2), Some("› production"));
    assert!(!cursor.scroll_right());
    assert_eq!(cursor.column(), 2);
    assert!(cursor.scroll_left() && cursor.scroll_left() && !cursor.scroll_left());
    assert_eq!(draw(&cursor).lines().next(), Some("  service     versio"));
}

/// A little services screen with every piece on it, to see how they sit together.
struct Services {
    cursor: Cursor,
    filter: tinsel::app::Input,
    toasts: tinsel::app::Toasts,
    confirm: bool,
}

impl App for Services {
    fn view(&self, frame: &mut Frame) {
        let [body, footer] = frame.area().rows([Fill(1), Fixed(1)]);
        let panel = Panel::new("services").count(3).focused(true);
        let inner = panel.inner(body);
        frame.render(body, panel);
        let [filter, rows] = inner.rows([Fixed(1), Fill(1)]);
        frame.render(filter, &self.filter);
        let grid = Grid::new(["service", "staging", "production"])
            .row(["api", "v2.1", "v2.1"])
            .row(["web", "v2.2", "v2.0"])
            .row(["worker", "v1.9", "v1.9"])
            .cursor(&self.cursor);
        frame.render(rows, grid);
        let footer_line = tinsel::app::Footer::new()
            .keys(&[("↑↓", "move"), ("/", "filter")])
            .status("refreshed just now");
        frame.render(footer, footer_line);
        frame.render(body.shrink(1, 1), &self.toasts);
        if self.confirm {
            let ask = Panel::new("restart").body("Restart web?").focused(true);
            frame.render(frame.area(), tinsel::app::Modal::new(ask));
        }
    }
}

#[test]
fn every_piece_together_in_plain_text() {
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let mut app = Services {
        cursor: Cursor::new(),
        filter: tinsel::app::Input::new().prompt("/").placeholder("filter by name"),
        toasts: tinsel::app::Toasts::new(),
        confirm: false,
    };
    app.cursor.navigate(3, &Event::Key(Key::Down));
    assert_eq!(
        app::snapshot(&app, 44, 9).plain(),
        [
            "╭─ ✦ services 3 ───────────────────────────╮",
            "│ / filter by name                         │",
            "│   service   staging   production         │",
            "│   ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈         │",
            "│   api       v2.1      v2.1               │",
            "│ › web       v2.2      v2.0               │",
            "│   worker    v1.9      v1.9               │",
            "╰──────────────────────────────────────────╯",
            "↑↓ move · / filter        refreshed just now",
        ]
        .join("\n")
    );

    app.toasts.push(tinsel::Tone::Success, "restarted web");
    app.confirm = true;
    let mut frame = Frame::new(44, 9).at(1.0);
    app.view(&mut frame);
    assert_eq!(
        frame.snapshot().plain(),
        [
            "╭─ ✦ services 3 ───────────────────────────╮",
            "│ / filter by name                         │",
            "│   service   staging   production         │",
            "│   ┈┈┈┈┈┈┈┈┈┈╭─ ✦ restart ──╮┈┈┈┈         │",
            "│   api       │ Restart web? │────────────╮│",
            "│ › web       ╰──────────────╯tarted web  ││",
            "│   worker    v1.9    ╰━━━━━━━━━━━━━━─────╯│",
            "╰──────────────────────────────────────────╯",
            "↑↓ move · / filter        refreshed just now",
        ]
        .join("\n")
    );
}

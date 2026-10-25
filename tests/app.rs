use tinsel::app::{self, App, Event, Flow, Frame, Key, List, Panel, Size::*, Tabs, hints};

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

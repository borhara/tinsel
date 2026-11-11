//! The app pieces in full colour. Colour mode is process-wide, so these live in their own
//! file (Cargo runs each file under `tests/` as a separate process).

use tinsel::app::{Cursor, Frame, Grid, List, Modal, Panel, Plot, Rect, Sparkles, Stat, Tabs};
use tinsel::{ColorMode, Theme, palette, set_color_mode, set_theme};

fn colour() {
    set_color_mode(ColorMode::TrueColor);
    set_theme(Theme::Dark);
}

#[test]
fn focused_panels_shimmer_and_quiet_ones_dont() {
    colour();
    let at = |t: f32, focused: bool| {
        let mut f = Frame::new(12, 3).at(t);
        f.render(f.area(), Panel::new("hi").focused(focused));
        f.snapshot().to_string()
    };
    assert_ne!(at(0.0, true), at(1.0, true), "the glint moves");
    assert_eq!(at(0.0, false), at(1.0, false), "a quiet border stays still");
    // the border is rose when focused, the quiet line colour otherwise
    let rose = format!("{};{};{}", palette::ROSE.0, palette::ROSE.1, palette::ROSE.2);
    assert!(at(0.0, true).contains(&rose) && !at(0.0, false).contains(&rose));
}

#[test]
fn sparkles_twinkle() {
    colour();
    let at = |t: f32| {
        let mut f = Frame::new(20, 4).at(t);
        f.render(f.area(), Sparkles::new().density(0.5));
        f.snapshot().to_string()
    };
    assert_ne!(at(0.0), at(2.0));
}

#[test]
fn the_chosen_row_and_tint_paint_the_background() {
    colour();
    let mut f = Frame::new(10, 2);
    f.render(f.area(), List::new(["a", "b"]).selected(1));
    let shown = f.snapshot();
    // the chosen row has a background colour (48;2;…) all the way across; the other doesn't
    assert!(shown.lines()[1].contains("48;2;"), "{shown:?}");
    assert!(!shown.lines()[0].contains("48;2;"));

    let mut f = Frame::new(4, 1);
    f.print(f.area(), 0, "ab");
    f.tint(Rect::new(0, 0, 4, 1), palette::BERRY);
    let berry = format!("48;2;{};{};{}", palette::BERRY.0, palette::BERRY.1, palette::BERRY.2);
    let shown = f.snapshot();
    let row = &shown.lines()[0];
    // tinted blanks aren't trimmed, since they're visibly coloured
    assert!(row.contains(&berry) && tinsel::text::width(row) == 4, "{row:?}");
}

#[test]
fn colours_survive_the_trip_through_cells() {
    colour();
    let mut f = Frame::new(30, 6);
    let [stat, rest] = f
        .area()
        .columns([tinsel::app::Size::Fixed(12), tinsel::app::Size::Fill(1)]);
    f.render(stat, Stat::new("n", "42").note("up", palette::SAGE));
    f.render(rest.row(0), Tabs::new(["one", "two"]).selected(0));
    f.render(rest.shrink(0, 1), Plot::new(&[1.0, 2.0, 3.0]).color(palette::AMBER));
    let shown = f.snapshot().to_string();
    for c in [palette::SAGE, palette::AMBER, palette::PETAL] {
        assert!(shown.contains(&format!("{};{};{}", c.0, c.1, c.2)), "missing {c:?}");
    }
    // every styled row ends by resetting, so nothing leaks onto the next
    assert!(
        f.snapshot()
            .lines()
            .iter()
            .filter(|l| l.contains('\x1b'))
            .all(|l| l.ends_with("\x1b[0m"))
    );
    // and the plain text underneath is still right
    assert!(f.snapshot().plain().starts_with("n"));
}

#[test]
fn the_grid_cursor_is_a_bar_on_a_band_and_quieter_unfocused() {
    colour();
    let mut cursor = Cursor::new();
    cursor.select(1);
    let draw = |focused: bool| {
        let mut f = Frame::new(20, 6);
        let grid = Grid::new(["name", "n"])
            .row(["api", "1"])
            .row(["web", "2"])
            .row(["db", "3"])
            .row(["cache", "4"])
            .striped()
            .cursor(&cursor)
            .focused(focused);
        f.render(f.area(), grid);
        f.snapshot()
    };
    let shown = draw(true);
    let rose = format!("{};{};{}", palette::ROSE.0, palette::ROSE.1, palette::ROSE.2);
    assert!(
        shown.lines()[3].contains('▌') && shown.lines()[3].contains(&rose),
        "{shown:?}"
    );
    // odd rows are striped, even ones left plain
    assert!(!shown.lines()[2].contains("48;2;") && !shown.lines()[4].contains("48;2;"));
    assert!(shown.lines()[5].contains("48;2;"));
    assert_ne!(draw(false).lines()[3], shown.lines()[3]);
    assert!(!draw(false).lines()[3].contains(&rose));
}

#[test]
fn a_modal_fades_what_is_behind_it_and_casts_a_shadow() {
    colour();
    let mut f = Frame::new(30, 9);
    for row in 0..9 {
        f.print(f.area(), row, &tinsel::paint(palette::ROSE, &"x".repeat(30)));
    }
    let before = f.snapshot();
    f.render(f.area(), Modal::new(Panel::new("sure?").body("y / n")));
    let after = f.snapshot();
    let rose = format!("38;2;{};{};{}", palette::ROSE.0, palette::ROSE.1, palette::ROSE.2);
    // the top row is all behind: the same text, faded
    assert_eq!(
        tinsel::text::strip_ansi(&after.lines()[0]),
        tinsel::text::strip_ansi(&before.lines()[0])
    );
    assert!(before.lines()[0].contains(&rose) && !after.lines()[0].contains(&rose));
    // and under the modal's bottom edge, a shadow
    let bottom = after.lines().iter().rposition(|l| l.contains('╰')).unwrap();
    assert!(
        after.lines()[bottom + 1].contains("48;2;"),
        "{:?}",
        after.lines()[bottom + 1]
    );
}

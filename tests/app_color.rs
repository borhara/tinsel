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
fn text_drawn_over_a_tint_keeps_it() {
    colour();
    let tinted = |tint_first: bool| {
        let mut f = Frame::new(4, 1);
        if tint_first {
            f.tint(f.area(), palette::BERRY);
        }
        f.print(f.area(), 0, "ab");
        if !tint_first {
            f.tint(f.area(), palette::BERRY);
        }
        f.snapshot().to_string()
    };
    assert_eq!(tinted(true), tinted(false));
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
    // under the modal's bottom edge, a shadow: the same text again, fainter still, and no
    // block of background painted over it
    let bottom = after.lines().iter().rposition(|l| l.contains('╰')).unwrap();
    let under = &after.lines()[bottom + 1];
    assert_eq!(
        tinsel::text::strip_ansi(under),
        tinsel::text::strip_ansi(&before.lines()[bottom + 1])
    );
    assert!(!under.contains("48;2;"), "{under:?}");
    let faded = "38;2;94;49;66"; // rose, faded once
    let shaded = "38;2;38;20;26"; // and again, in the shadow
    assert!(after.lines()[0].contains(faded) && !after.lines()[0].contains(shaded));
    assert!(under.contains(faded) && under.contains(shaded), "{under:?}");
}

#[test]
fn grid_headings_are_bold_rose_and_never_the_cursor_row() {
    colour();
    let mut cursor = Cursor::new();
    cursor.select(0);
    let mut f = Frame::new(16, 4);
    f.render(
        f.area(),
        Grid::new(["name"]).heading("group").row(["api"]).cursor(&cursor),
    );
    let shown = f.snapshot();
    let rose = format!("38;2;{};{};{}", palette::ROSE.0, palette::ROSE.1, palette::ROSE.2);
    let heading = &shown.lines()[2];
    assert!(heading.contains(&format!(";1;{rose}")), "bold and rose: {heading:?}");
    // the band is behind the cursor's row, not the heading above it
    assert!(!heading.contains("48;2;"), "{heading:?}");
    assert!(shown.lines()[3].contains("48;2;"), "{shown:?}");
}

#[test]
fn panel_titles_and_statuses_keep_their_own_colours() {
    colour();
    let rgb = |c: tinsel::Rgb| format!("38;2;{};{};{}", c.0, c.1, c.2);
    let top = |panel: Panel| {
        let mut f = Frame::new(40, 2);
        f.render(f.area(), panel);
        f.snapshot().lines()[0].clone()
    };
    let title = tinsel::Line::new().push("● ", palette::RED).push("api", palette::SAGE);
    let shown = top(Panel::new(title.to_string()).status("staging"));
    assert!(shown.contains(&rgb(palette::RED)) && shown.contains(&rgb(palette::SAGE)));
    // plain text gets the panel's own colours: a soft title, a muted status
    let shown = top(Panel::new("api").status("staging"));
    assert!(shown.contains(&rgb(palette::SOFT)) && shown.contains(&rgb(palette::MUTED)));
    let shown = top(Panel::new("api").status(tinsel::paint(palette::AMBER, "stale")));
    assert!(shown.contains(&rgb(palette::AMBER)) && !shown.contains(&rgb(palette::MUTED)));
    // a styled title is cut short by what shows, not by its escape codes
    let mut f = Frame::new(9, 2);
    f.render(f.area(), Panel::new(tinsel::paint(palette::RED, "services")));
    let shown = f.snapshot();
    assert_eq!(shown.plain().lines().next(), Some("╭─ s… ──╮"));
    assert!(shown.lines()[0].contains(&rgb(palette::RED)));
}

#[test]
fn tabs_take_their_own_colours_and_badges() {
    colour();
    let fg = |c: tinsel::Rgb| format!("38;2;{};{};{}", c.0, c.1, c.2);
    let mut f = Frame::new(40, 1);
    let tabs = Tabs::new(["overview", "errors", "deploy"])
        .color(1, palette::RED)
        .badge(2, "✔", palette::SAGE)
        .selected(2);
    f.render(f.area(), &tabs);
    let row = f.snapshot().lines()[0].clone();
    assert!(
        row.contains(&fg(palette::RED)) && row.contains(&fg(palette::SAGE)),
        "{row:?}"
    );
    // the chosen tab's pill runs under its badge, to its last space
    let pill = row.find("48;2;").unwrap();
    assert!(row[..pill].contains("errors") && !row[..pill].contains("deploy"));
    assert!(row[pill..].contains('✔'), "{row:?}");
    assert!(tinsel::text::strip_ansi(&row).ends_with("deploy ✔ "), "{row:?}");
}

#[test]
fn a_footer_short_of_room_keeps_its_colours() {
    colour();
    let fg = |c: tinsel::Rgb| format!("38;2;{};{};{}", c.0, c.1, c.2);
    let footer = tinsel::app::Footer::new()
        .keys(&[("q", "quit"), ("?", "help")])
        .status(tinsel::paint(palette::SAGE, "● connected"));
    let row = |w: usize| {
        let mut f = Frame::new(w, 1);
        f.render(f.area(), &footer);
        f.snapshot().lines()[0].clone()
    };
    let wide = row(40);
    assert!(wide.contains(&fg(palette::BLUSH)) && wide.contains(&fg(palette::SAGE)));
    // cut short, the status keeps its own colour and the keys are gone
    let narrow = row(6);
    assert_eq!(tinsel::text::strip_ansi(&narrow), "● con…");
    assert!(narrow.contains(&fg(palette::SAGE)) && !narrow.contains(&fg(palette::BLUSH)));
}

#[test]
fn faded_text_in_the_default_colour_gets_a_real_colour_and_isnt_bold() {
    colour();
    let mut f = Frame::new(4, 1);
    f.print(f.area(), 0, "\x1b[1mab");
    f.dim(f.area());
    let row = f.snapshot().lines()[0].clone();
    assert!(row.contains("38;2;"), "{row:?}");
    assert!(
        !row.contains("\x1b[1m") && !row.contains(";1;") && !row.contains("[1;"),
        "{row:?}"
    );
}

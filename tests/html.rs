//! Frames and blocks as HTML. The colours only show with colour on, and colour mode is
//! process-wide, so these live in their own file.

use std::sync::{Mutex, MutexGuard};

use tinsel::app::{Frame, Grid, List, Panel, Rect, Size};
use tinsel::{Block, ColorMode, Theme, palette, set_color_mode, set_theme};

/// Colour on and the dark theme, held until the test ends: one test here tries the light
/// theme, and the theme is process-wide too.
fn colour() -> MutexGuard<'static, ()> {
    static THEME: Mutex<()> = Mutex::new(());
    let guard = THEME.lock().unwrap_or_else(|e| e.into_inner());
    set_color_mode(ColorMode::TrueColor);
    set_theme(Theme::Dark);
    guard
}

fn hex(c: tinsel::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

#[test]
fn a_page_is_a_pre_with_a_line_per_row() {
    let _theme = colour();
    let mut f = Frame::new(12, 3);
    f.render(f.area(), Panel::new("hi").body("ok"));
    let page = f.html();
    assert!(page.starts_with("<!DOCTYPE html>\n<html>"), "{page}");
    assert!(page.contains("<meta charset=\"utf-8\">"));
    assert!(page.trim_end().ends_with("</html>"));
    let pre = f.html_pre();
    assert!(page.contains(&pre));
    assert!(pre.starts_with("<pre style=\"") && pre.ends_with("</pre>"));
    assert!(pre.contains("font-family:ui-monospace,monospace"));
    // three rows, so two line breaks, and every span is closed
    assert_eq!(pre.matches('\n').count(), 2);
    assert_eq!(pre.matches("<span").count(), pre.matches("</span>").count());
    // the text is all there, tags aside
    let text = strip_tags(&pre);
    assert_eq!(
        text,
        f.snapshot()
            .plain()
            .lines()
            .map(|l| format!("{l:12}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn text_is_escaped() {
    let _theme = colour();
    let mut f = Frame::new(20, 1);
    f.print(f.area(), 0, "<b>\"a\" & 'b'</b>");
    let pre = f.html_pre();
    assert!(pre.contains("&lt;b&gt;&quot;a&quot; &amp; 'b'&lt;/b&gt;"), "{pre}");
    assert!(!strip_tags(&pre).contains('<'));
}

#[test]
fn colours_and_attributes_become_css() {
    let _theme = colour();
    let mut f = Frame::new(30, 1);
    let text = format!(
        "{} {} {}",
        tinsel::paint(palette::ROSE, "rose"),
        tinsel::bold("loud"),
        tinsel::underline("under")
    );
    f.print(f.area(), 0, &text);
    let pre = f.html_pre();
    assert!(
        pre.contains(&format!("<span style=\"color:{}\">rose </span>", hex(palette::ROSE))),
        "{pre}"
    );
    assert!(pre.contains("<span style=\"font-weight:bold\">loud </span>"), "{pre}");
    assert!(
        pre.contains("<span style=\"text-decoration:underline\">under</span>"),
        "{pre}"
    );
    // the page's own colours are tinsel's text on a dark background
    assert!(pre.contains(&format!("color:{};", hex(palette::TEXT))));
}

#[test]
fn indexed_colours_reverse_and_dim_resolve_to_rgb() {
    let _theme = colour();
    let mut f = Frame::new(10, 1);
    // red, 256-colour 196 (pure red) on 21 (pure blue), and grey 244
    f.print(
        f.area(),
        0,
        "\x1b[31mr\x1b[0m\x1b[38;5;196;48;5;21mx\x1b[0m\x1b[38;5;244mg\x1b[0m",
    );
    let pre = f.html_pre();
    assert!(pre.contains("<span style=\"color:#cd0000\">r</span>"), "{pre}");
    assert!(
        pre.contains("<span style=\"color:#ff0000;background:#0000ff\">x</span>"),
        "{pre}"
    );
    assert!(pre.contains("<span style=\"color:#808080\">g       </span>"), "{pre}");

    let mut f = Frame::new(4, 1);
    f.print(
        f.area(),
        0,
        "\x1b[7;38;2;10;20;30mR\x1b[0m\x1b[2;38;2;200;200;200;48;2;0;0;0md\x1b[0m",
    );
    let pre = f.html_pre();
    // reversed: the text takes the page's background, the cell its ink
    assert!(pre.contains("background:#0a141e\">R</span>"), "{pre}");
    // dim: halfway to the background
    assert!(
        pre.contains("<span style=\"color:#646464;background:#000000\">d</span>"),
        "{pre}"
    );
}

#[test]
fn runs_of_the_same_style_share_a_span() {
    let _theme = colour();
    let mut f = Frame::new(10, 1);
    f.print(
        f.area(),
        0,
        &format!(
            "{}{}",
            tinsel::paint(palette::SAGE, "ab"),
            tinsel::paint(palette::SAGE, "cd")
        ),
    );
    let pre = f.html_pre();
    assert!(pre.contains(">abcd      </span>"), "{pre}");
    assert_eq!(pre.matches("<span").count(), 1);
}

#[test]
fn wide_characters_take_two_columns() {
    let _theme = colour();
    let mut f = Frame::new(6, 1);
    f.print(f.area(), 0, "a日b");
    let pre = f.html_pre();
    assert!(
        pre.contains("a<span style=\"display:inline-block;width:2ch\">日</span>b"),
        "{pre}"
    );
    // the cell a wide character covers isn't written again
    assert_eq!(strip_tags(&pre), "a日b  ");
}

#[test]
fn a_light_theme_gets_a_light_page() {
    let _theme = colour();
    let dark = Frame::new(2, 1).html_pre();
    set_theme(Theme::Light);
    let light = Frame::new(2, 1).html_pre();
    set_theme(Theme::Dark);
    assert_ne!(dark, light);
    assert!(light.contains("background:#fdfafc"), "{light}");
}

#[test]
fn blocks_become_pages_too() {
    let _theme = colour();
    let block = Block::from(vec![tinsel::paint(palette::AMBER, "warn"), "a\tb & c".to_string()]);
    let pre = block.html_pre();
    assert!(
        pre.contains(&format!("<span style=\"color:{}\">warn   </span>", hex(palette::AMBER))),
        "{pre}"
    );
    // as wide as its widest line, tabs and all
    assert_eq!(strip_tags(&pre), "warn   \na b &amp; c");
    assert!(block.html().contains(&pre));
}

#[test]
fn a_whole_screen_draws() {
    let _theme = colour();
    let mut f = Frame::new(40, 8);
    let [left, right] = f.area().columns([Size::Fixed(14), Size::Fill(1)]);
    f.render(left, Panel::new("hosts").body(List::new(["api", "web"]).selected(0)));
    f.render(right, Grid::new(["name", "n"]).row(["api", "1"]).row(["web", "2"]));
    f.tint(Rect::new(0, 7, 40, 1), palette::BERRY);
    let pre = f.html_pre();
    assert!(pre.contains(&format!("background:{}", hex(palette::BERRY))), "{pre}");
    assert_eq!(pre.matches('\n').count(), 7);
}

/// The text of some HTML, tags left out (entities kept).
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

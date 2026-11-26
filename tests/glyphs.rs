//! tinsel's symbols in ASCII, and ambiguous characters drawn wide, as text leaves for the
//! terminal and as it's measured.

use tinsel::app::{Frame, Panel};
use tinsel::{Glyphs, capture, text, with_ambiguous_wide, with_glyphs};

#[test]
fn a_panel_comes_out_in_ascii_the_same_size() {
    let printed = |g| {
        with_glyphs(g, || {
            capture(|| {
                let mut f = Frame::new(12, 3);
                f.render(f.area(), Panel::new("hi").body("x"));
                f.snapshot().print();
            })
        })
    };
    let unicode = printed(Glyphs::Unicode);
    let ascii = printed(Glyphs::Ascii);
    assert!(unicode.contains('╭') && unicode.contains('─'), "{unicode}");
    assert!(
        !ascii.contains('╭') && !ascii.contains('─') && ascii.contains('+') && ascii.contains('-'),
        "{ascii}"
    );
    let widths = |s: &str| s.lines().map(text::width).collect::<Vec<_>>();
    assert_eq!(
        widths(&ascii),
        widths(&unicode),
        "each symbol is one character, so nothing moves"
    );
}

#[test]
fn messages_come_out_in_ascii_too() {
    let said = |g| {
        with_glyphs(g, || {
            capture(|| {
                tinsel::success("done");
                tinsel::warn("careful");
            })
        })
    };
    assert_eq!(said(Glyphs::Unicode), "  ●  done\n  ▲  careful\n");
    assert_eq!(said(Glyphs::Ascii), "  *  done\n  ^  careful\n");
}

#[test]
fn ambiguous_characters_are_measured_as_the_terminal_draws_them() {
    // Greek and the degree sign are ambiguous; CJK is wide either way, and letters narrow
    let text = "α°日a";
    assert_eq!(with_ambiguous_wide(false, || text::width(text)), 5);
    assert_eq!(with_ambiguous_wide(true, || text::width(text)), 7);
    // tinsel's own symbols go to such a terminal as ASCII, so stay one column
    assert_eq!(with_ambiguous_wide(true, || text::width("─●")), 2);
}

//! A modal's shadow on a light background. The theme is process-wide, so this lives in its
//! own file (Cargo runs each file under `tests/` as a separate process).

use tinsel::app::{Frame, Modal, Panel};
use tinsel::{ColorMode, Theme, palette, set_color_mode, set_theme};

#[test]
fn on_a_light_background_the_shadow_is_pale_and_keeps_the_text() {
    set_color_mode(ColorMode::TrueColor);
    set_theme(Theme::Light);
    let mut f = Frame::new(30, 9);
    for row in 0..9 {
        f.print(f.area(), row, &tinsel::paint(palette::TEXT, &"x".repeat(30)));
    }
    let before = f.snapshot();
    f.render(f.area(), Modal::new(Panel::new("sure?").body("y / n")));
    let after = f.snapshot();
    let bottom = after.lines().iter().rposition(|l| l.contains('╰')).unwrap();
    let under = &after.lines()[bottom + 1];
    assert_eq!(
        tinsel::text::strip_ansi(under),
        tinsel::text::strip_ansi(&before.lines()[bottom + 1])
    );
    // the shadow's background is a light grey, not a dark block
    let bg = under
        .split("48;2;")
        .nth(1)
        .expect("a shadow behind the text")
        .split(['m', ';'])
        .take(3)
        .map(|c| c.parse::<u8>().unwrap())
        .collect::<Vec<_>>();
    assert!(bg.iter().all(|&c| c > 200), "{bg:?}");
    // and the rows that are only faded have none
    assert!(!after.lines()[0].contains("48;2;"));
}

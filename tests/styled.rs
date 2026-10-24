//! Checks that need real colour codes. Kept apart from render.rs because the colour mode is
//! process-wide and Cargo runs each test file as its own process.

use tinsel::{ColorMode, Line, Table, badge, palette, set_color_mode, text};

#[test]
fn table_lines_up_styled_cells() {
    // a styled cell must take up only its visible width
    set_color_mode(ColorMode::TrueColor);
    let table = Table::new(["app", "status"]).row(["tube".to_string(), badge("live", palette::ROSE)]);
    let widths: Vec<usize> = table.lines().lines().iter().map(|l| text::width(l)).collect();
    // "  " + 4 (tube) + 3 (gap) + 6 ("status" / " live ")
    assert_eq!(widths, [15, 15, 15]);
}

#[test]
fn line_measures_what_it_shows() {
    set_color_mode(ColorMode::TrueColor);
    let line = Line::new().push("● ", palette::ROSE).push("日本", palette::TEXT);
    assert_eq!(line.width(), 6);
}

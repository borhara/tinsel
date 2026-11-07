//! What a Region shows, checked with headless regions.

use std::sync::{Arc, Mutex};

use tinsel::{Block, ColorMode, Region, Table, set_color_mode};

#[test]
fn the_view_is_drawn_afresh_from_shared_state() {
    set_color_mode(ColorMode::None);
    let ready = Arc::new(Mutex::new(Vec::<&str>::new()));
    let shown = ready.clone();
    let region = Region::headless(move || {
        let mut table = Table::new(["service", "status"]);
        for name in shown.lock().unwrap().iter() {
            table = table.row([*name, "ready"]);
        }
        table.lines()
    });
    assert_eq!(region.snapshot().len(), 2);
    ready.lock().unwrap().extend(["api", "web"]);
    let lines = region.snapshot().plain();
    assert!(lines.contains("api") && lines.contains("web"), "{lines}");
    assert_eq!(region.snapshot().len(), 4);
}

#[test]
fn logged_lines_sit_above_the_view() {
    set_color_mode(ColorMode::None);
    let region = Region::headless(|| Block::from(vec!["  waiting on 2 services".to_string()])).keep();
    region.log("api is up");
    region.log("web is up");
    assert_eq!(
        region.snapshot().plain(),
        "api is up\nweb is up\n  waiting on 2 services"
    );
    region.finish();
}

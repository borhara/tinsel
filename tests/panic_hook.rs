//! The panic hook runs alongside one the program set first.

use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn a_hook_set_earlier_still_runs_once() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    std::panic::set_hook(Box::new(|_| {
        CALLS.fetch_add(1, Ordering::SeqCst);
    }));
    tinsel::install_panic_hook();
    tinsel::install_panic_hook();
    tinsel::set_color_mode(tinsel::ColorMode::None);
    let report = tinsel::capture(|| {
        let _ = std::panic::catch_unwind(|| panic!("out of tinsel"));
    });
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert!(report.contains("out of tinsel"), "{report}");
}

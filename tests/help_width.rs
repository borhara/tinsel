//! Help rows use the whole terminal. The width comes from `COLUMNS` here, which is
//! process-wide, so this lives in its own file.

use tinsel::{ColorMode, Help, set_color_mode};

#[test]
fn help_rows_only_wrap_when_the_terminal_is_too_narrow() {
    set_color_mode(ColorMode::None);
    let about = "what's deployed in each environment, from AWS, with the deployment CLI";
    let help = || {
        Help::new("tool", "does things")
            .command("status", about)
            .lines()
            .plain()
    };
    // SAFETY: the only test in this process
    unsafe { std::env::set_var("COLUMNS", "160") };
    assert!(help().contains(&format!("status  {about}")), "{}", help());
    unsafe { std::env::set_var("COLUMNS", "60") };
    assert!(!help().contains(about), "{}", help());
}

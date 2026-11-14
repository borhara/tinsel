//! Testing a program's own output, as someone using tinsel would: everything it prints,
//! caught with `capture`.

use tinsel::app::{self, App, Frame};
use tinsel::{ColorMode, Live, capture, palette, with_color_mode};

#[test]
fn a_whole_run_is_caught_steps_on_other_threads_included() {
    let printed = capture(|| {
        tinsel::info("starting");
        let live = Live::new("deploy", "my-app").no_history();
        let build = live.step("Build");
        let targets = [build.step("linux"), build.step("macos")];
        build.start();
        std::thread::scope(|s| {
            for t in &targets {
                s.spawn(move || {
                    t.start();
                    t.done();
                });
            }
        });
        build.done();
        live.finish();
        tinsel::spin("Counting", || ());
        tinsel::error("upload refused");
    });
    for want in [
        "  ›  starting\n",
        "[ok] linux",
        "[ok] macos",
        "[ok] Build",
        "[ok] Counting",
        "upload refused\n",
    ] {
        assert!(printed.contains(want), "{want:?} in:\n{printed}");
    }
    assert!(!printed.contains('\x1b'), "no colour or animation:\n{printed:?}");
}

#[test]
fn json_output_is_caught_as_json() {
    let printed = capture(|| {
        tinsel::set_output(tinsel::Output::Json);
        tinsel::success("deployed");
        tinsel::set_output(tinsel::Output::Pretty);
    });
    assert!(
        printed.contains(r#"{"event":"message","level":"success","text":"deployed"}"#),
        "{printed}"
    );
}

#[test]
fn colour_is_left_out_unless_chosen() {
    let plain = capture(|| tinsel::success("done"));
    let lit = with_color_mode(ColorMode::TrueColor, || capture(|| tinsel::success("done")));
    assert_eq!(plain, "  ●  done\n");
    assert!(lit.contains("\x1b[38;2;") && lit.contains("done"), "{lit:?}");
    // and the choice lasts only as long as the closure, even one that panics
    let _ = std::panic::catch_unwind(|| with_color_mode(ColorMode::TrueColor, || panic!("mid-way")));
    assert_eq!(
        with_color_mode(ColorMode::None, || tinsel::paint(palette::ROSE, "x")),
        "x"
    );
}

#[test]
fn an_app_prints_one_frame_instead_of_taking_the_screen() {
    struct Hello;
    impl App for Hello {
        fn view(&self, frame: &mut Frame) {
            frame.print(frame.area(), 0, "hello from the app");
        }
    }
    let printed = capture(|| {
        app::run(Hello).unwrap();
    });
    assert!(printed.starts_with("hello from the app"), "{printed:?}");
}

#[test]
fn a_panic_inside_still_ends_the_capture() {
    assert!(std::panic::catch_unwind(|| capture(|| panic!("in the middle"))).is_err());
    assert_eq!(capture(|| tinsel::success("after")), "  ●  after\n");
}

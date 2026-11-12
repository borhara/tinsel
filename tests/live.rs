//! What a Live display draws, checked with headless displays on a clock that only moves when
//! told. The time column sits at the right edge, which depends on the terminal's width, so
//! lines are compared with runs of spaces collapsed.

use std::time::Duration;

use tinsel::{ColorMode, Live, set_color_mode};

fn squashed(live: &Live) -> Vec<String> {
    live.snapshot()
        .plain()
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn steps_show_their_state_and_time() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("deploy", "my-app");
    let (build, upload, notify) = (live.step("Build"), live.step("Upload"), live.step("Notify"));
    build.start();
    live.advance(Duration::from_millis(1200));
    build.done();
    upload.start();
    live.advance(Duration::from_millis(300));
    upload.warn("retried once");
    assert_eq!(
        squashed(&live),
        [
            "",
            "deploy my-app",
            "",
            "● Build 1.2s",
            "▲ Upload 0.3s",
            "retried once",
            "○ Notify"
        ]
    );
    notify.start();
    notify.fail("webhook 500");
    assert_eq!(squashed(&live)[6..], ["✕ Notify 0.0s", "webhook 500"]);
}

#[test]
fn sub_steps_fold_away_once_everything_passes() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("release", "v2");
    let build = live.step("Build");
    let (linux, mac) = (build.step("linux"), build.step("macos"));
    build.start();
    linux.start();
    live.advance(Duration::from_secs(1));
    linux.done();
    // while the parent runs, its sub-steps show, indented
    let lines = live.snapshot().plain();
    assert!(lines.contains("\n        ●   linux"), "{lines}");
    assert!(lines.contains("\n        ○   macos"), "{lines}");

    mac.start();
    mac.done();
    build.done();
    assert_eq!(squashed(&live)[3..], ["● Build 1.0s"]);
}

#[test]
fn sub_steps_that_had_trouble_stay_visible() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("release", "v2");
    let build = live.step("Build");
    let mac = build.step("macos");
    build.start();
    mac.start();
    mac.warn("signing slow");
    build.done();
    assert_eq!(squashed(&live)[3..], ["● Build 0.0s", "▲ macos 0.0s", "signing slow"]);
}

#[test]
fn logs_sit_between_the_header_and_the_steps() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("build", "app");
    let compile = live.step("Compile");
    live.log("using cache");
    compile.log("2 warnings");
    assert_eq!(
        squashed(&live),
        ["", "build app", "", "using cache", "Compile 2 warnings", "○ Compile"]
    );
}

#[test]
fn timeline_places_steps_on_one_axis() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("release", "v1");
    let (a, b) = (live.step("Build"), live.step("Ship"));
    a.start();
    live.advance(Duration::from_secs(3));
    a.done();
    b.start();
    live.advance(Duration::from_secs(1));
    b.done();
    let lines: Vec<String> = live.timeline().plain().lines().map(String::from).collect();
    // Build fills the first three quarters of the track, Ship the last
    let build = lines[0].find('━').unwrap();
    let ship = lines[1].find('━').unwrap();
    assert!(ship > build + 20, "{lines:?}");
    assert!(lines[0].trim_end().ends_with("3.0s"));
    assert!(lines[3].trim_end().ends_with("4.0s"), "{lines:?}");
}

#[test]
fn retry_ends_on_a_warning_when_it_took_more_than_one_go() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("sync", "x");
    let mut tries = 0;
    let r: Result<u8, String> = live.step("Fetch").retry(3, || {
        tries += 1;
        if tries < 2 { Err("busy".into()) } else { Ok(7) }
    });
    assert_eq!(r, Ok(7));
    assert!(live.snapshot().plain().contains("succeeded on attempt 2"));
    let failed: Result<(), String> = live.step("Never").try_run(|| Err("nope".into()));
    assert!(failed.is_err());
    assert!(live.snapshot().plain().contains("✕   Never"));
}

#[cfg(unix)]
#[test]
fn commands_show_their_last_lines_when_they_fail() {
    use std::process::Command;
    set_color_mode(ColorMode::None);
    let live = Live::headless("ci", "x");
    let out = live.step("Echo").run(Command::new("sh").args(["-c", "echo hi"]));
    assert_eq!(out.unwrap(), "hi");
    let err = live
        .step("Fail")
        .command(Command::new("sh").args(["-c", "echo one; echo two; echo three; exit 4"]))
        .tail(2)
        .run()
        .unwrap_err();
    assert_eq!(err.status().and_then(|s| s.code()), Some(4));
    assert_eq!(err.output(), "one\ntwo\nthree");
    let snap = live.snapshot().plain();
    assert!(
        snap.contains("sh exited with status 4\n       two\n       three"),
        "{snap}"
    );
    assert!(!snap.contains("       one"));
}

#[test]
fn steps_can_be_skipped_cancelled_or_waiting() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("deploy", "my-app");
    let (lint, test, approve, ship) = (
        live.step("Lint"),
        live.step("Test"),
        live.step("Approve"),
        live.step("Ship"),
    );
    lint.skip("nothing changed");
    test.start();
    live.advance(Duration::from_millis(800));
    test.cancel("stopped by hand");
    approve.wait("waiting for approval");
    live.advance(Duration::from_secs(3));
    ship.cancel("");
    assert_eq!(
        squashed(&live)[3..],
        [
            "– Lint skipped",
            "nothing changed",
            "⊘ Test 0.8s",
            "stopped by hand",
            "◌ Approve 3.0s",
            "waiting for approval",
            "⊘ Ship cancelled",
        ]
    );
}

#[test]
fn a_waiting_step_can_start_once_someone_says_so() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("deploy", "my-app");
    let approve = live.step("Approve");
    approve.wait("waiting for approval");
    live.advance(Duration::from_secs(5));
    approve.start();
    live.advance(Duration::from_millis(400));
    approve.done();
    assert_eq!(squashed(&live)[3..], ["● Approve 0.4s"]);
}

#[test]
fn a_finished_step_can_run_again_with_a_fresh_timer() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("deploy", "my-app");
    let upload = live.step("Upload");
    upload.start();
    live.advance(Duration::from_secs(2));
    upload.fail("connection reset");
    upload.start();
    live.advance(Duration::from_millis(500));
    let running = squashed(&live);
    assert_eq!(running.len(), 4, "{running:?}");
    assert!(running[3].ends_with("Upload 0.5s"), "{running:?}");
    upload.done();
    assert_eq!(squashed(&live)[3..], ["● Upload 0.5s"]);
}

#[test]
fn skipped_sub_steps_fold_away_but_cancelled_ones_stay() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("release", "v2");
    let build = live.step("Build");
    let (api, docs) = (build.step("api"), build.step("docs"));
    build.start();
    api.start();
    api.done();
    docs.skip("");
    build.done();
    assert_eq!(squashed(&live)[3..], ["● Build 0.0s"]);

    let ship = live.step("Ship");
    let (web, worker) = (ship.step("web"), ship.step("worker"));
    ship.start();
    web.start();
    web.done();
    worker.cancel("");
    ship.done();
    assert_eq!(
        squashed(&live)[4..],
        ["● Ship 0.0s", "● web 0.0s", "⊘ worker cancelled"]
    );
}

#[test]
fn a_skipped_parent_folds_its_sub_steps() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("release", "v2");
    let docs = live.step("Docs");
    docs.step("reference");
    docs.skip("");
    assert_eq!(squashed(&live)[3..], ["– Docs skipped"]);
}

#[test]
fn progress_says_how_long_is_left_once_it_can_tell() {
    set_color_mode(ColorMode::None);
    let live = Live::headless("deploy", "my-app");
    let upload = live.step("Upload");
    upload.start();
    upload.progress(0.02, "1 / 20 MB");
    assert!(!live.snapshot().plain().contains("left"));
    live.advance(Duration::from_secs(4));
    upload.progress(0.25, "5 / 20 MB");
    let lines = squashed(&live);
    assert!(lines[4].ends_with(" 25% 5 / 20 MB · 12s left"), "{lines:?}");
}

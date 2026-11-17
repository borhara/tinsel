use super::*;

#[test]
fn estimates_never_look_finished_early() {
    assert!((estimated_fraction(21.0, 42.0) - 0.5).abs() < 1e-6);
    assert!(estimated_fraction(42.0, 42.0) < 0.95);
    assert!(estimated_fraction(1000.0, 42.0) < 0.991);
    // and keep creeping forward
    assert!(estimated_fraction(60.0, 42.0) > estimated_fraction(50.0, 42.0));
}

#[test]
fn history_keeps_one_time_per_step_and_only_for_steps_that_ran() {
    let live = Live::headless("deploy", "my-app");
    lock(&live.shared).history = Some(History::scratch());
    let (upload, lint, ship) = (live.step("Upload"), live.step("Lint"), live.step("Ship"));
    upload.start();
    live.advance(Duration::from_secs(2));
    upload.done();
    upload.start();
    live.advance(Duration::from_secs(3));
    upload.done();
    lint.skip("");
    ship.start();
    live.advance(Duration::from_secs(1));
    ship.cancel("");
    let mut s = lock(&live.shared);
    record_history(&mut s);
    let history = s.history.as_ref().unwrap();
    assert_eq!(history.runs("deploy › Upload"), [3.0]);
    assert!(history.runs("deploy › Lint").is_empty());
    assert!(history.runs("deploy › Ship").is_empty());
}

#[test]
fn only_a_terminal_gets_a_drawn_display() {
    use output::Output;
    assert_eq!(Mode::pick(Output::Pretty, true), Mode::Draw);
    assert_eq!(Mode::pick(Output::Pretty, false), Mode::Plain);
    assert_eq!(Mode::pick(Output::Json, true), Mode::Json);
    assert_eq!(Mode::pick(Output::Quiet, true), Mode::Quiet);
}

#[test]
fn approximations_round_up_to_whole_seconds() {
    assert_eq!(approx(41.2), "42s");
    assert_eq!(approx(0.3), "1s");
    assert_eq!(approx(187.0), "3m07s");
}

use crate::output::{Output, capture, with_output, with_terminal};

fn plain() {
    crate::set_color_mode(crate::ColorMode::None);
}

/// Every way a step can end, on one display.
fn run_every_state(live: &Live) {
    let (a, b, c, d, e, f, g) = (
        live.step("s-done"),
        live.step("s-warn"),
        live.step("s-fail"),
        live.step("s-skip"),
        live.step("s-cancel"),
        live.step("s-wait"),
        live.step("s-sub-parent"),
    );
    let sub = g.step("s-sub");
    a.start();
    a.log("step-log");
    a.done();
    b.start();
    b.warn("careful");
    c.start();
    c.fail("broke\nsecond line");
    d.skip("not needed");
    e.cancel("stopped");
    f.wait("approval");
    g.start();
    sub.start();
    sub.done();
    g.done();
    live.log("live-log");
}

#[test]
fn draws_on_a_terminal_and_leaves_the_last_frame() {
    plain();
    let out = capture(|| {
        with_terminal(true, || {
            let live = Live::new("d-title", "target")
                .no_history()
                .no_focus()
                .fps(240)
                .spinner(Spinner::Dots);
            run_every_state(&live);
            std::thread::sleep(Duration::from_millis(30));
            live.finish_with_timeline();
            let live = Live::new("d-card", "target")
                .remember_as("tinsel-unit-test")
                .no_history()
                .no_focus();
            live.step("one").done();
            live.finish_with_card(Card::success("card-line"));
        })
    });
    assert!(out.starts_with(HIDE_CURSOR) && out.contains(SHOW_CURSOR), "{out:?}");
    for want in [
        "d-title",
        "s-done",
        "careful",
        "second line",
        "live-log",
        "step-log",
        "0s",
        "card-line",
    ] {
        assert!(out.contains(want), "{want} in {out:?}");
    }
}

#[test]
fn line_breaks_in_steps_are_counted_as_rows() {
    plain();
    let live = Live::headless("rows", "x");
    let (a, b, c) = (live.step("two\nlines"), live.step("b"), live.step("c"));
    a.start();
    a.detail("first\nsecond\r\nthird");
    b.start();
    b.progress(0.5, "half\nway");
    c.fail("broke\nbadly");
    let lines = render(&lock(&live.shared), Instant::now(), 60, true);
    assert!(lines.iter().all(|l| !l.contains(['\n', '\r'])), "{lines:?}");
    let plain: Vec<String> = lines.iter().map(|l| crate::text::strip_ansi(l)).collect();
    assert!(plain[0].contains("two lines"), "{plain:?}");
    assert_eq!(
        plain[1..4].iter().map(|l| l.trim()).collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    assert!(plain.iter().any(|l| l.contains("half way")), "{plain:?}");
    // a drawn frame goes back up exactly as many rows as it took
    let mut s = lock(&live.shared);
    s.mode = Mode::Draw;
    let out = capture(|| paint_frame(&mut s, false));
    let rows = out.split("\x1b[?7l").nth(1).unwrap().matches('\n').count();
    assert_eq!(rows, s.drawn, "{out:?}");
}

#[test]
fn a_tall_display_redraws_only_what_fits() {
    plain();
    let out = capture(|| {
        with_terminal(true, || {
            let live = Live::new("tall", "x").no_history().no_focus().fps(240);
            let steps: Vec<Step> = (0..crate::term::height() + 5)
                .map(|i| live.step(format!("t{i}")))
                .collect();
            for s in &steps {
                s.start();
            }
            std::thread::sleep(Duration::from_millis(40));
            live.finish();
        })
    });
    assert!(out.contains(&format!("t{}", crate::term::height() + 4)));
}

#[test]
fn off_a_terminal_each_step_is_a_line() {
    plain();
    let out = capture(|| {
        with_terminal(false, || {
            let live = Live::new("p-title", "target");
            run_every_state(&live);
            live.finish_with_card(Card::success("p-card"));
        })
    });
    for want in [
        "p-title target\n",
        "  [ok] s-done (",
        "  [warn] s-warn (0ms) - careful\n",
        "  [FAIL] s-fail (0ms) - broke\n      | second line\n",
        "  [skip] s-skip - not needed\n",
        "  [cancelled] s-cancel - stopped\n",
        "  [wait] s-wait - approval\n",
        "    [ok] s-sub (",
        "    s-done: step-log\n",
        "    live-log\n",
        "p-card",
    ] {
        assert!(out.contains(want), "{want:?} in {out:?}");
    }
}

#[test]
fn as_json_every_change_is_an_event() {
    let out = capture(|| {
        with_output(Output::Json, || {
            let live = Live::new("j-title", "target");
            run_every_state(&live);
            live.finish_with_card(Card::success("j-card"));
        })
    });
    assert!(
        out.contains(r#"{"event":"live","title":"j-title","target":"target"}"#),
        "{out}"
    );
    for state in ["running", "done", "warn", "fail", "skipped", "cancelled", "waiting"] {
        assert!(out.contains(&format!(r#""state":"{state}""#)), "{state} in {out}");
    }
    assert!(out.contains(r#""path":["s-sub-parent","s-sub"]"#), "{out}");
    assert!(
        out.contains(r#"{"event":"log","title":"j-title","step":"s-done","text":"step-log"}"#),
        "{out}"
    );
    assert!(out.contains(r#""step":"","text":"live-log""#), "{out}");
    assert!(
        out.contains(r#"{"event":"finish","title":"j-title","outcome":"fail","#),
        "{out}"
    );
}

#[test]
fn quietly_only_failures_and_waits_print() {
    let out = capture(|| {
        with_output(Output::Quiet, || {
            let live = Live::new("q-title", "target");
            run_every_state(&live);
        })
    });
    assert!(!out.contains("q-title") && !out.contains("s-done"), "{out}");
}

#[test]
fn a_question_holds_every_display_still_until_it_is_answered() {
    plain();
    capture(|| {
        with_terminal(true, || {
            let live = Live::new("q-title", "x").no_history().no_focus().fps(240);
            live.step("q-waiting").wait("waiting for you");
            std::thread::sleep(Duration::from_millis(30));
            let paused = display::pause_all();
            {
                let s = lock(&live.shared);
                // its last frame left in place, and nothing reading stdin
                assert_eq!((s.paused, s.drawn), (1, 0));
                assert!(s.watch.is_none() && s.focus.is_none());
            }
            std::thread::sleep(Duration::from_millis(30));
            assert_eq!(lock(&live.shared).drawn, 0, "drew over the question");
            drop(paused);
            std::thread::sleep(Duration::from_millis(30));
            {
                let s = lock(&live.shared);
                assert_eq!(s.paused, 0);
                assert!(s.drawn > 0, "never drew again after the question");
                assert!(!s.refocus, "the watch was never asked to start again");
            }
            live.finish();
        })
    });
}

#[test]
fn a_panic_marks_running_steps_and_stops_drawing() {
    plain();
    capture(|| {
        let make = || with_terminal(true, || Live::new("a-title", "x").no_history().no_focus());
        let (busy, poisoned) = (make(), make());
        busy.step("a-running").start();
        let state = poisoned.shared.clone();
        let _ = std::thread::spawn(move || {
            let _held = state.lock().unwrap();
            panic!("poison the display");
        })
        .join();
        let held = busy.shared.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _held = held.lock().unwrap();
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(10));
        });
        rx.recv().unwrap();
        assert!(abandon_all());
        holder.join().unwrap();
        let s = lock(&busy.shared);
        assert!(s.abandoned && s.steps[0].state == State::Fail, "{}", s.steps[0].note);
        drop(s);
        assert!(!abandon_all());
    });
}

#[test]
fn a_drawn_run_remembers_its_timings() {
    let dir = std::env::temp_dir().join(format!("tinsel-live-{}", std::process::id()));
    capture(|| {
        let live = with_terminal(true, || Live::new("h-title", "x").no_focus());
        let build = live.step("Build");
        // after the first step, which opens the program's own history
        lock(&live.shared).history = History::open_in(&dir, "app");
        build.done();
        live.finish();
    });
    assert!(
        std::fs::read_to_string(dir.join("app.tsv"))
            .unwrap()
            .starts_with("h-title › Build\t")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn nothing_starts_until_the_first_step_so_every_setting_holds_from_the_start() {
    let printed = capture(|| {
        let live = Live::new("deploy", "my-app").remember_as("elsewhere").no_history();
        assert!(!live.started.is_completed());
        assert_eq!(live.history, None);
        live.step("Build").done();
        assert!(live.started.is_completed());
    });
    assert!(printed.starts_with("deploy my-app\n"), "{printed:?}");
    // finished with no steps, the header still shows
    assert_eq!(capture(|| Live::new("empty", "x").finish()), "empty x\n");
}

#[test]
fn focus_is_only_watched_when_wanted() {
    let live = Live::headless("f", "x");
    watch_focus(&live.shared);
    lock(&live.shared).wants_focus = true;
    // not drawing on a terminal, even when the tests run on one
    with_terminal(false, || watch_focus(&live.shared));
    assert!(lock(&live.shared).watch.is_none(), "no terminal to watch");
}

#[test]
fn the_summary_says_how_it_went() {
    let live = Live::headless("s", "x");
    let (a, b) = (live.step("a"), live.step("b"));
    let summary = || summary(&lock(&live.shared));
    assert!(summary().starts_with("● finished in"));
    a.warn("");
    b.warn("");
    assert!(summary().ends_with("with 2 warnings"), "{}", summary());
    b.cancel("");
    assert!(summary().starts_with("⊘ b cancelled after"));
    a.fail("");
    assert!(summary().starts_with("✕ a failed after"));
    assert_eq!(outcome(&lock(&live.shared)), "fail");
}

#[test]
fn retries_wait_longer_each_time_and_say_how_many_it_took() {
    assert_eq!(backoff(1), Duration::from_millis(500));
    assert_eq!(backoff(9), Duration::from_secs(8));
    assert_eq!(attempts_note("nope", 0), "nope");
    assert_eq!(attempts_note("nope\nmore", 2), "nope (after 3 attempts)\nmore");
    assert_eq!(fmt_secs(75.0), "1m15s");
    // the same as everywhere else, and never a panic
    assert_eq!(fmt_secs(0.3), format::duration(Duration::from_millis(300)));
    assert_eq!(fmt_secs(5000.0), format::duration(Duration::from_secs(5000)));
    assert_eq!(fmt_secs(-1.0), format::duration(Duration::ZERO));
}

#[test]
fn every_state_has_a_name() {
    let names = [State::Pending, State::Active, State::Waiting].map(State::name);
    assert_eq!(names, ["pending", "running", "waiting"]);
}

#[test]
fn options_that_change_nothing_on_screen_still_take() {
    let live = Live::headless("o", "x")
        .bar(BarStyle::Waveform)
        .notify_after(Duration::from_secs(1))
        .remember_as("ignored when headless");
    assert_eq!(live.notify_after, Some(Duration::from_secs(1)));
    assert!(live.no_notify().notify_after.is_none());
    let real = with_output(Output::Quiet, || Live::new("o", "x"));
    real.advance(Duration::from_secs(60));
    assert!(real.elapsed() < Duration::from_secs(60));
}

#[test]
fn the_outcome_is_the_worst_of_the_steps() {
    let live = Live::headless("o", "x");
    let (a, b, c) = (live.step("a"), live.step("b"), live.step("c"));
    let outcome = || outcome(&lock(&live.shared));
    assert_eq!(outcome(), "ok");
    a.warn("");
    assert_eq!(outcome(), "warn");
    b.cancel("");
    assert_eq!(outcome(), "cancelled");
    c.fail("");
    assert_eq!(outcome(), "fail");
}

#[test]
fn closures_run_as_steps() {
    let live = Live::headless("r", "x");
    assert_eq!(live.step("ok").try_run(|| Ok::<_, String>(1)), Ok(1));
    let started = live.step("started");
    started.start();
    assert_eq!(started.try_run(|| Err::<(), _>("late")), Err("late"));
    let mut tries = 0;
    let flaky = live.step("flaky").retry(1, || {
        tries += 1;
        if tries < 2 { Err("not yet") } else { Ok(tries) }
    });
    assert_eq!(flaky, Ok(2));
    let gave_up: Result<(), _> = live.step("gave up").retry(0, || Err("never"));
    assert_eq!(gave_up, Err("never"));
    let s = lock(&live.shared);
    let states: Vec<State> = s.steps.iter().map(|st| st.state).collect();
    assert_eq!(states, [State::Done, State::Fail, State::Warn, State::Fail]);
    assert_eq!(s.steps[2].note, "succeeded on attempt 2");
}

#[test]
fn a_command_that_isnt_there_isnt_retried() {
    let live = Live::headless("c", "x");
    let err = live
        .step("missing")
        .command(&mut Command::new("/nonexistent/tinsel-command"))
        .retries(3)
        .run()
        .unwrap_err();
    assert!(
        err.to_string()
            .starts_with("couldn't run /nonexistent/tinsel-command: "),
        "{err}"
    );
    assert!(std::error::Error::source(&err).is_some());
    assert!(err.status().is_none() && !err.timed_out() && err.output().is_empty());
}

#[cfg(unix)]
#[test]
fn commands_time_out_and_are_tried_again() {
    let live = Live::headless("c", "x");
    let step = live.step("slow");
    let err = step
        .command(Command::new("sleep").arg("5"))
        .timeout(Duration::from_millis(50))
        .retries(1)
        .run()
        .unwrap_err();
    assert!(err.timed_out() && err.status().is_none(), "{err}");
    assert_eq!(err.to_string(), "sleep timed out after 50ms");
    assert!(std::error::Error::source(&err).is_none());
    assert!(lock(&live.shared).steps[0].note.ends_with("(after 2 attempts)"));
}

#[cfg(unix)]
#[test]
fn commands_say_how_they_ended() {
    let live = Live::headless("c", "x");
    let code = live
        .step("code")
        .run(Command::new("sh").args(["-c", "exit 3"]))
        .unwrap_err();
    assert_eq!(
        (code.to_string().as_str(), code.status().and_then(|s| s.code())),
        ("sh exited with status 3", Some(3))
    );
    let killed = live
        .step("killed")
        .run(Command::new("sh").args(["-c", "kill -9 $$"]))
        .unwrap_err();
    assert_eq!(killed.to_string(), "sh was stopped by a signal");
    // a job left running in the background keeps the output open; it's let go soon after
    let started = Instant::now();
    let bg = live
        .step("bg")
        .run(Command::new("sh").args(["-c", "sleep 2 & echo started"]));
    assert_eq!(bg.unwrap(), "started");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn estimates_say_how_long_is_usual() {
    plain();
    let live = Live::headless("e", "x");
    let mut history = History::scratch();
    for secs in [10.0, 10.0] {
        history.record("e › build", secs);
    }
    lock(&live.shared).history = Some(history);
    let build = live.step("build");
    build.start();
    build.detail("compiling");
    let shown = |live: &Live, by: u64| {
        live.advance(Duration::from_secs(by));
        live.snapshot().plain()
    };
    let early = shown(&live, 4);
    assert!(
        early.contains("usually 10s · 6s left") && early.contains("compiling"),
        "{early}"
    );
    assert!(shown(&live, 7).contains("usually 10s · nearly there"));
    assert!(shown(&live, 5).contains("slower than usual · usually 10s"));
}

#[test]
fn steps_that_finished_while_you_were_away_twinkle_when_you_come_back() {
    let live = Live::headless("t", "x");
    let focus = Arc::new(Focus::default());
    lock(&live.shared).focus = Some(focus.clone());
    let t0 = lock(&live.shared).t0;
    let before = live.step("before leaving");
    before.done();
    live.advance(Duration::from_millis(10));
    focus.focus_changed(false, t0 + Duration::from_millis(10));
    let step = live.step("while away");
    step.start();
    live.advance(Duration::from_secs(5));
    step.done();
    let back = t0 + Duration::from_secs(6);
    focus.focus_changed(true, back);
    live.advance(Duration::from_millis(1100));
    let s = lock(&live.shared);
    let twinkling = render(&s, back + Duration::from_millis(100), 60, false).join("\n");
    let settled = render(&s, back + Duration::from_millis(100), 60, true).join("\n");
    assert_ne!(twinkling, settled);
}

#[test]
fn the_timeline_colours_every_state() {
    plain();
    let live = Live::headless("tl", "x");
    let (a, b, c) = (live.step("cancelled"), live.step("waiting"), live.step("running"));
    a.start();
    live.advance(Duration::from_secs(1));
    a.cancel("");
    b.start();
    b.wait("someone");
    c.start();
    live.advance(Duration::from_secs(1));
    let timeline = live.timeline().plain();
    assert!(timeline.matches('━').count() > 2, "{timeline}");
}

#[test]
fn sub_steps_of_sub_steps_fold_away_too() {
    plain();
    let live = Live::headless("n", "x");
    let (top, middle) = (live.step("top"), live.step("other"));
    let child = top.step("child");
    let grandchild = child.step("grandchild");
    for s in [&grandchild, &child, &top, &middle] {
        s.done();
    }
    assert!(!live.snapshot().plain().contains("grandchild"));
}

#[test]
fn progress_shows_how_far_and_how_long_is_left() {
    plain();
    let live = Live::headless("p", "x");
    let (step, gone) = (live.step("upload"), live.step("gone"));
    step.start();
    step.progress(0.01, "just started");
    assert!(!live.snapshot().plain().contains("left"), "too soon to say");
    live.advance(Duration::from_secs(10));
    step.progress(0.5, "17 / 34 MB");
    gone.start();
    live.advance(Duration::from_secs(2));
    gone.cancel("");
    let shown = live.snapshot().plain();
    assert!(shown.contains(" 50%  17 / 34 MB") && shown.contains("left"), "{shown}");
    assert!(shown.contains("2.0s"), "{shown}");
}

#[test]
fn progress_that_isnt_a_number_counts_as_none() {
    plain();
    let live = Live::headless("p", "x");
    let step = live.step("upload");
    step.start();
    step.progress(f32::NAN, "0 / 0");
    let shown = live.snapshot().plain();
    assert!(shown.contains("  0%  0 / 0") && !shown.contains("NaN"), "{shown}");
}

#[cfg(unix)]
#[test]
fn a_failed_command_shows_its_last_lines() {
    let live = Live::headless("c", "x");
    let err = live
        .step("noisy")
        .command(Command::new("sh").args(["-c", "echo one; echo two; exit 1"]))
        .tail(1)
        .run();
    assert!(err.is_err());
    assert_eq!(lock(&live.shared).steps[0].note, "sh exited with status 1\ntwo");
}

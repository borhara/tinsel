//! Tests that need a process of their own: a real terminal, a real pipe, or settings that
//! are process-wide. Each scenario runs the test binary again as a child process (on a
//! pseudo-terminal that the test types into and reads from, with its stdin piped in, or just
//! on its own) and the child does what a program using tinsel would. They live in the unit
//! tests, rather than under `tests/`, so their coverage counts with everything else. The
//! ones on a pseudo-terminal are Unix only; the rest run everywhere.

#[cfg(unix)]
use std::ffi::{CStr, c_char, c_ulong};
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::io::Read;
use std::io::Write;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::process::Child;
use std::process::{Command, ExitStatus, Stdio};
#[cfg(unix)]
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::thread::{self, JoinHandle};
#[cfg(unix)]
use std::time::{Duration, Instant};

const SCENARIO: &str = "TINSEL_TEST_SCENARIO";
/// This file's path among the unit tests, for running one of its tests on its own.
const SCENARIO_TEST: &str = "processes::scenario";

/// The child's half: runs the scenario the parent asked for, and does nothing when this test
/// is run any other way.
#[test]
#[ignore = "run by the other tests in this file, in a child process"]
fn scenario() {
    let Ok(name) = std::env::var(SCENARIO) else { return };
    let scenarios: &[(&str, fn())] = &[
        #[cfg(unix)]
        ("terminal", child::terminal),
        #[cfg(unix)]
        ("silent", child::silent),
        #[cfg(unix)]
        ("interrupted", child::interrupted),
        #[cfg(unix)]
        ("interrupted plainly", child::interrupted_plainly),
        #[cfg(unix)]
        ("children-ignored", child::children_ignored),
        ("piped", child::piped),
        ("settings", child::settings),
        ("panics", child::panics),
        ("ctrl-c", child::ctrl_c),
    ];
    let (_, run) = scenarios
        .iter()
        .find(|(n, _)| *n == name)
        .expect("a scenario by that name");
    run();
}

/// tinsel leaves on Ctrl-C with `_exit`, which skips everything registered to run at exit,
/// including writing coverage counters. In the unit tests alone, `_exit` leaves through
/// `exit` instead, so a run measured for coverage counts the handler too. The status is the
/// same either way.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub extern "C" fn _exit(code: i32) -> ! {
    std::process::exit(code)
}

// MARK: - The parent's half

#[cfg(unix)]
unsafe extern "C" {
    fn posix_openpt(flags: i32) -> i32;
    fn grantpt(fd: i32) -> i32;
    fn unlockpt(fd: i32) -> i32;
    fn ptsname(fd: i32) -> *const c_char;
    fn ioctl(fd: i32, request: c_ulong, ...) -> i32;
    fn setsid() -> i32;
}

#[cfg(target_os = "macos")]
mod consts {
    pub const O_NOCTTY: i32 = 0x20000;
    pub const TIOCSWINSZ: std::ffi::c_ulong = 0x8008_7467;
    pub const TIOCSCTTY: std::ffi::c_ulong = 0x2000_7461;
}

#[cfg(all(unix, not(target_os = "macos")))]
mod consts {
    pub const O_NOCTTY: i32 = 0o400;
    pub const TIOCSWINSZ: std::ffi::c_ulong = 0x5414;
    pub const TIOCSCTTY: std::ffi::c_ulong = 0x540E;
}

#[cfg(unix)]
#[repr(C)]
struct WinSize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

/// A child running a scenario on a pseudo-terminal of `cols`×`rows`, with everything it
/// draws collected as it arrives. Asked for its background colour, it says white, if it
/// `answers`.
#[cfg(unix)]
struct Terminal {
    master: File,
    seen: Arc<Mutex<Vec<u8>>>,
    child: Child,
    reader: JoinHandle<()>,
}

#[cfg(unix)]
impl Terminal {
    fn start(scenario: &str, cols: u16, rows: u16, answers: bool, env: &[(&str, &str)]) -> Terminal {
        // SAFETY: plain calls on a file descriptor this function owns
        let (master, slave) = unsafe {
            let fd = posix_openpt(2 | consts::O_NOCTTY);
            assert!(fd >= 0, "no pseudo-terminal");
            assert_eq!(grantpt(fd), 0);
            assert_eq!(unlockpt(fd), 0);
            let name = CStr::from_ptr(ptsname(fd)).to_str().unwrap().to_string();
            (File::from_raw_fd(fd), name)
        };
        let slave = OpenOptions::new().read(true).write(true).open(slave).unwrap();
        let size = WinSize {
            rows,
            cols,
            xpixel: 0,
            ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ reads one winsize struct through the pointer
        assert_eq!(
            unsafe { ioctl(slave.as_raw_fd(), consts::TIOCSWINSZ, &size as *const WinSize) },
            0
        );
        let mut command = child_command(scenario, env);
        command
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave);
        let child = command.spawn().unwrap();
        drop(command);

        let seen = Arc::new(Mutex::new(Vec::new()));
        let (mut from, mut to, log) = (master.try_clone().unwrap(), master.try_clone().unwrap(), seen.clone());
        let reader = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut answered = 0;
            // ends once the child and everything it started have closed the terminal
            while let Ok(n @ 1..) = from.read(&mut buf) {
                let mut seen = log.lock().unwrap();
                seen.extend_from_slice(&buf[..n]);
                let asked = String::from_utf8_lossy(&seen).matches("\x1b]11;?").count();
                for _ in answered..asked {
                    if !answers {
                        break;
                    }
                    let _ = to.write_all(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\");
                }
                answered = asked;
            }
        });
        Terminal {
            master,
            seen,
            child,
            reader,
        }
    }

    fn seen(&self) -> String {
        String::from_utf8_lossy(&self.seen.lock().unwrap()).into_owned()
    }

    /// Wait until the child has drawn `text`, then a moment more for it to settle into
    /// reading keys.
    fn wait_for(&self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.seen().contains(text) {
            assert!(Instant::now() < deadline, "never saw {text:?} in:\n{}", self.seen());
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(150));
    }

    fn type_in(&mut self, keys: &str) {
        self.master.write_all(keys.as_bytes()).unwrap();
        self.master.flush().unwrap();
    }

    fn finish(mut self) -> (ExitStatus, String) {
        let status = self.child.wait().unwrap();
        drop(self.master);
        let _ = self.reader.join();
        let seen = String::from_utf8_lossy(&self.seen.lock().unwrap()).into_owned();
        (status, seen)
    }
}

/// This test binary again, running only the scenario, without capturing its output.
fn child_command(scenario: &str, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([SCENARIO_TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(SCENARIO, scenario)
        .env_remove("NO_COLOR")
        .env_remove("TINSEL_THEME")
        .env_remove("COLORFGBG")
        .env_remove("TINSEL_NO_FOCUS")
        .env_remove("TINSEL_OUTPUT")
        .env_remove("TINSEL_TARGET")
        .env_remove("PAGER")
        .env_remove("LESS")
        .env_remove("TMUX")
        .env_remove("TINSEL_ACCENT")
        .env_remove("TINSEL_FPS")
        .env_remove("TINSEL_NO_HISTORY")
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .env_remove("RUST_BACKTRACE")
        .envs(env.iter().copied());
    command
}

fn assert_passed(status: ExitStatus, seen: &str) {
    assert!(status.success(), "the child failed ({status}):\n{seen}");
    assert!(seen.contains("test result: ok. 1 passed"), "{seen}");
}

#[cfg(unix)]
#[test]
fn on_a_terminal() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = std::env::temp_dir().join(format!("tinsel-processes-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let editor = scratch.join("editor.sh").display().to_string();
    std::fs::write(&editor, "#!/bin/sh\necho edited >> \"$1\"\n").unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = [
        ("VISUAL", editor.as_str()),
        ("COLORTERM", "truecolor"),
        ("TERM", "xterm-256color"),
        ("TERM_PROGRAM", "WezTerm"),
        ("RUST_BACKTRACE", "1"),
        ("XDG_CACHE_HOME", scratch.to_str().unwrap()),
    ];
    let mut t = Terminal::start("terminal", 100, 30, true, &env);

    // keys read one at a time, skipping a click, then a lone Esc, then nothing
    t.wait_for("[keys]");
    t.type_in("\x1b[<0;5;5Mx");
    thread::sleep(Duration::from_millis(300));
    t.type_in("\x1b");

    // a question answered at the keyboard
    t.wait_for("Ship it?");
    t.type_in("y");

    // a live display, looked away from and typed at
    t.wait_for("\x1b[?1004h");
    t.type_in("\x1b[Oab\x1b");
    t.wait_for("[focus off]");

    // a page too long for the screen, read and closed
    t.wait_for("page-line-0");
    t.type_in("q");

    // a full-screen app that hands the terminal back for a moment, then quits
    t.wait_for("app-ready");
    t.type_in("s");
    t.wait_for("app-back");
    t.type_in("q");

    // and one that panics: a worker thread first, which it survives, then its own
    t.wait_for("app-ready");
    t.type_in("w");
    t.wait_for("worker gave up");
    t.type_in("p");

    let (status, seen) = t.finish();
    std::fs::remove_dir_all(&scratch).unwrap();
    assert_passed(status, &seen);
    for want in ["\x1b]9;", "page-line-28", "panicked", "interrupted by a panic"] {
        assert!(seen.contains(want), "{want:?} in:\n{seen}");
    }
}

#[cfg(unix)]
#[test]
fn on_a_terminal_that_never_says_its_colour() {
    let t = Terminal::start("silent", 80, 24, false, &[("COLORTERM", "")]);
    let (status, seen) = t.finish();
    assert_passed(status, &seen);
}

#[cfg(unix)]
#[test]
fn ctrl_c_puts_the_terminal_back_and_says_why() {
    let mut t = Terminal::start("interrupted", 80, 24, true, &[]);
    t.wait_for("[running]");
    t.type_in("\x03");
    let (status, seen) = t.finish();
    assert_eq!(status.code(), Some(130), "{seen}");
    assert!(
        seen.contains("\x1b[?25h") && seen.contains("stopped by the test"),
        "{seen}"
    );
}

#[cfg(unix)]
#[test]
fn ctrl_c_with_nothing_to_say_and_no_terminal_settings_saved() {
    let mut t = Terminal::start("interrupted plainly", 80, 24, true, &[("TINSEL_NO_FOCUS", "1")]);
    t.wait_for("[running]");
    t.type_in("\x03");
    let (status, seen) = t.finish();
    assert_eq!(status.code(), Some(130), "{seen}");
}

#[cfg(unix)]
#[test]
fn a_command_can_vanish_before_its_status_is_read() {
    let out = child_command("children-ignored", &[]).output().unwrap();
    let seen = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_passed(out.status, &seen);
}

#[test]
fn ctrl_c_puts_things_back_and_says_why_without_a_terminal_too() {
    let out = child_command("ctrl-c", &[]).output().unwrap();
    let seen = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(130), "{seen}");
    assert!(
        seen.contains("\x1b[?25h") && seen.contains("stopped by the test"),
        "{seen}"
    );
}

#[test]
fn settings_are_decided_once_and_can_be_changed() {
    let out = child_command("settings", &[]).output().unwrap();
    let seen = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_passed(out.status, &seen);
    assert!(seen.contains("drawn on stderr"), "{seen}");
}

#[test]
fn panics_are_reported_whatever_they_carry() {
    let out = child_command("panics", &[]).output().unwrap();
    let seen = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_passed(out.status, &seen);
    for want in [
        "panicked in thread 'worker'",
        "panicked with a value that isn't text",
        "RUST_BACKTRACE=1",
    ] {
        assert!(seen.contains(want), "{want:?} in:\n{seen}");
    }
}

#[test]
fn with_answers_piped_in() {
    let mut child = child_command("piped", &[])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let answers = [
        "y",
        "yep",
        "",
        "maybe",
        "",
        "",
        "bad",
        "pw",
        "nope",
        "12",
        "abc",
        "",
        "2026-01-02",
        "soon",
    ];
    let mut stdin = child.stdin.take().unwrap();
    for answer in answers {
        writeln!(stdin, "{answer}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let seen = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_passed(out.status, &seen);
}

// MARK: - The child's half

mod child {
    use std::time::Duration;

    #[cfg(unix)]
    use crate::Block;
    #[cfg(unix)]
    use crate::app::{self, App, Event, Flow, Frame, Key, Keys};
    use crate::prompt::{self, Confirm, Error, Input};

    /// Make the terminal on stdin this process's own, as a shell would, so Ctrl-C typed there
    /// interrupts it and keys can be handed back to it.
    #[cfg(unix)]
    fn own_terminal() {
        // SAFETY: plain calls; the child isn't a process group leader, so setsid succeeds
        unsafe {
            assert!(super::setsid() > 0);
            assert_eq!(super::ioctl(0, super::consts::TIOCSCTTY, 0), 0);
        }
    }

    /// Something the parent waits to see before it types.
    #[cfg(unix)]
    fn marker(text: &str) {
        eprintln!("{text}");
    }

    #[cfg(unix)]
    pub fn terminal() {
        own_terminal();
        // what the terminal is: its size, colour and background, found by asking it
        assert_eq!((crate::width(), crate::content_width()), (100, 76));
        assert_eq!(crate::color_mode(), crate::ColorMode::TrueColor);
        assert_eq!(crate::theme(), crate::Theme::Light);
        crate::set_theme(crate::Theme::Dark);

        let mut keys = Keys::open().unwrap();
        marker("[keys]");
        assert_eq!(keys.next(Duration::from_secs(10)).unwrap(), Some(Key::Char('x')));
        assert_eq!(keys.next(Duration::from_secs(10)).unwrap(), Some(Key::Esc));
        assert_eq!(keys.next(Duration::from_millis(200)).unwrap(), None);
        drop(keys);

        assert!(prompt::confirm("Ship it?", false).unwrap());
        assert_eq!(prompt::editor("Notes", "start\n").unwrap(), "start\nedited\n");

        // a display that watches for focus, finishing unseen, so it says so
        let live = crate::Live::new("focus", "watched")
            .notify_after(Duration::ZERO)
            .no_history();
        let step = live.step("Build");
        step.start();
        std::thread::sleep(Duration::from_millis(1500));
        marker("[focus off]");
        step.done();
        live.finish();
        // what was typed meanwhile was handed back: read it, so it doesn't reach the pager
        let mut keys = Keys::open().unwrap();
        let mut typed = Vec::new();
        while let Some(key) = keys.next(Duration::from_millis(400)).unwrap() {
            typed.push(key);
        }
        drop(keys);
        // and one told to stop watching after it started
        let live = crate::Live::new("unwatched", "x").no_history();
        std::thread::sleep(Duration::from_millis(300));
        live.no_focus().finish();

        // a panic in the middle of drawing
        crate::install_panic_hook();
        let live = crate::Live::new("panicking", "x").no_history().no_focus();
        live.step("Deploy").start();
        assert!(std::panic::catch_unwind(|| panic!("on purpose")).is_err());
        drop(live);
        let _ = std::panic::take_hook();

        // Ctrl-C left to the program while something draws
        crate::set_ctrl_c(crate::CtrlC::Leave);
        crate::spin("left alone", || std::thread::sleep(Duration::from_millis(50)));
        crate::set_ctrl_c(crate::CtrlC::Exit);

        let page: Block = (0..60).map(|i| format!("page-line-{i}")).collect();
        page.page();

        app::run(Suspender::default()).unwrap();
        assert!(std::panic::catch_unwind(|| panic!("after the app")).is_err());
        let _ = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| eprintln!("{info}")));
        assert!(std::panic::catch_unwind(|| app::run(Suspender::default())).is_err());
    }

    #[cfg(unix)]
    pub fn silent() {
        assert_eq!(crate::color_mode(), crate::ColorMode::Ansi256);
        assert_eq!(crate::theme(), crate::Theme::Dark);
    }

    #[cfg(unix)]
    pub fn interrupted() {
        own_terminal();
        crate::set_ctrl_c(crate::CtrlC::ExitWith("stopped by the test".into()));
        let live = crate::Live::new("forever", "x").no_history();
        live.step("Wait").start();
        std::thread::sleep(Duration::from_millis(500));
        marker("[running]");
        // Ctrl-C ends it; if it never comes, finishing normally fails the test
        std::thread::sleep(Duration::from_secs(20));
    }

    #[cfg(unix)]
    pub fn interrupted_plainly() {
        own_terminal();
        let live = crate::Live::new("forever", "x").no_history();
        live.step("Wait").start();
        marker("[running]");
        // Ctrl-C ends it; if it never comes, finishing normally fails the test
        std::thread::sleep(Duration::from_secs(20));
    }

    #[cfg(unix)]
    pub fn children_ignored() {
        unsafe extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        const SIGCHLD: i32 = if cfg!(target_os = "macos") { 20 } else { 17 };
        // SAFETY: SIG_IGN (1) for SIGCHLD has finished children reaped at once
        unsafe { signal(SIGCHLD, 1) };
        let live = crate::Live::headless("ignored", "x");
        let err = live
            .step("Sleep")
            .run(std::process::Command::new("sleep").arg("0.2"))
            .unwrap_err();
        assert!(err.to_string().starts_with("couldn't run sleep"), "{err}");
    }

    /// Hands the terminal back on `s`, interrupting what it runs, and quits on `q`.
    #[cfg(unix)]
    #[derive(Default)]
    struct Suspender {
        back: bool,
    }

    #[cfg(unix)]
    impl App for Suspender {
        fn view(&self, frame: &mut Frame) {
            frame.print(frame.area(), 0, if self.back { "app-back" } else { "app-ready" });
        }

        fn event(&mut self, event: &Event) -> Flow {
            match event {
                Event::Key(Key::Char('s')) => {
                    let interrupted = app::suspend(|| {
                        unsafe extern "C" {
                            fn raise(sig: i32) -> i32;
                        }
                        // SAFETY: while suspended, Ctrl-C goes to a handler that does nothing
                        unsafe { raise(2) }
                    });
                    assert_eq!(interrupted.unwrap(), 0);
                    self.back = true;
                    Flow::Continue
                }
                Event::Key(Key::Char('w')) => {
                    assert!(std::thread::spawn(|| panic!("worker gave up")).join().is_err());
                    Flow::Continue
                }
                Event::Key(Key::Char('p')) => panic!("the app gave up"),
                Event::Key(Key::Char('q')) => Flow::Quit,
                _ => Flow::Continue,
            }
        }

        fn mouse(&self) -> bool {
            true
        }

        fn title(&self) -> Option<String> {
            Some("suspender".into())
        }
    }

    /// The process-wide settings: output shape, target, colour mode, accent, Ctrl-C and
    /// frame rate, each changed in turn.
    pub fn settings() {
        use crate::{ColorMode, Output, Target};
        // nothing chosen yet: worked out from the environment, which in a test is a pipe
        assert_eq!(crate::color_mode(), ColorMode::None);
        assert_eq!(crate::output(), Output::Pretty);
        assert_eq!(crate::target(), Target::Stdout);

        for out in [Output::Json, Output::Quiet, Output::Pretty] {
            crate::set_output(out);
            assert_eq!(crate::output(), out);
        }

        // moving to stderr works the colour mode out again, for that stream
        crate::set_target(Target::Stderr);
        crate::set_target(Target::Stderr);
        assert_eq!(crate::target(), Target::Stderr);
        assert_eq!(crate::color_mode(), ColorMode::None);
        crate::success("drawn on stderr");
        assert!(crate::width() > 0);
        crate::set_target(Target::Stdout);

        // a mode that was chosen stays chosen when the target moves
        crate::set_color_mode(ColorMode::Ansi256);
        crate::set_target(Target::Stderr);
        assert_eq!(crate::color_mode(), ColorMode::Ansi256);
        crate::set_target(Target::Stdout);

        // another accent moves the pinks, but nothing else
        crate::set_color_mode(ColorMode::TrueColor);
        let rose = crate::paint(crate::palette::ROSE, "x");
        crate::set_accent(crate::Accent::Blue);
        assert_ne!(crate::paint(crate::palette::ROSE, "x"), rose);
        crate::set_accent(crate::Accent::Pink);
        assert_eq!(crate::paint(crate::palette::ROSE, "x"), rose);

        for how in [
            crate::CtrlC::ExitWith("stopped".into()),
            crate::CtrlC::Leave,
            crate::CtrlC::Exit,
        ] {
            crate::set_ctrl_c(how);
        }

        crate::set_fps(30);
        assert_eq!(crate::fps(), 30);
        crate::set_fps(0);
        assert_eq!(crate::fps(), 1);
    }

    /// Ctrl-C while something's drawn, without a terminal: the handler still puts back what
    /// it can, says the program's line and exits with 130. This is how Windows is checked.
    pub fn ctrl_c() {
        crate::set_ctrl_c(crate::CtrlC::ExitWith("stopped by the test".into()));
        crate::term::remember_terminal();
        assert!(crate::term::guard_interrupt().is_some(), "a handler was set");
        crate::term::press_ctrl_c();
        // the handler exits; carrying on fails the test
        std::thread::sleep(Duration::from_secs(20));
    }

    /// The panic hook, which is process-wide too.
    pub fn panics() {
        use std::panic;
        crate::install_panic_hook();
        assert!(panic::catch_unwind(|| panic!("a message")).is_err());
        assert!(panic::catch_unwind(|| panic!("a {}", "formatted message")).is_err());
        assert!(panic::catch_unwind(|| panic::panic_any(42)).is_err());
        let named = std::thread::Builder::new()
            .name("worker".into())
            .spawn(|| panic!("in a thread"))
            .unwrap();
        assert!(named.join().is_err());
        assert!(std::thread::spawn(|| panic!("in an unnamed thread")).join().is_err());
        crate::set_output(crate::Output::Json);
        assert!(panic::catch_unwind(|| panic!("as an event")).is_err());
        let _ = panic::take_hook();
    }

    pub fn piped() {
        assert!(prompt::confirm("a", false).unwrap());
        assert!(Confirm::new("b").lenient().ask().unwrap());
        assert!(Confirm::new("c").default(true).ask().unwrap());
        assert!(matches!(Confirm::new("d").ask(), Err(Error::Invalid(_))));
        assert_eq!(prompt::input("e", Some("dflt")).unwrap(), "dflt");
        assert_eq!(Input::new("f").initial("init").ask().unwrap(), "init");
        let strict = Input::new("g").validate(|s| if s == "good" { Ok(()) } else { Err("not good".into()) });
        assert!(matches!(strict.ask(), Err(Error::Invalid(why)) if why == "not good"));
        assert_eq!(prompt::password("h").unwrap(), "pw");
        assert!(prompt::confirm_typing("i", "yes").is_err());
        assert_eq!(prompt::number::<u32>("j", None).unwrap(), 12);
        assert!(prompt::number::<i8>("k", None).is_err());
        let today = prompt::Date::today();
        assert_eq!(prompt::date("l", None).unwrap(), today);
        assert_eq!(prompt::date("m", None).unwrap().to_string(), "2026-01-02");
        assert!(prompt::date("n", None).is_err());
        // the answers have run out
        assert!(Confirm::new("o").end_of_input(true).ask().unwrap());
        assert!(!prompt::confirm("p", false).unwrap());
        assert_eq!(prompt::input("q", None).unwrap(), "");
        assert!(matches!(
            prompt::multiselect("r", &["x"], &[]),
            Err(Error::NotATerminal)
        ));
        assert!(matches!(prompt::editor("s", ""), Err(Error::NotATerminal)));
    }
}

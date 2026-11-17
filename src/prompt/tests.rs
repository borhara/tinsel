use super::*;

#[test]
fn a_script_answers_a_flow_of_questions_and_goes_back() {
    use crate::app::Key as K;
    let mut keys = vec![K::Down, K::Enter, K::Left, K::Enter, K::Char(' '), K::Enter];
    keys.extend(crate::prompt::typed("v2"));
    keys.extend([K::Enter, K::Char('y')]);
    let script = script(keys);
    let env = Select::new("Env").options(["dev", "sit"]).ask().unwrap();
    assert_eq!(env, 1);
    assert!(matches!(
        MultiSelect::new("Parts").options(["a", "b"]).back().ask(),
        Err(Error::Back)
    ));
    assert_eq!(
        Select::new("Env").options(["dev", "sit"]).default(env).ask().unwrap(),
        1
    );
    assert_eq!(MultiSelect::new("Parts").options(["a", "b"]).ask().unwrap(), [0]);
    assert_eq!(input("Version", None).unwrap(), "v2");
    assert!(confirm("Go?", false).unwrap());
    assert_eq!(script.remaining(), 0);
    assert_eq!(
        script.answered(),
        [
            "●  Env  sit",
            "●  Env  sit",
            "●  Parts  a",
            "●  Version  v2",
            "●  Go?  yes"
        ]
    );
    assert!(matches!(confirm("More?", false), Err(Error::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof));
    drop(script);
    assert!(!scripted());
}

#[test]
fn ctrl_c_in_a_script_cancels() {
    let script = script([crate::app::Key::Ctrl('c')]);
    assert!(matches!(confirm("Go?", true), Err(Error::Cancelled)));
    assert_eq!(script.answered(), ["✕  Go?  cancelled"]);
}

#[test]
fn fuzzy_matches_letters_in_order() {
    assert_eq!(fuzzy("production", "pdn"), Some(vec![0, 3, 9]));
    assert_eq!(fuzzy("Staging", "stg"), Some(vec![0, 1, 3]));
    assert_eq!(fuzzy("staging", "gts"), None);
    assert_eq!(fuzzy("anything", ""), Some(vec![]));
}

#[test]
fn dates_round_trip_through_days() {
    for d in [
        (1970, 1, 1),
        (2000, 2, 29),
        (2026, 11, 12),
        (1969, 12, 31),
        (2400, 12, 31),
    ] {
        let date = Date::new(d.0, d.1, d.2).unwrap();
        assert_eq!(Date::from_days(date.days()), date);
    }
    assert_eq!(Date::new(1970, 1, 1).unwrap().days(), 0);
    // 2026-11-12 is a Thursday
    assert_eq!(Date::new(2026, 11, 12).unwrap().weekday(), 3);
}

#[test]
fn months_clamp_to_their_length() {
    let jan31 = Date::new(2027, 1, 31).unwrap();
    assert_eq!(jan31.add_months(1), Date::new(2027, 2, 28).unwrap());
    assert_eq!(jan31.add_months(-2), Date::new(2026, 11, 30).unwrap());
    assert_eq!(Date::new(2026, 2, 29), None);
}

#[test]
fn dates_parse() {
    assert_eq!("2026-03-14".parse(), Ok(Date::new(2026, 3, 14).unwrap()));
    assert!("2026-13-01".parse::<Date>().is_err());
    assert!("soon".parse::<Date>().is_err());
    assert!("2026-300-01".parse::<Date>().is_err());
    assert!("2026-01-300".parse::<Date>().is_err());
}

#[test]
fn line_editing_moves_and_deletes() {
    let mut e = LineEdit::default();
    for c in "deploy now".chars() {
        e.key(if c == ' ' { Key::Space } else { Key::Char(c) });
    }
    e.key(Key::Ctrl('w'));
    assert_eq!(e.text, "deploy ");
    e.key(Key::Home);
    e.key(Key::Char('>'));
    assert_eq!(e.split(), (">", "deploy "));
    e.key(Key::Right);
    e.key(Key::Backspace);
    assert_eq!(e.text, ">eploy ");
    e.key(Key::End);
    e.key(Key::Ctrl('u'));
    assert_eq!(e.text, "");
}

fn keys_of(bytes: &[u8]) -> Vec<Key> {
    let at = std::cell::Cell::new(0);
    let mut read = || {
        at.set(at.get() + 1);
        Ok(bytes.get(at.get() - 1).copied())
    };
    let mut keys = Vec::new();
    while at.get() < bytes.len() {
        match read_key(&mut read).unwrap() {
            Typed::Key(key) => keys.push(key),
            Typed::Paste(text) => keys.extend(pasted_keys(&text)),
        }
    }
    keys
}

#[test]
fn alt_keys_move_by_words_rather_than_giving_up() {
    assert_eq!(keys_of(b"\x1bbx"), [Key::WordLeft, Key::Char('x')]);
    assert_eq!(keys_of(b"\x1bf\x1b\x7f"), [Key::WordRight, Key::Ctrl('w')]);
    assert_eq!(keys_of(b"\x1bzq"), [Key::Other, Key::Char('q')]);
    // with modifiers, read to the end of the sequence and no further
    assert_eq!(keys_of(b"\x1b[1;5Ca"), [Key::WordRight, Key::Char('a')]);
    assert_eq!(keys_of(b"\x1b[1;3D\x1b[1;2A"), [Key::WordLeft, Key::Up]);
    assert_eq!(keys_of(b"\x1b[3;5~b"), [Key::Delete, Key::Char('b')]);
    let mut e = LineEdit::default();
    e.set("deploy my app");
    e.key(Key::WordLeft);
    assert_eq!(e.split().1, "app");
    e.key(Key::WordLeft);
    e.key(Key::WordLeft);
    assert_eq!(e.at, 0);
    e.key(Key::WordRight);
    assert_eq!(e.split().1, " my app");
    e.key(Key::WordRight);
    assert_eq!(e.split().1, " app");
}

#[test]
fn a_paste_is_typed_in_whole_and_never_presses_enter() {
    let keys = keys_of(b"\x1b[200~one\r\ntwo\tthree\n\x1b[201~!");
    let typed: String = keys
        .iter()
        .map(|k| match k {
            Key::Char(c) => *c,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(typed, "one two three!");
    let mut ui = Ui::script(&[keys.as_slice(), &[Key::Enter]].concat());
    assert_eq!(Input::new("Paste").run(&mut ui).unwrap(), "one two three!");
}

#[test]
fn bytes_become_keys() {
    assert_eq!(keys_of(b"\x1b[D"), [Key::Left]);
    assert_eq!(keys_of(b"\x1b[5~a\r"), [Key::PageUp, Key::Char('a'), Key::Enter]);
    assert_eq!(keys_of("é\x03".as_bytes()), [Key::Char('é'), Key::Interrupt]);
    assert_eq!(keys_of(b"\x1b"), [Key::Esc]);
}

fn typed(s: &str) -> Vec<Key> {
    s.chars()
        .map(|c| if c == ' ' { Key::Space } else { Key::Char(c) })
        .collect()
}

#[test]
fn right_answers_like_enter() {
    assert!(
        Confirm::new("Go?")
            .default(true)
            .run(&mut Ui::script(&[Key::Right]))
            .unwrap()
    );
    let env = Select::new("Env").options(["dev", "sit"]);
    assert_eq!(env.run(&mut Ui::script(&[Key::Down, Key::Right])).unwrap(), 1);
    let parts = MultiSelect::new("Parts").options(["a", "b"]);
    assert_eq!(parts.run(&mut Ui::script(&[Key::Space, Key::Right])).unwrap(), [0]);
    // in typed text → moves first, and answers only from the end of the line
    let mut keys = typed("v2");
    keys.extend([Key::Home, Key::Right, Key::Char('.'), Key::Right, Key::Right]);
    assert_eq!(Input::new("Version").run(&mut Ui::script(&keys)).unwrap(), "v.2");
    // with a suggestion showing, → takes it, and the next → answers
    let region = Input::new("Region").suggestions(&["eu-west-1"]);
    let mut keys = typed("eu");
    keys.extend([Key::Right, Key::Right]);
    assert_eq!(region.run(&mut Ui::script(&keys)).unwrap(), "eu-west-1");
}

#[test]
fn initial_text_can_be_taken_or_edited() {
    let name = Input::new("Name").initial("api").default("web");
    let mut ui = Ui::script(&[Key::Enter]);
    assert_eq!(name.run(&mut ui).unwrap(), "api");
    assert_eq!(ui.shown(), ["  ●  Name  api"]);
    let keys = [Key::Backspace, Key::Char('p'), Key::Enter];
    assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "app");
    // cleared, the default is the answer as usual
    assert_eq!(name.run(&mut Ui::script(&[Key::Ctrl('u'), Key::Enter])).unwrap(), "web");
}

#[test]
fn confirm_takes_keys_and_the_default() {
    let go = Confirm::new("Go?").default(true);
    assert!(!go.run(&mut Ui::script(&[Key::Char('N')])).unwrap());
    let mut ui = Ui::script(&[Key::Down, Key::Enter]);
    assert!(go.run(&mut ui).unwrap());
    assert_eq!(ui.shown(), ["  ●  Go?  yes"]);
    assert!(matches!(go.run(&mut Ui::script(&[Key::Esc])), Err(Error::Cancelled)));
}

#[test]
fn input_edits_and_validates() {
    let q = Input::new("Name").validate(|s| if s.len() > 2 { Ok(()) } else { Err("too short".into()) });
    let mut keys = typed("ab");
    keys.push(Key::Enter);
    let mut ui = Ui::script(&keys);
    assert!(q.run(&mut ui).is_err());
    assert_eq!(ui.shown()[1], "     ✕ too short");
    keys.extend([Key::Home, Key::Char('>'), Key::Enter]);
    let mut ui = Ui::script(&keys);
    assert_eq!(q.run(&mut ui).unwrap(), ">ab");
    assert_eq!(ui.shown(), ["  ●  Name  >ab"]);
}

fn run_select(question: &str, options: &[&str], ui: &mut Ui) -> Result<usize> {
    Select::new(question).options(options.iter().copied()).run(ui)
}

fn run_multiselect(question: &str, options: &[&str], ticked: &[usize], ui: &mut Ui) -> Result<Vec<usize>> {
    MultiSelect::new(question)
        .options(options.iter().copied())
        .default(ticked)
        .run(ui)
}

const LONG: [&str; 10] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet",
];

#[test]
fn select_moves_and_wraps() {
    let pick = |keys: &[Key]| run_select("Pick", &["a", "b", "c"], &mut Ui::script(keys));
    assert_eq!(pick(&[Key::Down, Key::Down, Key::Enter]).unwrap(), 2);
    assert_eq!(pick(&[Key::Up, Key::Enter]).unwrap(), 2);
    assert_eq!(pick(&[Key::Char('j'), Key::Enter]).unwrap(), 1);
}

#[test]
fn esc_clears_the_filter_before_cancelling() {
    let mut keys = typed("ho");
    // the cursor stays on what it was on
    keys.extend([Key::Esc, Key::Down, Key::Enter]);
    assert_eq!(run_select("Pick", &LONG, &mut Ui::script(&keys)).unwrap(), 5);
    let mut ui = Ui::script(&typed("ho"));
    let _ = run_select("Pick", &LONG, &mut ui);
    assert_eq!(ui.shown()[0], "  ?  Pick  ho▏  2 of 10");
    assert_eq!(ui.shown()[1], "    › echo");
    let mut keys = typed("ho");
    keys.extend([Key::Esc, Key::Esc]);
    assert!(matches!(
        run_select("Pick", &LONG, &mut Ui::script(&keys)),
        Err(Error::Cancelled)
    ));
    let mut keys = typed("zz");
    keys.extend([Key::Esc, Key::Space, Key::Enter]);
    assert_eq!(
        run_multiselect("Pick", &LONG, &[], &mut Ui::script(&keys)).unwrap(),
        [0]
    );
}

#[test]
fn multiselect_toggles() {
    let keys = [Key::Space, Key::Down, Key::Space, Key::Down, Key::Space, Key::Enter];
    let mut ui = Ui::script(&keys);
    assert_eq!(
        run_multiselect("Pick", &["a", "b", "c"], &[1], &mut ui).unwrap(),
        [0, 2]
    );
    assert_eq!(ui.shown(), ["  ●  Pick  a, c"]);
    let keys = [Key::Char('a'), Key::Enter];
    assert_eq!(
        run_multiselect("Pick", &["a", "b"], &[], &mut Ui::script(&keys)).unwrap(),
        [0, 1]
    );
}

#[test]
fn confirm_reads_piped_answers() {
    for (line, want) in [("y", true), ("YES", true), (" no ", false), ("N", false)] {
        assert_eq!(yes_or_no(line, false, false).unwrap(), want, "{line}");
    }
    // an empty line, or nothing at all, takes the default
    assert!(yes_or_no("", true, false).unwrap());
    assert!(!yes_or_no("", false, false).unwrap());
    assert!(matches!(yes_or_no("maybe", true, false), Err(Error::Invalid(_))));
    assert!(matches!(yes_or_no("yep", false, false), Err(Error::Invalid(_))));
}

#[test]
fn lenient_confirm_takes_anything_starting_with_y_as_yes() {
    for (line, want) in [
        ("y", true),
        ("Yep", true),
        ("  yes please ", true),
        ("YES", true),
        ("n", false),
        ("nope", false),
        ("maybe", false),
        ("ok", false),
        ("1", false),
    ] {
        assert_eq!(yes_or_no(line, false, true).unwrap(), want, "{line}");
        assert_eq!(yes_or_no(line, true, true).unwrap(), want, "{line}");
    }
    // an empty line still takes the default
    assert!(yes_or_no("", true, true).unwrap());
    assert!(!yes_or_no("  ", false, true).unwrap());
    // and the keys at a terminal are the same
    let go = Confirm::new("Go?").default(true).lenient();
    assert!(!go.run(&mut Ui::script(&[Key::Char('q'), Key::Char('n')])).unwrap());
}

#[test]
fn going_back_is_opt_in() {
    let go = Confirm::new("Go?");
    assert!(!go.run(&mut Ui::script(&[Key::Left, Key::Enter])).unwrap());
    let mut ui = Ui::script(&[Key::Left]);
    assert!(matches!(go.back().run(&mut ui), Err(Error::Back)));
    // going back leaves nothing behind
    assert!(ui.shown().is_empty());

    let name = Input::new("Name");
    let mut keys = typed("ab");
    keys.extend([Key::Left, Key::Left, Key::Left, Key::Enter]);
    assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "ab");
    let name = name.back();
    assert!(matches!(name.run(&mut Ui::script(&keys)), Err(Error::Back)));
    // ← only goes back from the start of the line
    let mut keys = typed("ab");
    keys.extend([Key::Left, Key::Char('>'), Key::Enter]);
    assert_eq!(name.run(&mut Ui::script(&keys)).unwrap(), "a>b");
}

fn services<'a>() -> Select<'a> {
    Select::new("Service")
        .heading("Frontend")
        .option("web")
        .disabled("docs", "no deploys on Fridays")
        .heading("Backend")
        .option("api")
        .disabled("cron", "retired")
        .option("worker")
}

#[test]
fn headings_and_disabled_options_are_skipped() {
    let mut ui = Ui::script(&[Key::Down]);
    let _ = services().run(&mut ui);
    assert_eq!(
        ui.shown(),
        [
            "  ?  Service",
            "    Frontend",
            "      web",
            "      docs  (no deploys on Fridays)",
            "    Backend",
            "    › api",
            "      cron  (retired)",
            "      worker",
            "    ↑↓ move · enter choose",
        ]
    );
    let pick = |keys: &[Key]| services().run(&mut Ui::script(keys)).unwrap();
    assert_eq!(pick(&[Key::Down, Key::Down, Key::Enter]), 4);
    assert_eq!(pick(&[Key::Up, Key::Enter]), 4);
    assert_eq!(pick(&[Key::Down, Key::Down, Key::Down, Key::Enter]), 0);
    // a disabled default isn't where the cursor starts
    assert_eq!(services().default(1).run(&mut Ui::script(&[Key::Enter])).unwrap(), 0);
    assert_eq!(services().default(2).run(&mut Ui::script(&[Key::Enter])).unwrap(), 2);
}

#[test]
fn multiselect_never_ticks_disabled_options() {
    let restart = || {
        MultiSelect::new("Restart")
            .option("api")
            .disabled("cron", "retired")
            .option("worker")
            .default(&[1])
    };
    let mut ui = Ui::script(&[Key::Down, Key::Space, Key::Enter]);
    assert_eq!(restart().run(&mut ui).unwrap(), [2]);
    assert_eq!(ui.shown(), ["  ●  Restart  worker"]);
    assert_eq!(
        restart().run(&mut Ui::script(&[Key::Char('a'), Key::Enter])).unwrap(),
        [0, 2]
    );
    let mut ui = Ui::script(&[Key::Down]);
    let _ = restart().run(&mut ui);
    assert_eq!(ui.shown()[2], "      – cron  (retired)");
}

#[test]
fn validation_keeps_the_question_open() {
    let at_least_one = MultiSelect::new("Restart").options(["api", "worker"]).validate(|p| {
        if p.is_empty() {
            Err("pick at least one".into())
        } else {
            Ok(())
        }
    });
    let mut ui = Ui::script(&[Key::Enter]);
    assert!(at_least_one.run(&mut ui).is_err());
    assert_eq!(ui.shown()[1], "     ✕ pick at least one");
    let keys = [Key::Enter, Key::Space, Key::Enter];
    assert_eq!(at_least_one.run(&mut Ui::script(&keys)).unwrap(), [0]);

    let small = Select::new("Size")
        .options(["small", "large"])
        .validate(|i| if i == 1 { Err("too big".into()) } else { Ok(()) });
    let mut ui = Ui::script(&[Key::Down, Key::Enter]);
    assert!(small.run(&mut ui).is_err());
    assert_eq!(ui.shown()[1], "     ✕ too big");
    let keys = [Key::Down, Key::Enter, Key::Up, Key::Enter];
    assert_eq!(small.run(&mut Ui::script(&keys)).unwrap(), 0);
}

#[test]
fn lists_go_back_and_show_how() {
    let mut ui = Ui::script(&[Key::Down]);
    let _ = services().back().run(&mut ui);
    assert_eq!(ui.shown().last().unwrap(), "    ↑↓ move · enter choose · ← back");
    assert!(matches!(
        services().back().run(&mut Ui::script(&[Key::Left])),
        Err(Error::Back)
    ));
    let restart = MultiSelect::new("Restart").options(["api"]).back();
    assert!(matches!(restart.run(&mut Ui::script(&[Key::Left])), Err(Error::Back)));
    let tag = Dropdown::new("Tag").options(["v1.0.0"]).back();
    let mut ui = Ui::script(&[Key::Left]);
    assert!(matches!(tag.run(&mut ui), Err(Error::Back)));
    assert!(ui.shown().is_empty());
    // without .back(), ← does nothing
    assert_eq!(services().run(&mut Ui::script(&[Key::Left, Key::Enter])).unwrap(), 0);
}

#[test]
fn hints_sit_under_the_question() {
    let mut ui = Ui::script(&[]);
    let _ = Select::new("Region")
        .hint("where it runs")
        .options(["eu", "us"])
        .run(&mut ui);
    assert_eq!(ui.shown()[..3], ["  ?  Region", "    where it runs", "    › eu"]);
}

fn tags<'a>() -> Dropdown<'a> {
    let mut tags = Dropdown::new("Version")
        .option("v1.12.0")
        .note("today")
        .badge("latest")
        .option("v1.12.0-rc.1")
        .note("2 days ago")
        .dim();
    for minor in (0..12).rev() {
        tags = tags
            .option(format!("v1.{minor}.0"))
            .note(format!("{} weeks ago", 13 - minor));
    }
    tags.rows(5)
}

#[test]
fn dropdowns_scroll_with_counts_above_and_below() {
    let shown = |keys: &[Key], default: Option<usize>| {
        let mut tags = tags();
        if let Some(d) = default {
            tags = tags.default(d);
        }
        let mut ui = Ui::script(keys);
        let _ = tags.run(&mut ui);
        ui.shown()
    };
    assert_eq!(
        shown(&[], Some(4)),
        [
            "  ?  Version",
            "      ↑ 2 more",
            "      v1.11.0            2 weeks ago",
            "      v1.10.0            3 weeks ago",
            "    › v1.9.0             4 weeks ago",
            "      v1.8.0             5 weeks ago",
            "      v1.7.0             6 weeks ago",
            "      ↓ 7 more",
            "    type to filter · ↑↓ move · enter choose",
        ]
    );
    assert_eq!(
        shown(&[], None)[1..5],
        [
            "",
            "    › v1.12.0 [latest]         today",
            "      v1.12.0-rc.1        2 days ago",
            "      v1.11.0            2 weeks ago",
        ]
    );
    assert_eq!(shown(&[Key::Up], Some(4))[4], "    › v1.10.0            3 weeks ago");
    assert_eq!(
        shown(&[Key::PageDown], Some(4))[4],
        "    › v1.4.0             9 weeks ago"
    );
    // ↑ from the top wraps to the bottom
    assert_eq!(
        shown(&[Key::Up], None)[1..8],
        [
            "      ↑ 9 more",
            "      v1.4.0             9 weeks ago",
            "      v1.3.0            10 weeks ago",
            "      v1.2.0            11 weeks ago",
            "      v1.1.0            12 weeks ago",
            "    › v1.0.0            13 weeks ago",
            "",
        ]
    );
}

#[test]
fn dropdowns_filter_and_take_typed_answers() {
    let mut keys = typed("1.1");
    keys.push(Key::Enter);
    let mut ui = Ui::script(&keys);
    assert_eq!(tags().run(&mut ui).unwrap(), "v1.12.0");
    assert_eq!(ui.shown(), ["  ●  Version  v1.12.0"]);
    let mut ui = Ui::script(&typed("1.1"));
    let _ = tags().run(&mut ui);
    assert_eq!(ui.shown()[0], "  ?  Version  1.1▏  5 of 14");

    // a typed answer is only offered when asked for
    let mut keys = typed("v2.0.0");
    keys.push(Key::Enter);
    assert!(tags().run(&mut Ui::script(&keys)).is_err());
    let custom = tags().allow_custom().validate(|s| {
        if s.starts_with('v') {
            Ok(())
        } else {
            Err("tags start with v".into())
        }
    });
    let mut ui = Ui::script(&keys);
    assert_eq!(custom.run(&mut ui).unwrap(), "v2.0.0");
    assert_eq!(ui.shown(), ["  ●  Version  v2.0.0"]);
    let mut ui = Ui::script(&typed("v2.0.0"));
    let _ = custom.run(&mut ui);
    assert_eq!(ui.shown()[1], "    › use “v2.0.0”");
    let mut keys = typed("2.0.0");
    keys.push(Key::Enter);
    let mut ui = Ui::script(&keys);
    assert!(custom.run(&mut ui).is_err());
    assert_eq!(ui.shown()[1], "     ✕ tags start with v");
    // Esc clears what's typed, then cancels
    let mut keys = typed("zz");
    keys.extend([Key::Esc, Key::Enter]);
    assert_eq!(custom.run(&mut Ui::script(&keys)).unwrap(), "v1.12.0");
    let mut keys = typed("zz");
    keys.extend([Key::Esc, Key::Esc]);
    assert!(matches!(custom.run(&mut Ui::script(&keys)), Err(Error::Cancelled)));
}

#[test]
fn without_a_terminal_lists_answer_with_their_default() {
    assert!(matches!(services().unasked(), Err(Error::NotATerminal)));
    assert_eq!(services().default(2).unasked().unwrap(), 2);
    // a default that can't be picked is no answer
    assert!(matches!(services().default(1).unasked(), Err(Error::NotATerminal)));
    let picky = services().default(2).validate(|_| Err("no".into()));
    assert!(matches!(picky.unasked(), Err(Error::Invalid(_))));

    let restart = || MultiSelect::new("Restart").option("api").disabled("cron", "retired");
    assert!(matches!(restart().unasked(), Err(Error::NotATerminal)));
    assert_eq!(restart().default(&[0, 1]).unasked().unwrap(), [0]);
    assert_eq!(restart().default(&[]).unasked().unwrap(), [] as [usize; 0]);
    let at_least_one = restart()
        .default(&[])
        .validate(|p| if p.is_empty() { Err("pick one".into()) } else { Ok(()) });
    assert!(matches!(at_least_one.unasked(), Err(Error::Invalid(_))));

    assert!(matches!(tags().unasked(), Err(Error::NotATerminal)));
    assert_eq!(tags().default(1).unasked().unwrap(), "v1.12.0-rc.1");
}

#[test]
fn summaries_keep_whole_names() {
    assert_eq!(summary(&[]), "none");
    assert_eq!(summary(&["api", "web"]), "api, web");
    let many = ["api", "web", "worker", "scheduler", "gateway", "search", "billing"];
    assert_eq!(summary(&many), "api, web, worker, scheduler and 3 more");
    assert!(summary(&many).len() <= 40);
}

use crate::app::Key as K;
use crate::output::with_terminal;

#[test]
fn errors_say_what_happened() {
    let io = Error::from(io::Error::other("gone"));
    let all = [
        (Error::Cancelled, "cancelled"),
        (Error::Back, "went back to the previous question"),
        (Error::NotATerminal, "this question needs a terminal to answer it"),
        (Error::NoOptions, "there was nothing to choose from"),
        (Error::Invalid("no".into()), "invalid answer: no"),
    ];
    for (e, said) in all {
        assert_eq!(e.to_string(), said);
        assert!(std::error::Error::source(&e).is_none());
    }
    assert_eq!(io.to_string(), "couldn't talk to the terminal: gone");
    assert!(std::error::Error::source(&io).is_some());
}

#[test]
fn every_key_the_terminal_sends() {
    let keys = keys_of(b"\t \x7f\x08\x1b[A\x1b[B\x1b[C\x1bOH\x1b[F\x01\x1c\x80");
    assert_eq!(
        keys,
        [
            Key::Tab,
            Key::Space,
            Key::Backspace,
            Key::Backspace,
            Key::Up,
            Key::Down,
            Key::Right,
            Key::Home,
            Key::End,
            Key::Ctrl('a'),
            Key::Other,
            Key::Other,
        ]
    );
    let tilde = keys_of(b"\x1b[1~\x1b[7~\x1b[4~\x1b[8~\x1b[3~\x1b[6~\x1b[2~\x1b[15~\x1bOx");
    assert_eq!(
        tilde,
        [
            Key::Home,
            Key::Home,
            Key::End,
            Key::End,
            Key::Delete,
            Key::PageDown,
            Key::Other,
            Key::Other,
            Key::Other,
        ]
    );
    assert_eq!(keys_of("✦🎉".as_bytes()), [Key::Char('✦'), Key::Char('🎉')]);
}

#[test]
fn every_app_key_has_a_question_key() {
    let pairs = [
        (K::Ctrl('x'), Key::Ctrl('x')),
        (K::Tab, Key::Tab),
        (K::Backspace, Key::Backspace),
        (K::Delete, Key::Delete),
        (K::Esc, Key::Esc),
        (K::Up, Key::Up),
        (K::Down, Key::Down),
        (K::Left, Key::Left),
        (K::Right, Key::Right),
        (K::Home, Key::Home),
        (K::End, Key::End),
        (K::PageUp, Key::PageUp),
        (K::PageDown, Key::PageDown),
        (K::BackTab, Key::Other),
        (K::F(1), Key::Other),
    ];
    for (from, to) in pairs {
        assert_eq!(Key::from(from), to, "{from:?}");
    }
}

#[test]
fn a_frame_counts_the_rows_a_long_line_wraps_onto() {
    let long = "x".repeat(60);
    let (_, drawn) = frame(0, &[long.clone(), "next".into()], 40, 24);
    assert_eq!(drawn, 3);
    let (s, _) = frame(drawn, &[long], 40, 24);
    assert!(s.starts_with("\x1b[3F"), "{s:?}");
    // exactly the width still takes one row
    assert_eq!(frame(0, &["y".repeat(40)], 40, 24).1, 1);
}

#[test]
fn a_frame_taller_than_the_screen_draws_what_fits() {
    let lines: Vec<String> = (0..30).map(|i| format!("option {i}")).collect();
    let (s, drawn) = frame(0, &lines, 80, 10);
    assert_eq!(drawn, 9);
    assert!(s.contains("option 8") && !s.contains("option 9"), "{s:?}");
}

#[test]
fn frames_draw_over_themselves_on_screen() {
    let mut frame = Frame::default();
    frame.draw(&["one".into(), "two".into()]);
    frame.draw(&["three".into()]);
    assert_eq!(
        (frame.drawn, frame.lines.as_slice()),
        (1, ["three".to_string()].as_slice())
    );
}

#[test]
fn passwords_show_dots() {
    let mut keys = typed("hunter2");
    keys.push(Key::Enter);
    let mut ui = Ui::script(&keys);
    assert_eq!(Input::new("Password").password().run(&mut ui).unwrap(), "hunter2");
    assert_eq!(ui.shown(), ["  ●  Password  •••••••"]);
    let mut ui = Ui::script(&[Key::Char('a'), Key::Left]);
    let _ = Input::new("Password").password().run(&mut ui);
    assert_eq!(ui.shown(), ["  ?  Password  ▏•"]);
    assert!(matches!(
        Input::new("x").run(&mut Ui::script(&[Key::Esc])),
        Err(Error::Cancelled)
    ));
}

#[test]
fn tab_cycles_through_many_suggestions() {
    let many = ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"];
    let q = Input::new("Pick").suggestions(&many);
    let mut ui = Ui::script(&[Key::Char('a'), Key::Tab, Key::Tab]);
    let _ = q.run(&mut ui);
    assert_eq!(ui.shown()[0], "  ?  Pick  a2▏");
    assert_eq!(ui.shown()[1], "     a1  a2  a3  a4  a5  a6  +2");
    let keys = [Key::Char('a'), Key::Char('7'), Key::Tab, Key::Enter];
    assert_eq!(q.run(&mut Ui::script(&keys)).unwrap(), "a7");
}

#[test]
fn delete_takes_the_character_under_the_cursor() {
    let mut e = LineEdit::default();
    e.set("abc");
    e.key(Key::Home);
    assert!(e.key(Key::Delete));
    assert!(e.key(Key::Ctrl('d')));
    assert_eq!(e.text, "c");
    assert!(!e.key(Key::PageUp));
}

#[test]
fn paths_complete_from_the_folder_typed_so_far() {
    let dir = std::env::temp_dir().join(format!("tinsel-paths-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "").unwrap();
    std::fs::write(dir.join(".hidden"), "").unwrap();
    let typed = format!("{}/", dir.display());
    assert_eq!(
        complete_path_in(&typed, ""),
        [format!("{typed}Cargo.toml"), format!("{typed}src/")]
    );
    assert_eq!(complete_path_in(&format!("{typed}."), ""), [format!("{typed}.hidden")]);
    let home = dir.display().to_string();
    assert_eq!(complete_path_in("~/C", &home), ["~/Cargo.toml"]);
    // `~` alone stands for the home folder, and no home means no `~`
    let sep = std::path::MAIN_SEPARATOR;
    assert_eq!(complete_path_in("~", &home), [format!("~{sep}")]);
    assert_eq!(complete_path_in("~", "/h"), [format!("~{sep}")]);
    assert_eq!(complete_path_in("~nobody-here/x", &home), Vec::<String>::new());
    assert_eq!(complete_path_in("~", ""), Vec::<String>::new());
    assert_eq!(complete_path_in("/nonexistent/x", &home), Vec::<String>::new());
    assert!(!complete_path("").is_empty(), "the current folder has files");
    let q = Input::new("Path").paths();
    let keys: Vec<Key> = typed
        .chars()
        .map(Key::Char)
        .chain([Key::Char('s'), Key::Tab, Key::Enter])
        .collect();
    assert_eq!(q.run(&mut Ui::script(&keys)).unwrap(), format!("{typed}src/"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn numbers_and_typed_confirmations() {
    assert_eq!(kind_of_number("u8"), "whole number of 0 or more");
    assert_eq!(kind_of_number("i32"), "whole number");
    assert_eq!(kind_of_number("f64"), "number");
    let _script = script([K::Enter]);
    assert_eq!(number::<u32>("Replicas", Some(3)).unwrap(), 3);
    drop(_script);
    let _script = script(crate::prompt::typed("my-app").into_iter().chain([K::Enter]));
    assert!(confirm_typing("Delete?", "my-app").is_ok());
}

#[test]
fn notes_and_badges_need_an_option_to_go_on() {
    let q = Select::new("q")
        .note("n")
        .badge("b")
        .dim()
        .option("a")
        .badge("new")
        .note("a note")
        .dim();
    let mut ui = Ui::script(&[]);
    crate::style::with_color_mode(crate::ColorMode::TrueColor, || {
        let _ = q.run(&mut ui);
    });
    assert!(
        ui.frame.lines.iter().any(|l| l.contains(" new ")),
        "{:?}",
        ui.frame.lines
    );
}

#[test]
fn long_lists_page_and_filter() {
    let pick = |keys: &[Key]| run_select("Pick", &LONG, &mut Ui::script(keys));
    assert_eq!(
        pick(&[Key::PageDown, Key::PageUp, Key::PageDown, Key::Enter]).unwrap(),
        8
    );
    let mut keys = typed("zz");
    keys.extend([Key::PageUp, Key::Enter, Key::Backspace, Key::Backspace, Key::Enter]);
    assert_eq!(pick(&keys).unwrap(), 0);
    let mut keys = typed("ech");
    keys.extend([Key::Ctrl('u'), Key::Enter]);
    assert_eq!(pick(&keys).unwrap(), 4);
    let short = |keys: &[Key]| run_select("Pick", &["a", "b"], &mut Ui::script(keys));
    assert_eq!(short(&[Key::Char('k'), Key::Enter]).unwrap(), 1);
}

#[test]
fn multiselect_toggles_whats_shown_and_cancels() {
    let mut keys = typed("zz");
    keys.extend([Key::Space, Key::Ctrl('u'), Key::Ctrl('a'), Key::Enter]);
    let all: Vec<usize> = (0..LONG.len()).collect();
    assert_eq!(
        run_multiselect("Pick", &LONG, &[], &mut Ui::script(&keys)).unwrap(),
        all
    );
    assert!(matches!(
        run_multiselect("Pick", &["a"], &[], &mut Ui::script(&[Key::Esc])),
        Err(Error::Cancelled)
    ));
}

#[test]
fn the_shortcuts_ask_through_a_script() {
    let _script = script([K::Down, K::Enter]);
    assert_eq!(select("Env", &["dev", "prod"]).unwrap(), 1);
    drop(_script);
    let _script = script([K::Char(' '), K::Enter]);
    assert_eq!(multiselect("Parts", &["a", "b"], &[]).unwrap(), [0]);
}

#[test]
fn asking_without_a_terminal_or_options() {
    with_terminal(false, || {
        assert_eq!(Select::new("q").option("a").default(0).ask().unwrap(), 0);
        assert_eq!(MultiSelect::new("q").option("a").default(&[0]).ask().unwrap(), [0]);
        assert_eq!(Dropdown::new("q").option("a").default(0).ask().unwrap(), "a");
    });
    let _script = script([K::Enter]);
    assert!(matches!(
        Select::new("q").disabled("a", "no").ask(),
        Err(Error::NoOptions)
    ));
    // nothing to choose from is an answer, unless none isn't allowed
    assert!(MultiSelect::new("q").ask().unwrap().is_empty());
    let at_least_one = MultiSelect::new("q").validate(|p| if p.is_empty() { Err("pick one".into()) } else { Ok(()) });
    assert!(matches!(at_least_one.ask(), Err(Error::NoOptions)));
    assert!(matches!(
        Dropdown::new("q").disabled("a", "no").ask(),
        Err(Error::NoOptions)
    ));
    assert_eq!(Dropdown::new("q").option("a").ask().unwrap(), "a");
}

#[test]
fn dropdowns_take_spaces_and_ignore_keys_they_dont_use() {
    let keys = [Key::Char('a'), Key::Space, Key::Char('b'), Key::Tab, Key::Enter];
    let q = Dropdown::new("Name").option("x").allow_custom();
    assert_eq!(q.run(&mut Ui::script(&keys)).unwrap(), "a b");
}

#[test]
fn dates_are_picked_on_a_calendar() {
    let start = Date::new(2026, 11, 12).unwrap();
    let moves = [
        K::Left,
        K::Char('l'),
        K::Up,
        K::Char('j'),
        K::PageUp,
        K::Char(']'),
        K::Right,
        K::Char('h'),
        K::Down,
        K::Char('k'),
        K::PageDown,
        K::Char('['),
        K::Char('x'),
        K::Enter,
    ];
    let script = script(moves);
    assert_eq!(date("When", Some(start)).unwrap(), start);
    assert_eq!(script.answered(), ["●  When  2026-11-12"]);
    let _script = crate::prompt::script([K::Char('t'), K::Enter]);
    assert_eq!(date("When", Some(start)).unwrap(), Date::today());
    let _cancel = crate::prompt::script([K::Esc]);
    assert!(matches!(date("When", None), Err(Error::Cancelled)));
    // a month that ends on a Sunday has no part-week left over
    let may = calendar(Date::new(2026, 5, 31).unwrap(), Date::new(2026, 5, 1).unwrap());
    assert!(may.last().unwrap().contains("31"));
}

#[test]
fn the_editor_comes_from_the_environment() {
    let set = |s: &str| Some(s.to_string());
    assert_eq!(editor_command(set("code -w"), set("nano")), "code -w");
    assert_eq!(editor_command(set(" "), set("nano")), "nano");
    assert_eq!(editor_command(None, None), if cfg!(windows) { "notepad" } else { "vi" });
}

#[cfg(unix)]
#[test]
fn the_editor_writes_the_answer() {
    let dir = std::env::temp_dir().join(format!("tinsel-editor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tool = |name: &str, body: &str| {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    };
    let writes = tool("writes", r#"printf 'one\n\ntwo\n' >> "$1""#);
    assert_eq!(edit("Notes", "zero\n", &writes).unwrap(), "zero\none\n\ntwo\n");
    assert!(matches!(edit("Notes", "", "false"), Err(Error::Cancelled)));
    assert!(matches!(edit("Notes", "", "/nonexistent/editor"), Err(Error::Io(_))));
    let deletes = tool("deletes", r#"rm "$1""#);
    assert!(matches!(edit("Notes", "", &deletes), Err(Error::Io(_))));
    std::fs::remove_dir_all(&dir).unwrap();
}

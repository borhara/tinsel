//! What each component draws, checked as plain text. Widths that depend on the terminal
//! (rules, callout wrapping, ls-style columns) are checked loosely; everything else exactly.

use tinsel::prelude::*;
use tinsel::{ColorMode, Spinner, bars, columns, diff, meter, set_color_mode, sparkline, text};

fn plain() {
    set_color_mode(ColorMode::None);
}

#[test]
fn table_sizes_columns_and_right_aligns_numbers() {
    plain();
    let table = Table::new(["app", "size", "builds"])
        .row(["tube", "34.2 MB", "128"])
        .row(["music", "12.8 MB", "9"]);
    assert_eq!(
        table.lines().plain(),
        [
            "  app        size   builds",
            "  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈",
            "  tube    34.2 MB      128",
            "  music   12.8 MB        9",
        ]
        .join("\n")
    );
}

#[test]
fn tree_draws_guides() {
    plain();
    let tree = Tree::new("dot")
        .child(Tree::new("tube").leaf("ios").leaf("macos"))
        .leaf("README.md");
    assert_eq!(
        tree.lines().plain(),
        ["  dot", "  ├─ tube", "  │  ├─ ios", "  │  └─ macos", "  └─ README.md"].join("\n")
    );
}

#[test]
fn card_puts_a_blank_line_under_the_headline() {
    plain();
    let card = Card::success("deployed")
        .aside("in 2.0s")
        .kv("url", "https://x.y")
        .width(30);
    assert_eq!(
        card.lines().plain(),
        [
            "",
            "  ╭────────────────────────────╮",
            "  │  ● deployed in 2.0s        │",
            "  │                            │",
            "  │  url       https://x.y     │",
            "  ╰────────────────────────────╯",
        ]
        .join("\n")
    );
}

#[test]
fn headline_only_card_has_no_trailing_blank() {
    plain();
    assert_eq!(Card::failure("broke").width(20).lines().len(), 4);
}

#[test]
fn report_nests_causes_and_ends_with_the_hint() {
    plain();
    let report = Report::new("couldn't deploy")
        .cause("AccessDenied")
        .cause("no trust policy")
        .hint("run fix");
    assert_eq!(
        report.lines().plain(),
        [
            "  ✕  couldn't deploy",
            "     ╰─ AccessDenied",
            "        ╰─ no trust policy",
            "  →  run fix"
        ]
        .join("\n")
    );
}

#[test]
fn report_from_error_walks_the_source_chain() {
    plain();
    #[derive(Debug)]
    struct Outer(std::io::Error);
    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("upload failed")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    let err = Outer(std::io::Error::other("connection reset"));
    let text = Report::from_error("couldn't deploy", &err).lines().plain();
    assert!(text.contains("╰─ upload failed"));
    assert!(text.contains("   ╰─ connection reset"));
}

#[test]
fn diff_shows_removal_before_addition() {
    plain();
    let d = diff("a\nb\nc", "a\nB\nc\nd");
    assert_eq!(d.plain(), ["     a", "  -  b", "  +  B", "     c", "  +  d"].join("\n"));
}

#[test]
fn code_marks_the_highlighted_line() {
    plain();
    let c = Code::new("one\ntwo\nthree").first_line(9).highlight(10);
    assert_eq!(
        c.lines().plain(),
        [" 9 │ one", "10 › two", "11 │ three"]
            .map(|l| format!("  {l}"))
            .join("\n")
    );
}

#[test]
fn pairs_align_values() {
    plain();
    assert_eq!(
        pairs(&[("a", "1"), ("long key", "2")]).plain(),
        "  a          1\n  long key   2"
    );
}

#[test]
fn columns_keep_every_item() {
    plain();
    let items = ["one", "two", "three", "four", "five"];
    let text = columns(&items).plain();
    assert!(items.iter().all(|i| text.contains(i)));
}

#[test]
fn tones_have_their_marks() {
    plain();
    assert_eq!(Tone::Success.line("ok"), "  ●  ok");
    assert_eq!(Tone::Warn.line("hm"), "  ▲  hm");
    assert_eq!(Tone::Error.line("no"), "  ✕  no");
    assert_eq!(Tone::Hint.line("try"), "  →  try");
}

#[test]
fn callout_wraps_under_its_bar() {
    plain();
    let block = callout(Callout::Note, "Title", &"word ".repeat(40));
    assert!(block.len() > 2);
    assert!(block.lines().iter().all(|l| l.starts_with("  ▎ ")));
}

#[test]
fn charts_have_the_right_shape() {
    plain();
    assert_eq!(text::width(&sparkline(&[1.0, 5.0, 3.0, 8.0])), 4);
    assert_eq!(sparkline(&[0.0, 10.0]), "▁█");
    assert!(meter("disk", 50.0, 100.0, 10).ends_with(" 50%"));
    assert_eq!(bars(&[("a", 2.0), ("bb", 1.0)], 10).len(), 2);
}

#[test]
fn every_spinner_fills_exactly_its_width() {
    plain();
    for spinner in Spinner::ALL {
        for i in 0..200 {
            let frame = spinner.frame(i as f32 * 0.037);
            assert_eq!(frame.width(), spinner.width(), "{} at frame {i}", spinner.name());
        }
        assert_eq!(Spinner::from_name(spinner.name()), Some(spinner));
    }
}

#[test]
fn pop_settles() {
    assert!(tinsel::pop(0.0).is_some());
    assert!(tinsel::pop(1.0).is_none());
}

#[test]
fn badge_and_link_degrade_to_text() {
    plain();
    assert_eq!(badge("live", palette::ROSE), " live ");
    assert_eq!(tinsel::link("docs", "https://docs.rs"), "docs");
}

#[test]
fn table_sorts_by_value_with_units_and_keeps_highlights() {
    plain();
    let table = Table::new(["file", "size"])
        .row(["big", "1.2 MB"])
        .row(["small", "900 KB"])
        .row(["tiny", "12 B"])
        .highlight(0)
        .sort_by(1);
    assert_eq!(
        table.lines().plain(),
        [
            "  file      size",
            "  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈",
            "  tiny      12 B",
            "  small   900 KB",
            "✦ big     1.2 MB",
        ]
        .join("\n")
    );
}

#[test]
fn table_draws_a_border_and_footer() {
    plain();
    let table = Table::new(["app", "n"])
        .row(["tube", "3"])
        .footer(["total", "3"])
        .border(tinsel::Border::Rounded);
    assert_eq!(
        table.lines().plain(),
        [
            "  ╭──────────────╮",
            "  │   app     n  │",
            "  │   ┈┈┈┈┈┈┈┈┈  │",
            "  │   tube    3  │",
            "  │   ┈┈┈┈┈┈┈┈┈  │",
            "  │   total   3  │",
            "  ╰──────────────╯",
        ]
        .join("\n")
    );
}

#[test]
fn table_wraps_and_truncates_columns() {
    plain();
    let table = Table::new(["k", "v"])
        .row(["a", "one two three"])
        .wrap(1, 7)
        .row(["b", "abcdefghij"])
        .max_width(1, 7);
    // the later limit wins: truncate
    assert_eq!(
        table.lines().plain().lines().skip(2).collect::<Vec<_>>(),
        ["  a   one tw…", "  b   abcdef…"]
    );
    let wrapped = Table::new(["k", "v"]).row(["a", "one two three"]).wrap(1, 7);
    assert_eq!(
        wrapped.lines().plain().lines().skip(2).collect::<Vec<_>>(),
        ["  a   one two", "      three"]
    );
}

#[test]
fn code_puts_a_caret_under_the_column() {
    plain();
    let file = "a = 1\nb = x2\nc = 3\n";
    let code = Code::around(file, 2, 1).path("f.toml").caret(5, 2, "bad");
    assert_eq!(
        code.lines().plain(),
        [
            "    ╭─ f.toml:2",
            "  1 │ a = 1",
            "  2 › b = x2",
            "    │     ^^ bad",
            "  3 │ c = 3",
        ]
        .join("\n")
    );
}

#[test]
fn report_shows_code_before_the_hint() {
    plain();
    let report = Report::new("bad config")
        .code(Code::new("x = ?").highlight(1))
        .hint("fix it");
    assert_eq!(
        report.lines().plain(),
        ["  ✕  bad config", "     1 › x = ?", "  →  fix it"].join("\n")
    );
}

#[test]
fn histogram_counts_into_bins() {
    plain();
    let h = tinsel::histogram(&[1.0, 1.0, 2.0, 4.0], 3, 2);
    assert_eq!(h.plain(), ["  ██", "  ██████", "  ──────", "  1    4"].join("\n"));
}

#[test]
fn heatmap_shades_without_colour() {
    plain();
    let h = tinsel::heatmap(&[("a", &[0.0, 4.0]), ("bb", &[2.0, 1.0])]);
    assert_eq!(h.plain(), ["  a     ██", "  bb  ▒▒░░"].join("\n"));
}

#[test]
fn waveform_keeps_its_shape_and_width_as_it_plays() {
    plain();
    let start = tinsel::waveform(0.0, 30).to_string();
    assert_eq!(text::width(&start), 30);
    // without colour, playing through changes nothing: the shape is fixed, only the colour moves
    assert_eq!(tinsel::waveform(0.7, 30).to_string(), start);
    assert_eq!(tinsel::waveform(1.0, 30).to_string(), start);
    assert!(start.chars().all(|c| "▁▂▃▄▅▆▇█".contains(c)));
}

#[test]
fn table_titles_gaps_and_empty_cells() {
    plain();
    let table = Table::new(["name", "note"])
        .row(["a", "one two three"])
        .row(["bb", ""])
        .wrap(1, 7)
        .title("t")
        .border(tinsel::Border::Rounded);
    assert_eq!(
        table.lines().plain(),
        [
            "  ╭─ ✦ t ─────────────╮",
            "  │   name   note     │",
            "  │   ┈┈┈┈┈┈┈┈┈┈┈┈┈┈  │",
            "  │   a      one two  │",
            "  │          three    │",
            "  │                   │",
            "  │   bb     ·        │",
            "  ╰───────────────────╯",
        ]
        .join("\n")
    );
}

#[test]
fn square_tables_are_a_grid() {
    plain();
    let table = Table::new(["app", "n"])
        .row(["tube", "3"])
        .row(["x", "12"])
        .footer(["total", "15"])
        .border(tinsel::Border::Square);
    assert_eq!(
        table.lines().plain(),
        [
            "  ┌───────┬────┐",
            "  │ app   │  n │",
            "  ├───────┼────┤",
            "  │ tube  │  3 │",
            "  │ x     │ 12 │",
            "  ├───────┼────┤",
            "  │ total │ 15 │",
            "  └───────┴────┘",
        ]
        .join("\n")
    );
}

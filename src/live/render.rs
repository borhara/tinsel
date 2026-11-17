//! Drawing the steps, frame by frame.

use super::*;

pub(super) fn draw(shared: &Mutex<Shared>, settled: bool) {
    paint_frame(&mut lock(shared), settled);
}

/// Redraw the display over its last frame: the header the first time, then anything logged
/// since, then the steps.
pub(super) fn paint_frame(s: &mut Shared, settled: bool) {
    // a display finishing while a question is open still draws its last frame: that one is
    // never redrawn, so skipping it would lose it
    if s.abandoned || (s.paused > 0 && !settled) {
        return;
    }
    let mut buf = if s.drawn > 0 {
        format!("\x1b[{}F", s.drawn)
    } else {
        String::new()
    };
    let mut kept = Vec::new();
    if !std::mem::replace(&mut s.header_drawn, true) {
        kept = header(s);
    }
    kept.append(&mut s.logs);
    for line in &kept {
        buf += line;
        buf += "\x1b[K\n";
    }
    // no wrapping while drawing steps: an over-long line is cut at the edge rather than
    // taking up two rows and throwing off the next redraw
    buf += "\x1b[?7l";
    let lines = render(s, s.now(), content_width(), settled);
    let rest = &lines[s.committed.min(lines.len())..];
    // the cursor can only move back up as far as the top of the screen, so a display taller
    // than the terminal leaves its first rows in scrollback and redraws only what fits. The
    // last frame is drawn in full, since nothing is redrawn after it.
    let room = term::height().saturating_sub(1).max(1);
    let (commit, tail) = if !settled && rest.len() > room {
        rest.split_at(rest.len() - room)
    } else {
        (&rest[..0], rest)
    };
    for line in commit.iter().chain(tail) {
        buf += line;
        buf += "\x1b[K\n";
    }
    buf += "\x1b[J\x1b[?7h";
    s.drawn = tail.len();
    s.committed += commit.len();
    output::write(&buf);
}

pub(super) fn header(s: &Shared) -> Vec<String> {
    let header = format!("  {}  {}", paint(ROSE, &s.title), paint(SOFT, &s.target));
    vec![String::new(), header, String::new()]
}

/// What a frame needs to know beyond the step itself.
pub(super) struct Frame {
    spinner: Spinner,
    bar: BarStyle,
    now: Instant,
    t: f32,
    width: usize,
    settled: bool,
    /// If someone just came back to the window: when they left and when they returned.
    returned: Option<(Instant, Instant)>,
}

/// The steps (and the closing card, if there is one), without the header.
pub(super) fn render(s: &Shared, now: Instant, width: usize, settled: bool) -> Vec<String> {
    let frame = Frame {
        spinner: s.spinner,
        bar: s.bar,
        now,
        t: (now - s.t0).as_secs_f32(),
        width,
        settled,
        returned: s.focus.as_ref().and_then(|f| f.just_returned(now)),
    };
    let mut lines = Vec::new();
    for &idx in &s.roots {
        walk(s, idx, &frame, &mut lines);
    }
    lines.extend(s.footer.iter().cloned());
    // one entry a row, since that's how the next frame counts its way back up
    if lines.iter().any(|l| l.contains('\n')) {
        lines = lines.iter().flat_map(|l| l.split('\n')).map(str::to_string).collect();
    }
    lines
}

/// Text for a single row: line breaks in it become spaces.
pub(super) fn one_row(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains(['\n', '\r']) {
        text.replace("\r\n", " ").replace(['\n', '\r'], " ").into()
    } else {
        text.into()
    }
}

/// A step, then its sub-steps unless they've all quietly succeeded.
pub(super) fn walk(s: &Shared, idx: usize, frame: &Frame, out: &mut Vec<String>) {
    let step = &s.steps[idx];
    step_lines(step, frame, out);
    let folded = matches!(step.state, State::Done | State::Skipped) && step.children.iter().all(|&c| !troubled(s, c));
    if !folded {
        for &child in &step.children {
            walk(s, child, frame, out);
        }
    }
}

/// Whether a step or anything under it warned, failed, was cancelled or is waiting.
pub(super) fn troubled(s: &Shared, idx: usize) -> bool {
    let step = &s.steps[idx];
    matches!(
        step.state,
        State::Warn | State::Fail | State::Cancelled | State::Waiting
    ) || step.children.iter().any(|&c| troubled(s, c))
}

pub(super) fn step_lines(st: &StepData, f: &Frame, out: &mut Vec<String>) {
    // marks sit centred in the spinner's column, so labels stay put when a step finishes;
    // sub-steps start where their parent's label does
    let gutter = f.spinner.width();
    let nest = st.depth * (gutter + 2);
    let indent = " ".repeat(2 + nest + gutter + 2);
    let took = st
        .started
        .map(|b| fmt_secs((st.ended.unwrap_or(f.now) - b).as_secs_f64()))
        .unwrap_or_default();
    let row_with = |glyph: Line, label: Line, right: &str, right_color: Rgb| {
        let left = Line::new().space(2 + nest).append(glyph).space(2).append(label);
        let pad = f.width.saturating_sub(left.width() + right.chars().count() + 2).max(1);
        format!("{left}{}{}", " ".repeat(pad), paint(right_color, right))
    };
    let row = |glyph: Line, label: Line, right_color: Rgb| row_with(glyph, label, &took, right_color);
    let centred = |mark: Line| {
        Line::new()
            .space((gutter - 1) / 2)
            .append(mark)
            .space(gutter - 1 - (gutter - 1) / 2)
    };
    let label = one_row(&st.label);
    let plain = |glyph: &str, glyph_color: Rgb, label_color: Rgb| {
        (
            centred(Line::new().push(glyph_color, glyph)),
            Line::new().push(label_color, &label),
        )
    };
    // a note's first line says what happened; any more (a failed command's last output) are quieter
    let note = |color: Rgb, out: &mut Vec<String>| {
        for (i, line) in st.note.lines().enumerate() {
            out.push(format!("{indent}{}", paint(if i == 0 { color } else { MUTED }, line)));
        }
    };

    match st.state {
        State::Pending => {
            let (g, l) = plain("○", MUTED, MUTED);
            out.push(row(g, l, MUTED));
        }
        State::Active => {
            out.push(row(f.spinner.frame(f.t), widgets::shimmer(&label, f.t), MUTED));
            let usual = st.estimate.filter(|e| *e >= MIN_ESTIMATE);
            if let Some((p, text)) = &st.progress {
                let mut stats = paint(SOFT, &format!("{:>3.0}%  {}", p * 100.0, one_row(text)));
                if let Some(left) = st.started.and_then(|b| time_left((f.now - b).as_secs_f64(), *p)) {
                    stats += &paint(MUTED, &format!(" · {} left", approx(left)));
                }
                out.push(format!("{indent}{}  {stats}", f.bar.draw(*p, 30, f.t)));
            } else if let (Some(usual), Some(started)) = (usual, st.started) {
                let elapsed = (f.now - started).as_secs_f64();
                let (text, color) = if elapsed <= usual {
                    (
                        format!("usually {} · {} left", approx(usual), approx(usual - elapsed)),
                        SOFT,
                    )
                } else if elapsed <= usual * 1.25 {
                    (format!("usually {} · nearly there", approx(usual)), SOFT)
                } else {
                    (
                        format!("slower than usual · usually {}", approx(usual)),
                        mix(AMBER, SOFT, 0.35),
                    )
                };
                let bar = f.bar.draw(estimated_fraction(elapsed, usual), 30, f.t);
                out.push(format!("{indent}{bar}  {}", paint(color, &text)));
            }
            if st.progress.is_none() {
                for line in st.detail.lines() {
                    out.push(format!("{indent}{}", paint(MUTED, line)));
                }
            }
        }
        State::Done => {
            let mut age = st.ended.map_or(f32::MAX, |e| (f.now - e).as_secs_f32());
            // finished while you were away? twinkle again now you're back
            if let (Some((left, back)), Some(ended)) = (f.returned, st.ended) {
                if ended >= left && ended <= back {
                    age = (f.now - back).as_secs_f32();
                }
            }
            let mark = widgets::pop(age)
                .filter(|_| !f.settled)
                .unwrap_or_else(|| Line::new().push(ROSE, "●"));
            out.push(row(centred(mark), Line::new().push(TEXT, &label), MUTED));
        }
        State::Warn => {
            let (g, l) = plain("▲", AMBER, TEXT);
            out.push(row(g, l, MUTED));
            note(mix(AMBER, MUTED, 0.35), out);
        }
        State::Fail => {
            let (g, l) = plain("✕", RED, RED);
            out.push(row(g, l, RED));
            note(mix(RED, SOFT, 0.3), out);
        }
        State::Skipped => {
            let (g, l) = plain("–", MUTED, MUTED);
            out.push(row_with(g, l, "skipped", MUTED));
            note(MUTED, out);
        }
        State::Cancelled => {
            let (g, l) = plain("⊘", SOFT, SOFT);
            let right = if st.started.is_some() {
                took.as_str()
            } else {
                "cancelled"
            };
            out.push(row_with(g, l, right, MUTED));
            note(MUTED, out);
        }
        State::Waiting => {
            // a slow breath, so it reads as patient rather than busy
            let glow = if f.settled {
                AMBER
            } else {
                mix(AMBER, MUTED, 0.5 - 0.5 * (f.t * 2.4).cos())
            };
            let (g, l) = plain("◌", glow, TEXT);
            let waited = st
                .waiting_since
                .map(|w| fmt_secs((f.now - w).as_secs_f64()))
                .unwrap_or_default();
            out.push(row_with(g, l, &waited, MUTED));
            note(mix(AMBER, MUTED, 0.35), out);
        }
    }
}

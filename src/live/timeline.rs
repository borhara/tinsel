//! The timeline printed when a run ends.

use super::*;

/// When each step ran, as bars on one time axis: which steps overlapped, and where the time
/// went. Steps that never started are listed without a bar.
pub(super) fn timeline(s: &Shared, width: usize) -> Vec<String> {
    let now = s.now();
    let mut order = Vec::new();
    fn visit(s: &Shared, idx: usize, order: &mut Vec<usize>) {
        order.push(idx);
        for &c in &s.steps[idx].children {
            visit(s, c, order);
        }
    }
    for &r in &s.roots {
        visit(s, r, &mut order);
    }
    let end = s
        .steps
        .iter()
        .filter_map(|st| st.started.map(|b| st.ended.unwrap_or(now).max(b)))
        .max()
        .unwrap_or(now);
    let total = (end - s.t0).as_secs_f64().max(0.001);
    let label = |st: &StepData| format!("{}{}", "  ".repeat(st.depth), st.label);
    let label_w = order
        .iter()
        .map(|&i| crate::text::width(&label(&s.steps[i])))
        .max()
        .unwrap_or(0);
    let time_w = 7;
    let track = width.saturating_sub(2 + label_w + 2 + 2 + time_w).max(10);
    let col = |at: Instant| ((at - s.t0).as_secs_f64() / total * track as f64).round() as usize;

    let mut out = Vec::new();
    for &i in &order {
        let st = &s.steps[i];
        let name = crate::text::pad(&label(st), label_w);
        let Some(began) = st.started else {
            out.push(format!("  {}", paint(MUTED, &name)));
            continue;
        };
        let finished = st.ended.unwrap_or(now);
        let (a, b) = (col(began).min(track - 1), col(finished).min(track));
        let color = match st.state {
            State::Done => ROSE,
            State::Warn => AMBER,
            State::Fail => RED,
            State::Cancelled => MUTED,
            State::Waiting => mix(AMBER, MUTED, 0.35),
            _ => crate::style::palette::BLUSH,
        };
        let bar = "━".repeat(b.saturating_sub(a).max(1));
        let took = fmt_secs((finished - began).as_secs_f64());
        let gap = track.saturating_sub(a + crate::text::width(&bar));
        out.push(format!(
            "  {}  {}{}{}  {}",
            paint(if st.depth > 0 { SOFT } else { TEXT }, &name),
            " ".repeat(a),
            paint(color, &bar),
            " ".repeat(gap),
            paint(MUTED, &format!("{took:>time_w$}"))
        ));
    }
    let axis_pad = " ".repeat(2 + label_w + 2);
    out.push(format!(
        "{axis_pad}{}",
        paint(crate::style::palette::LINE, &"─".repeat(track))
    ));
    let total_label = fmt_secs(total);
    out.push(format!(
        "{axis_pad}{}{}",
        paint(MUTED, "0s"),
        paint(MUTED, &format!("{total_label:>w$}", w = track.saturating_sub(2)))
    ));
    out
}

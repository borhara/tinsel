//! Tiny charts that sit inline with other output.

use crate::block::Block;
use crate::style::palette::{AMBER, MUTED, RED, ROSE, SOFT, TEXT};
use crate::style::{RAMP, paint, sample};
use crate::text;
use crate::widgets::bar;

const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Values as a row of rising blocks, taller ones brighter: `▁▂▄▇▅▃`.
pub fn sparkline(values: &[f64]) -> String {
    let (lo, hi) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(f64::EPSILON);
    values
        .iter()
        .map(|&v| {
            let t = ((v - lo) / span) as f32;
            paint(sample(&RAMP, t), LEVELS[((t * 7.0).round() as usize).min(7)])
        })
        .collect()
}

/// A horizontal bar per label, scaled to the largest value, with the value at the end.
pub fn bars(rows: &[(&str, f64)], width: usize) -> Block {
    let label_w = rows.iter().map(|(l, _)| text::width(l)).max().unwrap_or(0);
    let max = rows.iter().map(|(_, v)| *v).fold(0.0, f64::max).max(f64::EPSILON);
    rows.iter()
        .map(|(label, v)| {
            let value = if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                format!("{v:.1}")
            };
            format!(
                "  {}  {}  {}",
                paint(TEXT, &text::pad(label, label_w)),
                bar((v / max) as f32, width),
                paint(SOFT, &value)
            )
        })
        .collect()
}

/// A labelled gauge that warms from rose to amber to red as it fills: disk, memory, quotas.
pub fn meter(label: &str, value: f64, max: f64, width: usize) -> String {
    let f = (value / max.max(f64::EPSILON)).clamp(0.0, 1.0);
    let color = if f < 0.6 {
        ROSE
    } else if f < 0.85 {
        AMBER
    } else {
        RED
    };
    let filled = (f * width as f64).round() as usize;
    format!(
        "  {}  {}{}  {}",
        paint(TEXT, label),
        paint(color, &"━".repeat(filled)),
        paint(crate::style::palette::TRACK, &"━".repeat(width - filled)),
        paint(if f < 0.6 { MUTED } else { color }, &format!("{:>3.0}%", f * 100.0))
    )
}

/// How values are spread: `bins` columns, `height` rows tall, with the range underneath.
///
/// ```
/// let latencies = [12.0, 14.0, 15.0, 15.5, 16.0, 18.0, 22.0, 41.0];
/// tinsel::histogram(&latencies, 8, 4).print();
/// ```
pub fn histogram(values: &[f64], bins: usize, height: usize) -> Block {
    let (bins, height) = (bins.max(1), height.max(1));
    if values.is_empty() {
        return Block::new();
    }
    let (lo, hi) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(f64::EPSILON);
    let mut counts = vec![0usize; bins];
    for &v in values {
        counts[(((v - lo) / span) * bins as f64).min(bins as f64 - 1.0) as usize] += 1;
    }
    let tallest = *counts.iter().max().unwrap_or(&1) as f64;
    let mut out = Block::new();
    for row in (0..height).rev() {
        let line: String = counts
            .iter()
            .map(|&c| {
                // how much of this row the bar fills, 0..=1
                let level = c as f64 / tallest * height as f64 - row as f64;
                let cell = if level >= 1.0 {
                    "█"
                } else if level > 0.0 {
                    LEVELS[((level * 8.0) as usize).min(7)]
                } else {
                    " "
                };
                let t = (row as f32 + 1.0) / height as f32;
                paint(sample(&RAMP, t), &cell.repeat(2))
            })
            .collect();
        out.push(format!("  {}", line.trim_end()));
    }
    let width = bins * 2;
    out.push(format!("  {}", paint(crate::style::palette::LINE, &"─".repeat(width))));
    let (lo, hi) = (number(lo), number(hi));
    let gap = width.saturating_sub(text::width(&lo) + text::width(&hi)).max(1);
    out.push(format!(
        "  {}{}{}",
        paint(MUTED, &lo),
        " ".repeat(gap),
        paint(MUTED, &hi)
    ));
    out
}

/// A grid of values as shaded cells, darkest for the smallest: activity by day, load by hour.
/// Each row is a label and its values; all rows share one scale.
///
/// ```
/// tinsel::heatmap(&[
///     ("mon", &[0.0, 2.0, 5.0, 9.0, 4.0]),
///     ("tue", &[1.0, 3.0, 8.0, 7.0, 2.0]),
/// ]).print();
/// ```
pub fn heatmap(rows: &[(&str, &[f64])]) -> Block {
    const SHADES: [&str; 5] = [" ", "░", "▒", "▓", "█"];
    let (lo, hi) = rows
        .iter()
        .flat_map(|(_, vs)| vs.iter())
        .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(f64::EPSILON);
    let label_w = rows.iter().map(|(l, _)| text::width(l)).max().unwrap_or(0);
    let plain = crate::style::color_mode() == crate::style::ColorMode::None;
    let ramp = [
        crate::style::palette::TRACK,
        crate::style::palette::BERRY,
        ROSE,
        crate::style::palette::SHINE,
    ];
    rows.iter()
        .map(|(label, values)| {
            let cells: String = values
                .iter()
                .map(|&v| {
                    let t = ((v - lo) / span) as f32;
                    if plain {
                        SHADES[((t * 4.0).round() as usize).min(4)].repeat(2)
                    } else {
                        paint(sample(&ramp, t), "██")
                    }
                })
                .collect();
            format!("  {}  {cells}", paint(SOFT, &text::pad(label, label_w)))
        })
        .collect()
}

/// A number short enough for an axis label.
fn number(v: f64) -> String {
    if v.fract() == 0.0 || v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

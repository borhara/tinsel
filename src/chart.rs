//! Tiny charts that sit inline with other output.

use crate::block::Block;
use crate::style::palette::{AMBER, MUTED, RED, ROSE, SOFT, TEXT};
use crate::style::{RAMP, paint, sample};
use crate::text;
use crate::widgets::{LEVELS, bar};

/// Values as a row of rising blocks, taller ones brighter: `▁▂▄▇▅▃`. A value that isn't a
/// finite number (NaN, for a reading that's missing) leaves a gap.
///
/// ```
/// tinsel::set_color_mode(tinsel::ColorMode::None);
/// assert_eq!(tinsel::sparkline(&[1.0, f64::NAN, 3.0]), "▁ █");
/// ```
pub fn sparkline(values: &[f64]) -> String {
    let (lo, hi) = range(values.iter()).unwrap_or((0.0, 0.0));
    values
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                return " ".to_string();
            }
            let t = position(v, lo, hi) as f32;
            paint(sample(&RAMP, t), LEVELS[((t * 7.0).round() as usize).min(7)])
        })
        .collect()
}

/// A horizontal bar per label, scaled to the largest value, with the value at the end. A
/// value that isn't a number gets an empty bar and `–`, and an infinite one a full bar.
pub fn bars(rows: &[(&str, f64)], width: usize) -> Block {
    let label_w = rows.iter().map(|(l, _)| text::width(l)).max().unwrap_or(0);
    // scaled to the largest finite value, so one infinity doesn't flatten the rest
    let max = rows
        .iter()
        .map(|(_, v)| *v)
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max)
        .max(f64::EPSILON);
    rows.iter()
        .map(|(label, v)| {
            let value = if v.is_nan() {
                "–".to_string()
            } else if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                format!("{v:.1}")
            };
            format!(
                "  {}  {}  {}",
                paint(TEXT, &text::pad(label, label_w)),
                bar(crate::widgets::fraction((v / max) as f32), width),
                paint(SOFT, &value)
            )
        })
        .collect()
}

/// A labelled gauge that warms from rose to amber to red as it fills: disk, memory, quotas.
/// A value that isn't a number, or a `max` that isn't a finite number above zero, leaves it
/// empty, with `–` for the percentage.
pub fn meter(label: &str, value: f64, max: f64, width: usize) -> String {
    // with nothing sensible to measure against, there's no fraction to show
    let known = !value.is_nan() && max.is_finite() && max > 0.0;
    let f = if known {
        crate::widgets::fraction((value / max) as f32) as f64
    } else {
        0.0
    };
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
        paint(
            if f < 0.6 { MUTED } else { color },
            &if !known {
                "   –".to_string()
            } else {
                format!("{:>3.0}%", f * 100.0)
            }
        )
    )
}

/// How values are spread: `bins` columns, `height` rows tall, with the range underneath.
/// Values that aren't finite numbers are left out.
///
/// ```
/// let latencies = [12.0, 14.0, 15.0, 15.5, 16.0, 18.0, 22.0, 41.0];
/// tinsel::histogram(&latencies, 8, 4).print();
/// ```
pub fn histogram(values: &[f64], bins: usize, height: usize) -> Block {
    let (bins, height) = (bins.max(1), height.max(1));
    let Some((lo, hi)) = range(values.iter()) else {
        return Block::new();
    };
    let mut counts = vec![0usize; bins];
    for &v in values.iter().filter(|v| v.is_finite()) {
        counts[(position(v, lo, hi) * bins as f64).min(bins as f64 - 1.0) as usize] += 1;
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
/// Each row is a label and its values; all rows share one scale. A value that isn't a
/// finite number leaves its cell empty.
///
/// ```
/// tinsel::heatmap(&[
///     ("mon", &[0.0, 2.0, 5.0, 9.0, 4.0]),
///     ("tue", &[1.0, 3.0, 8.0, 7.0, 2.0]),
/// ]).print();
/// ```
pub fn heatmap(rows: &[(&str, &[f64])]) -> Block {
    const SHADES: [&str; 5] = [" ", "░", "▒", "▓", "█"];
    let (lo, hi) = range(rows.iter().flat_map(|(_, vs)| vs.iter())).unwrap_or((0.0, 0.0));
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
                    let t = position(v, lo, hi) as f32;
                    if !v.is_finite() {
                        "  ".to_string()
                    } else if plain {
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

/// The smallest and largest of the finite values, if there are any.
fn range<'a>(values: impl Iterator<Item = &'a f64>) -> Option<(f64, f64)> {
    values
        .copied()
        .filter(|v| v.is_finite())
        .fold(None, |range, v| match range {
            None => Some((v, v)),
            Some((lo, hi)) => Some((v.min(lo), v.max(hi))),
        })
}

/// How far `v` is from `lo` to `hi`, from 0 to 1. Worked out in halves, so a range as wide
/// as `f64` allows doesn't overflow to infinity; a range of one value puts it at 0.
fn position(v: f64, lo: f64, hi: f64) -> f64 {
    let span = hi / 2.0 - lo / 2.0;
    if span > 0.0 {
        ((v / 2.0 - lo / 2.0) / span).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A number short enough for an axis label: a big one as `1.2e20`.
fn number(v: f64) -> String {
    if v.abs() >= 1e6 {
        format!("{v:.1e}")
    } else if v.fract() == 0.0 || v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_read_as_missing() {
        crate::set_color_mode(crate::ColorMode::None);
        assert!(meter("cpu", f64::NAN, 100.0, 4).ends_with("━━━━     –"));
        assert!(meter("cpu", 50.0, 100.0, 4).ends_with(" 50%"));
        let shown = bars(&[("a", 2.0), ("b", f64::NAN)], 4).plain();
        assert!(shown.ends_with("b  ━━━━  –"), "{shown}");
        assert_eq!(text::strip_ansi(&sparkline(&[f64::NAN])), " ");
        assert_eq!(sparkline(&[1.0, f64::INFINITY, 3.0]), "▁ █");
    }

    #[test]
    fn values_that_arent_numbers_are_left_out_of_histograms_and_heatmaps() {
        plain();
        assert!(histogram(&[f64::NAN, f64::INFINITY], 4, 4).is_empty());
        let with = histogram(&[1.0, 2.0, f64::NAN, 3.0, f64::NAN, f64::NAN], 3, 3).plain();
        assert_eq!(with, histogram(&[1.0, 2.0, 3.0], 3, 3).plain());
        let row: &[f64] = &[0.0, f64::NAN, 1.0];
        assert!(heatmap(&[("cpu", row)]).plain().ends_with("cpu      ██"));
    }

    fn plain() {
        crate::set_color_mode(crate::ColorMode::None);
    }

    #[test]
    fn bars_show_fractions_to_one_place() {
        plain();
        let out = bars(&[("a", 1.5), ("bb", 3.0)], 6).plain();
        assert!(out.contains("1.5") && out.contains(" 3"), "{out}");
    }

    #[test]
    fn meters_warm_as_they_fill() {
        let colour = |v| crate::style::with_color_mode(crate::ColorMode::TrueColor, || meter("disk", v, 100.0, 10));
        let (cool, warm, hot) = (colour(10.0), colour(70.0), colour(95.0));
        assert!(cool != warm && warm != hot && cool != hot);
    }

    #[test]
    fn histograms_draw_partial_rows_and_nothing_for_no_values() {
        plain();
        assert!(histogram(&[], 4, 4).is_empty());
        let out = histogram(&[1.0, 1.0, 1.0, 2.0, 3.0], 3, 2).plain();
        assert!(out.lines().any(|l| LEVELS.iter().any(|lv| l.contains(lv))), "{out}");
    }

    #[test]
    fn heatmaps_paint_cells_in_colour() {
        let row: &[f64] = &[0.0, 0.5, 1.0];
        let out = crate::style::with_color_mode(crate::ColorMode::TrueColor, || heatmap(&[("cpu", row)]));
        assert!(out.lines()[0].contains("██"));
    }

    #[test]
    fn axis_numbers_are_short() {
        assert_eq!(number(3.0), "3");
        assert_eq!(number(250.7), "251");
        assert_eq!(number(2.25), "2.2");
        assert_eq!(number(999_999.0), "999999");
        assert_eq!(number(1.234e20), "1.2e20");
        assert_eq!(number(-f64::MAX), "-1.8e308");
    }

    #[test]
    fn an_infinite_bar_is_full_and_the_rest_still_scale() {
        plain();
        let out = bars(&[("a", 2.0), ("b", 4.0), ("c", f64::INFINITY)], 4).plain();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].contains("a  ━━━━") && lines[1].contains("b  ━━━━"), "{out}");
        crate::style::with_color_mode(crate::ColorMode::TrueColor, || {
            let out = bars(&[("a", 2.0), ("b", 4.0), ("c", f64::INFINITY)], 4);
            let (half, full) = (bar(0.5, 4).to_string(), bar(1.0, 4).to_string());
            assert!(out.lines()[0].contains(&half), "{out:?}");
            assert!(
                out.lines()[1].contains(&full) && out.lines()[2].contains(&full),
                "{out:?}"
            );
        });
    }

    #[test]
    fn meters_without_a_sensible_max_are_empty() {
        plain();
        for max in [f64::NAN, 0.0, -5.0, f64::INFINITY] {
            assert!(meter("cpu", 5.0, max, 4).ends_with("━━━━     –"), "{max}");
        }
        let hot = crate::style::with_color_mode(crate::ColorMode::TrueColor, || meter("cpu", 5.0, f64::NAN, 4));
        assert!(!hot.contains("100%"), "{hot:?}");
    }

    #[test]
    fn the_widest_ranges_dont_overflow() {
        plain();
        let wide = [-f64::MAX, 0.0, f64::MAX];
        assert_eq!(sparkline(&wide), "▁▅█");
        let out = histogram(&wide, 3, 1).plain();
        assert_eq!(out, "  ██████\n  ──────\n  -1.8e308 1.8e308");
        let row: &[f64] = &wide;
        assert_eq!(heatmap(&[("x", row)]).plain(), "  x    ▒▒██");
    }

    #[test]
    fn heatmaps_shade_without_colour() {
        plain();
        let row: &[f64] = &[0.0, 1.0];
        assert!(heatmap(&[("cpu", row)]).plain().contains("██"));
    }
}

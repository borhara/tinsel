//! Box-drawing and block characters drawn as shapes for HTML, the way kitty, WezTerm and
//! xterm.js draw them, so lines join across cells and rows whatever font the page uses, and
//! a font without them can't knock the grid out of line.
//!
//! Straight lines and blocks are CSS backgrounds, a few dozen bytes a cell, as frames have a
//! lot of them; curves, diagonals and dashes, of which there are few, are SVG.

/// How `c` is drawn filling its cell, or `None` if the font should draw it.
pub(super) enum Shape {
    // background layers, each a solid rectangle placed and sized in the cell
    Css(String),
    // SVG inside a 1 by 1 box
    Svg(String),
}

// stroke widths in CSS pixels, the same whatever size the cell is drawn at
const LIGHT: f32 = 1.2;
const HEAVY: f32 = 2.4;

pub(super) fn shape(c: char) -> Option<Shape> {
    // lines from the middle of the cell to its edges: left, right, up, down
    const L: u8 = 1;
    const R: u8 = 2;
    const U: u8 = 4;
    const D: u8 = 8;
    let lines = |ends: u8, w: f32| {
        // a half line reaches half its width past the middle, so a corner has no notch
        let half = format!("calc(50% + {}px)", w / 2.0);
        let mut layers = Vec::new();
        match ends & (L | R) {
            x if x == L | R => layers.push(format!("0 50%/100% {w}px")),
            x if x == L => layers.push(format!("0 50%/{half} {w}px")),
            x if x == R => layers.push(format!("100% 50%/{half} {w}px")),
            _ => {}
        }
        match ends & (U | D) {
            x if x == U | D => layers.push(format!("50% 0/{w}px 100%")),
            x if x == U => layers.push(format!("50% 0/{w}px {half}")),
            x if x == D => layers.push(format!("50% 100%/{w}px {half}")),
            _ => {}
        }
        Shape::Css(
            layers
                .iter()
                .map(|at| format!("{SOLID} {at} no-repeat"))
                .collect::<Vec<_>>()
                .join(","),
        )
    };
    let block = |at: &str| Shape::Css(format!("{SOLID} {at} no-repeat"));
    let shade = |percent: u8| {
        Shape::Css(format!(
            "linear-gradient(color-mix(in srgb,currentColor {percent}%,transparent) 0 0)"
        ))
    };
    Some(match c {
        '─' => lines(L | R, LIGHT),
        '│' => lines(U | D, LIGHT),
        '┌' => lines(R | D, LIGHT),
        '┐' => lines(L | D, LIGHT),
        '└' => lines(U | R, LIGHT),
        '┘' => lines(U | L, LIGHT),
        '├' => lines(U | D | R, LIGHT),
        '┤' => lines(U | D | L, LIGHT),
        '┬' => lines(L | R | D, LIGHT),
        '┴' => lines(L | R | U, LIGHT),
        '┼' => lines(L | R | U | D, LIGHT),
        '╴' => lines(L, LIGHT),
        '╵' => lines(U, LIGHT),
        '╶' => lines(R, LIGHT),
        '╷' => lines(D, LIGHT),
        '━' => lines(L | R, HEAVY),
        '┃' => lines(U | D, HEAVY),
        '▁'..='█' => {
            let eighths = c as u32 - '▁' as u32 + 1;
            block(&format!("0 100%/100% {}%", eighths as f32 * 12.5))
        }
        '▏' => block("0 0/12.5% 100%"),
        '▎' => block("0 0/25% 100%"),
        '▌' => block("0 0/50% 100%"),
        '▐' => block("100% 0/50% 100%"),
        '░' => shade(25),
        '▒' => shade(50),
        '▓' => shade(75),
        '┄' => Shape::Svg(stroke("M0 .5H1", "stroke-dasharray=\"0.18 0.155\" ")),
        '┈' => Shape::Svg(stroke("M0 .5H1", "stroke-dasharray=\"0.13 0.12\" ")),
        // rounded corners curve from one edge's middle to the other's
        '╭' => Shape::Svg(stroke("M1 .5Q.5 .5 .5 1", "")),
        '╮' => Shape::Svg(stroke("M0 .5Q.5 .5 .5 1", "")),
        '╰' => Shape::Svg(stroke("M.5 0Q.5 .5 1 .5", "")),
        '╯' => Shape::Svg(stroke("M.5 0Q.5 .5 0 .5", "")),
        '╱' => Shape::Svg(stroke("M1 0L0 1", "")),
        _ => return None,
    })
}

// a solid rectangle in the text's colour
const SOLID: &str = "linear-gradient(currentColor 0 0)";

fn stroke(d: &str, extra: &str) -> String {
    format!(
        "<path d=\"{d}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"{LIGHT}\" {extra}\
         vector-effect=\"non-scaling-stroke\"/>"
    )
}

/// How many of `available` cells of `c` in a row go into one box: all of them for a line
/// across or a block, which are the same all along, and one at a time for anything else.
pub(super) fn run(c: char, available: usize) -> usize {
    if matches!(c, '─' | '━' | '▁'..='█' | '░' | '▒' | '▓') {
        available
    } else {
        1
    }
}

/// `n` cells of `c` drawn as `shape`. The characters are still there, unseen, so copying the
/// page gives the text back. A box is a whole cell tall, taller than the text beside it, so
/// it takes the background colour of what it's in to fill it.
pub(super) fn drawn(c: char, shape: &Shape, n: usize) -> String {
    let text: String = std::iter::repeat_n(c, n).collect();
    let cell = format!(
        "display:inline-block;width:{n}ch;height:1.25em;vertical-align:top;-webkit-text-fill-color:transparent"
    );
    match shape {
        Shape::Css(layers) => {
            format!("<span style=\"{cell};background:{layers};background-color:inherit\">{text}</span>")
        }
        Shape::Svg(svg) => format!(
            "<span style=\"{cell};position:relative;background:inherit\">{text}<svg viewBox=\"0 0 1 1\" \
             preserveAspectRatio=\"none\" aria-hidden=\"true\" style=\"position:absolute;inset:0;width:100%;\
             height:100%;overflow:visible\">{svg}</svg></span>"
        ),
    }
}

/// A character kept to `cols` cells, centred in them, whatever width the font gives it.
pub(super) fn boxed_cell(text: &str, cols: usize) -> String {
    format!("<span style=\"display:inline-block;width:{cols}ch;text-align:center;background:inherit\">{text}</span>")
}

/// Whether `c` should sit in a box exactly one cell wide: anything past the Latin
/// alphabets, which a font may well not have, so the browser borrows it from another font
/// whose width doesn't match.
pub(super) fn boxed(c: char) -> bool {
    c as u32 > 0x24f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(c: char) -> String {
        match shape(c) {
            Some(Shape::Css(s)) => s,
            _ => panic!("{c} isn't drawn with CSS"),
        }
    }

    #[test]
    fn straight_lines_and_blocks_are_backgrounds() {
        assert_eq!(css('─'), "linear-gradient(currentColor 0 0) 0 50%/100% 1.2px no-repeat");
        assert_eq!(css('┼').matches("linear-gradient").count(), 2, "across and down");
        assert!(
            css('┌').contains("100% 50%/calc(50% + 0.6px) 1.2px"),
            "a corner's arms meet with no notch"
        );
        assert!(css('━').contains("2.4px"));
        assert_eq!(css('▄'), "linear-gradient(currentColor 0 0) 0 100%/100% 50% no-repeat");
        assert!(css('▒').contains("color-mix(in srgb,currentColor 50%,transparent)"));
    }

    #[test]
    fn curves_diagonals_and_dashes_are_svg_and_text_is_left_to_the_font() {
        for c in ['╭', '╯', '╱', '┄'] {
            assert!(matches!(shape(c), Some(Shape::Svg(_))), "{c}");
        }
        for c in ['a', '●', '✧', '⣾', ' '] {
            assert!(shape(c).is_none(), "{c}");
        }
    }

    #[test]
    fn rows_of_a_line_or_block_are_one_box_and_keep_their_text() {
        assert_eq!(run('─', 5), 5);
        assert_eq!(run('█', 3), 3);
        assert_eq!(run('│', 5), 1, "a line down goes across one cell at a time");
        let edge = drawn('─', &shape('─').unwrap(), 5);
        assert!(edge.contains("width:5ch") && edge.contains("─────"), "{edge}");
        assert!(edge.len() < 300, "small, as frames have many: {}", edge.len());
    }

    #[test]
    fn only_characters_past_latin_are_boxed() {
        assert!(boxed('✧') && boxed('●') && boxed('→') && boxed('⣾'));
        assert!(!boxed('a') && !boxed('é') && !boxed('ł') && !boxed(' '));
    }
}

//! Cutting the screen into rectangles.

/// A rectangle of cells on the screen, counted from 0 at the top left.
///
/// Start from [`Frame::area`](super::Frame::area) and cut it up:
///
/// ```
/// use tinsel::app::{Rect, Size::*};
///
/// let screen = Rect::new(0, 0, 80, 24);
/// let [header, body, footer] = screen.rows([Fixed(3), Fill(1), Fixed(1)]);
/// let [sidebar, main] = body.columns_gap(1, [Fixed(24), Fill(1)]);
/// assert_eq!((sidebar.width, main.x, main.width), (24, 25, 55));
/// assert_eq!((header.height, body.height, footer.y), (3, 20, 23));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    /// The leftmost column.
    pub x: usize,
    /// The top row.
    pub y: usize,
    /// How many columns wide.
    pub width: usize,
    /// How many rows tall.
    pub height: usize,
}

/// How much of a split one piece gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Size {
    /// Exactly this many cells, if there's room.
    Fixed(usize),
    /// This percentage of the whole.
    Percent(usize),
    /// A share of whatever's left after the fixed and percentage pieces, by weight:
    /// `Fill(2)` gets twice what `Fill(1)` does.
    Fill(usize),
}

impl Rect {
    /// A rectangle at `x`, `y`, `width` columns by `height` rows.
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Rect { x, y, width, height }
    }

    /// Cut into rows, top to bottom.
    pub fn rows<const N: usize>(self, sizes: [Size; N]) -> [Rect; N] {
        self.rows_gap(0, sizes)
    }

    /// Cut into columns, left to right.
    pub fn columns<const N: usize>(self, sizes: [Size; N]) -> [Rect; N] {
        self.columns_gap(0, sizes)
    }

    /// Cut into rows with `gap` empty rows between them.
    pub fn rows_gap<const N: usize>(self, gap: usize, sizes: [Size; N]) -> [Rect; N] {
        let lens = split(self.height, gap, &sizes);
        let mut y = self.y;
        lens.map(|h| {
            let r = Rect::new(self.x, y, self.width, h);
            y += h + gap;
            r
        })
    }

    /// Cut into columns with `gap` empty columns between them.
    pub fn columns_gap<const N: usize>(self, gap: usize, sizes: [Size; N]) -> [Rect; N] {
        let lens = split(self.width, gap, &sizes);
        let mut x = self.x;
        lens.map(|w| {
            let r = Rect::new(x, self.y, w, self.height);
            x += w + gap;
            r
        })
    }

    /// The rectangle with `x` columns taken off each side and `y` rows off the top and bottom.
    #[must_use]
    pub fn shrink(self, x: usize, y: usize) -> Rect {
        Rect::new(
            self.x + x.min(self.width / 2),
            self.y + y.min(self.height / 2),
            self.width.saturating_sub(2 * x),
            self.height.saturating_sub(2 * y),
        )
    }

    /// A `width` by `height` rectangle in the middle of this one, for dialogs and popups.
    /// Clamped to fit.
    #[must_use]
    pub fn center(self, width: usize, height: usize) -> Rect {
        let (w, h) = (width.min(self.width), height.min(self.height));
        Rect::new(self.x + (self.width - w) / 2, self.y + (self.height - h) / 2, w, h)
    }

    /// One row of the rectangle, counted from 0. Empty if it's past the bottom.
    #[must_use]
    pub fn row(self, n: usize) -> Rect {
        if n >= self.height {
            return Rect::new(self.x, self.y.saturating_add(self.height), self.width, 0);
        }
        Rect::new(self.x, self.y + n, self.width, 1)
    }

    /// Whether the cell at `x`, `y` is inside: which panel a click landed in.
    pub fn contains(self, x: usize, y: usize) -> bool {
        // saturating, so a rectangle reaching past the edge of the numbers can't overflow
        (self.x..self.x.saturating_add(self.width)).contains(&x)
            && (self.y..self.y.saturating_add(self.height)).contains(&y)
    }

    /// Whether there's no room in it at all.
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The part of this rectangle that's also inside `other`.
    /// When they don't overlap, the result is empty and sits on `other`'s edge.
    #[must_use]
    pub fn intersect(self, other: Rect) -> Rect {
        let (other_right, other_bottom) = (
            other.x.saturating_add(other.width),
            other.y.saturating_add(other.height),
        );
        let right = self.x.saturating_add(self.width).min(other_right);
        let bottom = self.y.saturating_add(self.height).min(other_bottom);
        let x = self.x.max(other.x).min(other_right);
        let y = self.y.max(other.y).min(other_bottom);
        Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }
}

/// Lengths for each piece of `total`, with `gap` between them.
fn split<const N: usize>(total: usize, gap: usize, sizes: &[Size; N]) -> [usize; N] {
    let mut lens = [0; N];
    split_into(total, gap, sizes, &mut lens);
    lens
}

/// [`split`], into `lens`, one for each of `sizes`.
fn split_into(total: usize, gap: usize, sizes: &[Size], lens: &mut [usize]) {
    let n = sizes.len();
    let mut left = total.saturating_sub(gap * n.saturating_sub(1));
    let whole = left;
    for (len, size) in lens.iter_mut().zip(sizes) {
        let want = match *size {
            Size::Fixed(n) => n,
            Size::Percent(p) => whole * p.min(100) / 100,
            Size::Fill(_) => continue,
        };
        *len = want.min(left);
        left -= *len;
    }
    let weights: usize = sizes.iter().map(|s| if let Size::Fill(w) = s { *w } else { 0 }).sum();
    if weights == 0 {
        return;
    }
    let share = left;
    let last = sizes.iter().rposition(|s| matches!(s, Size::Fill(w) if *w > 0));
    for (i, (len, size)) in lens.iter_mut().zip(sizes).enumerate() {
        if let Size::Fill(w) = *size {
            // the last fill takes the remainder, so rounding never leaves a gap at the end
            *len = if Some(i) == last { left } else { share * w / weights };
            left -= *len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Size::*;
    use super::*;

    #[test]
    fn rectangles_out_at_the_edge_of_the_numbers_dont_overflow() {
        let huge = Rect::new(usize::MAX - 1, usize::MAX - 1, 10, 10);
        assert!(huge.contains(usize::MAX - 1, usize::MAX - 1));
        assert!(!huge.contains(0, 0));
        let screen = Rect::new(0, 0, 80, 24);
        assert!(huge.intersect(screen).is_empty());
        assert!(screen.intersect(huge).is_empty());
        assert_eq!(
            Rect::new(2, 3, usize::MAX, usize::MAX).intersect(screen),
            Rect::new(2, 3, 78, 21)
        );
        assert!(huge.row(20).is_empty());

        let mut frame = super::super::Frame::new(5, 1);
        assert_eq!(frame.print(Rect::new(1, 0, usize::MAX, 1), 0, "hello"), 4);
    }

    #[test]
    fn splits_fixed_percent_and_fill() {
        assert_eq!(split(100, 0, &[Fixed(10), Percent(20), Fill(1)]), [10, 20, 70]);
        assert_eq!(split(10, 0, &[Fill(1), Fill(2)]), [3, 7]);
        assert_eq!(split(11, 1, &[Fill(1), Fill(1)]), [5, 5]);
        // too little room: fixed pieces get what's there, in order
        assert_eq!(split(5, 0, &[Fixed(4), Fixed(4), Fill(1)]), [4, 1, 0]);
    }

    #[test]
    fn centers_and_shrinks() {
        let r = Rect::new(0, 0, 80, 24);
        assert_eq!(r.center(40, 10), Rect::new(20, 7, 40, 10));
        assert_eq!(r.center(100, 100), r);
        assert_eq!(r.shrink(2, 1), Rect::new(2, 1, 76, 22));
        assert_eq!(Rect::new(0, 0, 3, 1).shrink(2, 2).width, 0);
    }

    #[test]
    fn cuts_rows_and_columns_and_answers_questions() {
        let r = Rect::new(2, 1, 10, 4);
        assert_eq!(
            r.columns([Fixed(3), Fill(1)]),
            [Rect::new(2, 1, 3, 4), Rect::new(5, 1, 7, 4)]
        );
        assert_eq!(
            r.rows([Percent(50), Fill(1)]),
            [Rect::new(2, 1, 10, 2), Rect::new(2, 3, 10, 2)]
        );
        assert_eq!(r.row(1), Rect::new(2, 2, 10, 1));
        assert!(r.row(9).is_empty() && r.row(9).y == 5);
        assert!(r.contains(2, 1) && r.contains(11, 4));
        assert!(!r.contains(12, 1) && !r.contains(2, 5) && !r.contains(1, 1));
        assert!(Rect::new(0, 0, 0, 3).is_empty() && !r.is_empty());
        // weights of zero get nothing, and no fills at all leaves the rest unused
        assert_eq!(split(10, 0, &[Fill(0), Fill(1)]), [0, 10]);
        assert_eq!(split(10, 0, &[Fixed(2)]), [2]);
        assert_eq!(split(10, 0, &[Fill(0)]), [0]);
    }

    #[test]
    fn intersections_that_miss_stay_on_the_edge() {
        let screen = Rect::new(0, 0, 10, 3);
        assert_eq!(Rect::new(15, 2, 5, 1).intersect(screen), Rect::new(10, 2, 0, 1));
        assert_eq!(Rect::new(2, 9, 4, 4).intersect(screen), Rect::new(2, 3, 4, 0));
    }
}

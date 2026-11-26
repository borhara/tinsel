//! Decoding the pictures kitty can't take as files, JPEG and GIF, into pixels to send it
//! instead. Behind the `decode` feature, so apps that never show pictures needn't build it.
//!
//! Files come from anywhere, so a broken one is an error, never a panic.

mod gif;
mod jpeg;

use super::Format;

// past the biggest cameras, and a sign of a broken or hostile file
const MOST_PIXELS: usize = 80_000_000;

/// A picture as rows of red, green, blue and alpha bytes, top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}x{})", self.width, self.height)
    }
}

/// The pixels of a picture in `format`, turned as EXIF's `orientation` says, or why they
/// can't be had.
pub(crate) fn decode(bytes: &[u8], format: Format, orientation: u8) -> Result<Pixels, String> {
    let pixels = match format {
        Format::Jpeg => jpeg::decode(bytes)?,
        Format::Gif => gif::decode(bytes)?,
        Format::Png => return Err("PNG goes to the terminal as it is".into()),
    };
    Ok(pixels.oriented(orientation))
}

impl Pixels {
    /// The picture turned and mirrored as EXIF's orientation `o` says, so it's the way up it's
    /// meant to be seen: 1 is as it is, 6 a quarter turn clockwise, 8 one anticlockwise, and
    /// the others half turns and mirror images.
    pub fn oriented(self, o: u8) -> Pixels {
        if !(2..=8).contains(&o) {
            return self;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let (nw, nh) = if o >= 5 { (h, w) } else { (w, h) };
        let mut rgba = vec![0u8; self.rgba.len()];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = match o {
                    2 => (w - 1 - x, y),
                    3 => (w - 1 - x, h - 1 - y),
                    4 => (x, h - 1 - y),
                    5 => (y, x),
                    6 => (h - 1 - y, x),
                    7 => (h - 1 - y, w - 1 - x),
                    _ => (y, w - 1 - x),
                };
                rgba[(dy * nw + dx) * 4..][..4].copy_from_slice(&self.rgba[(y * w + x) * 4..][..4]);
            }
        }
        Pixels {
            width: nw as u32,
            height: nh as u32,
            rgba,
        }
    }

    /// Whether every pixel is solid, so the alpha bytes needn't be sent.
    pub fn opaque(&self) -> bool {
        self.rgba.chunks_exact(4).all(|p| p[3] == 255)
    }

    /// The picture made smaller to fit `width` by `height`, keeping its shape, each new
    /// pixel the average of those it covers; as it is if it already fits.
    pub fn shrunk_to(&self, width: u32, height: u32) -> Pixels {
        let scale = (f64::from(width) / f64::from(self.width))
            .min(f64::from(height) / f64::from(self.height))
            .min(1.0);
        if scale >= 1.0 {
            return self.clone();
        }
        let nw = ((f64::from(self.width) * scale).round() as u32).max(1);
        let nh = ((f64::from(self.height) * scale).round() as u32).max(1);
        let (w, h) = (self.width as usize, self.height as usize);
        let mut rgba = Vec::with_capacity(nw as usize * nh as usize * 4);
        for ny in 0..nh as usize {
            let (y0, y1) = (
                ny * h / nh as usize,
                ((ny + 1) * h / nh as usize).max(ny * h / nh as usize + 1),
            );
            for nx in 0..nw as usize {
                let (x0, x1) = (
                    nx * w / nw as usize,
                    ((nx + 1) * w / nw as usize).max(nx * w / nw as usize + 1),
                );
                let mut sum = [0u32; 4];
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        let p = &self.rgba[(y * w + x) * 4..][..4];
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += u32::from(*v);
                        }
                    }
                }
                let n = ((y1.min(h) - y0) * (x1.min(w) - x0)).max(1) as u32;
                rgba.extend(sum.iter().map(|s| ((s + n / 2) / n) as u8));
            }
        }
        Pixels {
            width: nw,
            height: nh,
            rgba,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // small pictures written by macOS's ImageIO, beside ImageIO's own reading of each as RGBA
    const FIXTURES: &[(&str, &[u8], &[u8], Format)] = &[
        (
            "baseline",
            include_bytes!("testdata/baseline.jpg"),
            include_bytes!("testdata/baseline.jpg.rgba"),
            Format::Jpeg,
        ),
        (
            "small",
            include_bytes!("testdata/small.jpg"),
            include_bytes!("testdata/small.jpg.rgba"),
            Format::Jpeg,
        ),
        (
            "progressive",
            include_bytes!("testdata/progressive.jpg"),
            include_bytes!("testdata/progressive.jpg.rgba"),
            Format::Jpeg,
        ),
        (
            "gray",
            include_bytes!("testdata/gray.jpg"),
            include_bytes!("testdata/gray.jpg.rgba"),
            Format::Jpeg,
        ),
        (
            "gif",
            include_bytes!("testdata/picture.gif"),
            include_bytes!("testdata/picture.gif.rgba"),
            Format::Gif,
        ),
    ];

    #[test]
    fn pictures_decode_as_imageio_reads_them() {
        for &(name, file, want, format) in FIXTURES {
            let p = decode(file, format, 1).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!((p.width, p.height), (37, 23), "{name}");
            let diffs: Vec<u32> = p
                .rgba
                .iter()
                .zip(want)
                .map(|(a, b)| u32::from(a.abs_diff(*b)))
                .collect();
            let mean = diffs.iter().sum::<u32>() as f64 / diffs.len() as f64;
            let far = diffs.iter().filter(|&&d| d > 24).count() as f64 / diffs.len() as f64;
            if format == Format::Gif {
                assert_eq!(p.rgba, want, "{name}: a GIF's colours are exact");
            } else {
                // decoders round and blend half-size colour a little differently, which only
                // shows much where colour jumps between neighbouring pixels
                assert!(
                    mean <= 2.0 && far < 0.02,
                    "{name}: mean {mean:.2}, {:.1}% far off",
                    far * 100.0
                );
            }
        }
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        for &(name, file, _, format) in FIXTURES {
            for len in 0..file.len() {
                let _ = decode(&file[..len], format, 1);
            }
            // bytes changed here and there through the file
            for at in (0..file.len()).step_by(7) {
                for flip in [0x01u8, 0x80, 0xff] {
                    let mut bad = file.to_vec();
                    bad[at] ^= flip;
                    let _ = decode(&bad, format, 1);
                }
            }
            let _ = name;
        }
    }

    #[test]
    fn each_orientation_turns_the_right_way() {
        // 3x2, numbered across then down: 1 2 3 / 4 5 6
        let p = Pixels {
            width: 3,
            height: 2,
            rgba: (1..=6u8).flat_map(|n| [n, 0, 0, 255]).collect(),
        };
        let shown = |o: u8| {
            let q = p.clone().oriented(o);
            let rows: Vec<Vec<u8>> = q
                .rgba
                .chunks(q.width as usize * 4)
                .map(|r| r.iter().step_by(4).copied().collect())
                .collect();
            rows
        };
        assert_eq!(shown(1), [vec![1, 2, 3], vec![4, 5, 6]]);
        assert_eq!(shown(2), [vec![3, 2, 1], vec![6, 5, 4]], "mirrored");
        assert_eq!(shown(3), [vec![6, 5, 4], vec![3, 2, 1]], "half a turn");
        assert_eq!(shown(4), [vec![4, 5, 6], vec![1, 2, 3]], "upside-down mirror");
        assert_eq!(shown(5), [vec![1, 4], vec![2, 5], vec![3, 6]], "transposed");
        assert_eq!(
            shown(6),
            [vec![4, 1], vec![5, 2], vec![6, 3]],
            "a quarter turn clockwise"
        );
        assert_eq!(shown(7), [vec![6, 3], vec![5, 2], vec![4, 1]], "transverse");
        assert_eq!(
            shown(8),
            [vec![3, 6], vec![2, 5], vec![1, 4]],
            "a quarter turn anticlockwise"
        );
    }

    #[test]
    fn a_rotated_photo_decodes_turned() {
        let rotated = decode(include_bytes!("testdata/rotated.jpg"), Format::Jpeg, 6).unwrap();
        let upright = decode(include_bytes!("testdata/baseline.jpg"), Format::Jpeg, 1).unwrap();
        assert_eq!((rotated.width, rotated.height), (23, 37));
        assert_eq!(rotated, upright.oriented(6), "the same picture, a quarter turn round");
    }

    #[test]
    fn shrinking_averages_and_keeps_the_shape() {
        // 4x2: a black left half and a white right half
        let mut rgba = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let p = Pixels {
            width: 4,
            height: 2,
            rgba,
        };
        let half = p.shrunk_to(2, 100);
        assert_eq!((half.width, half.height), (2, 1));
        assert_eq!(half.rgba, [0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(p.shrunk_to(10, 10), p, "as it is when it fits");
        assert!(p.opaque());
    }
}

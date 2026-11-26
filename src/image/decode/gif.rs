//! GIF's first frame, painted onto its screen: what a terminal showing it would show.

use super::Pixels;

pub(super) fn decode(b: &[u8]) -> Result<Pixels, String> {
    let mut r = Reader { b, at: 0 };
    let sig = r.take(6)?;
    if sig != b"GIF87a" && sig != b"GIF89a" {
        return Err("not a GIF".into());
    }
    let (width, height) = (r.u16()?, r.u16()?);
    let packed = r.byte()?;
    r.take(2)?; // background colour and aspect ratio
    let global = if packed & 0x80 != 0 {
        Some(r.take(3 << ((packed & 7) + 1))?)
    } else {
        None
    };
    let mut transparent = None;
    loop {
        match r.byte()? {
            0x21 => {
                let label = r.byte()?;
                let data = r.blocks()?;
                // a graphic control extension says which colour is see-through
                if label == 0xf9 && data.len() >= 4 && data[0] & 1 != 0 {
                    transparent = Some(data[3]);
                }
            }
            0x2c => {
                let (left, top, w, h) = (r.u16()?, r.u16()?, r.u16()?, r.u16()?);
                let packed = r.byte()?;
                let local = if packed & 0x80 != 0 {
                    Some(r.take(3 << ((packed & 7) + 1))?)
                } else {
                    None
                };
                let palette = local.or(global).ok_or("a GIF with no colours")?;
                let min_code = r.byte()?;
                let data = r.blocks()?;
                let indexes = lzw(&data, min_code, w * h)?;
                let rows = rows(h, packed & 0x40 != 0);
                let mut rgba = vec![0u8; width * height * 4];
                for (i, &index) in indexes.iter().enumerate() {
                    let (x, y) = (left + i % w, top + rows[i / w]);
                    if x >= width || y >= height || Some(index) == transparent {
                        continue;
                    }
                    let c = palette
                        .get(usize::from(index) * 3..usize::from(index) * 3 + 3)
                        .unwrap_or(&[0, 0, 0]);
                    rgba[(y * width + x) * 4..][..4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
                return Ok(Pixels {
                    width: width as u32,
                    height: height as u32,
                    rgba,
                });
            }
            0x3b => return Err("a GIF with no pictures in it".into()),
            other => return Err(format!("a GIF with a block it doesn't know: {other:#04x}")),
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let got = self.b.get(self.at..self.at + n).ok_or("a GIF cut short")?;
        self.at += n;
        Ok(got)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<usize, String> {
        let b = self.take(2)?;
        Ok(usize::from(u16::from_le_bytes([b[0], b[1]])))
    }

    // sub-blocks, each its length then that many bytes, up to an empty one
    fn blocks(&mut self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        loop {
            let n = usize::from(self.byte()?);
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(self.take(n)?);
        }
    }
}

// which row of the picture each row of data is: in order, or in interlaced passes
fn rows(h: usize, interlaced: bool) -> Vec<usize> {
    if !interlaced {
        return (0..h).collect();
    }
    [(0, 8), (4, 8), (2, 4), (1, 2)]
        .iter()
        .flat_map(|&(start, step)| (start..h).step_by(step))
        .collect()
}

/// GIF's variable-width LZW, giving `len` colour indexes; a picture whose data runs short is
/// filled out with the first colour, as browsers do.
fn lzw(data: &[u8], min: u8, len: usize) -> Result<Vec<u8>, String> {
    if !(1..=11).contains(&min) {
        return Err(format!("a GIF with a code size of {min}"));
    }
    const NONE: u16 = u16::MAX;
    let clear = 1u16 << min;
    let end = clear + 1;
    // each code as the code before its last byte, and that byte
    let mut prefix = vec![NONE; 4096];
    let mut suffix: Vec<u8> = (0..4096u32).map(|c| c as u8).collect();
    let mut size = u32::from(min) + 1;
    let mut next = end + 1;
    let mut prev = NONE;
    let mut out = Vec::with_capacity(len);
    let mut stack = Vec::new();
    let (mut acc, mut bits, mut at) = (0u32, 0u32, 0usize);
    while out.len() < len {
        while bits < size && at < data.len() {
            acc |= u32::from(data[at]) << bits;
            bits += 8;
            at += 1;
        }
        if bits < size {
            break;
        }
        let code = (acc & ((1 << size) - 1)) as u16;
        acc >>= size;
        bits -= size;
        if code == clear {
            size = u32::from(min) + 1;
            next = end + 1;
            prev = NONE;
            continue;
        }
        if code == end {
            break;
        }
        let first = if code < next {
            spell(code, &prefix, &suffix, &mut stack);
            stack.last().copied().unwrap_or(0)
        } else if code == next && prev != NONE {
            // the code being made: the string before, and its own first byte
            spell(prev, &prefix, &suffix, &mut stack);
            let first = stack.last().copied().unwrap_or(0);
            stack.insert(0, first);
            first
        } else {
            return Err("a GIF whose picture data is broken".into());
        };
        out.extend(stack.iter().rev());
        if prev != NONE && next < 4096 {
            prefix[usize::from(next)] = prev;
            suffix[usize::from(next)] = first;
            next += 1;
            if u32::from(next) == 1 << size && size < 12 {
                size += 1;
            }
        }
        prev = code;
    }
    out.resize(len, 0);
    Ok(out)
}

// the bytes of `code`'s string into `stack`, first one last
fn spell(code: u16, prefix: &[u16], suffix: &[u8], stack: &mut Vec<u8>) {
    stack.clear();
    let mut c = code;
    while c != u16::MAX && stack.len() < 4096 {
        stack.push(suffix[usize::from(c)]);
        c = prefix[usize::from(c)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tiny_gif() {
        // 3x2: red, green, blue on top, then blue, green, red; the standard 2-bit encoding
        let mut b = b"GIF89a\x03\x00\x02\x00\x81\x00\x00".to_vec();
        b.extend([255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
        b.extend(b"\x2c\x00\x00\x00\x00\x03\x00\x02\x00\x00\x02");
        // codes 4 (clear), 0, 1, 2, 2, 1, 0, 5 (end), three bits each
        let codes = [4u32, 0, 1, 2, 2, 1, 0, 5];
        let (mut acc, mut bits, mut data) = (0u32, 0u32, Vec::new());
        for (i, c) in codes.iter().enumerate() {
            // codes grow to 4 bits once the table reaches 8, after the third picture code
            let size = if i >= 4 { 4 } else { 3 };
            acc |= c << bits;
            bits += size;
            while bits >= 8 {
                data.push(acc as u8);
                acc >>= 8;
                bits -= 8;
            }
        }
        if bits > 0 {
            data.push(acc as u8);
        }
        b.push(data.len() as u8);
        b.extend(&data);
        b.extend(b"\x00\x3b");
        let p = decode(&b).unwrap();
        assert_eq!((p.width, p.height), (3, 2));
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        assert_eq!(&p.rgba[0..4], &red);
        assert_eq!(&p.rgba[8..12], &blue);
        assert_eq!(&p.rgba[12..16], &blue);
        assert_eq!(&p.rgba[20..24], &red);
    }

    #[test]
    fn interlaced_rows_come_in_passes() {
        assert_eq!(rows(10, true), [0, 8, 4, 2, 6, 1, 3, 5, 7, 9]);
        assert_eq!(rows(3, false), [0, 1, 2]);
    }

    #[test]
    fn broken_gifs_are_errors() {
        assert!(decode(b"GIF89a").is_err());
        assert!(
            decode(b"GIF89a\x01\x00\x01\x00\x00\x00\x00\x3b")
                .unwrap_err()
                .contains("no pictures")
        );
        assert!(decode(b"PNG").is_err());
    }
}

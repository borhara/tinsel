//! JPEG: baseline and progressive Huffman-coded pictures, in grey or colour, with any
//! chroma subsampling, which is nearly every JPEG there is. Arithmetic-coded, lossless,
//! hierarchical, 12-bit and CMYK pictures are rare enough to say so instead.

use super::Pixels;

pub(super) fn decode(b: &[u8]) -> Result<Pixels, String> {
    let mut d = Decoder::default();
    let mut at = 2;
    if !b.starts_with(&[0xff, 0xd8]) {
        return Err("not a JPEG".into());
    }
    loop {
        // markers may be padded with any number of 0xff
        while b.get(at) == Some(&0xff) && b.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        let (Some(&0xff), Some(&marker)) = (b.get(at), b.get(at + 1)) else {
            // a picture whose end marker is missing still shows what it has
            if d.frame.is_some() {
                break;
            }
            return Err("a JPEG cut short".into());
        };
        at += 2;
        match marker {
            0xd8 | 0x01 | 0xd0..=0xd7 => continue,
            0xd9 => break,
            _ => {}
        }
        let len = usize::from(u16::from_be_bytes([
            *b.get(at).ok_or(SHORT)?,
            *b.get(at + 1).ok_or(SHORT)?,
        ]));
        let seg = b.get(at + 2..at + len).ok_or(SHORT)?;
        at += len;
        match marker {
            0xc0..=0xc2 => d.frame(seg, marker == 0xc2)?,
            0xc3 | 0xc7 | 0xcb | 0xcf => return Err("a lossless JPEG, which tinsel can't read".into()),
            0xc5 | 0xc6 | 0xcd | 0xce => return Err("a hierarchical JPEG, which tinsel can't read".into()),
            0xc9 | 0xca => return Err("an arithmetic-coded JPEG, which tinsel can't read".into()),
            0xc4 => d.huffman(seg)?,
            0xdb => d.quant(seg)?,
            0xdd => {
                d.restart = usize::from(u16::from_be_bytes([
                    *seg.first().ok_or(SHORT)?,
                    *seg.get(1).ok_or(SHORT)?,
                ]))
            }
            0xee if seg.starts_with(b"Adobe") => d.adobe = seg.get(11).copied(),
            0xda => at = d.scan(seg, b, at)?,
            _ => {}
        }
    }
    d.finish()
}

const SHORT: &str = "a JPEG cut short";

// where each coefficient in a block's zigzag order sits in the 8x8 square
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63,
];

struct Decoder {
    frame: Option<Frame>,
    quant: [[u16; 64]; 4],
    dc: [Option<Huffman>; 4],
    ac: [Option<Huffman>; 4],
    restart: usize,
    adobe: Option<u8>,
}

impl Default for Decoder {
    fn default() -> Self {
        Decoder {
            frame: None,
            quant: [[1; 64]; 4],
            dc: Default::default(),
            ac: Default::default(),
            restart: 0,
            adobe: None,
        }
    }
}

struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    comps: Vec<Comp>,
    hmax: usize,
    vmax: usize,
    mcus_x: usize,
    mcus_y: usize,
}

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    quant: usize,
    // blocks across and down, padded out to whole MCUs
    bw: usize,
    bh: usize,
    // each block's coefficients, in zigzag order
    coefs: Vec<[i16; 64]>,
}

impl Decoder {
    fn frame(&mut self, s: &[u8], progressive: bool) -> Result<(), String> {
        if self.frame.is_some() {
            return Err("a JPEG with two pictures in it".into());
        }
        let (&precision, rest) = s.split_first().ok_or(SHORT)?;
        if precision != 8 {
            return Err(format!("a {precision}-bit JPEG, which tinsel can't read"));
        }
        let height = usize::from(u16::from_be_bytes([
            *rest.first().ok_or(SHORT)?,
            *rest.get(1).ok_or(SHORT)?,
        ]));
        let width = usize::from(u16::from_be_bytes([
            *rest.get(2).ok_or(SHORT)?,
            *rest.get(3).ok_or(SHORT)?,
        ]));
        let n = usize::from(*rest.get(4).ok_or(SHORT)?);
        if width == 0 || height == 0 {
            return Err("a JPEG whose height comes after the picture, which tinsel can't read".into());
        }
        if n != 1 && n != 3 {
            return Err(format!(
                "a JPEG with {n} colour channels: tinsel reads grey and colour ones"
            ));
        }
        let mut comps = Vec::new();
        for i in 0..n {
            let c = rest.get(5 + i * 3..8 + i * 3).ok_or(SHORT)?;
            let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) || c[2] > 3 {
                return Err("a JPEG with a broken frame header".into());
            }
            comps.push(Comp {
                id: c[0],
                h,
                v,
                quant: usize::from(c[2]),
                bw: 0,
                bh: 0,
                coefs: Vec::new(),
            });
        }
        let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
        let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
        let mcus_x = width.div_ceil(8 * hmax);
        let mcus_y = height.div_ceil(8 * vmax);
        // past the biggest cameras, and a sign of a broken or hostile file
        if width * height > 80_000_000 {
            return Err(format!("a JPEG of {width}x{height}, too big to show"));
        }
        for c in &mut comps {
            c.bw = mcus_x * c.h;
            c.bh = mcus_y * c.v;
            c.coefs = vec![[0; 64]; c.bw * c.bh];
        }
        self.frame = Some(Frame {
            width,
            height,
            progressive,
            comps,
            hmax,
            vmax,
            mcus_x,
            mcus_y,
        });
        Ok(())
    }

    fn huffman(&mut self, mut s: &[u8]) -> Result<(), String> {
        while let Some(&tc_th) = s.first() {
            let counts: [u8; 16] = s.get(1..17).ok_or(SHORT)?.try_into().map_err(|_| SHORT)?;
            let total: usize = counts.iter().map(|&c| usize::from(c)).sum();
            let values = s.get(17..17 + total).ok_or(SHORT)?;
            let table = Huffman::new(&counts, values)?;
            let slot = usize::from(tc_th & 3);
            if tc_th >> 4 == 0 {
                self.dc[slot] = Some(table);
            } else {
                self.ac[slot] = Some(table);
            }
            s = &s[17 + total..];
        }
        Ok(())
    }

    fn quant(&mut self, mut s: &[u8]) -> Result<(), String> {
        while let Some(&pq_tq) = s.first() {
            let wide = pq_tq >> 4 != 0;
            let size = if wide { 128 } else { 64 };
            let data = s.get(1..1 + size).ok_or(SHORT)?;
            let table = &mut self.quant[usize::from(pq_tq & 3)];
            for (k, q) in table.iter_mut().enumerate() {
                *q = if wide {
                    u16::from_be_bytes([data[k * 2], data[k * 2 + 1]])
                } else {
                    u16::from(data[k])
                };
            }
            s = &s[1 + size..];
        }
        Ok(())
    }

    /// Decodes the scan whose header is `s`, its data starting at `at` in `b`, and gives
    /// where the data ends.
    fn scan(&mut self, s: &[u8], b: &[u8], at: usize) -> Result<usize, String> {
        let frame = self
            .frame
            .as_mut()
            .ok_or("a JPEG with a scan before its frame header")?;
        let n = usize::from(*s.first().ok_or(SHORT)?);
        if n == 0 || n > 4 {
            return Err("a JPEG with a broken scan header".into());
        }
        let mut which = Vec::new();
        for i in 0..n {
            let c = s.get(1 + i * 2..3 + i * 2).ok_or(SHORT)?;
            let index = frame
                .comps
                .iter()
                .position(|x| x.id == c[0])
                .ok_or("a JPEG scan of a channel it doesn't have")?;
            which.push((index, usize::from(c[1] >> 4), usize::from(c[1] & 15)));
        }
        let tail = s.get(1 + n * 2..4 + n * 2).ok_or(SHORT)?;
        let (ss, se, ah, al) = (usize::from(tail[0]), usize::from(tail[1]), tail[2] >> 4, tail[2] & 15);
        if se > 63 || ss > se || al > 13 || (!frame.progressive && (ss != 0 || se != 63 || ah != 0 || al != 0)) {
            return Err("a JPEG with a broken scan header".into());
        }
        if frame.progressive && ss == 0 && se != 0 || frame.progressive && ss > 0 && n != 1 {
            return Err("a JPEG with a broken progressive scan".into());
        }
        let mut tables = Vec::new();
        for &(_, td, ta) in &which {
            let dc = self.dc[td & 3].as_ref();
            let ac = self.ac[ta & 3].as_ref();
            if ss == 0 && ah == 0 && dc.is_none() || se > 0 && ac.is_none() {
                return Err("a JPEG scan whose Huffman table is missing".into());
            }
            tables.push((dc, ac));
        }
        let mut bits = Bits::new(b, at);
        let mut preds = vec![0i32; n];
        let mut eobrun = 0u32;
        let scan = Scan { ss, se, ah, al };
        let decode_block = |bits: &mut Bits, coefs: &mut [i16; 64], i: usize, preds: &mut [i32], eobrun: &mut u32| {
            let (dc, ac) = tables[i];
            scan.block(bits, coefs, dc, ac, &mut preds[i], eobrun)
        };
        let restart = self.restart;
        let mut units = 0usize;
        let reset = |bits: &mut Bits, preds: &mut [i32], eobrun: &mut u32, units: &mut usize, total: usize| {
            *units += 1;
            if restart > 0 && *units % restart == 0 && *units < total {
                bits.restart();
                preds.iter_mut().for_each(|p| *p = 0);
                *eobrun = 0;
            }
        };
        if n == 1 {
            // one channel on its own goes block by block across what it covers, not by MCU
            let (ci, _, _) = which[0];
            let c = &mut frame.comps[ci];
            let cw = (frame.width * c.h).div_ceil(frame.hmax).div_ceil(8);
            let ch = (frame.height * c.v).div_ceil(frame.vmax).div_ceil(8);
            let total = cw * ch;
            for row in 0..ch {
                for col in 0..cw {
                    decode_block(&mut bits, &mut c.coefs[row * c.bw + col], 0, &mut preds, &mut eobrun)?;
                    reset(&mut bits, &mut preds, &mut eobrun, &mut units, total);
                }
            }
        } else {
            let total = frame.mcus_x * frame.mcus_y;
            for my in 0..frame.mcus_y {
                for mx in 0..frame.mcus_x {
                    for (i, &(ci, _, _)) in which.iter().enumerate() {
                        let c = &mut frame.comps[ci];
                        for v in 0..c.v {
                            for h in 0..c.h {
                                let index = (my * c.v + v) * c.bw + mx * c.h + h;
                                decode_block(&mut bits, &mut c.coefs[index], i, &mut preds, &mut eobrun)?;
                            }
                        }
                    }
                    reset(&mut bits, &mut preds, &mut eobrun, &mut units, total);
                }
            }
        }
        Ok(bits.end())
    }

    /// The picture from the coefficients the scans left: back to samples, then to colour.
    fn finish(self) -> Result<Pixels, String> {
        let frame = self.frame.ok_or("a JPEG with no picture in it")?;
        let (w, h) = (frame.width, frame.height);
        let mut planes = Vec::new();
        for c in &frame.comps {
            let q = &self.quant[c.quant];
            let stride = c.bw * 8;
            let mut plane = vec![0u8; stride * c.bh * 8];
            let mut block = [0f32; 64];
            for by in 0..c.bh {
                for bx in 0..c.bw {
                    let coefs = &c.coefs[by * c.bw + bx];
                    block.fill(0.0);
                    for k in 0..64 {
                        block[ZIGZAG[k]] = (i32::from(coefs[k]) * i32::from(q[k])) as f32;
                    }
                    idct(&mut block);
                    for y in 0..8 {
                        let row = &mut plane[(by * 8 + y) * stride + bx * 8..][..8];
                        for x in 0..8 {
                            row[x] = (block[y * 8 + x] + 128.0).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
            planes.push((plane, stride, c.h, c.v));
        }
        // three channels are YCbCr, unless Adobe's marker says they're RGB as they are
        let rgb = frame.comps.len() == 3 && (self.adobe == Some(0) || frame.comps.iter().map(|c| c.id).eq(*b"RGB"));
        let mut rgba = Vec::with_capacity(w * h * 4);
        // a channel at a pixel; one stored at less than full size is blended from the samples
        // around it, as libjpeg's fancy upsampling does, rather than repeated in blocks
        let sample = |(plane, stride, ch, cv): &(Vec<u8>, usize, usize, usize), x: usize, y: usize| {
            let at = |sx: usize, sy: usize| f32::from(plane[sy * stride + sx]);
            if *ch == frame.hmax && *cv == frame.vmax {
                return at(x, y);
            }
            // where the pixel's centre falls among the channel's samples, kept inside it
            let (fx, fy) = (frame.hmax as f32 / *ch as f32, frame.vmax as f32 / *cv as f32);
            let (cw, chh) = ((w * ch).div_ceil(frame.hmax), (h * cv).div_ceil(frame.vmax));
            let sx = ((x as f32 + 0.5) / fx - 0.5).clamp(0.0, (cw - 1) as f32);
            let sy = ((y as f32 + 0.5) / fy - 0.5).clamp(0.0, (chh - 1) as f32);
            let (x0, y0) = (sx as usize, sy as usize);
            let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(chh - 1));
            let (tx, ty) = (sx - x0 as f32, sy - y0 as f32);
            let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
            let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
            top * (1.0 - ty) + bottom * ty
        };
        for y in 0..h {
            for x in 0..w {
                let px = if planes.len() == 1 {
                    let g = sample(&planes[0], x, y).round() as u8;
                    [g, g, g]
                } else {
                    let (a, b, c) = (
                        sample(&planes[0], x, y),
                        sample(&planes[1], x, y),
                        sample(&planes[2], x, y),
                    );
                    if rgb {
                        [a.round() as u8, b.round() as u8, c.round() as u8]
                    } else {
                        let (cb, cr) = (b - 128.0, c - 128.0);
                        let to = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                        [
                            to(a + 1.402 * cr),
                            to(a - 0.344_136 * cb - 0.714_136 * cr),
                            to(a + 1.772 * cb),
                        ]
                    }
                };
                rgba.extend([px[0], px[1], px[2], 255]);
            }
        }
        Ok(Pixels {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}

#[derive(Clone, Copy)]
struct Scan {
    ss: usize,
    se: usize,
    ah: u8,
    al: u8,
}

impl Scan {
    /// One block's part of this scan: all of it for a baseline picture, or the coefficients
    /// and bits a progressive scan carries.
    fn block(
        self,
        bits: &mut Bits,
        coefs: &mut [i16; 64],
        dc: Option<&Huffman>,
        ac: Option<&Huffman>,
        pred: &mut i32,
        eobrun: &mut u32,
    ) -> Result<(), String> {
        let Scan { ss, se, ah, al } = self;
        if ss == 0 {
            if ah == 0 {
                let t = dc.ok_or(MISSING)?.decode(bits)?;
                if t > 15 {
                    return Err(BROKEN.into());
                }
                // wrapping, as only a broken file gets anywhere near the ends
                *pred = pred.wrapping_add(extend(bits.get(t), t));
                coefs[0] = pred.wrapping_shl(u32::from(al)) as i16;
            } else if bits.get(1) == 1 {
                coefs[0] |= 1 << al;
            }
            if se == 0 {
                return Ok(());
            }
        }
        let ac = ac.ok_or(MISSING)?;
        let start = ss.max(1);
        if ah == 0 {
            // the coefficients themselves: all at once for baseline, or a band of them
            if *eobrun > 0 {
                *eobrun -= 1;
                return Ok(());
            }
            let mut k = start;
            while k <= se {
                let rs = ac.decode(bits)?;
                let (r, s) = (usize::from(rs >> 4), rs & 15);
                if s == 0 {
                    if r < 15 {
                        // the rest of the band is zero, here and in the blocks after
                        *eobrun = (1 << r) - 1 + if r > 0 { bits.get(r as u8) } else { 0 };
                        break;
                    }
                    k += 16;
                    continue;
                }
                k += r;
                if k > 63 {
                    return Err(BROKEN.into());
                }
                coefs[k] = extend(bits.get(s), s).wrapping_shl(u32::from(al)) as i16;
                k += 1;
            }
            return Ok(());
        }
        // a refining scan: one more bit of each coefficient already there, and new ones
        let (p1, m1) = (1i16 << al, -1i16 << al);
        let refine = |bits: &mut Bits, c: &mut i16| {
            if bits.get(1) == 1 && *c & p1 == 0 {
                *c = c.wrapping_add(if *c >= 0 { p1 } else { m1 });
            }
        };
        let mut k = start;
        if *eobrun == 0 {
            while k <= se {
                let rs = ac.decode(bits)?;
                let (mut r, s) = (i32::from(rs >> 4), rs & 15);
                let mut value = 0;
                if s != 0 {
                    value = if bits.get(1) == 1 { p1 } else { m1 };
                } else if r != 15 {
                    *eobrun = (1 << r) + if r > 0 { bits.get(r as u8) } else { 0 };
                    break;
                }
                // past `r` zeros, refining what's set on the way, to where the new one goes
                while k <= se {
                    if coefs[k] != 0 {
                        refine(bits, &mut coefs[k]);
                    } else {
                        if r == 0 {
                            break;
                        }
                        r -= 1;
                    }
                    k += 1;
                }
                if value != 0 && k <= se {
                    coefs[k] = value;
                }
                k += 1;
            }
        }
        if *eobrun > 0 {
            while k <= se {
                if coefs[k] != 0 {
                    refine(bits, &mut coefs[k]);
                }
                k += 1;
            }
            *eobrun -= 1;
        }
        Ok(())
    }
}

const MISSING: &str = "a JPEG scan whose Huffman table is missing";
const BROKEN: &str = "a JPEG whose picture data is broken";

// a value of `t` bits, read as JPEG writes negatives
fn extend(v: u32, t: u8) -> i32 {
    if t == 0 {
        return 0;
    }
    let v = v as i32;
    if v < 1 << (t - 1) { v - (1 << t) + 1 } else { v }
}

/// A Huffman table: a lookup for codes up to 9 bits, and the slower way for longer ones.
struct Huffman {
    // for each 9-bit prefix, the code's length and value, or length 0 for a longer code
    fast: Vec<(u8, u8)>,
    max: [i32; 17],
    offset: [i32; 17],
    values: Vec<u8>,
}

impl Huffman {
    fn new(counts: &[u8; 16], values: &[u8]) -> Result<Huffman, String> {
        let mut fast = vec![(0, 0); 512];
        let mut max = [-1; 17];
        let mut offset = [0; 17];
        let (mut code, mut k) = (0i32, 0usize);
        for len in 1..=16 {
            let n = usize::from(counts[len - 1]);
            offset[len] = k as i32 - code;
            for _ in 0..n {
                if len <= 9 {
                    let first = (code << (9 - len)) as usize;
                    for slot in fast
                        .get_mut(first..first + (1 << (9 - len)))
                        .ok_or("a broken JPEG Huffman table")?
                    {
                        *slot = (len as u8, values[k]);
                    }
                }
                code += 1;
                k += 1;
            }
            if n > 0 {
                max[len] = code - 1;
            }
            if code > 1 << len {
                return Err("a broken JPEG Huffman table".into());
            }
            code <<= 1;
        }
        Ok(Huffman {
            fast,
            max,
            offset,
            values: values.to_vec(),
        })
    }

    fn decode(&self, bits: &mut Bits) -> Result<u8, String> {
        let (len, value) = self.fast[bits.peek(9) as usize];
        if len > 0 {
            bits.skip(len);
            return Ok(value);
        }
        for len in 10..=16u8 {
            let code = bits.peek(len) as i32;
            if code <= self.max[usize::from(len)] {
                bits.skip(len);
                let at = (code + self.offset[usize::from(len)]) as usize;
                return self.values.get(at).copied().ok_or_else(|| BROKEN.to_string());
            }
        }
        Err(BROKEN.into())
    }
}

/// The bits of a scan's data, with stuffed zero bytes taken out; past its end, or at a
/// marker, zeros.
struct Bits<'a> {
    b: &'a [u8],
    at: usize,
    acc: u64,
    n: u32,
    marker: bool,
}

impl<'a> Bits<'a> {
    fn new(b: &'a [u8], at: usize) -> Bits<'a> {
        Bits {
            b,
            at,
            acc: 0,
            n: 0,
            marker: false,
        }
    }

    fn fill(&mut self) {
        while self.n <= 56 {
            let mut byte = 0;
            if !self.marker && self.at < self.b.len() {
                byte = self.b[self.at];
                if byte == 0xff {
                    if self.b.get(self.at + 1) == Some(&0) {
                        self.at += 2;
                    } else {
                        self.marker = true;
                        byte = 0;
                    }
                } else {
                    self.at += 1;
                }
            }
            self.acc |= u64::from(byte) << (56 - self.n);
            self.n += 8;
        }
    }

    fn peek(&mut self, n: u8) -> u32 {
        if self.n < u32::from(n) {
            self.fill();
        }
        (self.acc >> (64 - u32::from(n))) as u32
    }

    fn skip(&mut self, n: u8) {
        self.acc <<= n;
        self.n = self.n.saturating_sub(u32::from(n));
    }

    fn get(&mut self, n: u8) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.peek(n);
        self.skip(n);
        v
    }

    // past a restart marker, starting afresh on a byte
    fn restart(&mut self) {
        self.acc = 0;
        self.n = 0;
        self.marker = false;
        while self.b.get(self.at) == Some(&0xff) && self.b.get(self.at + 1) == Some(&0xff) {
            self.at += 1;
        }
        while self.b.get(self.at) == Some(&0xff) && self.b.get(self.at + 1).is_some_and(|m| (0xd0..=0xd7).contains(m)) {
            self.at += 2;
        }
    }

    // where the data ends: at the marker after it
    fn end(&self) -> usize {
        let mut at = self.at;
        while at + 1 < self.b.len()
            && !(self.b[at] == 0xff && self.b[at + 1] != 0 && !(0xd0..=0xd7).contains(&self.b[at + 1]))
        {
            at += 1;
        }
        at
    }
}

// cos((2x + 1)uπ / 16), with C(u) folded in, for the 8-point inverse DCT
fn cosines() -> &'static [[f32; 8]; 8] {
    static TABLE: std::sync::OnceLock<[[f32; 8]; 8]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (x, row) in t.iter_mut().enumerate() {
            for (u, c) in row.iter_mut().enumerate() {
                let scale = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *c = scale * 0.5 * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos();
            }
        }
        t
    })
}

/// The 8x8 inverse DCT, rows then columns; a block of only its DC term is one shade.
fn idct(block: &mut [f32; 64]) {
    if block[1..].iter().all(|&c| c == 0.0) {
        let v = block[0] / 8.0;
        block.fill(v);
        return;
    }
    let t = cosines();
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            tmp[y * 8 + x] = (0..8).map(|u| t[x][u] * block[y * 8 + u]).sum();
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            block[y * 8 + x] = (0..8).map(|v| t[y][v] * tmp[v * 8 + x]).sum();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_values_come_back() {
        assert_eq!(extend(0, 1), -1);
        assert_eq!(extend(1, 1), 1);
        assert_eq!(extend(2, 3), -5);
        assert_eq!(extend(6, 3), 6);
        assert_eq!(extend(0, 0), 0);
    }

    #[test]
    fn a_block_of_one_shade() {
        let mut b = [0f32; 64];
        b[0] = 80.0;
        idct(&mut b);
        assert!(b.iter().all(|&v| (v - 10.0).abs() < 1e-4));
    }

    #[test]
    fn kinds_tinsel_cant_read_say_so() {
        let header = |marker: u8| {
            let mut b = vec![0xff, 0xd8, 0xff, marker, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0];
            b.extend([0xff, 0xd9]);
            b
        };
        assert!(decode(&header(0xc9)).unwrap_err().contains("arithmetic"));
        assert!(decode(&header(0xc3)).unwrap_err().contains("lossless"));
        let mut twelve = header(0xc0);
        twelve[6] = 12;
        assert!(decode(&twelve).unwrap_err().contains("12-bit"));
        assert!(decode(b"\xff\xd8").is_err());
        assert!(decode(b"nope").is_err());
    }
}

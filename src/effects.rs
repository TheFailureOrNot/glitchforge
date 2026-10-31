//! Pixel-space glitch and stylisation effects.
//! Every effect is isolated behind a named, seed-stable entry point.

use anyhow::{anyhow, Result};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, RgbImage};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const PALETTE: [[u8; 3]; 8] = [
    [0, 255, 0],
    [255, 0, 255],
    [0, 0, 255],
    [255, 0, 0],
    [0, 255, 255],
    [255, 255, 0],
    [90, 40, 255],
    [255, 60, 160],
];

#[derive(Clone, Debug)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Raster {
    pub fn from_rgb(image: &RgbImage) -> Self {
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            data: image.as_raw().clone(),
        }
    }
    pub fn into_rgb(self) -> RgbImage {
        RgbImage::from_raw(self.width as u32, self.height as u32, self.data)
            .expect("Raster always maintains an exact RGB buffer")
    }
    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        (y * self.width + x) * 3
    }
    #[inline]
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let i = self.idx(x.min(self.width - 1), y.min(self.height - 1));
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: [u8; 3]) {
        let i = self.idx(x, y);
        self.data[i..i + 3].copy_from_slice(&c);
    }
    #[inline]
    pub fn channel(&self, x: usize, y: usize, c: usize) -> u8 {
        self.data[self.idx(x, y) + c]
    }
    #[inline]
    pub fn set_channel(&mut self, x: usize, y: usize, c: usize, value: u8) {
        let i = self.idx(x, y) + c;
        self.data[i] = value;
    }
    pub fn pixels(&self) -> usize {
        self.width * self.height
    }
}

fn rng(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}
fn ri(r: &mut ChaCha8Rng, a: usize, b: usize) -> usize {
    if b <= a {
        a
    } else {
        r.gen_range(a..b)
    }
}
fn rf(r: &mut ChaCha8Rng, a: f32, b: f32) -> f32 {
    if b <= a {
        a
    } else {
        r.gen_range(a..b)
    }
}
fn chance(r: &mut ChaCha8Rng, p: f32) -> bool {
    r.gen::<f32>() < p.clamp(0.0, 1.0)
}
fn palette(r: &mut ChaCha8Rng) -> [u8; 3] {
    PALETTE[ri(r, 0, PALETTE.len())]
}
fn normal(r: &mut ChaCha8Rng) -> f32 {
    let u1 = (1.0 - r.gen::<f32>()).max(1.0e-7);
    let u2 = r.gen::<f32>();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}
fn strength(k: f32) -> f32 {
    if k.is_finite() {
        k.clamp(0.0, 3.0)
    } else {
        0.0
    }
}
fn wrap(value: isize, max: usize) -> usize {
    value.rem_euclid(max.max(1) as isize) as usize
}
fn luma(c: [u8; 3]) -> f32 {
    c[0] as f32 * 0.2126 + c[1] as f32 * 0.7152 + c[2] as f32 * 0.0722
}
fn mix(a: [u8; 3], b: [u8; 3], alpha: f32) -> [u8; 3] {
    let t = alpha.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| {
        (a[i] as f32 * (1.0 - t) + b[i] as f32 * t)
            .clamp(0.0, 255.0)
            .round() as u8
    })
}
fn copy_rect(
    dst: &mut Raster,
    src: &Raster,
    sx: usize,
    sy: usize,
    dx: usize,
    dy: usize,
    w: usize,
    h: usize,
) {
    for yy in 0..h {
        for xx in 0..w {
            dst.set(dx + xx, dy + yy, src.pixel(sx + xx, sy + yy));
        }
    }
}
fn blend_pixels(a: &Raster, b: &Raster, alpha: f32) -> Raster {
    let mut out = a.clone();
    for (i, px) in out.data.iter_mut().enumerate() {
        *px = (a.data[i] as f32 * (1.0 - alpha) + b.data[i] as f32 * alpha)
            .clamp(0.0, 255.0)
            .round() as u8;
    }
    out
}
pub fn blend(a: &Raster, b: &Raster, alpha: f32) -> Raster {
    blend_pixels(a, b, alpha.clamp(0.0, 1.0))
}

pub fn apply_realistic(id: &str, image: &Raster, k: f32, seed: u64) -> Result<Raster> {
    let k = strength(k);
    let mut r = rng(seed);
    let result = match id {
        "edge_corruption" => edge_corruption(image, &mut r, k),
        "block_displace" => block_displace(image, &mut r, k),
        "slice_shift" => slice_shift(image, &mut r, k),
        "stall_smear" => stall_smear(image, &mut r, k),
        "channel_shift" => channel_shift(image, &mut r, k),
        "chroma_bleed" => chroma_bleed(image, &mut r, k),
        "macroblock_decay" => macroblock_decay(image, &mut r, k),
        "bit_flip" => bit_flip(image, &mut r, k),
        "stride_drift" => stride_drift(image, &mut r, k),
        "jpeg_databend" => jpeg_databend(image, &mut r, k)?,
        "dropout_blocks" => dropout_blocks(image, &mut r, k),
        "streak_lines" => streak_lines(image, &mut r, k),
        "ghost" => ghost(image, &mut r, k),
        "scanline_noise" => scanline_noise(image, &mut r, k),
        "color_banding" => color_banding(image, &mut r, k),
        "chromatic_aberration" => chromatic_aberration(image, k),
        "analog_noise" => analog_noise(image, &mut r, k),
        "rolling_shutter" => rolling_shutter(image, &mut r, k),
        "interlace_artifact" => interlace_artifact(image, &mut r, k),
        _ => return Err(anyhow!("Unknown realistic effect: {id}")),
    };
    Ok(result)
}

pub fn apply_artistic(id: &str, image: &Raster, k: f32, seed: u64) -> Result<Raster> {
    let k = strength(k);
    let mut r = rng(seed);
    let result = match id {
        "art_duotone" => art_duotone(image, k),
        "art_wave_warp" => art_wave_warp(image, &mut r, k),
        "art_halftone" => art_halftone(image, &mut r, k),
        "art_scanlines" => art_scanlines(image, &mut r, k),
        "art_light_streaks" => art_light_streaks(image, &mut r, k),
        "art_neon_edges" => art_neon_edges(image, k),
        "art_pixel_sort" => art_pixel_sort(image, &mut r, k),
        "art_chromatic_echo" => art_chromatic_echo(image, k),
        _ => return Err(anyhow!("Unknown artistic effect: {id}")),
    };
    Ok(result)
}

fn edge_corruption(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let max_strip = (w as f32 * (0.018 + 0.025 * k).min(0.14)).max(2.0) as usize;
    for side in 0..2 {
        let mut y = 0;
        while y < h {
            let bh = (ri(r, 2, ((h as f32 * 0.055) as usize).max(4) + 1)).min(h - y);
            if chance(r, (0.28 + 0.22 * k).min(0.95)) {
                let bw = ri(r, 1, max_strip.max(2) + 1).min(w).max(1);
                let bx = if side == 0 { 0 } else { w - bw };
                let mode = ri(r, 0, 4);
                for yy in y..(y + bh) {
                    for xx in bx..(bx + bw) {
                        let c = match mode {
                            0 => {
                                let col = palette(r);
                                let g = 1.0 - (yy - y) as f32 / bh.max(1) as f32 * rf(r, 0.0, 0.65);
                                col.map(|v| (v as f32 * g) as u8)
                            }
                            1 => {
                                let sx = ri(r, 0, w);
                                let src = img.pixel(sx, yy.min(h - 1));
                                src.map(|v| (v as f32 * rf(r, 1.05, 2.3)).min(255.0) as u8)
                            }
                            2 => [r.gen(), r.gen(), r.gen()],
                            _ => {
                                let src = img.pixel(bx.min(w - 1), y);
                                src.map(|v| (v as u16 * 2).min(255) as u8)
                            }
                        };
                        out.set(xx, yy, c);
                    }
                }
                if chance(r, 0.5) && bh > 2 {
                    let shift = ri(r, 1, 10.min(bw + 1).max(2)) as isize;
                    for yy in (y..y + bh).step_by(2) {
                        let row: Vec<[u8; 3]> = (bx..bx + bw).map(|x| out.pixel(x, yy)).collect();
                        for x in 0..bw {
                            out.set(bx + x, yy, row[wrap(x as isize + shift, bw)]);
                        }
                    }
                }
            }
            y += bh.max(1);
        }
    }
    out
}

fn block_displace(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let source = img.clone();
    let mut out = img.clone();
    let count = ((12.0 * k * (1.0 + (w * h) as f32 / 1_500_000.0).sqrt()) as usize + 1).min(500);
    let base = ((w.min(h) / 28).max(4)).min(64);
    let choices = [base, (base / 2).max(4), (base * 2).min(96)];
    for _ in 0..count {
        let bs = choices[ri(r, 0, choices.len())];
        let bw = (bs * ri(r, 1, 5)).min(w);
        let bh = (bs * ri(r, 1, 4)).min(h);
        if bw < 2 || bh < 2 {
            continue;
        }
        let sx = ri(r, 0, w - bw + 1);
        let sy = ri(r, 0, h - bh + 1);
        let (dx, dy) = if chance(r, 0.72) {
            let dx = (sx as isize + bs as isize * ri(r, 0, 11) as isize - bs as isize * 5)
                .clamp(0, (w - bw) as isize) as usize;
            let dy = (sy as isize + bs as isize * ri(r, 0, 7) as isize - bs as isize * 3)
                .clamp(0, (h - bh) as isize) as usize;
            (dx, dy)
        } else {
            (ri(r, 0, w - bw + 1), ri(r, 0, h - bh + 1))
        };
        copy_rect(&mut out, &source, sx, sy, dx, dy, bw, bh);
    }
    out
}

fn slice_shift(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let count = (5.0 * k) as usize + 2;
    for _ in 0..count.min(300) {
        let bh = ri(r, 1, ((h as f32 * 0.035) as usize).max(2)).min(h).max(1);
        let y = ri(r, 0, h - bh + 1);
        let mag =
            (w as f32 * [0.003, 0.008, 0.018, 0.045][ri(r, 0, 4)] * (0.5 + k)).max(1.0) as isize;
        let shift = ri(r, 0, (mag * 2 + 1) as usize) as isize - mag;
        let mut rows = Vec::with_capacity(bh * w);
        for yy in y..y + bh {
            for x in 0..w {
                rows.push(out.pixel(x, yy));
            }
        }
        let mut shifted = Vec::with_capacity(rows.len());
        for yy in 0..bh {
            for x in 0..w {
                shifted.push(rows[yy * w + wrap(x as isize - shift, w)]);
            }
        }
        for yy in 0..bh {
            for x in 0..w {
                out.set(x, y + yy, shifted[yy * w + x]);
            }
        }
        if chance(r, 0.28) {
            let c = ri(r, 0, 3);
            let cmag = (w as f32 * 0.012 * k).max(1.0) as isize;
            let cshift = ri(r, 0, (cmag * 2 + 1) as usize) as isize - cmag;
            for yy in y..y + bh {
                let row: Vec<u8> = (0..w).map(|x| out.channel(x, yy, c)).collect();
                for x in 0..w {
                    out.set_channel(x, yy, c, row[wrap(x as isize - cshift, w)]);
                }
            }
        }
    }
    out
}

fn stall_smear(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((3.0 * k) as usize + 1).min(200) {
        let y = ri(r, 0, h);
        let len = ri(r, 2, ((h as f32 * (0.035 + 0.06 * k)) as usize).max(3))
            .min(h - y)
            .max(1);
        if chance(r, 0.65) {
            let x0 = ri(r, 0, w);
            let x1 = ri(r, x0 + 1, w + 1);
            let row: Vec<[u8; 3]> = (x0..x1).map(|x| out.pixel(x, y)).collect();
            for yy in y..y + len {
                for (j, x) in (x0..x1).enumerate() {
                    out.set(x, yy, row[j]);
                }
            }
        } else {
            let xw = ri(r, 1, ((w as f32 * 0.12) as usize).max(2)).min(w);
            let xs = ri(r, 0, w - xw + 1);
            for yy in y..y + len {
                let p = out.pixel(xs, yy);
                for x in xs..xs + xw {
                    out.set(x, yy, p);
                }
            }
        }
    }
    out
}

fn channel_shift(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let mx = (w as f32 * 0.0025 * k).max(1.0) as isize;
    let my = (h as f32 * 0.0015 * k).max(1.0) as isize;
    for c in [0, 2] {
        let dx = ri(r, 0, (mx * 2 + 1) as usize) as isize - mx;
        let dy = ri(r, 0, (my * 2 + 1) as usize) as isize - my;
        for y in 0..h {
            for x in 0..w {
                out.set_channel(
                    x,
                    y,
                    c,
                    img.channel(wrap(x as isize - dx, w), wrap(y as isize - dy, h), c),
                );
            }
        }
    }
    out
}

fn rgb_to_ycc(p: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
    [
        0.299 * r + 0.587 * g + 0.114 * b,
        128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b,
        128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b,
    ]
}
fn ycc_to_rgb(p: [f32; 3]) -> [u8; 3] {
    let (y, cb, cr) = (p[0], p[1] - 128.0, p[2] - 128.0);
    [
        (y + 1.402 * cr).clamp(0.0, 255.0) as u8,
        (y - 0.344136 * cb - 0.714136 * cr).clamp(0.0, 255.0) as u8,
        (y + 1.772 * cb).clamp(0.0, 255.0) as u8,
    ]
}
fn chroma_bleed(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut ycc: Vec<[f32; 3]> = (0..h)
        .flat_map(|y| (0..w).map(move |x| rgb_to_ycc(img.pixel(x, y))))
        .collect();
    for _ in 0..((2.0 * k) as usize + 1).min(100) {
        let bh = ri(r, 1, ((h as f32 * 0.22) as usize).max(2)).min(h);
        let y = ri(r, 0, h - bh + 1);
        let lim = (w as f32 * (0.004 + 0.012 * k)).max(1.0) as isize;
        let dx = ri(r, 0, (lim * 2 + 1) as usize) as isize - lim;
        for yy in y..y + bh {
            let row: Vec<[f32; 3]> = (0..w).map(|x| ycc[yy * w + x]).collect();
            for x in 0..w {
                let mut p = row[wrap(x as isize - dx, w)];
                p[2] = row[wrap(x as isize + dx, w)][2];
                ycc[yy * w + x] = p;
            }
        }
        if chance(r, 0.55) {
            let factor = [4, 8, 12, 16, 24][ri(r, 0, 5)];
            for yy in y..y + bh {
                for x in 0..w {
                    let sx = (x / factor * factor).min(w - 1);
                    let sy = (yy / factor * factor).min(h - 1);
                    let chroma = ycc[sy * w + sx];
                    ycc[yy * w + x][1] = chroma[1];
                    ycc[yy * w + x][2] = chroma[2];
                }
            }
        }
    }
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            out.set(x, y, ycc_to_rgb(ycc[y * w + x]));
        }
    }
    out
}

fn macroblock_decay(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((4.0 * k) as usize + 1).min(150) {
        let bw_min = ((w as f32 * 0.04) as usize).max(1);
        let bw_max = ((w as f32 * (0.12 + 0.22 * k.min(1.0))) as usize).max(2);
        let bh_min = ((h as f32 * 0.04) as usize).max(1);
        let bh_max = ((h as f32 * (0.10 + 0.20 * k.min(1.0))) as usize).max(2);
        let bw = ri(r, bw_min, bw_max + 1).min(w);
        let bh = ri(r, bh_min, bh_max + 1).min(h);
        let x = ri(r, 0, w - bw + 1);
        let y = ri(r, 0, h - bh + 1);
        let factor = [3, 4, 6, 8, 12][ri(r, 0, 5)];
        let small_w = (bw / factor).max(1);
        let small_h = (bh / factor).max(1);
        let mut small = vec![[0u8; 3]; small_w * small_h];
        for sy in 0..small_h {
            for sx in 0..small_w {
                let x0 = sx * bw / small_w;
                let x1 = ((sx + 1) * bw / small_w).max(x0 + 1);
                let y0 = sy * bh / small_h;
                let y1 = ((sy + 1) * bh / small_h).max(y0 + 1);
                let mut sum = [0u32; 3];
                let mut n = 0u32;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let p = out.pixel(x + xx, y + yy);
                        for c in 0..3 {
                            sum[c] += p[c] as u32;
                        }
                        n += 1;
                    }
                }
                small[sy * small_w + sx] = [0, 1, 2].map(|c| (sum[c] / n.max(1)) as u8);
            }
        }
        let shift = ri(r, 2, (3.0 + k * 1.4).min(7.0) as usize).clamp(2, 6);
        for yy in 0..bh {
            for xx in 0..bw {
                let mut p = small
                    [(yy / factor).min(small_h - 1) * small_w + (xx / factor).min(small_w - 1)];
                for c in 0..3 {
                    p[c] = (p[c] >> shift) << shift;
                }
                if chance(r, 0.28) {
                    for c in 0..3 {
                        p[c] = (p[c] as f32 * rf(r, 1.05, 1.32)).min(255.0) as u8;
                    }
                }
                out.set(x + xx, y + yy, p);
            }
        }
    }
    out
}

fn bit_flip(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let mut out = img.clone();
    let n = ((img.data.len() as f32 * 2.5e-5 * k) as usize + 1).min(150_000);
    if n == 0 {
        return out;
    }
    for _ in 0..n {
        let base = ri(r, 0, out.data.len());
        let cluster = ri(r, 1, 4);
        for j in 0..cluster {
            let idx = (base + j).min(out.data.len() - 1);
            let bit = 1u8 << ri(r, 2, 8);
            out.data[idx] ^= bit;
        }
    }
    out
}

fn stride_drift(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((2.0 * k) as usize + 1).min(100) {
        let y = ri(r, 0, h);
        let bh = ri(r, 1, ((h as f32 * (0.025 + 0.08 * k)) as usize).max(2))
            .min(h - y)
            .max(1);
        let max_shift = (w as f32 * (0.002 + 0.025 * k)).max(1.0) as isize;
        let dx = ri(r, 0, (max_shift * 2 + 1) as usize) as isize - max_shift;
        for yy in y..y + bh {
            let row: Vec<[u8; 3]> = (0..w).map(|x| out.pixel(x, yy)).collect();
            for x in 0..w {
                out.set(x, yy, row[wrap(x as isize - dx, w)]);
            }
            if chance(r, 0.35) {
                let row: Vec<[u8; 3]> = (0..w).map(|x| out.pixel(x, yy)).collect();
                for x in 0..w {
                    out.set(x, yy, [row[x][1], row[x][2], row[x][0]]);
                }
            }
        }
    }
    out
}

fn jpeg_databend(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Result<Raster> {
    let quality = (94.0 - 25.0 * k + ri(r, 0, 11) as f32 - 5.0).clamp(18.0, 96.0) as u8;
    let mut bytes = Vec::new();
    {
        let mut encoder = JpegEncoder::new_with_quality(&mut bytes, quality);
        encoder.encode(
            &img.data,
            img.width as u32,
            img.height as u32,
            ExtendedColorType::Rgb8,
        )?;
    }
    let decoded = image::load_from_memory(&bytes)?.to_rgb8();
    Ok(Raster::from_rgb(&decoded))
}

fn dropout_blocks(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((3.0 * k) as usize + 1).min(180) {
        let bw = ([8, 16, 32, 48][ri(r, 0, 4)] * ri(r, 1, 5)).min(w).max(1);
        let bh = ([8, 16, 24, 32][ri(r, 0, 4)] * ri(r, 1, 4)).min(h).max(1);
        let x = ri(r, 0, w - bw + 1);
        let y = ri(r, 0, h - bh + 1);
        let mode = ri(r, 0, 4);
        let mut mean = [0u32; 3];
        if mode == 1 {
            for yy in y..y + bh {
                for xx in x..x + bw {
                    let p = out.pixel(xx, yy);
                    for c in 0..3 {
                        mean[c] += p[c] as u32;
                    }
                }
            }
        }
        let avg = [0, 1, 2].map(|c| (mean[c] / (bw * bh).max(1) as u32) as u8);
        let col = palette(r).map(|v| (v as f32 * rf(r, 0.25, 0.85)) as u8);
        let gray = ri(r, 90, 166) as u8;
        for yy in y..y + bh {
            for xx in x..x + bw {
                out.set(
                    xx,
                    yy,
                    match mode {
                        0 => [0, 0, 0],
                        1 => avg,
                        2 => col,
                        _ => [gray; 3],
                    },
                );
            }
        }
    }
    out
}

fn streak_lines(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((5.0 * k) as usize + 2).min(400) {
        let y = ri(r, 0, h);
        let x0 = ri(r, 0, w);
        let length = ri(r, (w / 70).max(1), ((w as f32 * 0.16) as usize).max(2))
            .min(w - x0)
            .max(1);
        let thick = [1, 1, 2, 3][ri(r, 0, 4)].min(h - y).max(1);
        let color = if chance(r, 0.34) {
            palette(r)
        } else {
            [220, 224, 232]
        };
        let alpha = rf(r, 0.35, 0.85);
        for yy in y..y + thick {
            for x in x0..x0 + length {
                out.set(x, yy, mix(out.pixel(x, yy), color, alpha));
            }
        }
    }
    out
}

fn ghost(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((2.0 * k) as usize + 1).min(100) {
        let bh = ri(
            r,
            1,
            ((h as f32 * (0.08 + 0.22 * k.min(1.0))) as usize).max(2),
        )
        .min(h);
        let y = ri(r, 0, h - bh + 1);
        let max_dx = (w as f32 * 0.08 * k).max(1.0) as isize;
        let dx = ri(r, 0, (max_dx * 2 + 1) as usize) as isize - max_dx;
        let alpha = rf(r, 0.12, (0.22 + 0.12 * k).min(0.55));
        for yy in y..y + bh {
            for x in 0..w {
                out.set(
                    x,
                    yy,
                    mix(
                        out.pixel(x, yy),
                        img.pixel(wrap(x as isize - dx, w), yy),
                        alpha,
                    ),
                );
            }
        }
    }
    out
}

fn scanline_noise(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for y in 0..h {
        let gain = normal(r) * 0.012 * k;
        for x in 0..w {
            let mut p = img.pixel(x, y);
            for c in 0..3 {
                p[c] = (p[c] as f32 * (1.0 + gain)).clamp(0.0, 255.0) as u8;
            }
            out.set(x, y, p);
        }
    }
    let count = (h as f32 * (0.01 + 0.025 * k)) as usize;
    for _ in 0..count.min(500) {
        let y = ri(r, 0, h);
        let delta = rf(r, -18.0, 18.0) * k;
        for x in 0..w {
            let mut p = out.pixel(x, y);
            for c in 0..3 {
                p[c] = (p[c] as f32 + delta).clamp(0.0, 255.0) as u8;
            }
            out.set(x, y, p);
        }
    }
    out
}

fn color_banding(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let levels = (52.0 - 13.0 * k) as u16;
    let step = (256 / levels.max(5)).max(2);
    for _ in 0..((2.0 * k) as usize + 1).min(100) {
        let bw = ((w as f32 * rf(r, 0.12, 0.58)) as usize).max(1).min(w);
        let bh = ((h as f32 * rf(r, 0.10, 0.52)) as usize).max(1).min(h);
        let x = ri(r, 0, w - bw + 1);
        let y = ri(r, 0, h - bh + 1);
        for yy in y..y + bh {
            for xx in x..x + bw {
                let mut p = out.pixel(xx, yy);
                for c in 0..3 {
                    p[c] = (((p[c] as u16 / step) * step + step / 2).min(255)) as u8;
                }
                out.set(xx, yy, p);
            }
        }
    }
    out
}

fn chromatic_aberration(img: &Raster, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let max_shift = ((w.min(h) as f32 * 0.006 * k) as usize).max(1);
    let centre = (h - 1) as f32 / 2.0;
    for y in 0..h {
        let radial = (y as f32 - centre).abs() / centre.max(1.0);
        let dx = (radial * max_shift as f32).round() as isize;
        for x in 0..w {
            out.set_channel(x, y, 0, img.channel(wrap(x as isize - dx, w), y, 0));
            out.set_channel(x, y, 2, img.channel(wrap(x as isize + dx, w), y, 2));
        }
    }
    out
}

fn analog_noise(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let mut out = img.clone();
    for i in (0..out.data.len()).step_by(3) {
        let luma = normal(r) * (2.2 + 7.5 * k);
        for c in 0..3 {
            let chroma = normal(r) * (1.0 + 3.0 * k);
            out.data[i + c] = (img.data[i + c] as f32 + luma + chroma).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn rolling_shutter(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let amplitude = (w as f32 * 0.012 * k).max(1.0) as f32;
    let period = ((h as f32 * rf(r, 0.12, 0.4)) as usize).max(8);
    let phase = rf(r, 0.0, std::f32::consts::TAU);
    let step = (period / 20).max(1);
    let mut out = img.clone();
    for y0 in (0..h).step_by(step) {
        let y1 = (y0 + step).min(h);
        let dx = (((y0 as f32 / period as f32) * std::f32::consts::TAU + phase).sin() * amplitude)
            .round() as isize;
        for y in y0..y1 {
            for x in 0..w {
                out.set(x, y, img.pixel(wrap(x as isize - dx, w), y));
            }
        }
    }
    out
}

fn interlace_artifact(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    for _ in 0..((2.0 * k) as usize + 1).min(120) {
        let bh = ri(r, 2, ((h as f32 * 0.16) as usize).max(3)).min(h);
        let y = ri(r, 0, h - bh + 1);
        let max_dx = (w as f32 * 0.01 * k).max(1.0) as isize;
        let dx = ri(r, 0, (max_dx * 2 + 1) as usize) as isize - max_dx;
        let mut region: Vec<[u8; 3]> = (y..y + bh)
            .flat_map(|yy| (0..w).map(move |x| img.pixel(x, yy)))
            .collect();
        for yy in (1..bh).step_by(2) {
            for x in 0..w {
                region[yy * w + x] = img.pixel(wrap(x as isize - dx, w), y + yy);
            }
            if chance(r, 0.45) && bh > 2 {
                for x in 0..w {
                    region[yy * w + x] = region[(yy - 1) * w + x];
                }
            }
        }
        for yy in 0..bh {
            for x in 0..w {
                out.set(x, y + yy, region[yy * w + x]);
            }
        }
    }
    out
}

// Artistic layer ------------------------------------------------------------
fn art_duotone(img: &Raster, k: f32) -> Raster {
    let mut out = img.clone();
    let amount = (0.42 + 0.34 * k).clamp(0.0, 0.96);
    for i in (0..img.data.len()).step_by(3) {
        let lum = (img.data[i] as f32 * 0.2126
            + img.data[i + 1] as f32 * 0.7152
            + img.data[i + 2] as f32 * 0.0722)
            / 255.0;
        let ramp = lum.powf(0.78 + 0.10 * (1.0 - k.min(1.0)));
        let shadow = [5.0, 2.0, 11.0];
        let hi = [255.0, 18.0, 58.0];
        let mut mapped = [0.0; 3];
        for c in 0..3 {
            mapped[c] = shadow[c] + (hi[c] - shadow[c]) * ramp;
        }
        mapped[1] += ((lum - 0.55) * 17.0).clamp(0.0, 8.0);
        for c in 0..3 {
            out.data[i + c] = (img.data[i + c] as f32 * (1.0 - amount) + mapped[c] * amount)
                .clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn art_wave_warp(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let phase = rf(r, 0.0, std::f32::consts::TAU);
    let cy = rf(r, 0.018, 0.045);
    let cx = rf(r, 0.004, 0.012);
    let amp = (w as f32 * (0.0025 + 0.009 * k)).max(1.0);
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let disp = ((y as f32 * cy + x as f32 * cx + phase).sin() * amp)
                + ((y as f32 * cy * 0.43 - phase * 0.7).sin() * amp * 0.62);
            let sx = (x as f32 + disp).round().clamp(0.0, (w - 1) as f32) as usize;
            out.set(x, y, img.pixel(sx, y));
        }
    }
    out
}

fn art_halftone(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let spacing = (15.0 - k.min(2.5) * 2.2).max(5.0) as usize;
    let phase_x = ri(r, 0, spacing);
    let phase_y = ri(r, 0, spacing);
    let amount = (0.28 + 0.24 * k).clamp(0.0, 0.82);
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let dx = ((x + phase_x) % spacing) as f32 - spacing as f32 / 2.0;
            let dy = ((y + phase_y) % spacing) as f32 - spacing as f32 / 2.0;
            let lum = luma(img.pixel(x, y)) / 255.0;
            let radius = spacing as f32 * 0.43 * (1.0 - lum).max(0.0).powf(0.72);
            if (dx * dx + dy * dy).sqrt() <= radius {
                let old = img.pixel(x, y);
                out.set(
                    x,
                    y,
                    old.map(|v| (v as f32 * (1.0 - amount * 0.66)).round() as u8),
                );
            }
        }
    }
    out
}

fn art_scanlines(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let period = rf(r, 2.0, 4.2);
    let phase = rf(r, 0.0, std::f32::consts::TAU);
    let vertical_frequency = rf(r, 0.035, 0.075);
    let mut out = img.clone();
    for y in 0..h {
        let horizontal = 0.5 + 0.5 * ((y as f32 / period) * std::f32::consts::TAU + phase).cos();
        for x in 0..w {
            let vertical = 0.5 + 0.5 * (x as f32 * vertical_frequency + phase * 0.4).sin();
            let attenuation =
                (1.0 - (0.035 + 0.075 * k) * horizontal) * (1.0 - 0.012 * k * vertical);
            out.set(
                x,
                y,
                img.pixel(x, y)
                    .map(|v| (v as f32 * attenuation).clamp(0.0, 255.0) as u8),
            );
        }
    }
    out
}

fn art_light_streaks(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let count = (8.0 + 18.0 * k + (w * h) as f32 / 180_000.0).min(260.0) as usize;
    let mut row_weights = Vec::with_capacity(h);
    let mut total_weight = 0.0f32;
    for row in 0..h {
        let row_luma = (0..w).map(|x| luma(img.pixel(x, row))).sum::<f32>() / w as f32;
        total_weight += (0.22 + row_luma / 255.0).powi(2);
        row_weights.push(total_weight);
    }
    for _ in 0..count {
        let y = if chance(r, 0.82) {
            let target = rf(r, 0.0, total_weight.max(f32::EPSILON));
            row_weights
                .iter()
                .position(|weight| *weight >= target)
                .unwrap_or(h - 1)
        } else {
            ri(r, 0, h)
        };
        let x0 = ri(r, 0, w);
        let max_len = (w as f32 * rf(r, 0.035, 0.30) * (0.55 + 0.38 * k)).max(2.0) as usize;
        let x1 = (x0 + max_len).min(w - 1);
        let thick = [1, 1, 2, 2, 3, 4][ri(r, 0, 6)];
        let choice = rf(r, 0.0, 1.0);
        let color = if choice < 0.31 {
            [255, 8, 38]
        } else if choice < 0.65 {
            [255, 24, 62]
        } else if choice < 0.85 {
            [255, 62, 86]
        } else if choice < 0.96 {
            [255, 210, 220]
        } else {
            [255, 255, 255]
        };
        let alpha = (75.0 + 48.0 * k + ri(r, 0, 51) as f32 - 20.0).clamp(35.0, 235.0) / 255.0;
        for yy in y.saturating_sub(thick)..(y + thick + 1).min(h) {
            for x in x0..=x1 {
                out.set(x, yy, mix(out.pixel(x, yy), color, alpha * 0.22));
            }
        }
        for x in x0..=x1 {
            out.set(x, y, mix(out.pixel(x, y), color, alpha));
        }
        if chance(r, 0.42) {
            let gap = ri(r, 2, (w / 45).max(3));
            let sx = (x1 + gap).min(w - 1);
            let ex = (sx + (max_len as f32 * rf(r, 0.12, 0.55)) as usize).min(w - 1);
            for x in sx..=ex {
                out.set(x, y, mix(out.pixel(x, y), color, alpha * 0.72));
            }
        }
    }
    // A modest horizontal bloom keeps the treatment luminous without a full-frame blur.
    let source = out.clone();
    for y in 0..h {
        for x in 1..w.saturating_sub(1) {
            let a = source.pixel(x - 1, y);
            let b = source.pixel(x + 1, y);
            let p = source.pixel(x, y);
            out.set(
                x,
                y,
                mix(
                    p,
                    [
                        ((a[0] as u16 + b[0] as u16) / 2) as u8,
                        ((a[1] as u16 + b[1] as u16) / 2) as u8,
                        ((a[2] as u16 + b[2] as u16) / 2) as u8,
                    ],
                    0.06 + 0.025 * k,
                ),
            );
        }
    }
    out
}

fn art_neon_edges(img: &Raster, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let mut edges = vec![0.0f32; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let gx = luma(img.pixel(x + 1, y - 1))
                + 2.0 * luma(img.pixel(x + 1, y))
                + luma(img.pixel(x + 1, y + 1))
                - luma(img.pixel(x - 1, y - 1))
                - 2.0 * luma(img.pixel(x - 1, y))
                - luma(img.pixel(x - 1, y + 1));
            let gy = luma(img.pixel(x - 1, y + 1))
                + 2.0 * luma(img.pixel(x, y + 1))
                + luma(img.pixel(x + 1, y + 1))
                - luma(img.pixel(x - 1, y - 1))
                - 2.0 * luma(img.pixel(x, y - 1))
                - luma(img.pixel(x + 1, y - 1));
            edges[y * w + x] = (gx * gx + gy * gy).sqrt().min(255.0);
        }
    }
    let threshold = 34.0 - k.min(2.0) * 7.0;
    for y in 0..h {
        for x in 0..w {
            let edge = if edges[y * w + x] < threshold {
                0.0
            } else {
                (edges[y * w + x] * 1.22).min(255.0) / 255.0
            };
            let left = if x > 0 { edges[y * w + x - 1] } else { 0.0 };
            let right = if x + 1 < w { edges[y * w + x + 1] } else { 0.0 };
            let glow = ((edges[y * w + x] + left + right) / 3.0 / 255.0).min(1.0);
            let neon = [8.0 + 247.0 * edge, 4.0 + 19.0 * edge, 11.0 + 64.0 * edge];
            let base = img.pixel(x, y);
            let a = (0.18 + 0.16 * k).clamp(0.0, 0.68);
            let b = (0.08 + 0.09 * k).clamp(0.0, 0.38);
            let mut result = [0; 3];
            for c in 0..3 {
                let blended = base[c] as f32 * (1.0 - a) + neon[c] * a;
                let glow_col = [230.0, 0.0, 56.0][c];
                result[c] = (blended * (1.0 - b) + glow_col * glow * b).clamp(0.0, 255.0) as u8;
            }
            out.set(x, y, result);
        }
    }
    out
}

fn art_pixel_sort(img: &Raster, r: &mut ChaCha8Rng, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let mut out = img.clone();
    let rows = (3.0 + 5.0 * k) as usize;
    let iterations = (5.0 + 18.0 * k) as usize;
    for _ in 0..iterations.min(220) {
        let y = ri(r, 0, h);
        let x0 = ri(r, 0, w.saturating_sub(2).max(1));
        let length = ((w as f32 * rf(r, 0.025, 0.24) * (0.5 + 0.28 * k)) as usize)
            .max(3)
            .min(w - x0);
        for yy in y..(y + rows).min(h) {
            let mut segment: Vec<[u8; 3]> = (x0..x0 + length).map(|x| out.pixel(x, yy)).collect();
            segment.sort_by(|a, b| {
                luma(*a)
                    .partial_cmp(&luma(*b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if chance(r, 0.5) {
                segment.reverse();
            }
            for (i, px) in segment.into_iter().enumerate() {
                out.set(x0 + i, yy, px);
            }
        }
    }
    out
}

fn art_chromatic_echo(img: &Raster, k: f32) -> Raster {
    let (w, h) = (img.width, img.height);
    let dx = (w as f32 * (0.0025 + 0.0065 * k)).max(1.0) as isize;
    let dy = (h as f32 * (0.0008 + 0.0020 * k)).max(0.0) as isize;
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let base = img.pixel(x, y);
            let red = img.pixel(wrap(x as isize - dx, w), wrap(y as isize - dy, h));
            let blue = img.pixel(wrap(x as isize + dx, w), wrap(y as isize + dy, h));
            let mut p = base;
            p[0] = (base[0] as f32 * (1.0 - 0.16 * k) + red[0] as f32 * (0.16 * k))
                .clamp(0.0, 255.0) as u8;
            p[1] = (base[1] as f32 * (1.0 - 0.035 * k)
                + (red[1] as f32 + blue[1] as f32) * (0.0175 * k))
                .clamp(0.0, 255.0) as u8;
            p[2] = (base[2] as f32 * (1.0 - 0.13 * k) + blue[2] as f32 * (0.13 * k))
                .clamp(0.0, 255.0) as u8;
            out.set(x, y, p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use image::Rgb;

    #[test]
    fn all_effects_preserve_rgb_buffer_shape() {
        let image = RgbImage::from_fn(64, 48, |x, y| {
            Rgb([(x * 3) as u8, (y * 4) as u8, ((x + y) * 2) as u8])
        });
        let raster = Raster::from_rgb(&image);
        for e in presets::realistic_effects() {
            let out = apply_realistic(e.id, &raster, 0.8, 123).unwrap();
            assert_eq!(out.data.len(), raster.data.len(), "{}", e.id);
        }
        for e in presets::artistic_effects() {
            let out = apply_artistic(e.id, &raster, 0.8, 123).unwrap();
            assert_eq!(out.data.len(), raster.data.len(), "{}", e.id);
        }
    }

    #[test]
    fn all_effects_handle_minimal_nonzero_dimensions() {
        for (width, height) in [(1, 1), (1, 8), (8, 1), (3, 2)] {
            let image =
                RgbImage::from_fn(width, height, |x, y| Rgb([x as u8 * 31, y as u8 * 29, 127]));
            let raster = Raster::from_rgb(&image);
            for effect in presets::realistic_effects() {
                let output = apply_realistic(effect.id, &raster, 0.8, 7).unwrap();
                assert_eq!(
                    output.data.len(),
                    raster.data.len(),
                    "{} ({width}x{height})",
                    effect.id
                );
            }
            for effect in presets::artistic_effects() {
                let output = apply_artistic(effect.id, &raster, 0.8, 7).unwrap();
                assert_eq!(
                    output.data.len(),
                    raster.data.len(),
                    "{} ({width}x{height})",
                    effect.id
                );
            }
        }
    }
}

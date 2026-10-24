//! Every tone in the app is dither, never grey: ink dots on paper, as a
//! 1-bit screen drew them.
//!
//! - A fill shader for boxes: ordered (8×8 Bayer) dither of a flat density,
//!   a ramp or a radial falloff; and the two exact patterns,
//!   the 2 pt checker (50%) and classic Mac title-bar stripes. Adapted from
//!   the dither in zgui's `composer_states` example (MIT).
//! - Atkinson error diffusion for photos, on the CPU, at a chunky cell.
//! - A 5×7 pixel font for the wordmark.
use super::theme::{ACCENT, INK, PAPER};
use std::sync::Arc;
use zgui::{
    compose::prelude::*,
    decoration::{FillShader, ShaderUniforms},
    image::ImageData,
    scene::Color,
};

const WGSL: &str = r#"
fn d_fract(x: f32) -> f32 { return x - floor(x); }
fn d_b2(x: f32, y: f32) -> f32 { let fx = floor(x); let fy = floor(y); return d_fract(fx * 0.5 + fy * fy * 0.75); }
fn d_b4(x: f32, y: f32) -> f32 { return d_b2(0.5 * x, 0.5 * y) * 0.25 + d_b2(x, y); }
fn d_bayer(x: f32, y: f32) -> f32 { return d_b4(0.5 * x, 0.5 * y) * 0.25 + d_b2(x, y); }
fn d_hash(px: f32, py: f32) -> f32 {
    var x = d_fract(px * 123.34);
    var y = d_fract(py * 456.21);
    let d = x * (x + 45.32) + y * (y + 45.32);
    x = x + d; y = y + d;
    return d_fract(x * y);
}
fn d_noise(px: f32, py: f32) -> f32 {
    let ix = floor(px); let iy = floor(py);
    let fx = px - ix; let fy = py - iy;
    let ux = fx * fx * (3.0 - 2.0 * fx); let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = d_hash(ix, iy); let b = d_hash(ix + 1.0, iy);
    let c = d_hash(ix, iy + 1.0); let e = d_hash(ix + 1.0, iy + 1.0);
    return a + (b - a) * ux + (c + (e - c) * ux - (a + (b - a) * ux)) * uy;
}
fn d_fbm(px: f32, py: f32) -> f32 {
    var v = 0.0; var amp = 0.5; var x = px; var y = py;
    for (var i = 0; i < 4; i = i + 1) {
        v = v + amp * d_noise(x, y);
        x = x * 2.03 + 1.7; y = y * 2.03 + 9.2; amp = amp * 0.5;
    }
    return v;
}
// params[0]: ink. params[1]: mode, cell (pt), a, b. params[2]: c, d, e, f.
// Modes: 0 flat density a; 1 ramp a→b at angle c (degrees); 2 radial a (centre)
// →b (edge) about (c, d), radius e; 3 drifting noise, density a..b, scale c,
// speed d; 4 checker; 5 title-bar stripes (every other row).
fn fill(input: FillInput) -> vec4<f32> {
    let ink = input.params[0];
    let mode = input.params[1].x;
    let cell = max(input.params[1].y, 0.5);
    let a = input.params[1].z; let b = input.params[1].w;
    let c = input.params[2].x; let d = input.params[2].y; let e = input.params[2].z;
    let gx = floor(input.p.x / cell); let gy = floor(input.p.y / cell);
    let u = (gx + 0.5) * cell / max(input.size.x, 1.0);
    let w = (gy + 0.5) * cell / max(input.size.y, 1.0);
    var v = a;
    if (mode > 3.5 && mode < 4.5) {
        let on = d_fract((gx + gy) * 0.5) < 0.25;
        return vec4<f32>(ink.rgb, select(0.0, ink.a, on));
    }
    if (mode > 4.5) {
        let on = d_fract(gy * 0.5) < 0.25;
        return vec4<f32>(ink.rgb, select(0.0, ink.a, on));
    }
    if (mode > 0.5 && mode < 1.5) {
        let r = radians(c);
        let s = clamp((u - 0.5) * cos(r) + (w - 0.5) * sin(r) + 0.5, 0.0, 1.0);
        v = a + (b - a) * s;
    } else if (mode > 1.5 && mode < 2.5) {
        let m = input.size.x / max(input.size.y, 1.0);
        let s = min(1.0, length(vec2<f32>((u - c) * m, w - d)) / max(e, 0.001));
        v = a + (b - a) * s * s;
    } else if (mode > 2.5) {
        let m = input.size.x / max(input.size.y, 1.0);
        let n = d_fbm(u * m * c + input.time * d, w * c + input.time * d * 0.6);
        v = a + (b - a) * clamp((n - 0.25) * 2.0, 0.0, 1.0);
    }
    let threshold = d_bayer(gx, gy) + 0.5 / 64.0;
    let on = threshold <= clamp(v, 0.0, 1.0);
    return vec4<f32>(ink.rgb, select(0.0, ink.a, on));
}
"#;

thread_local! {
    static STILL: Arc<FillShader> = FillShader::new(WGSL, None);
}

fn still() -> Arc<FillShader> {
    STILL.with(Arc::clone)
}

fn rgba(c: Color) -> [f32; 4] {
    [
        f32::from(c.0) / 255.,
        f32::from(c.1) / 255.,
        f32::from(c.2) / 255.,
        f32::from(c.3) / 255.,
    ]
}

fn uniforms(color: Color, mode: f32, cell: f32, a: f32, b: f32, rest: [f32; 4]) -> ShaderUniforms {
    ShaderUniforms {
        time: 0.,
        params: [rgba(color), [mode, cell, a, b], rest, [0.; 4]],
    }
}

/// A dither of `density` (0 to 1) in `color`, at `cell` points.
pub fn level(color: Color, density: f32, cell: f32) -> Styles {
    Styles::new()
        .bg_shader(still())
        .shader_uniforms(uniforms(color, 0., cell, density, 0., [0.; 4]))
}

/// Densest (`centre`) in the middle, `edge` at `radius` (a share of the
/// height) and beyond.
pub fn radial(color: Color, centre: f32, edge: f32, radius: f32, cell: f32) -> Styles {
    Styles::new().bg_shader(still()).shader_uniforms(uniforms(
        color,
        2.,
        cell,
        centre,
        edge,
        [0.5, 0.5, radius, 0.],
    ))
}

/// The 2 pt checker: solweb's 50% tone.
pub fn checker(color: Color) -> Styles {
    Styles::new()
        .bg_shader(still())
        .shader_uniforms(uniforms(color, 4., 1., 0., 0., [0.; 4]))
}

/// Classic Mac title-bar stripes: a line every other point.
pub fn stripes(color: Color) -> Styles {
    Styles::new()
        .bg_shader(still())
        .shader_uniforms(uniforms(color, 5., 1., 0., 0., [0.; 4]))
}

/// A view filled with `styles` (a dither above).
pub fn patch(styles: Styles) -> View {
    div().style(styles)
}

// Photos ----------------------------------------------------------------------

/// `rgba` (`width` × `height`) as ink on paper by Atkinson's error diffusion,
/// in cells of `cell` pixels, each drawn as a solid `cell`-square: a 1-bit
/// plate. The image keeps its size.
pub fn atkinson(rgba: &[u8], width: u32, height: u32, cell: u32) -> Arc<ImageData> {
    let (w, h, cell) = (width as usize, height as usize, cell.max(1) as usize);
    let (cw, ch) = (w.div_ceil(cell), h.div_ceil(cell));
    // Each cell's average luminance, gently lifted so shadows keep detail.
    let mut lum = vec![0f32; cw * ch];
    for cy in 0..ch {
        for cx in 0..cw {
            let (mut sum, mut n) = (0f32, 0f32);
            for y in cy * cell..((cy + 1) * cell).min(h) {
                for x in cx * cell..((cx + 1) * cell).min(w) {
                    let i = (y * w + x) * 4;
                    let l = 0.2126 * f32::from(rgba[i])
                        + 0.7152 * f32::from(rgba[i + 1])
                        + 0.0722 * f32::from(rgba[i + 2]);
                    sum += l;
                    n += 1.;
                }
            }
            lum[cy * cw + cx] = (sum / n.max(1.) / 255.).powf(0.8);
        }
    }
    // Atkinson: six neighbours take an eighth of the error each.
    let mut on = vec![false; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let i = y * cw + x;
            let old = lum[i];
            let dark = old < 0.5;
            on[i] = dark;
            let err = (old - if dark { 0. } else { 1. }) / 8.;
            for (dx, dy) in [(1, 0), (2, 0), (-1, 1), (0, 1), (1, 1), (0, 2)] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx >= 0 && (nx as usize) < cw && (ny as usize) < ch {
                    lum[ny as usize * cw + nx as usize] += err;
                }
            }
        }
    }
    let (ink, paper) = (INK, PAPER);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let c = if on[(y / cell) * cw + x / cell] {
                ink
            } else {
                paper
            };
            let i = (y * w + x) * 4;
            out[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
        }
    }
    Arc::new(ImageData::new(width, height, out).expect("a dithered plate"))
}

// The wordmark ----------------------------------------------------------------

/// Five by seven bits a glyph, top row first.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        _ => [0; 7],
    }
}

/// `text` in the pixel font, `bit` points a pixel, with a 50% shadow one
/// pixel down and right: how solweb draws its wordmark on a canvas.
pub fn wordmark(text: &str, bit: f32) -> View {
    // Drawn at four device pixels a bit: crisp at 1× and 2×.
    const PX: usize = 4;
    let glyphs: Vec<[u8; 7]> = text.chars().map(glyph).collect();
    let (gap, n) = (2usize, glyphs.len());
    let bits_w = n * 5 + n.saturating_sub(1) * gap + 1;
    let bits_h = 7 + 1;
    let (w, h) = (bits_w * PX, bits_h * PX);
    let mut out = vec![0u8; w * h * 4];
    let mut put = |bx: usize, by: usize, c: Color| {
        for y in by * PX..(by + 1) * PX {
            for x in bx * PX..(bx + 1) * PX {
                let i = (y * w + x) * 4;
                out[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, c.3]);
            }
        }
    };
    let lit = |bx: usize, by: usize| -> bool {
        if by >= 7 {
            return false;
        }
        let g = bx / (5 + gap);
        let col = bx % (5 + gap);
        g < n && col < 5 && glyphs[g][by] & (1 << (4 - col)) != 0
    };
    for by in 0..bits_h {
        for bx in 0..bits_w {
            // The letter, and its shadow down-right on every other bit.
            let shadow = bx > 0 && by > 0 && lit(bx - 1, by - 1) && (bx + by) % 2 == 0;
            if lit(bx, by) || shadow {
                put(bx, by, INK);
            }
        }
    }
    let image = Arc::new(ImageData::new(w as u32, h as u32, out).expect("the wordmark"));
    zgui::compose::image(text.to_owned(), image).size(bits_w as f32 * bit, bits_h as f32 * bit)
}

// The little guy --------------------------------------------------------------

/// A small file, smiling: `#` ink, `.` paper, `o` a cheek, blank clear.
const GUY: [&str; 16] = [
    "########    ",
    "#......##   ",
    "#......#.#  ",
    "#......#..# ",
    "#......#####",
    "#..........#",
    "#..#....#..#",
    "#..#....#..#",
    "#.o......o.#",
    "#..#....#..#",
    "#...####...#",
    "#..........#",
    "#..........#",
    "############",
    "  #     #   ",
    " ##    ##   ",
];

/// The little file who lives on the front page, `bit` points a pixel,
/// with the wordmark's dithered shadow. `blink` closes his eyes.
pub fn little_guy(blink: bool, bit: f32) -> View {
    const PX: usize = 4;
    let rows: Vec<Vec<u8>> = GUY
        .iter()
        .enumerate()
        .map(|(y, row)| {
            let mut row = row.as_bytes().to_vec();
            // Eyes shut: only their lower bit stays.
            if blink && y == 6 {
                row[3] = b'.';
                row[8] = b'.';
            }
            row
        })
        .collect();
    let (bits_w, bits_h) = (rows[0].len() + 1, rows.len() + 1);
    let at = |bx: usize, by: usize| {
        rows.get(by)
            .and_then(|r| r.get(bx))
            .copied()
            .unwrap_or(b' ')
    };
    let (w, h) = (bits_w * PX, bits_h * PX);
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bits_h {
        for bx in 0..bits_w {
            // The shadow falls from the body only, so the legs stay clear.
            let shadow = bx > 0
                && (1..GUY.len() - 2).contains(&by)
                && at(bx - 1, by - 1) != b' '
                && (bx + by) % 2 == 0;
            let c = match at(bx, by) {
                b'#' => INK,
                b'.' => PAPER,
                b'o' => ACCENT,
                _ if shadow => INK,
                _ => continue,
            };
            for y in by * PX..(by + 1) * PX {
                for x in bx * PX..(bx + 1) * PX {
                    let i = (y * w + x) * 4;
                    out[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, c.3]);
                }
            }
        }
    }
    let image = Arc::new(ImageData::new(w as u32, h as u32, out).expect("the little guy"));
    let key = if blink {
        "little-guy-blink"
    } else {
        "little-guy"
    };
    zgui::compose::image(key.to_owned(), image).size(bits_w as f32 * bit, bits_h as f32 * bit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atkinson_keeps_the_size_and_uses_two_tones() {
        // A horizontal grey ramp.
        let (w, h) = (64u32, 16u32);
        let mut rgba = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let v = (x * 255 / (w - 1)) as u8;
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let out = atkinson(&rgba, w, h, 2);
        assert_eq!((out.width(), out.height()), (w, h));
        let px = out.pixels();
        let ink = px.chunks(4).filter(|p| p[0] == INK.0).count();
        let paper = px.chunks(4).filter(|p| p[0] == PAPER.0).count();
        assert_eq!(ink + paper, (w * h) as usize);
        // Darker on the left: more ink in the left half than the right.
        let left = (0..h as usize)
            .flat_map(|y| (0..w as usize / 2).map(move |x| (y, x)))
            .filter(|(y, x)| px[(y * w as usize + x) * 4] == INK.0)
            .count();
        assert!(left * 2 > ink, "left {left} of {ink}");
    }
}

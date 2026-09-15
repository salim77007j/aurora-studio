//! Layer blend modes (CSS/PDF-compatible set, like Photoshop).
use crate::img::{Rect, Rgba8};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 16] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    pub fn from_i32(v: i32) -> BlendMode {
        Self::ALL.get(v.max(0) as usize).copied().unwrap_or(BlendMode::Normal)
    }
    pub fn index(&self) -> i32 {
        Self::ALL.iter().position(|m| m == self).unwrap_or(0) as i32
    }
    pub fn name(&self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::HardLight => "Hard Light",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
        }
    }
    pub fn from_name(s: &str) -> BlendMode {
        let s = s.trim();
        Self::ALL.iter().copied().find(|m| m.name().eq_ignore_ascii_case(s)).unwrap_or(BlendMode::Normal)
    }
}

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

#[inline]
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut c = c;
    if n < 0.0 {
        c = [l + (c[0] - l) * l / (l - n), l + (c[1] - l) * l / (l - n), l + (c[2] - l) * l / (l - n)];
    }
    if x > 1.0 {
        c = [l + (c[0] - l) * (1.0 - l) / (x - l), l + (c[1] - l) * (1.0 - l) / (x - l), l + (c[2] - l) * (1.0 - l) / (x - l)];
    }
    c.map(|v| v.clamp(0.0, 1.0))
}

#[inline]
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    clip_color([c[0] + l - lum(c), c[1] + l - lum(c), c[2] + l - lum(c)])
}

#[inline]
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

#[inline]
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mut idx: [usize; 3] = [0, 1, 2];
    idx.sort_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap());
    // idx[0] has min, idx[2] max
    let (min, mid, max) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0f32; 3];
    if c[max] > c[min] {
        out[mid] = ((c[mid] - c[min]) * s / (c[max] - c[min])).clamp(0.0, 1.0);
        out[max] = s.clamp(0.0, 1.0);
    }
    out[min] = 0.0;
    out
}

/// Blend a source color over an opaque backdrop color, both straight RGB 0..1.
pub fn blend_rgb(mode: BlendMode, base: [f32; 3], src: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::Normal => src,
        BlendMode::Multiply => base.iter().zip(src.iter()).map(|(b, s)| b * s).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::Screen => [1.0 - (1.0 - base[0]) * (1.0 - src[0]), 1.0 - (1.0 - base[1]) * (1.0 - src[1]), 1.0 - (1.0 - base[2]) * (1.0 - src[2])],
        BlendMode::Overlay => {
            let f = |b: f32, s: f32| if b <= 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) };
            [f(base[0], src[0]), f(base[1], src[1]), f(base[2], src[2])]
        }
        BlendMode::Darken => base.iter().zip(src.iter()).map(|(b, s)| b.min(*s)).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::Lighten => base.iter().zip(src.iter()).map(|(b, s)| b.max(*s)).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::ColorDodge => base.iter().zip(src.iter()).map(|(b, s)| if *s >= 1.0 { 1.0 } else { (b / (1.0 - s)).min(1.0) }).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::ColorBurn => base.iter().zip(src.iter()).map(|(b, s)| if *s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / s).min(1.0) }).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::HardLight => base.iter().zip(src.iter()).map(|(b, s)| if *s <= 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) }).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::SoftLight => {
            let f = |b: f32, s: f32| -> f32 {
                let d: f32 = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b - (1.0 - 2.0 * s) * b * (1.0 - d)
            };
            [f(base[0], src[0]), f(base[1], src[1]), f(base[2], src[2])]
        }
        BlendMode::Difference => base.iter().zip(src.iter()).map(|(b, s)| (b - s).abs()).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::Exclusion => base.iter().zip(src.iter()).map(|(b, s)| b + s - 2.0 * b * s).collect::<Vec<_>>().try_into().unwrap(),
        BlendMode::Hue => set_lum(set_sat(src, sat(base)), lum(base)),
        BlendMode::Saturation => set_lum(set_sat(base, sat(src)), lum(base)),
        BlendMode::Color => set_lum(src, lum(base)),
        BlendMode::Luminosity => set_lum(base, lum(src)),
    }
}

/// Composite `src` onto `dst` (both straight RGBA) with a blend mode and opacity.
/// Result keeps straight alpha: out_a = src_a*op + dst_a*(1-src_a*op).
pub fn blend_pixel(mode: BlendMode, dst: [u8; 4], src: [u8; 4], opacity: f32) -> [u8; 4] {
    let sa = (src[3] as f32 / 255.0) * opacity.clamp(0.0, 1.0);
    if sa >= 1.0 && mode == BlendMode::Normal {
        return src;
    }
    if sa <= 0.0 {
        return dst;
    }
    let da = dst[3] as f32 / 255.0;
    // Blend against backdrop weighted by dst alpha (treat empty backdrop as transparent-black;
    // for non-separable modes blend against dst color regardless of alpha for painterly behavior).
    let b: [f32; 3] = dst[0..3].iter().map(|v| *v as f32 / 255.0).collect::<Vec<_>>().try_into().unwrap();
    let s: [f32; 3] = src[0..3].iter().map(|v| *v as f32 / 255.0).collect::<Vec<_>>().try_into().unwrap();
    let blended = blend_rgb(mode, b, s);
    let mut out = [0u8; 4];
    for c in 0..3 {
        let cb = b[c];
        let cs = if mode == BlendMode::Normal { s[c] } else { s[c] * (1.0 - da) + blended[c] * da };
        // straight-alpha lerp between backdrop color and blended color
        let v = cs * sa + cb * da * (1.0 - sa);
        out[c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    let oa = sa + da * (1.0 - sa);
    out[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}

/// Composite src onto dst region (dst full-size image, src same size, region) — used by compositing.
/// `dst_region` is in dst-local coordinates; `src_origin` is the doc-space position of dst's (0,0),
/// so src is sampled at (src_origin + dst-local coords).
pub fn composite_region(
    dst: &mut Rgba8,
    src: &Rgba8,
    mode: BlendMode,
    opacity: f32,
    dst_region: Rect,
    src_origin: (i32, i32),
    mask: Option<&crate::img::Gray8>,
) {
    let region = dst_region.clip_to(dst.w, dst.h);
    if region.is_empty() {
        return;
    }
    let op = opacity.clamp(0.0, 1.0);
    if op <= 0.0 {
        return;
    }
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let sx = src_origin.0 + x;
            let sy = src_origin.1 + y;
            let mut spx = src.get(sx, sy);
            if let Some(m) = mask {
                let mv = m.get(sx, sy);
                if mv == 0 {
                    continue;
                }
                spx[3] = ((spx[3] as u32 * mv as u32) / 255) as u8;
            }
            if spx[3] == 0 {
                continue;
            }
            let dpx = dst.get(x, y);
            let out = blend_pixel(mode, dpx, spx, op);
            dst.set(x, y, out);
        }
    }
}

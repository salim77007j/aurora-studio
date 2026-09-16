//! Color adjustments: curves, levels, brightness/contrast, hue/saturation/lightness.
//! All operate on the active layer (selection-aware) with LUT-based processing.
use crate::img::Rect;
use crate::layer::Document;
use rayon::prelude::*;

/// Monotonic cubic spline through control points → 256-entry LUT.
pub fn curve_lut(points: &[(f32, f32)]) -> [u8; 256] {
    let mut lut = [0u8; 256];
    if points.is_empty() {
        return lut;
    }
    let mut pts: Vec<(f32, f32)> = points.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // ensure monotone x in 0..1
    if pts.len() == 1 {
        let y = (pts[0].1.clamp(0.0, 1.0) * 255.0) as u8;
        lut.fill(y);
        return lut;
    }
    // Fritsch–Carlson monotone cubic
    let n = pts.len();
    let mut dx = vec![0.0f32; n - 1];
    let mut dy = vec![0.0f32; n - 1];
    let mut slope = vec![0.0f32; n - 1];
    for i in 0..n - 1 {
        dx[i] = (pts[i + 1].0 - pts[i].0).max(1e-6);
        dy[i] = pts[i + 1].1 - pts[i].1;
        slope[i] = dy[i] / dx[i];
    }
    let mut m = vec![0.0f32; n];
    m[0] = slope[0];
    m[n - 1] = slope[n - 2];
    for i in 1..n - 1 {
        if slope[i - 1] * slope[i] <= 0.0 {
            m[i] = 0.0;
        } else {
            let wx = dx[i - 1] + dx[i];
            m[i] = 3.0 * (dx[i] / wx * slope[i - 1] + dx[i - 1] / wx * slope[i]);
        }
    }
    for i in 0..n - 1 {
        let s = slope[i];
        if s <= 1e-9 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let a = m[i] / s;
        let b = m[i + 1] / s;
        let s2 = a * a + b * b;
        if s2 > 9.0 {
            let t = 3.0 / s2.sqrt();
            m[i] = t * a * s;
            m[i + 1] = t * b * s;
        }
    }
    for x in 0..256 {
        let u = x as f32 / 255.0;
        let mut v = u;
        if u <= pts[0].0 {
            v = pts[0].1;
        } else if u >= pts[n - 1].0 {
            v = pts[n - 1].1;
        } else {
            for i in 0..n - 1 {
                if u >= pts[i].0 && u <= pts[i + 1].0 {
                    let h = pts[i + 1].0 - pts[i].0;
                    let t = (u - pts[i].0) / h;
                    let t2 = t * t;
                    let t3 = t2 * t;
                    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                    let h10 = t3 - 2.0 * t2 + t;
                    let h01 = -2.0 * t3 + 3.0 * t2;
                    let h11 = t3 - t2;
                    v = h00 * pts[i].1 + h10 * h * m[i] + h01 * pts[i + 1].1 + h11 * h * m[i + 1];
                    break;
                }
            }
        }
        lut[x] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    lut
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CurvesParams {
    pub rgb: Vec<(f32, f32)>,
    pub r: Vec<(f32, f32)>,
    pub g: Vec<(f32, f32)>,
    pub b: Vec<(f32, f32)>,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct LevelsParams {
    pub in_black: f32,   // 0..255
    pub in_white: f32,   // 0..255
    pub gamma: f32,      // 0.1..10
    pub out_black: f32,  // 0..255
    pub out_white: f32,  // 0..255
    pub channel: i32,    // -1 rgb, 0 r, 1 g, 2 b
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct HslParams {
    pub hue: f32,        // -180..180
    pub saturation: f32, // -100..100
    pub lightness: f32,  // -100..100
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct BcParams {
    pub brightness: f32, // -100..100
    pub contrast: f32,   // -100..100
}

fn apply_lut(doc: &mut Document, luts: [[u8; 256]; 3], master: &[u8; 256], region: &Rect, label: &str) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let layer_id = doc.active_id;
    let rect = *region;
    if rect.is_empty() {
        return None;
    }
    let sel = doc.selection.as_gray();
    let (before, after) = {
        let layer = doc.get_layer_mut(layer_id)?;
        let n = (rect.w as usize) * (rect.h as usize) * 4;
        let mut before = vec![0u8; n];
        let mut o = 0;
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let v = layer.pixels.get(x, y);
                before[o..o + 4].copy_from_slice(&v);
                o += 4;
            }
        }
        let mut after = before.clone();
        after.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
            let x = rect.x + (i as i32 % rect.w as i32);
            let y = rect.y + (i as i32 / rect.w as i32);
            let f = match &sel {
                Some(s) => s.get(x, y) as f32 / 255.0,
                None => 1.0,
            };
            if f <= 0.0 {
                return;
            }
            for c in 0..3 {
                let orig = px[c];
                let mut v = master[orig as usize] as u16;
                v = luts[c][v as usize] as u16;
                // blend with original by selection factor
                px[c] = (orig as f32 * (1.0 - f) + v as f32 * f).round().clamp(0.0, 255.0) as u8;
            }
        });
        let mut o = 0;
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                layer.pixels.set(x, y, [after[o], after[o + 1], after[o + 2], after[o + 3]]);
                o += 4;
            }
        }
        (before, after)
    };
    doc.mark_dirty(rect);
    let _ = label;
    Some((rect, before, after))
}

/// Apply curves adjustment (selection-aware).
pub fn curves(doc: &mut Document, params: &CurvesParams) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let master = curve_lut(&params.rgb);
    let luts = [curve_lut(&params.r), curve_lut(&params.g), curve_lut(&params.b)];
    apply_lut(doc, luts, &master, &region, "Curves")
}

/// Apply levels adjustment.
pub fn levels(doc: &mut Document, p: LevelsParams) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let mut master = [0u8; 256];
    let ib = p.in_black.clamp(0.0, 254.0);
    let iw = p.in_white.clamp(ib + 1.0, 255.0);
    let g = p.gamma.clamp(0.01, 20.0);
    for i in 0..256 {
        let t = ((i as f32 - ib) / (iw - ib)).clamp(0.0, 1.0);
        let v = t.powf(1.0 / g);
        master[i] = (p.out_black + v * (p.out_white - p.out_black)).round().clamp(0.0, 255.0) as u8;
    }
    // per-channel variants when channel >= 0: apply only to that channel
    let luts: [[u8; 256]; 3] = match p.channel {
        0 => [master, identity(), identity()],
        1 => [identity(), master, identity()],
        2 => [identity(), identity(), master],
        _ => [master, master, master],
    };
    apply_lut(doc, luts, &identity(), &region, "Levels")
}

pub fn identity() -> [u8; 256] {
    let mut l = [0u8; 256];
    for (i, v) in l.iter_mut().enumerate() {
        *v = i as u8;
    }
    l
}

/// Apply brightness/contrast.
pub fn brightness_contrast(doc: &mut Document, p: BcParams) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let mut master = [0u8; 256];
    let b = p.brightness.clamp(-100.0, 100.0) * 1.275; // -127.5..127.5
    let c = p.contrast.clamp(-100.0, 100.0) / 100.0;
    let k = if c >= 0.0 { 1.0 + c * 4.0 } else { 1.0 + c };
    for i in 0..256 {
        let mut v = i as f32 + b;
        v = (v - 127.5) * k + 127.5;
        master[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    let luts = [master, master, master];
    apply_lut(doc, luts, &identity(), &region, "Brightness/Contrast")
}

/// Apply hue/saturation/lightness.
pub fn hue_saturation(doc: &mut Document, p: HslParams) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let sel = doc.selection.as_gray();
    if rect_is_empty(&region) {
        return None;
    }
    let layer_id = doc.active_id;
    let (before, after) = {
        let layer = doc.get_layer_mut(layer_id)?;
        let n = (region.w as usize) * (region.h as usize) * 4;
        let mut before = vec![0u8; n];
        let mut o = 0;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let v = layer.pixels.get(x, y);
                before[o..o + 4].copy_from_slice(&v);
                o += 4;
            }
        }
        let mut after = before.clone();
        let hue_r = p.hue.clamp(-180.0, 180.0) / 360.0;
        let sat = 1.0 + p.saturation.clamp(-100.0, 100.0) / 100.0;
        let light = p.lightness.clamp(-100.0, 100.0) / 100.0;
        after.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
            let x = region.x + (i as i32 % region.w as i32);
            let y = region.y + (i as i32 / region.w as i32);
            let f = match &sel {
                Some(s) => s.get(x, y) as f32 / 255.0,
                None => 1.0,
            };
            if f <= 0.0 {
                return;
            }
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let (mut h, mut s, mut l) = rgb_to_hsl(r, g, b);
            h = (h + hue_r).rem_euclid(1.0);
            s = (s * sat).clamp(0.0, 1.0);
            if light >= 0.0 {
                l = l + (1.0 - l) * light;
            } else {
                l = l * (1.0 + light);
            }
            let (r2, g2, b2) = hsl_to_rgb(h, s, l);
            for (c, v) in [(0, r2), (1, g2), (2, b2)] {
                let orig = px[c];
                let v2 = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                px[c] = (orig as f32 * (1.0 - f) + v2 as f32 * f).round().clamp(0.0, 255.0) as u8;
            }
        });
        let mut o = 0;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                layer.pixels.set(x, y, [after[o], after[o + 1], after[o + 2], after[o + 3]]);
                o += 4;
            }
        }
        (before, after)
    };
    doc.mark_dirty(region);
    Some((region, before, after))
}

// ══════════════════════════════════════════════════════════════════════
// v3.0 professional adjustments
// ══════════════════════════════════════════════════════════════════════

/// Generic selection-aware per-pixel color op over the active layer.
/// f gets (r,g,b as f32 0..255) and returns the new (r,g,b); alpha untouched.
fn per_pixel(doc: &mut Document, label: &str, f: impl Fn(f32, f32, f32) -> (f32, f32, f32) + Sync + Send) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    if rect_is_empty(&region) {
        return None;
    }
    let layer_id = doc.active_id;
    let sel = doc.selection.as_gray();
    let (before, after) = {
        let layer = doc.get_layer_mut(layer_id)?;
        let n = (region.w as usize) * (region.h as usize) * 4;
        let mut before = vec![0u8; n];
        let mut o = 0;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let v = layer.pixels.get(x, y);
                before[o..o + 4].copy_from_slice(&v);
                o += 4;
            }
        }
        let mut after = before.clone();
        after.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
            let x = region.x + (i as i32 % region.w as i32);
            let y = region.y + (i as i32 / region.w as i32);
            let factor = match &sel {
                Some(s) => s.get(x, y) as f32 / 255.0,
                None => 1.0,
            };
            if factor <= 0.0 {
                return;
            }
            let (r, g, b) = f(px[0] as f32, px[1] as f32, px[2] as f32);
            for (c, v) in [(0, r), (1, g), (2, b)] {
                let orig = px[c] as f32;
                let v2 = v.round().clamp(0.0, 255.0);
                px[c] = (orig * (1.0 - factor) + v2 * factor).round().clamp(0.0, 255.0) as u8;
            }
        });
        let mut o = 0;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                layer.pixels.set(x, y, [after[o], after[o + 1], after[o + 2], after[o + 3]]);
                o += 4;
            }
        }
        (before, after)
    };
    doc.mark_dirty(region);
    let _ = label;
    Some((region, before, after))
}

/// Exposure (stops) + gamma correction — camera-style tone control.
pub fn exposure_gamma(doc: &mut Document, exposure_stops: f32, gamma: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let ev = exposure_stops.clamp(-4.0, 4.0);
    let mul = 2.0f32.powf(ev);
    let g = 1.0 / gamma.clamp(0.1, 4.0);
    per_pixel(doc, "Exposure", move |r, gg, b| {
        let ch = |v: f32| {
            let x = (v / 255.0 * mul).clamp(0.0, 1.0);
            x.powf(g) * 255.0
        };
        (ch(r), ch(gg), ch(b))
    })
}

/// Vibrance: saturation boost that protects already-saturated colors & skin tones.
pub fn vibrance(doc: &mut Document, amount: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let amt = amount.clamp(-100, 100) as f32 / 100.0;
    per_pixel(doc, "Vibrance", move |r, g, b| {
        let mx = r.max(g).max(b);
        let mn = r.min(g).min(b);
        let avg = (r + g + b) / 3.0;
        let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        // less effect on saturated pixels, extra protection for skin hues
        let k = amt * (1.0 - sat);
        let mut out = [r, g, b];
        out[0] += (mx - avg) * k;
        out[1] += (mx - avg) * k;
        out[2] += (mx - avg) * k;
        // skin-tone protection: hue near orange gets half effect
        if mx > mn {
            let hue = {
                let d = mx - mn;
                let h = if mx == r { (g - b) / d } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
                h.rem_euclid(6.0) * 60.0
            };
            if (10.0..=50.0).contains(&hue) && mx >= 95.0 && mn >= 40.0 {
                let warm = 0.5;
                out[0] = r + (out[0] - r) * warm;
                out[1] = g + (out[1] - g) * warm;
                out[2] = b + (out[2] - b) * warm;
            }
        }
        (out[0], out[1], out[2])
    })
}

/// White balance via Kelvin-style temperature (-100 cool .. +100 warm) and green-magenta tint.
pub fn white_balance(doc: &mut Document, temperature: i32, tint: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let t = temperature.clamp(-100, 100) as f32 / 100.0;
    let ti = tint.clamp(-100, 100) as f32 / 100.0;
    per_pixel(doc, "White Balance", move |r, g, b| {
        (
            r + t * 46.0,
            g + ti * 30.0 - t * 8.0,
            b - t * 46.0,
        )
    })
}

/// Recover highlights and open shadows independently.
pub fn shadows_highlights(doc: &mut Document, shadows: i32, highlights: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let sh = shadows.clamp(-100, 100) as f32 / 100.0;
    let hi = highlights.clamp(-100, 100) as f32 / 100.0;
    per_pixel(doc, "Shadows/Highlights", move |r, g, b| {
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let t = lum / 255.0;
        // shadow weight: strong in dark tones
        let sw = (1.0 - t * 2.0).clamp(0.0, 1.0);
        // highlight weight: strong in bright tones
        let hw = (t * 2.0 - 1.0).clamp(0.0, 1.0);
        let lift = sh * 90.0 * sw * (1.0 - t);
        let pull = hi * (lum) * hw; // pull highlights toward black proportionally
        (r + lift - pull * 0.85, g + lift - pull * 0.85, b + lift - pull * 0.85)
    })
}

/// Color balance: independent cyan-red, magenta-green, yellow-blue shifts.
pub fn color_balance(doc: &mut Document, cr: i32, mg: i32, yb: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let a = cr.clamp(-100, 100) as f32 * 0.6;
    let b2 = mg.clamp(-100, 100) as f32 * 0.6;
    let c = yb.clamp(-100, 100) as f32 * 0.6;
    per_pixel(doc, "Color Balance", move |r, g, b| (r + a, g + b2, b + c))
}

/// Black & white conversion with per-channel luminance weights.
pub fn black_white(doc: &mut Document, rw: i32, gw: i32, bw: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let wr = 0.299 + rw.clamp(-100, 100) as f32 / 250.0;
    let wg = 0.587 + gw.clamp(-100, 100) as f32 / 250.0;
    let wb = 0.114 + bw.clamp(-100, 100) as f32 / 250.0;
    let sum_inv = 1.0 / (wr + wg + wb);
    per_pixel(doc, "Black & White", move |r, g, b| {
        let v = (r * wr + g * wg + b * wb) * sum_inv;
        (v, v, v)
    })
}

/// Simple desaturation (average luminance).
pub fn desaturate(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    per_pixel(doc, "Desaturate", |r, g, b| {
        let v = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        (v, v, v)
    })
}

/// Invert colors (alpha preserved).
pub fn invert(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    per_pixel(doc, "Invert", |r, g, b| (255.0 - r, 255.0 - g, 255.0 - b))
}

/// Threshold: binary black/white split at the given luminance level.
pub fn threshold(doc: &mut Document, level: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let lv = level.clamp(0, 255) as f32;
    per_pixel(doc, "Threshold", move |r, g, b| {
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let v = if lum >= lv { 255.0 } else { 0.0 };
        (v, v, v)
    })
}

/// Posterize to n levels per channel.
pub fn posterize(doc: &mut Document, levels: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let n = levels.clamp(2, 64) as f32;
    let step = 255.0 / (n - 1.0);
    per_pixel(doc, "Posterize", move |r, g, b| {
        let q = |v: f32| (v / step).round() * step;
        (q(r), q(g), q(b))
    })
}

/// Photo filter: uniform color tint (like a colored glass over the lens).
pub fn photo_filter(doc: &mut Document, tr: u8, tg: u8, tb: u8, density: i32, preserve_luma: bool) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let d = density.clamp(0, 100) as f32 / 100.0;
    let (fr, fg, fb) = (tr as f32, tg as f32, tb as f32);
    per_pixel(doc, "Photo Filter", move |r, g, b| {
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let mut out = (
            r + (fr - r) * d,
            g + (fg - g) * d,
            b + (fb - b) * d,
        );
        if preserve_luma {
            let lum2 = 0.2126 * out.0 + 0.7152 * out.1 + 0.0722 * out.2;
            let k = if lum2.abs() > 1e-3 { lum / lum2 } else { 1.0 };
            out = (out.0 * k, out.1 * k, out.2 * k);
        }
        out
    })
}

/// Gradient map: map luminance onto a 2-stop color ramp.
pub fn gradient_map(doc: &mut Document, r0: u8, g0: u8, b0: u8, r1: u8, g1: u8, b1: u8) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    per_pixel(doc, "Gradient Map", move |r, g, b| {
        let t = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
        (
            r0 as f32 + (r1 as f32 - r0 as f32) * t,
            g0 as f32 + (g1 as f32 - g0 as f32) * t,
            b0 as f32 + (b1 as f32 - b0 as f32) * t,
        )
    })
}

// ---- auto corrections (computed from actual histogram of the region) ----

fn region_luminance_stats(doc: &mut Document) -> Option<(u8, u8, [u64; 256])> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    if rect_is_empty(&region) {
        return None;
    }
    let layer_id = doc.active_id;
    let layer = doc.get_layer(layer_id)?;
    let mut hist = [0u64; 256];
    let mut lo = 255u8;
    let mut hi = 0u8;
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let px = layer.pixels.get(x, y);
            if px[3] == 0 { continue; }
            let lum = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32).round() as usize;
            hist[lum.min(255)] += 1;
            if (lum as u8) < lo { lo = lum as u8; }
            if (lum as u8) > hi { hi = lum as u8; }
        }
    }
    Some((lo, hi, hist))
}

/// Auto tone: stretch in-black/in-white to clip 0.1% outliers on luminance.
pub fn auto_tone(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let (lo, hi, hist) = region_luminance_stats(doc)?;
    let total: u64 = hist.iter().sum();
    if total == 0 || hi <= lo + 4 {
        return None; // already full-range; avoid degrading
    }
    // robust 0.1% clip
    let clip = total / 1000;
    let mut acc = 0u64;
    let mut inb = 0usize;
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if acc > clip { inb = i; break; }
    }
    acc = 0;
    let mut inw = 255usize;
    for (i, &c) in hist.iter().enumerate().rev() {
        acc += c;
        if acc > clip { inw = i; break; }
    }
    if inw <= inb + 4 { return None; }
    levels(doc, LevelsParams { in_black: inb as f32, in_white: inw as f32, gamma: 1.0, out_black: 0.0, out_white: 255.0, channel: -1 })
}

/// Auto contrast: neutral-gray preserving stretch (no color cast change).
pub fn auto_contrast(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    auto_tone(doc)
}

/// Auto color: per-channel 0.5% clip stretch — removes mild color casts.
pub fn auto_color(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    if rect_is_empty(&region) {
        return None;
    }
    let layer_id = doc.active_id;
    let layer = doc.get_layer(layer_id)?;
    let mut hists = [[0u64; 256]; 3];
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let px = layer.pixels.get(x, y);
            if px[3] == 0 { continue; }
            for c in 0..3 {
                hists[c][px[c] as usize] += 1;
            }
        }
    }
    let total: u64 = hists[0].iter().sum();
    if total == 0 { return None; }
    let clip = (total / 200).max(1);
    let mut luts = [identity(), identity(), identity()];
    for c in 0..3 {
        let mut acc = 0u64;
        let mut inb = 0usize;
        for (i, &cnt) in hists[c].iter().enumerate() {
            acc += cnt;
            if acc > clip { inb = i; break; }
        }
        acc = 0;
        let mut inw = 255usize;
        for (i, &cnt) in hists[c].iter().enumerate().rev() {
            acc += cnt;
            if acc > clip { inw = i; break; }
        }
        if inw <= inb + 4 { continue; }
        for i in 0..256 {
            let t = ((i as f32 - inb as f32) / (inw - inb) as f32).clamp(0.0, 1.0);
            luts[c][i] = (t * 255.0).round() as u8;
        }
    }
    apply_lut(doc, luts, &identity(), &region, "Auto Color")
}

/// Clarity: mid-tone local contrast (unsharp on luminance with big radius, masked to mids).
pub fn clarity(doc: &mut Document, amount: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region_full = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(-100, 100) as f32 / 100.0;
    let sigma = (doc.w.max(doc.h) as f32 / 60.0).clamp(4.0, 40.0);
    // use the blur as a luminance base, then adjust contrast in midtones
    let result = process_like_filter(doc, region_full, "Clarity", move |src, w, h, rect| {
        let blurred = crate::filters::blur_luminance(src, w, h, rect, sigma);
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                for c in 0..3 {
                    let o = src[oo + c] as f32;
                    let b = blurred[po + c] as f32;
                    let lum = o / 255.0;
                    // midtone mask (0 at extremes)
                    let mask = 1.0 - (2.0 * lum - 1.0).abs().powf(2.0);
                    out[po + c] = (o + amt * 1.6 * (o - b) * mask).round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    });
    result
}

/// Engine-internal hook so adjust.rs can reuse the filters' selection-aware process().
fn process_like_filter(
    doc: &mut Document,
    region: Rect,
    label: &str,
    f: impl Fn(&[u8], u32, u32, Rect) -> Vec<u8> + Sync + Send,
) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    crate::filters::process_pub(doc, region, label, f)
}

fn rect_is_empty(r: &Rect) -> bool { r.w == 0 || r.h == 0 }

pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s.abs() < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| -> f32 {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

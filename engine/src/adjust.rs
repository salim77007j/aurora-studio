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

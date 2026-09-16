//! Filters: gaussian blur, sharpen, noise, pixelate, twirl, wave, emboss.
//! All operate on the active layer (selection-aware).
use crate::img::Rect;
use crate::layer::Document;
use rayon::prelude::*;

/// A pixel operation applied within `region` of the active layer, blended by selection mask.
fn process(
    doc: &mut Document,
    region: Rect,
    label: &str,
    f: impl Fn(&[u8], u32, u32, Rect) -> Vec<u8> + Sync + Send,
) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let layer_id = doc.active_id;
    let rect = doc.selection.bounds().unwrap_or(region).clip_to(doc.w, doc.h).intersect(&region.clip_to(doc.w, doc.h))?;
    if rect.is_empty() {
        return None;
    }
    let sel = doc.selection.as_gray();
    let (before, after) = {
        let (dw, dh) = (doc.w, doc.h);
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
        // run filter over an expanded working copy for neighborhood ops
        let after = f(&layer.pixels.data, dw, dh, rect);
        let mut o = 0;
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let mut px = [after[o], after[o + 1], after[o + 2], after[o + 3]];
                let factor = match &sel {
                    Some(s) => s.get(x, y) as f32 / 255.0,
                    None => 1.0,
                };
                if factor < 1.0 {
                    for c in 0..4 {
                        px[c] = (before[o + c] as f32 * (1.0 - factor) + px[c] as f32 * factor).round().clamp(0.0, 255.0) as u8;
                    }
                }
                layer.pixels.set(x, y, px);
                o += 4;
            }
        }
        (before, after)
    };
    doc.mark_dirty(rect);
    let _ = label;
    Some((rect, before, after))
}

#[allow(clippy::too_many_arguments)]
fn gaussian_blur_impl(src: &[u8], w: u32, h: u32, region: Rect, sigma: f32) -> Vec<u8> {
    // separable 3-pass box blur approximation on a padded working copy
    let pad = (sigma.ceil() as i32 * 3 + 1).max(1) as i32;
    let x0 = (region.x - pad).max(0);
    let y0 = (region.y - pad).max(0);
    let x1 = (region.right() + pad).min(w as i32);
    let y1 = (region.bottom() + pad).min(h as i32);
    let ww = (x1 - x0).max(1) as usize;
    let wh = (y1 - y0).max(1) as usize;
    // premultiplied float copy
    let mut work = vec![0.0f32; ww * wh * 4];
    for y in 0..wh {
        for x in 0..ww {
            let o = ((y + y0 as usize) * w as usize + (x + x0 as usize)) * 4;
            let a = src[o + 3] as f32 / 255.0;
            for c in 0..3 {
                work[(y * ww + x) * 4 + c] = src[o + c] as f32 * a;
            }
            work[(y * ww + x) * 4 + 3] = src[o + 3] as f32;
        }
    }
    let boxes = kovesi_boxes(sigma, 3);
    let mut tmp = work.clone();
    for r in boxes {
        if r < 1 { continue; }
        box_h(&mut work, &mut tmp, ww, wh, r as usize);
        box_v(&mut tmp, &mut work, ww, wh, r as usize);
    }
    // back to straight RGBA for the region
    let mut out = vec![0u8; (region.w as usize) * (region.h as usize) * 4];
    for y in 0..region.h as usize {
        for x in 0..region.w as usize {
            let wx = x + (region.x - x0) as usize;
            let wy = y + (region.y - y0) as usize;
            let o = (wy * ww + wx) * 4;
            let a = (work[o + 3] / 255.0).clamp(0.0, 1.0);
            let po = (y * region.w as usize + x) * 4;
            if a > 1e-6 {
                for c in 0..3 {
                    out[po + c] = (work[o + c] / a).round().clamp(0.0, 255.0) as u8;
                }
            } else {
                out[po..po + 3].fill(0);
            }
            out[po + 3] = work[o + 3].round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn kovesi_boxes(sigma: f32, n: usize) -> Vec<u32> {
    let si = sigma * ((2.0 * std::f32::consts::PI / n as f32).sqrt() / 2.0 + 0.5);
    let w = (si * 3.0).max(1.0) as u32;
    vec![w; n]
}

fn box_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let inv = 1.0 / (2 * r + 1) as f32;
    for y in 0..h {
        for c in 0..4 {
            let row = y * w;
            let mut acc = 0.0f32;
            for i in 0..=(2 * r) {
                acc += src[(row + (i.min(w - 1))) * 4 + c];
            }
            for x in 0..w {
                dst[(row + x) * 4 + c] = acc * inv;
                let add = src[(row + (x + 2 * r + 1).min(w - 1)) * 4 + c];
                let sub = src[(row + x.saturating_sub(2 * r)) * 4 + c];
                acc += add - sub;
            }
        }
    }
}

fn box_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let inv = 1.0 / (2 * r + 1) as f32;
    for x in 0..w {
        for c in 0..4 {
            let mut acc = 0.0f32;
            for i in 0..=(2 * r) {
                acc += src[((i.min(h - 1)) * w + x) * 4 + c];
            }
            for y in 0..h {
                dst[(y * w + x) * 4 + c] = acc * inv;
                let add = src[((y + 2 * r + 1).min(h - 1) * w + x) * 4 + c];
                let sub = src[(y.saturating_sub(2 * r) * w + x) * 4 + c];
                acc += add - sub;
            }
        }
    }
}

pub fn gaussian_blur(doc: &mut Document, radius: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sigma = radius.clamp(0.1, 250.0);
    process(doc, region, "Gaussian Blur", move |src, w, h, rect| {
        gaussian_blur_impl(src, w, h, rect, sigma)
    })
}

pub fn sharpen(doc: &mut Document, amount: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(0.0, 3.0);
    process(doc, region, "Sharpen", move |src, w, h, rect| {
        // unsharp mask: blur radius 1, out = orig + amount*(orig - blur)
        let blurred = gaussian_blur_impl(src, w, h, rect, 1.2);
        let mut out = blurred.clone();
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let po = (y * rect.w as usize + x) * 4;
                // original pixel coords
                let ox = rect.x as usize + x;
                let oy = rect.y as usize + y;
                let oo = (oy * w as usize + ox) * 4;
                for c in 0..3 {
                    let o = src[oo + c] as f32;
                    let b = blurred[po + c] as f32;
                    out[po + c] = (o + amt * (o - b)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    })
}

pub fn add_noise(doc: &mut Document, amount: i32, monochrome: bool) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let a = amount.clamp(0, 100) as f32 / 100.0;
    process(doc, region, "Add Noise", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let mut seed = 0x9E3779B9u32;
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let ox = rect.x as usize + x;
                let oy = rect.y as usize + y;
                let oo = (oy * w as usize + ox) * 4;
                let po = (y * rect.w as usize + x) * 4;
                let mut n = [0f32; 3];
                for c in 0..3 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let rv = ((seed >> 16) & 0xff) as f32 / 127.5 - 1.0;
                    n[c] = rv * a * 255.0;
                }
                if monochrome {
                    let m = (n[0] + n[1] + n[2]) / 3.0;
                    n = [m; 3];
                }
                for c in 0..3 {
                    out[po + c] = (src[oo + c] as f32 + n[c]).round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

pub fn pixelate(doc: &mut Document, size: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let s = size.clamp(2, 512) as i32;
    process(doc, region, "Pixelate", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let bx0 = rect.x / s * s;
        let by0 = rect.y / s * s;
        let blocks_x = (rect.right() - bx0 + s - 1) / s;
        let blocks_y = (rect.bottom() - by0 + s - 1) / s;
        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let x0 = (bx0 + bx * s).max(rect.x);
                let y0 = (by0 + by * s).max(rect.y);
                let x1 = (bx0 + (bx + 1) * s).min(rect.right());
                let y1 = (by0 + (by + 1) * s).min(rect.bottom());
                if x1 <= x0 || y1 <= y0 { continue; }
                let mut acc = [0u64; 4];
                let mut count = 0u64;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let o = ((y as usize) * w as usize + x as usize) * 4;
                        for c in 0..4 {
                            acc[c] += src[o + c] as u64;
                        }
                        count += 1;
                    }
                }
                let avg: [u8; 4] = acc.map(|v| (v / count) as u8);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let po = ((y - rect.y) as usize * rect.w as usize + (x - rect.x) as usize) * 4;
                        out[po..po + 4].copy_from_slice(&avg);
                    }
                }
            }
        }
        let _ = h;
        out
    })
}

pub fn twirl(doc: &mut Document, cx: f32, cy: f32, radius: f32, angle_deg: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let ang = angle_deg.to_radians();
    process(doc, region, "Twirl", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let img = Rgba8Ref { w, h, data: src };
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let dx = px as f32 - cx;
                let dy = py as f32 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let mut sxp = px as f32;
                let mut syp = py as f32;
                if d < radius {
                    let t = 1.0 - d / radius;
                    let a = ang * t * t;
                    let ca = a.cos();
                    let sa = a.sin();
                    sxp = cx + dx * ca - dy * sa;
                    syp = cy + dx * sa + dy * ca;
                }
                let s = img.sample_bilinear(sxp, syp);
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                out[po] = s[0].round().clamp(0.0, 255.0) as u8;
                out[po + 1] = s[1].round().clamp(0.0, 255.0) as u8;
                out[po + 2] = s[2].round().clamp(0.0, 255.0) as u8;
                out[po + 3] = s[3].round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    })
}

pub fn wave(doc: &mut Document, amplitude: f32, wavelength: f32, vertical: bool) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let a = amplitude.clamp(1.0, 500.0);
    let wl = wavelength.clamp(4.0, 1000.0);
    process(doc, region, "Wave", move |src, w, h, rect| {
        let img = Rgba8Ref { w, h, data: src };
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let (sxp, syp) = if vertical {
                    (px as f32, py as f32 + a * (2.0 * std::f32::consts::PI * px as f32 / wl).sin())
                } else {
                    (px as f32 + a * (2.0 * std::f32::consts::PI * py as f32 / wl).sin(), py as f32)
                };
                let s = img.sample_bilinear(sxp, syp);
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                out[po] = s[0].round().clamp(0.0, 255.0) as u8;
                out[po + 1] = s[1].round().clamp(0.0, 255.0) as u8;
                out[po + 2] = s[2].round().clamp(0.0, 255.0) as u8;
                out[po + 3] = s[3].round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    })
}

pub fn emboss(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    process(doc, region, "Emboss", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let at = |x: i64, y: i64, c: usize| -> f32 {
            let cx = x.clamp(0, w as i64 - 1) as usize;
            let cy = y.clamp(0, h as i64 - 1) as usize;
            src[(cy * w as usize + cx) * 4 + c] as f32
        };
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let gx = rect.x + x;
                let gy = rect.y + y;
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..3 {
                    let v = (at(gx as i64 - 1, gy as i64 - 1, c) + 2.0 * at(gx as i64, gy as i64 - 1, c) + at(gx as i64 + 1, gy as i64 - 1, c))
                        - (at(gx as i64 - 1, gy as i64 + 1, c) + 2.0 * at(gx as i64, gy as i64 + 1, c) + at(gx as i64 + 1, gy as i64 + 1, c))
                        + 128.0;
                    out[po + c] = v.round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[((gy as usize) * w as usize + gx as usize) * 4 + 3];
            }
        }
        out
    })
}

struct Rgba8Ref<'a> {
    w: u32,
    h: u32,
    data: &'a [u8],
}

// ══════════════════════════════════════════════════════════════════════
// v3.0 professional effects library
// ══════════════════════════════════════════════════════════════════════

/// Public access to the selection-aware pixel pipeline (used by adjust::clarity).
pub fn process_pub(
    doc: &mut Document,
    region: Rect,
    label: &str,
    f: impl Fn(&[u8], u32, u32, Rect) -> Vec<u8> + Sync + Send,
) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    process(doc, region, label, f)
}

/// crate-internal: blurred copy of the region (straight RGBA) used by clarity/unsharp variants.
pub fn blur_luminance(src: &[u8], w: u32, h: u32, region: Rect, sigma: f32) -> Vec<u8> {
    gaussian_blur_impl(src, w, h, region, sigma)
}

/// Box blur (uniform radius) — fast softening.
pub fn box_blur(doc: &mut Document, radius: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let r = radius.clamp(1, 100) as f32;
    process(doc, region, "Box Blur", move |src, w, h, rect| {
        gaussian_blur_impl(src, w, h, rect, r / 1.9)
    })
}

/// Directional motion blur: `length` px along `angle_deg`.
pub fn motion_blur(doc: &mut Document, length: i32, angle_deg: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let len = length.clamp(1, 400) as i32;
    let (dx, dy) = {
        let a = angle_deg.to_radians();
        (a.cos(), a.sin())
    };
    process(doc, region, "Motion Blur", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let samples = len.max(1);
        let at = |x: i64, y: i64, c: usize| -> f32 {
            let cx = x.clamp(0, w as i64 - 1) as usize;
            let cy = y.clamp(0, h as i64 - 1) as usize;
            src[(cy * w as usize + cx) * 4 + c] as f32
        };
        for y in 0..rect.h as i64 {
            for x in 0..rect.w as i64 {
                let gx = rect.x as i64 + x;
                let gy = rect.y as i64 + y;
                let mut acc = [0f32; 3];
                for s in 0..samples {
                    let t = s as f32 - samples as f32 / 2.0;
                    let sx = (gx as f32 + dx * t).round() as i64;
                    let sy = (gy as f32 + dy * t).round() as i64;
                    for c in 0..3 {
                        acc[c] += at(sx, sy, c);
                    }
                }
                let po = ((y as usize) * rect.w as usize + x as usize) * 4;
                for c in 0..3 {
                    out[po + c] = (acc[c] / samples as f32).round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[((gy as usize) * w as usize + gx as usize) * 4 + 3];
            }
        }
        out
    })
}

/// Radial (zoom) blur radiating from center — speed/rush effect.
pub fn zoom_blur(doc: &mut Document, amount: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(1, 100) as f32 / 100.0;
    process(doc, region, "Zoom Blur", move |src, w, h, rect| {
        let img = Rgba8Ref { w, h, data: src };
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let steps = 16;
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let vx = px as f32 - cx;
                let vy = py as f32 - cy;
                let mut acc = [0f32; 4];
                for s in 0..steps {
                    let t = 1.0 - amt * (s as f32 / steps as f32);
                    let smp = img.sample_bilinear(cx + vx * t, cy + vy * t);
                    for c in 0..4 {
                        acc[c] += smp[c];
                    }
                }
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..4 {
                    out[po + c] = (acc[c] / steps as f32).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    })
}

/// Unsharp mask with user-controlled radius, strength and edge threshold.
pub fn unsharp_mask(doc: &mut Document, radius: f32, strength: f32, threshold: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sig = radius.clamp(0.1, 60.0);
    let amt = strength.clamp(0.0, 5.0);
    let thr = threshold.clamp(0, 128) as f32;
    process(doc, region, "Unsharp Mask", move |src, w, h, rect| {
        let blurred = gaussian_blur_impl(src, w, h, rect, sig);
        let mut out = blurred.clone();
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let po = (y * rect.w as usize + x) * 4;
                let ox = rect.x as usize + x;
                let oy = rect.y as usize + y;
                let oo = (oy * w as usize + ox) * 4;
                for c in 0..3 {
                    let o = src[oo + c] as f32;
                    let b = blurred[po + c] as f32;
                    let diff = o - b;
                    if diff.abs() >= thr {
                        out[po + c] = (o + amt * diff).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        out
    })
}

/// Sobel find-edges (keeps alpha).
pub fn find_edges(doc: &mut Document, invert_output: bool) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    process(doc, region, "Find Edges", move |src, w, h, rect| {
        let at = |x: i64, y: i64, c: usize| -> f32 {
            let cx = x.clamp(0, w as i64 - 1) as usize;
            let cy = y.clamp(0, h as i64 - 1) as usize;
            let o = (cy * w as usize + cx) * 4 + c;
            0.2126 * src[o] as f32 + if c == 0 { 0.0 } else { 0.0 }
        };
        // grayscale first for sobel
        let gray_at = |x: i64, y: i64| -> f32 {
            let cx = x.clamp(0, w as i64 - 1) as usize;
            let cy = y.clamp(0, h as i64 - 1) as usize;
            let o = (cy * w as usize + cx) * 4;
            0.2126 * src[o] as f32 + 0.7152 * src[o + 1] as f32 + 0.0722 * src[o + 2] as f32
        };
        let _ = at;
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i64 {
            for x in 0..rect.w as i64 {
                let gx = rect.x as i64 + x;
                let gy = rect.y as i64 + y;
                let tl = gray_at(gx - 1, gy - 1); let tc = gray_at(gx, gy - 1); let tr = gray_at(gx + 1, gy - 1);
                let ml = gray_at(gx - 1, gy); let mr = gray_at(gx + 1, gy);
                let bl = gray_at(gx - 1, gy + 1); let bc = gray_at(gx, gy + 1); let br = gray_at(gx + 1, gy + 1);
                let gxv = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
                let gyv = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
                let mag = (gxv * gxv + gyv * gyv).sqrt().clamp(0.0, 255.0);
                let v = if invert_output { 255.0 - mag } else { mag };
                let po = ((y as usize) * rect.w as usize + x as usize) * 4;
                out[po] = v.round() as u8;
                out[po + 1] = v.round() as u8;
                out[po + 2] = v.round() as u8;
                out[po + 3] = src[((gy as usize) * w as usize + gx as usize) * 4 + 3];
            }
        }
        out
    })
}

/// Kuwahara oil-paint effect (radius r → painterly flattening).
pub fn oil_paint(doc: &mut Document, radius: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let r = radius.clamp(1, 10) as i64;
    process(doc, region, "Oil Paint", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let lum = |x: i64, y: i64| -> i32 {
            let cx = x.clamp(0, w as i64 - 1) as usize;
            let cy = y.clamp(0, h as i64 - 1) as usize;
            let o = (cy * w as usize + cx) * 4;
            ((src[o] as i32 * 77 + src[o + 1] as i32 * 151 + src[o + 2] as i32 * 28) >> 8) / 16
        };
        for y in 0..rect.h as i64 {
            for x in 0..rect.w as i64 {
                let gx = rect.x as i64 + x;
                let gy = rect.y as i64 + y;
                let mut best_cnt = 0i32;
                let mut best_var = f32::MAX;
                let mut best_r = 0i64; let mut best_g = 0i64; let mut best_b = 0i64;
                // 4 quadrants: pick the flattest (lowest luminance variance)
                for q in 0..4i64 {
                    let (x0, x1) = if q & 1 == 0 { (-r, 0) } else { (0, r) };
                    let (y0, y1) = if q & 2 == 0 { (-r, 0) } else { (0, r) };
                    let mut counts = [0i32; 16];
                    let mut sr = 0i64; let mut sg = 0i64; let mut sb = 0i64;
                    for dy in y0..=y1 {
                        for dx in x0..=x1 {
                            let cx = (gx + dx).clamp(0, w as i64 - 1) as usize;
                            let cy = (gy + dy).clamp(0, h as i64 - 1) as usize;
                            let o = (cy * w as usize + cx) * 4;
                            let l = lum(gx + dx, gy + dy) as usize;
                            counts[l.min(15)] += 1;
                            sr += src[o] as i64;
                            sg += src[o + 1] as i64;
                            sb += src[o + 2] as i64;
                        }
                    }
                    let cnt = ((x1 - x0 + 1) * (y1 - y0 + 1)) as i32;
                    let variance = {
                        let mean = counts.iter().sum::<i32>() as f32 / 16.0;
                        counts.iter().map(|&c| (c as f32 - mean).powi(2)).sum::<f32>() / 16.0
                    };
                    if variance < best_var {
                        best_var = variance;
                        best_cnt = cnt;
                        best_r = sr; best_g = sg; best_b = sb;
                    }
                }
                let po = ((y as usize) * rect.w as usize + x as usize) * 4;
                out[po] = (best_r / best_cnt.max(1) as i64).clamp(0, 255) as u8;
                out[po + 1] = (best_g / best_cnt.max(1) as i64).clamp(0, 255) as u8;
                out[po + 2] = (best_b / best_cnt.max(1) as i64).clamp(0, 255) as u8;
                out[po + 3] = src[((gy as usize) * w as usize + gx as usize) * 4 + 3];
            }
        }
        out
    })
}

/// Halftone dots (newspaper style) — dot radius encodes local luminance.
pub fn halftone(doc: &mut Document, cell: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let s = cell.clamp(3, 64) as i32;
    process(doc, region, "Halftone", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let lum_of = |x: i32, y: i32| -> f32 {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { return 255.0; }
            let o = (y as usize * w as usize + x as usize) * 4;
            0.2126 * src[o] as f32 + 0.7152 * src[o + 1] as f32 + 0.0722 * src[o + 2] as f32
        };
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let cellx = px.div_euclid(s) * s + s / 2;
                let celly = py.div_euclid(s) * s + s / 2;
                let lum = lum_of(cellx, celly);
                let dxr = (px - cellx) as f32;
                let dyr = (py - celly) as f32;
                let dist = (dxr * dxr + dyr * dyr).sqrt();
                let maxr = s as f32 / 2.0;
                let dotr = (1.0 - lum / 255.0) * maxr;
                let v = if dist <= dotr { 0.0 } else { 255.0 };
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                out[po] = v as u8;
                out[po + 1] = v as u8;
                out[po + 2] = v as u8;
                out[po + 3] = src[((py as usize) * w as usize + px as usize) * 4 + 3];
            }
        }
        out
    })
}

/// Charcoal sketch: grayscale → invert → blur → color-dodge blend.
pub fn charcoal(doc: &mut Document, detail: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sig = (detail.clamp(1, 20) as f32) * 0.6;
    process(doc, region, "Charcoal", move |src, w, h, rect| {
        let mut gray = src.to_vec();
        for px in gray.chunks_exact_mut(4) {
            let l = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as u8;
            px[0] = 255 - l; px[1] = 255 - l; px[2] = 255 - l;
        }
        let blurred = gaussian_blur_impl(&gray, w, h, rect, sig);
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                for c in 0..3 {
                    let base = src[oo + c] as u32;
                    let top = blurred[po + c] as u32;
                    // color dodge
                    let v = if top == 255 { 255 } else { (base * 255 / (255 - top)).min(255) };
                    out[po + c] = v as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

/// Pencil sketch: high-pass edge sketch with soft shading.
pub fn pencil_sketch(doc: &mut Document, strength: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let k = strength.clamp(1, 20) as f32;
    process(doc, region, "Pencil Sketch", move |src, w, h, rect| {
        let mut gray = src.to_vec();
        for px in gray.chunks_exact_mut(4) {
            let l = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as u8;
            px[0] = l; px[1] = l; px[2] = l;
        }
        let blurred = gaussian_blur_impl(&gray, w, h, rect, k * 0.8);
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                for c in 0..3 {
                    let g = gray[oo + c] as f32;
                    let b = blurred[po + c] as f32;
                    let edge = (g - b).abs();
                    let shade = (g * 0.55 + 115.0).clamp(0.0, 255.0);
                    let v = (shade - edge * 2.2).clamp(10.0, 255.0);
                    out[po + c] = v.round() as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

/// Median 3×3 — smooths noise while keeping edges (photo noise reduction).
pub fn median_denoise(doc: &mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    process(doc, region, "Noise Reduction", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let mut win = [0u8; 9];
        for y in 0..rect.h as i64 {
            for x in 0..rect.w as i64 {
                let gx = rect.x as i64 + x;
                let gy = rect.y as i64 + y;
                let po = ((y as usize) * rect.w as usize + x as usize) * 4;
                for c in 0..4 {
                    let mut i = 0;
                    for dy in -1..=1i64 {
                        for dx in -1..=1i64 {
                            let cx = (gx + dx).clamp(0, w as i64 - 1) as usize;
                            let cy = (gy + dy).clamp(0, h as i64 - 1) as usize;
                            win[i] = src[(cy * w as usize + cx) * 4 + c];
                            i += 1;
                        }
                    }
                    win.sort_unstable();
                    out[po + c] = win[4];
                }
            }
        }
        out
    })
}

/// Vignette: darken edges; amount -100 (white edges) .. 100 (black edges).
pub fn vignette(doc: &mut Document, amount: i32, roundness: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(-100, 100) as f32 / 100.0;
    let rnd = roundness.clamp(0, 100) as f32 / 100.0;
    process(doc, region, "Vignette", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let cx = rect.x as f32 + rect.w as f32 / 2.0;
        let cy = rect.y as f32 + rect.h as f32 / 2.0;
        let maxd = ((cx.powi(2) + cy.powi(2)) as f64).sqrt() as f32;
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let dx = (px as f32 - cx) / (w as f32 * 0.5);
                let dy = (py as f32 - cy) / (h as f32 * 0.5);
                // elliptical distance shaped by roundness
                let d = ((dx.powf(2.0 + rnd * 2.0) + dy.powf(2.0 + rnd * 2.0)).powf(1.0 / (2.0 + rnd * 2.0))) / 1.414;
                let t = (d * 1.6).clamp(0.0, 1.0);
                let k = if amt >= 0.0 { 1.0 - amt * t * t } else { 1.0 + (-amt) * t * t * 0.6 };
                let oo = (py as usize * w as usize + px as usize) * 4;
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..3 {
                    out[po + c] = (src[oo + c] as f32 * k).round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        let _ = maxd;
        out
    })
}

/// Bloom / soft glow: bright-pass blurred add (screen blend).
pub fn bloom(doc: &mut Document, radius: f32, intensity: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sig = radius.clamp(1.0, 80.0);
    let inten = intensity.clamp(0, 100) as f32 / 100.0;
    process(doc, region, "Bloom", move |src, w, h, rect| {
        let mut bright = src.to_vec();
        for px in bright.chunks_exact_mut(4) {
            let l = 0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32;
            if l < 160.0 {
                px[0] = 0; px[1] = 0; px[2] = 0;
            } else {
                let k = ((l - 160.0) / 95.0).clamp(0.0, 1.0);
                px[0] = (px[0] as f32 * k) as u8;
                px[1] = (px[1] as f32 * k) as u8;
                px[2] = (px[2] as f32 * k) as u8;
            }
        }
        let blurred = gaussian_blur_impl(&bright, w, h, rect, sig);
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for i in 0..out.len() / 4 {
            let po = i * 4;
            for c in 0..3 {
                let base = src[((rect.y as usize + (i / rect.w as usize)) * w as usize + rect.x as usize + (i % rect.w as usize)) * 4 + c] as f32;
                let glow = blurred[po + c] as f32;
                let screen = 255.0 - (255.0 - base) * (255.0 - glow * inten) / 255.0;
                out[po + c] = screen.round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    })
}

/// Film grain: fine animated-style grain with size parameter.
pub fn film_grain(doc: &mut Document, amount: i32, size: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let a = amount.clamp(0, 100) as f32 * 0.9;
    let cell = size.clamp(1, 8) as u64;
    process(doc, region, "Film Grain", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let mut seed = 0x853C49E6748FEA9Bu64;
        let mut next = move || {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            seed
        };
        let cellw = ((rect.w as usize) / cell.max(1) as usize).max(1);
        let rows = ((rect.h as usize) + cell as usize - 1) / cell as usize;
        let mut noise: Vec<Vec<f32>> = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut row = Vec::with_capacity(cellw + 1);
            for _ in 0..cellw + 1 {
                row.push(((next() % 2001) as f32 / 1000.0 - 1.0) * a);
            }
            noise.push(row);
        }
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let n = noise[y / cell as usize][x / cell as usize];
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                for c in 0..3 {
                    out[po + c] = (src[oo + c] as f32 + n).round().clamp(0.0, 255.0) as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

/// Scanlines / CRT retro effect.
pub fn scanlines(doc: &mut Document, spacing: i32, intensity: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sp = spacing.clamp(2, 32) as i32;
    let k = 1.0 - intensity.clamp(0, 100) as f32 / 100.0 * 0.65;
    process(doc, region, "Scanlines", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            let py = rect.y + y;
            let dark = (py % sp) >= sp / 2;
            let kk = if dark { k } else { 1.0 };
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let oo = (py as usize * w as usize + px as usize) * 4;
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..3 {
                    out[po + c] = (src[oo + c] as f32 * kk).round() as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

/// Glitch: horizontal RGB band displacement.
pub fn glitch(doc: &mut Document, strength: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let s = strength.clamp(1, 100) as i64;
    process(doc, region, "Glitch", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        let mut seed = 0xDEADBEEFu32;
        let mut rand = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 16) & 0x7fff) as i64
        };
        for y in 0..rect.h as i32 {
            let py = rect.y + y;
            let band = (py / 8) % 7 == 0;
            let shift = if band { (rand() % (s * 2 + 1)) - s } else { 0 };
            let rshift = if band { (rand() % (s + 1)) - s / 2 } else { 0 };
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let sample = |sx: i32, c: usize| -> u8 {
                    let cx = sx.clamp(0, w as i32 - 1) as usize;
                    let cy = py.clamp(0, h as i32 - 1) as usize;
                    src[(cy * w as usize + cx) * 4 + c]
                };
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                out[po] = sample(px + rshift as i32, 0);
                out[po + 1] = sample(px + (shift / 2) as i32, 1);
                out[po + 2] = sample(px + shift as i32, 2);
                out[po + 3] = src[(py.clamp(0, h as i32 - 1) as usize * w as usize + px.clamp(0, w as i32 - 1) as usize) * 4 + 3];
            }
        }
        out
    })
}

/// Chromatic aberration: radial channel separation.
pub fn chromatic_aberration(doc: &mut Document, amount: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(0, 100) as f32 / 100.0 * 14.0;
    process(doc, region, "Chromatic Aberration", move |src, w, h, rect| {
        let img = Rgba8Ref { w, h, data: src };
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let vx = px as f32 - cx;
                let vy = py as f32 - cy;
                let r = img.sample_bilinear(cx + vx * (1.0 + amt / 400.0), cy + vy * (1.0 + amt / 400.0));
                let b = img.sample_bilinear(cx - vx * (1.0 + amt / 400.0), cy - vy * (1.0 + amt / 400.0));
                let g = img.sample_bilinear(px as f32, py as f32);
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                out[po] = r[0].round().clamp(0.0, 255.0) as u8;
                out[po + 1] = g[1].round().clamp(0.0, 255.0) as u8;
                out[po + 2] = b[2].round().clamp(0.0, 255.0) as u8;
                out[po + 3] = g[3].round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    })
}

/// Duotone: luminance ramp between shadow and highlight colors.
pub fn duotone(doc: &mut Document, sr: u8, sg: u8, sb: u8, hr: u8, hg: u8, hb: u8) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    process(doc, Rect::new(0, 0, doc.w, doc.h), "Duotone", move |src, w, h, rect| {
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                let t = (0.2126 * src[oo] as f32 + 0.7152 * src[oo + 1] as f32 + 0.0722 * src[oo + 2] as f32) / 255.0;
                out[po] = (sr as f32 + (hr as f32 - sr as f32) * t).round() as u8;
                out[po + 1] = (sg as f32 + (hg as f32 - sg as f32) * t).round() as u8;
                out[po + 2] = (sb as f32 + (hb as f32 - sb as f32) * t).round() as u8;
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

/// Ripple distortion (concentric sine waves).
pub fn ripple(doc: &mut Document, amplitude: f32, wavelength: f32, cx: f32, cy: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let a = amplitude.clamp(1.0, 200.0);
    let wl = wavelength.clamp(4.0, 500.0);
    let ccx = if cx.is_finite() && cx >= 0.0 { cx } else { doc.w as f32 / 2.0 };
    let ccy = if cy.is_finite() && cy >= 0.0 { cy } else { doc.h as f32 / 2.0 };
    process(doc, region, "Ripple", move |src, w, h, rect| {
        let img = Rgba8Ref { w, h, data: src };
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let dx = px as f32 - ccx;
                let dy = py as f32 - ccy;
                let d = (dx * dx + dy * dy).sqrt();
                let off = a * (2.0 * std::f32::consts::PI * d / wl).sin();
                let s = img.sample_bilinear(ccx + dx / d.max(1e-3) * (d + off), ccy + dy / d.max(1e-3) * (d + off));
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..4 {
                    out[po + c] = s[c].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    })
}

/// Pinch / bulge: negative pinches inward, positive bulges outward.
pub fn pinch(doc: &mut Document, amount: i32, cx: f32, cy: f32, radius: f32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let amt = amount.clamp(-100, 100) as f32 / 100.0;
    let rad = if radius > 1.0 { radius } else { (doc.w.min(doc.h) as f32) * 0.5 };
    let ccx = if cx.is_finite() && cx >= 0.0 { cx } else { doc.w as f32 / 2.0 };
    let ccy = if cy.is_finite() && cy >= 0.0 { cy } else { doc.h as f32 / 2.0 };
    process(doc, region, "Pinch", move |src, w, h, rect| {
        let img = Rgba8Ref { w, h, data: src };
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as i32 {
            for x in 0..rect.w as i32 {
                let px = rect.x + x;
                let py = rect.y + y;
                let dx = px as f32 - ccx;
                let dy = py as f32 - ccy;
                let d = (dx * dx + dy * dy).sqrt();
                let (sxp, syp) = if d < rad && d > 1e-3 {
                    let t = (1.0 - d / rad).clamp(0.0, 1.0);
                    let scale = if amt >= 0.0 { 1.0 - amt * t } else { 1.0 - amt * t * 0.6 };
                    (ccx + dx * scale, ccy + dy * scale)
                } else {
                    (px as f32, py as f32)
                };
                let s = img.sample_bilinear(sxp, syp);
                let po = (y as usize * rect.w as usize + x as usize) * 4;
                for c in 0..4 {
                    out[po + c] = s[c].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    })
}

/// Render fractal clouds (fBm value noise) onto the active layer.
pub fn clouds(doc: &mut Document, scale: f32, seed: u32, opacity: i32) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
    let region = Rect::new(0, 0, doc.w, doc.h);
    let sc = scale.clamp(0.5, 64.0);
    let op = opacity.clamp(1, 100) as f32 / 100.0;
    process(doc, region, "Clouds", move |src, w, h, rect| {
        // value-noise grid + 5 octaves
        let mut seed = seed as u64 | 1;
        let mut rand = move || {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            (seed >> 11) as f32 / (1u64 << 53) as f32
        };
        let gw = ((rect.w as f32 / sc).ceil() as usize).max(2) + 3;
        let gh = ((rect.h as f32 / sc).ceil() as usize).max(2) + 3;
        let mut grid = vec![0.0f32; gw * gh];
        for g in grid.iter_mut() { *g = rand(); }
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let noise_at = |fx: f32, fy: f32| -> f32 {
            let x = fx / sc;
            let y = fy / sc;
            let x0 = x.floor() as usize;
            let y0 = y.floor() as usize;
            let tx = smooth(x - x.floor());
            let ty = smooth(y - y.floor());
            let g = |xx: usize, yy: usize| grid[yy.min(gh - 1) * gw + xx.min(gw - 1)];
            let a = g(x0, y0) * (1.0 - tx) + g(x0 + 1, y0) * tx;
            let b = g(x0, y0 + 1) * (1.0 - tx) + g(x0 + 1, y0 + 1) * tx;
            a * (1.0 - ty) + b * ty
        };
        let mut out = vec![0u8; (rect.w as usize) * (rect.h as usize) * 4];
        for y in 0..rect.h as usize {
            for x in 0..rect.w as usize {
                let fx = (rect.x + x as i32) as f32;
                let fy = (rect.y + y as i32) as f32;
                let mut v = 0.0f32;
                let mut amp = 0.5f32;
                let mut f = 1.0f32;
                for _ in 0..5 {
                    v += noise_at(fx * f, fy * f) * amp;
                    f *= 2.0;
                    amp *= 0.5;
                }
                let v = (v.clamp(0.0, 1.0) * 255.0) as u8;
                let oo = ((rect.y as usize + y) * w as usize + rect.x as usize + x) * 4;
                let po = (y * rect.w as usize + x) * 4;
                for c in 0..3 {
                    out[po + c] = (v as f32 * op + src[oo + c] as f32 * (1.0 - op)).round() as u8;
                }
                out[po + 3] = src[oo + 3];
            }
        }
        out
    })
}

impl Rgba8Ref<'_> {
    fn sample_bilinear(&self, fx: f32, fy: f32) -> [f32; 4] {
        let w = self.w as i32;
        let h = self.h as i32;
        if w == 0 || h == 0 { return [0.0; 4]; }
        let x = fx - 0.5;
        let y = fy - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let tx = x - x0;
        let ty = y - y0;
        let (xi, yi) = (x0 as i32, y0 as i32);
        let mut acc = [0.0f32; 4];
        for (dy, wy) in [(0, 1.0 - ty), (1, ty)] {
            for (dx, wx) in [(0, 1.0 - tx), (1, tx)] {
                let (cx, cy) = (xi + dx, yi + dy);
                let wgt = wx * wy;
                if wgt <= 0.0 { continue; }
                let px = if cx < 0 || cy < 0 || cx >= w || cy >= h {
                    [0u8; 4]
                } else {
                    let o = ((cy as u32 * self.w + cx as u32) * 4) as usize;
                    [self.data[o], self.data[o + 1], self.data[o + 2], self.data[o + 3]]
                };
                for c in 0..4 {
                    acc[c] += wgt * px[c] as f32;
                }
            }
        }
        acc
    }
}

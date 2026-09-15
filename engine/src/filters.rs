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

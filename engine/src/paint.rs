//! Paint tools: paint bucket (flood fill), gradient, fill selection/layer.
use crate::img::{Gray8, Rect};
use crate::layer::Document;

/// Paint bucket: flood fill on the active layer with tolerance.
pub fn bucket(doc: &mut Document, sx: i32, sy: i32, color: [u8; 4], tolerance: i32, contiguous: bool) -> Option<Rect> {
    if sx < 0 || sy < 0 || sx >= doc.w as i32 || sy >= doc.h as i32 {
        return None;
    }
    let w = doc.w as usize;
    let layer_id = doc.active_id;
    let src = doc.get_layer(layer_id)?.pixels.data.clone();
    let seed = ((sy as usize) * w + sx as usize) * 4;
    let target = [src[seed], src[seed + 1], src[seed + 2], src[seed + 3]];
    let tol = tolerance.clamp(0, 255) as i64;
    let tol2 = tol * tol * 4;
    #[inline]
    fn diff2(a: &[u8], b: &[u8; 4]) -> i64 {
        let dr = a[0] as i64 - b[0] as i64;
        let dg = a[1] as i64 - b[1] as i64;
        let db = a[2] as i64 - b[2] as i64;
        let da = a[3] as i64 - b[3] as i64;
        dr * dr + dg * dg + db * db + da * da
    }

    let sel = doc.selection.as_gray();
    let mut changed = Rect::default();

    {
        let (dw, dh) = (doc.w, doc.h);
        let layer = doc.get_layer_mut(layer_id)?;
        if contiguous {
            let w32 = dw as i32;
            let h32 = dh as i32;
            let mut visited = vec![false; w * dh as usize];
            let mut stack = vec![(sx, sy)];
            while let Some((x, y)) = stack.pop() {
                if x < 0 || y < 0 || x >= w32 || y >= h32 { continue; }
                let idx = (y as usize) * w + x as usize;
                if visited[idx] { continue; }
                let o = idx * 4;
                if diff2(&src[o..o + 4], &target) > tol2 { continue; }
                let mut lx = x;
                while lx >= 0 {
                    let i2 = (y as usize) * w + lx as usize;
                    if visited[i2] { break; }
                    let o2 = i2 * 4;
                    if diff2(&src[o2..o2 + 4], &target) > tol2 { break; }
                    lx -= 1;
                }
                lx += 1;
                let mut rx = x;
                while rx < w32 {
                    let i2 = (y as usize) * w + rx as usize;
                    if visited[i2] { break; }
                    let o2 = i2 * 4;
                    if diff2(&src[o2..o2 + 4], &target) > tol2 { break; }
                    rx += 1;
                }
                for xx in lx..rx {
                    let i2 = (y as usize) * w + xx as usize;
                    visited[i2] = true;
                    if let Some(s) = &sel {
                        if s.data[i2] == 0 { continue; }
                    }
                    if target != color {
                        layer.pixels.data[i2 * 4..i2 * 4 + 4].copy_from_slice(&color);
                        changed = changed.union(&Rect::new(xx, y, 1, 1));
                    }
                    if y > 0 && !visited[i2 - w] { stack.push((xx, y - 1)); }
                    if y < h32 - 1 && !visited[i2 + w] { stack.push((xx, y + 1)); }
                }
            }
        } else {
            for i in 0..w * dh as usize {
                if diff2(&src[i * 4..i * 4 + 4], &target) <= tol2 {
                    if let Some(s) = &sel {
                        if s.data[i] == 0 { continue; }
                    }
                    if target != color {
                        layer.pixels.data[i * 4..i * 4 + 4].copy_from_slice(&color);
                        let x = (i % w) as i32;
                        let y = (i / w) as i32;
                        changed = changed.union(&Rect::new(x, y, 1, 1));
                    }
                }
            }
        }
    }
    if changed.is_empty() {
        None
    } else {
        doc.mark_dirty(changed);
        Some(changed)
    }
}

/// Fill (active layer, selection or whole layer).
pub fn fill(doc: &mut Document, color: [u8; 4]) -> Option<Rect> {
    let layer_id = doc.active_id;
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let sel = doc.selection.as_gray();
    {
        let layer = doc.get_layer_mut(layer_id)?;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                if let Some(s) = &sel {
                    if s.get(x, y) == 0 { continue; }
                }
                layer.pixels.set(x, y, color);
            }
        }
    }
    doc.mark_dirty(region);
    Some(region)
}

/// Gradient kinds.
#[derive(Clone, Copy, PartialEq)]
pub enum GradientKind { Linear, Radial }

#[derive(Clone, Copy, PartialEq)]
pub enum GradientFill { FgBg, FgTransparent }

/// Draw a gradient on the active layer (respects selection).
pub fn gradient(doc: &mut Document, x0: f32, y0: f32, x1: f32, y1: f32, kind: GradientKind, fill: GradientFill, fg: [u8; 4], bg: [u8; 4], dither: bool) -> Option<Rect> {
    let layer_id = doc.active_id;
    let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
    let sel = doc.selection.as_gray();
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    let max_r = len2.sqrt().max(1.0);
    {
        let layer = doc.get_layer_mut(layer_id)?;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                if let Some(s) = &sel {
                    if s.get(x, y) == 0 { continue; }
                }
                let t = match kind {
                    GradientKind::Linear => {
                        if len2 <= 1e-6 { 0.0 } else {
                            (((x as f32 + 0.5 - x0) * dx + (y as f32 + 0.5 - y0) * dy) / len2).clamp(0.0, 1.0)
                        }
                    }
                    GradientKind::Radial => {
                        let r = ((x as f32 + 0.5 - x0).powi(2) + (y as f32 + 0.5 - y0).powi(2)).sqrt();
                        (r / max_r).clamp(0.0, 1.0)
                    }
                };
                let mut t = t;
                if dither {
                    // ordered dithering 4x4
                    const M: [f32; 16] = [0.0, 0.5, 0.125, 0.625, 0.75, 0.25, 0.875, 0.375, 0.1875, 0.6875, 0.0625, 0.5625, 0.9375, 0.4375, 0.8125, 0.3125];
                    let d = M[((y & 3) * 4 + (x & 3)) as usize] - 0.5;
                    t = (t + d / 255.0).clamp(0.0, 1.0);
                }
                let a: [f32; 4] = match fill {
                    GradientFill::FgBg => fg.map(|v| v as f32),
                    GradientFill::FgTransparent => [fg[0] as f32, fg[1] as f32, fg[2] as f32, fg[3] as f32],
                };
                let b: [f32; 4] = match fill {
                    GradientFill::FgBg => bg.map(|v| v as f32),
                    GradientFill::FgTransparent => [fg[0] as f32, fg[1] as f32, fg[2] as f32, 0.0],
                };
                let px = [
                    (a[0] + (b[0] - a[0]) * t).round().clamp(0.0, 255.0) as u8,
                    (a[1] + (b[1] - a[1]) * t).round().clamp(0.0, 255.0) as u8,
                    (a[2] + (b[2] - a[2]) * t).round().clamp(0.0, 255.0) as u8,
                    (a[3] + (b[3] - a[3]) * t).round().clamp(0.0, 255.0) as u8,
                ];
                layer.pixels.set(x, y, px);
            }
        }
    }
    doc.mark_dirty(region);
    Some(region)
}

/// Mask helper type re-export for other modules.
pub type SelMask = Gray8;

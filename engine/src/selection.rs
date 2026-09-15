//! Selection: rect/ellipse/lasso/wand masks, feather, invert, contour extraction.
use crate::img::{Gray8, Rect};
use crate::layer::Document;
use rayon::prelude::*;

/// Build rect selection mask.
pub fn rect_mask(doc: &Document, x: i32, y: i32, w: i32, h: i32) -> Gray8 {
    let mut m = Gray8::zeros(doc.w, doc.h);
    let r = Rect::new(x, y, w.max(0) as u32, h.max(0) as u32).clip_to(doc.w, doc.h);
    for yy in r.y..r.bottom() {
        for xx in r.x..r.right() {
            m.set(xx, yy, 255);
        }
    }
    m
}

/// Build ellipse selection mask with anti-aliased edges.
pub fn ellipse_mask(doc: &Document, x: i32, y: i32, w: i32, h: i32) -> Gray8 {
    let mut m = Gray8::zeros(doc.w, doc.h);
    if w <= 0 || h <= 0 {
        return m;
    }
    let cx = x as f32 + w as f32 / 2.0;
    let cy = y as f32 + h as f32 / 2.0;
    let rx = w as f32 / 2.0;
    let ry = h as f32 / 2.0;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(doc.w as i32);
    let y1 = (y + h).min(doc.h as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            // signed distance approx
            let dx = (px as f32 + 0.5 - cx) / rx;
            let dy = (py as f32 + 0.5 - cy) / ry;
            let d = (dx * dx + dy * dy).sqrt();
            // AA over ~1px boundary: derivative of ellipse metric varies; approximate with local scale
            let edge = 1.0 / (dx * dx / (rx * rx) + dy * dy / (ry * ry)).sqrt().max(1e-6);
            let aa = (edge * 0.7).clamp(0.5, 2.0);
            let v = ((1.0 - (d - 1.0) * aa).clamp(0.0, 1.0) * 255.0) as u8;
            if v > 0 {
                m.set(px, py, v);
            }
        }
    }
    m
}

/// Build lasso (polygon) selection mask — even-odd scanline fill.
pub fn polygon_mask(doc: &Document, pts: &[(f32, f32)]) -> Gray8 {
    let mut m = Gray8::zeros(doc.w, doc.h);
    if pts.len() < 3 {
        return m;
    }
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(doc.h as f32 - 1.0) as i32;
    let n = pts.len();
    for py in min_y..=max_y {
        let sy = py as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..n {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % n];
            if (y0 <= sy && y1 > sy) || (y1 <= sy && y0 > sy) {
                let t = (sy - y0) / (y1 - y0);
                xs.push(x0 + t * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut k = 0;
        while k + 1 < xs.len() {
            let xa = xs[k].ceil().max(0.0) as i32;
            let xb = (xs[k + 1] - 0.001).floor().min(doc.w as f32 - 1.0) as i32;
            for px in xa..=xb {
                m.set(px, py, 255);
            }
            k += 2;
        }
    }
    m
}

/// Magic wand: flood fill selection from a seed point based on color tolerance.
/// `sample`: 0 = composite, 1 = active layer.
pub fn wand_mask(doc: &Document, sx: i32, sy: i32, tolerance: i32, contiguous: bool, sample_layer: bool) -> Gray8 {
    let mut m = Gray8::zeros(doc.w, doc.h);
    if sx < 0 || sy < 0 || sx >= doc.w as i32 || sy >= doc.h as i32 {
        return m;
    }
    let reference: Vec<u8> = if sample_layer {
        let active = doc.active_id;
        match doc.get_layer(active) {
            Some(l) => l.pixels.data.clone(),
            None => return m,
        }
    } else {
        doc.flatten().data
    };
    let w = doc.w as usize;
    let tol = tolerance.clamp(0, 255) as i32;
    let seed = ((sy as usize) * w + sx as usize) * 4;
    let target = [reference[seed], reference[seed + 1], reference[seed + 2], reference[seed + 3]];

    let tol2 = (tol * tol * 4) as i64;
    #[inline]
    fn diff2(a: &[u8], b: &[u8; 4]) -> i64 {
        let dr = a[0] as i64 - b[0] as i64;
        let dg = a[1] as i64 - b[1] as i64;
        let db = a[2] as i64 - b[2] as i64;
        let da = a[3] as i64 - b[3] as i64;
        dr * dr + dg * dg + db * db + da * da
    }

    if contiguous {
        // scanline flood fill
        let w32 = doc.w as i32;
        let h32 = doc.h as i32;
        let mut visited = vec![false; w * doc.h as usize];
        let mut stack = vec![(sx, sy)];
        while let Some((x, y)) = stack.pop() {
            if x < 0 || y < 0 || x >= w32 || y >= h32 { continue; }
            let idx = (y as usize) * w + x as usize;
            if visited[idx] { continue; }
            let px_off = idx * 4;
            let px = [reference[px_off], reference[px_off + 1], reference[px_off + 2], reference[px_off + 3]];
            if diff2(&reference[px_off..px_off + 4], &target) > tol2 { continue; }
            // mark whole scanline run
            let mut lx = x;
            while lx >= 0 {
                let i2 = (y as usize) * w + lx as usize;
                if visited[i2] { break; }
                let o2 = i2 * 4;
                if diff2(&reference[o2..o2 + 4], &target) > tol2 { break; }
                lx -= 1;
            }
            lx += 1;
            let mut rx = x;
            while rx < w32 {
                let i2 = (y as usize) * w + rx as usize;
                if visited[i2] { break; }
                let o2 = i2 * 4;
                if diff2(&reference[o2..o2 + 4], &target) > tol2 { break; }
                rx += 1;
            }
            for xx in lx..rx {
                let i2 = (y as usize) * w + xx as usize;
                visited[i2] = true;
                m.data[i2] = 255;
                if y > 0 {
                    let iu = ((y - 1) as usize) * w + xx as usize;
                    if !visited[iu] { stack.push((xx, y - 1)); }
                }
                if y < h32 - 1 {
                    let id = ((y + 1) as usize) * w + xx as usize;
                    if !visited[id] { stack.push((xx, y + 1)); }
                }
            }
        }
    } else {
        // global: every matching pixel
        m.data.par_iter_mut().enumerate().for_each(|(i, v)| {
            let o = i * 4;
            *v = if diff2(&reference[o..o + 4], &target) <= tol2 { 255 } else { 0 };
        });
    }
    m
}

/// Gaussian-ish feather: 3 box blurs on the mask.
pub fn feather(mask: &mut Gray8, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    let boxes = boxes_for_gauss(radius, 3);
    for r in boxes {
        if r < 1 { continue; }
        box_blur_h(mask, r as usize);
        box_blur_v(mask, r as usize);
    }
}

fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<u32> {
    let w = ((3.0 * ((sigma * (2.0 * std::f32::consts::PI / n as f32).sqrt()) / 2.0 + 0.5)) as u32).max(1);
    vec![w; n]
}

fn box_blur_h(m: &mut Gray8, r: usize) {
    let w = m.w as usize;
    let h = m.h as usize;
    if w < 2 || h < 2 { return; }
    let src = m.data.clone();
    let inv = 1.0 / (r * 2 + 1) as f32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let mut acc = row[0] as u32 * (r as u32 + 1);
        for i in 0..r {
            acc += row[i.min(w - 1)] as u32;
        }
        for x in 0..w {
            m.data[y * w + x] = (acc as f32 * inv).round() as u8;
            let add = row[(x + r + 1).min(w - 1)] as u32;
            let sub = row[x.saturating_sub(r)] as u32;
            acc = acc + add - sub;
        }
    }
}

fn box_blur_v(m: &mut Gray8, r: usize) {
    let w = m.w as usize;
    let h = m.h as usize;
    if w < 2 || h < 2 { return; }
    let src = m.data.clone();
    let inv = 1.0 / (r * 2 + 1) as f32;
    for x in 0..w {
        let mut acc: u32 = src[x] as u32 * (r as u32 + 1);
        for i in 0..r {
            acc += src[(i.min(h - 1)) * w + x] as u32;
        }
        for y in 0..h {
            m.data[y * w + x] = (acc as f32 * inv).round() as u8;
            let add = src[((y + r + 1).min(h - 1)) * w + x] as u32;
            let sub = src[y.saturating_sub(r) * w + x] as u32;
            acc = acc + add - sub;
        }
    }
}

/// Invert selection mask.
pub fn invert(doc: &mut Document) {
    let n = (doc.w * doc.h) as usize;
    if doc.selection.data.len() != n {
        doc.selection = crate::layer::Selection::none(doc.w, doc.h);
    }
    for v in doc.selection.data.iter_mut() {
        *v = 255 - *v;
    }
    doc.selection.active = doc.selection.data.iter().any(|&v| v > 0);
}

/// Selection contour polylines (marching squares, thresholded at >0), simplified.
pub fn contour(doc: &Document) -> Vec<Vec<(f32, f32)>> {
    if !doc.selection.active {
        return Vec::new();
    }
    let w = doc.w as i32;
    let h = doc.h as i32;
    let at = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w || y >= h { return false; }
        doc.selection.data[(y as usize) * (w as usize) + x as usize] > 127
    };
    // collect boundary edges as unit segments, then chain them into loops
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct Pt(i32, i32);
    use std::collections::HashMap as Map;
    let mut next: Map<Pt, Pt> = Map::new();
    for y in -1..h {
        for x in -1..w {
            // for each cell corner arrangement add boundary segments
            let tl = at(x, y);
            let tr = at(x + 1, y);
            let bl = at(x, y + 1);
            let br = at(x + 1, y + 1);
            let mut edges: Vec<(Pt, Pt)> = Vec::new();
            let t = Pt(x, y);
            let r = Pt(x + 1, y);
            let b = Pt(x + 1, y + 1);
            let l = Pt(x, y + 1);
            let top = tl != tr;
            let right = tr != br;
            let bottom = bl != br;
            let left = tl != bl;
            if top { edges.push((t, r)); }
            if right { edges.push((r, b)); }
            if bottom { edges.push((b, l)); }
            if left { edges.push((l, t)); }
            if edges.len() == 2 {
                // keep orientation: inside on the left
                next.insert(edges[0].0, edges[0].1);
                next.insert(edges[1].0, edges[1].1);
            } else if edges.len() == 4 {
                // saddle: pick consistent pairing
                next.insert(t, r);
                next.insert(r, t);
                next.insert(b, l);
                next.insert(l, b);
            }
        }
    }
    let mut loops: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut visited: Map<Pt, bool> = Map::new();
    for start in next.keys() {
        if *visited.get(start).unwrap_or(&false) { continue; }
        let mut loop_pts: Vec<(f32, f32)> = Vec::new();
        let mut cur = *start;
        loop {
            if *visited.get(&cur).unwrap_or(&false) { break; }
            visited.insert(cur, true);
            loop_pts.push((cur.0 as f32 + 0.5, cur.1 as f32 + 0.5));
            match next.get(&cur) {
                Some(&n) => {
                    if n == *start { break; }
                    cur = n;
                }
                None => break,
            }
        }
        if loop_pts.len() >= 3 {
            let simp = simplify(&loop_pts, 0.8);
            if simp.len() >= 3 {
                loops.push(simp);
            }
        }
    }
    loops
}

/// Ramer–Douglas–Peucker simplification.
fn simplify(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 { continue; }
        let (ax, ay) = pts[a];
        let (bx, by) = pts[b];
        let dx = bx - ax;
        let dy = by - ay;
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let mut best = 0usize;
        let mut best_d = -1.0f32;
        for i in a + 1..b {
            let (px, py) = pts[i];
            let d = ((px - ax) * dy - (py - ay) * dx).abs() / len;
            if d > best_d {
                best_d = d;
                best = i;
            }
        }
        if best_d > eps {
            keep[best] = true;
            stack.push((a, best));
            stack.push((best, b));
        }
    }
    pts.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, p)| *p).collect()
}

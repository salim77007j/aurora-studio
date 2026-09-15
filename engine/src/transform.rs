//! Geometric transforms: resize, canvas ops, crop, flip, rotate, affine & perspective warps.
use crate::img::{Rgba8, Rect};
use crate::layer::Document;
use rayon::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Interp { Nearest, Bilinear, Bicubic }

/// Resize one RGBA image.
pub fn resize_image(src: &Rgba8, nw: u32, nh: u32, interp: Interp) -> Rgba8 {
    let nw = nw.max(1);
    let nh = nh.max(1);
    if src.w == nw && src.h == nh {
        return src.clone();
    }
    let mut out = Rgba8::new(nw, nh);
    let sx = src.w as f32 / nw as f32;
    let sy = src.h as f32 / nh as f32;
    let rows: Vec<Vec<u8>> = (0..nh).into_par_iter().map(|y| {
        let mut row = vec![0u8; (nw as usize) * 4];
        for x in 0..nw {
            let fx = (x as f32 + 0.5) * sx;
            let fy = (y as f32 + 0.5) * sy;
            let px: [u8; 4] = match interp {
                Interp::Nearest => src.get(fx as i32, fy as i32),
                Interp::Bilinear => src.sample_bilinear(fx, fy).map(|v| v.round().clamp(0.0, 255.0) as u8),
                Interp::Bicubic => src.sample_bicubic(fx, fy).map(|v| v.round().clamp(0.0, 255.0) as u8),
            };
            row[(x as usize) * 4..(x as usize) * 4 + 4].copy_from_slice(&px);
        }
        row
    }).collect();
    for (y, row) in rows.into_iter().enumerate() {
        out.data[y * (nw as usize) * 4..(y + 1) * (nw as usize) * 4].copy_from_slice(&row);
    }
    out
}

fn rgba_to_gray(img: &Rgba8) -> crate::img::Gray8 {
    crate::img::Gray8 { w: img.w, h: img.h, data: img.data.chunks_exact(4).map(|c| c[0]).collect() }
}

fn gray_to_rgba(g: &crate::img::Gray8) -> Rgba8 {
    let mut out = Rgba8::new(g.w, g.h);
    for (i, &v) in g.data.iter().enumerate() {
        out.data[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
    }
    out
}

/// Resize whole document image (all layers + masks), scales content, changes canvas size.
pub fn resize_doc_image(doc: &mut Document, nw: u32, nh: u32, interp: Interp) {
    let nw = nw.clamp(1, 16384);
    let nh = nh.clamp(1, 16384);
    let ids = doc.layer_ids();
    for id in ids {
        if let Some(l) = doc.get_layer_mut(id) {
            l.pixels = resize_image(&l.pixels, nw, nh, interp);
            if let Some(m) = &l.mask {
                let rgba = gray_to_rgba(m);
                l.mask = Some(rgba_to_gray(&resize_image(&rgba, nw, nh, interp)));
            }
        }
    }
    doc.w = nw;
    doc.h = nh;
    doc.selection = crate::layer::Selection::none(nw, nh);
    doc.composite_reset();
}

/// Resize canvas without scaling content (anchor: 0..8 = 3x3 grid, 4 = center).
pub fn resize_canvas(doc: &mut Document, nw: u32, nh: u32, anchor: i32) {
    let nw = nw.clamp(1, 16384);
    let nh = nh.clamp(1, 16384);
    let cols = anchor % 3; // 0 left, 1 center, 2 right
    let rows = anchor / 3; // 0 top, 1 middle, 2 bottom
    let dx: i64 = match cols {
        0 => 0,
        1 => ((nw as f64 - doc.w as f64) / 2.0).round() as i64,
        _ => nw as i64 - doc.w as i64,
    };
    let dy: i64 = match rows {
        0 => 0,
        1 => ((nh as f64 - doc.h as f64) / 2.0).round() as i64,
        _ => nh as i64 - doc.h as i64,
    };
    let ids = doc.layer_ids();
    for id in ids {
        if let Some(l) = doc.get_layer_mut(id) {
            l.pixels = translate_image(&l.pixels, nw, nh, dx as i32, dy as i32);
            if let Some(m) = &l.mask {
                let rgba = gray_to_rgba(m);
                let moved = translate_image(&rgba, nw, nh, dx as i32, dy as i32);
                l.mask = Some(rgba_to_gray(&moved));
            }
        }
    }
    doc.w = nw;
    doc.h = nh;
    doc.selection = crate::layer::Selection::none(nw, nh);
    doc.composite_reset();
}

/// Copy image into new canvas of (nw, nh) offset by (dx, dy).
pub fn translate_image(src: &Rgba8, nw: u32, nh: u32, dx: i32, dy: i32) -> Rgba8 {
    let mut out = Rgba8::new(nw.max(1), nh.max(1));
    let sx0 = 0i64.max(-dx as i64);
    let sy0 = 0i64.max(-dy as i64);
    let sx1 = (src.w as i64).min(nw as i64 - dx as i64);
    let sy1 = (src.h as i64).min(nh as i64 - dy as i64);
    if sx1 > sx0 && sy1 > sy0 {
        for y in sy0..sy1 {
            let src_off = (y as usize) * (src.w as usize) + sx0 as usize;
            let dst_off = ((y + dy as i64) as usize) * (out.w as usize) + (sx0 + dx as i64) as usize;
            let len = (sx1 - sx0) as usize * 4;
            out.data[dst_off * 4..dst_off * 4 + len].copy_from_slice(&src.data[src_off * 4..src_off * 4 + len]);
        }
    }
    out
}

/// Crop to rect.
pub fn crop(doc: &mut Document, x: i32, y: i32, w: u32, h: u32) {
    let w = w.clamp(1, 16384);
    let h = h.clamp(1, 16384);
    let ids = doc.layer_ids();
    for id in ids {
        if let Some(l) = doc.get_layer_mut(id) {
            l.pixels = crop_image(&l.pixels, x, y, w, h);
            if let Some(m) = &l.mask {
                let rgba = gray_to_rgba(m);
                l.mask = Some(rgba_to_gray(&crop_image(&rgba, x, y, w, h)));
            }
        }
    }
    doc.w = w;
    doc.h = h;
    doc.selection = crate::layer::Selection::none(w, h);
    doc.composite_reset();
}

pub fn crop_image(src: &Rgba8, x: i32, y: i32, w: u32, h: u32) -> Rgba8 {
    let mut out = Rgba8::new(w.max(1), h.max(1));
    for ty in 0..h as i32 {
        for tx in 0..w as i32 {
            let px = src.get(x + tx, y + ty);
            out.set(tx, ty, px);
        }
    }
    out
}

/// Flip layer. axis: 0 = horizontal, 1 = vertical.
pub fn flip_layer(doc: &mut Document, layer_id: u64, axis: i32) {
    if let Some(l) = doc.get_layer_mut(layer_id) {
        l.pixels = flip_image(&l.pixels, axis);
        if let Some(m) = &l.mask {
            let rgba = gray_to_rgba(m);
            l.mask = Some(rgba_to_gray(&flip_image(&rgba, axis)));
        }
    }
    doc.mark_dirty(Rect::new(0, 0, doc.w, doc.h));
}

pub fn flip_image(src: &Rgba8, axis: i32) -> Rgba8 {
    let mut out = Rgba8::new(src.w, src.h);
    for y in 0..src.h as i32 {
        for x in 0..src.w as i32 {
            let (sx, sy) = if axis == 0 {
                (src.w as i32 - 1 - x, y)
            } else {
                (x, src.h as i32 - 1 - y)
            };
            out.set(x, y, src.get(sx, sy));
        }
    }
    out
}

/// Rotate arbitrary angle (degrees, clockwise) around layer center, expanding to fit if `expand`.
pub fn rotate_layer(doc: &mut Document, layer_id: u64, degrees: f32, expand: bool) {
    let Some(src) = doc.get_layer(layer_id).map(|l| (l.pixels.clone(), l.mask.clone())) else { return };
    let (src_img, src_mask) = src;
    let (w, h) = (src_img.w, src_img.h);
    let rad = degrees.to_radians();
    let sa = rad.sin();
    let ca = rad.cos();
    let (nw, nh) = if expand {
        let nw = (w as f32 * ca.abs() + h as f32 * sa.abs()).ceil() as u32;
        let nh = (w as f32 * sa.abs() + h as f32 * ca.abs()).ceil() as u32;
        (nw, nh)
    } else {
        (w, h)
    };
    let rotated = rotate_image(&src_img, degrees, nw, nh);
    let mask_rot = src_mask.map(|m| rgba_to_gray(&rotate_image(&gray_to_rgba(&m), degrees, nw, nh)));
    if let Some(l) = doc.get_layer_mut(layer_id) {
        l.pixels = rotated;
        l.mask = mask_rot;
    }
    if expand && (nw > doc.w || nh > doc.h) {
        let ndw = nw.max(doc.w);
        let ndh = nh.max(doc.h);
        grow_doc_to(doc, ndw, ndh);
    }
    doc.mark_dirty(Rect::new(0, 0, doc.w.max(nw), doc.h.max(nh)));
}

fn grow_doc_to(doc: &mut Document, nw: u32, nh: u32) {
    let ids = doc.layer_ids();
    for id in ids {
        if let Some(l) = doc.get_layer_mut(id) {
            l.pixels = translate_image(&l.pixels, nw, nh, 0, 0);
            if let Some(m) = &l.mask {
                let mut g = crate::img::Gray8 { w: nw, h: nh, data: vec![0; (nw * nh) as usize] };
                for y in 0..m.h.min(nh) {
                    for x in 0..m.w.min(nw) {
                        g.data[(y * nw + x) as usize] = m.get(x as i32, y as i32);
                    }
                }
                l.mask = Some(g);
            }
        }
    }
    doc.w = nw;
    doc.h = nh;
}

/// Rotate image by degrees clockwise about the SOURCE image center; output canvas nw x nh
/// with the source center placed at output center.
pub fn rotate_image(src: &Rgba8, degrees: f32, nw: u32, nh: u32) -> Rgba8 {
    let rad = -degrees.to_radians(); // inverse mapping rotates backwards
    let sa = rad.sin();
    let ca = rad.cos();
    let cx_out = nw as f32 / 2.0;
    let cy_out = nh as f32 / 2.0;
    let cx_src = src.w as f32 / 2.0;
    let cy_src = src.h as f32 / 2.0;
    let rows: Vec<Vec<u8>> = (0..nh).into_par_iter().map(|y| {
        let mut row = vec![0u8; (nw as usize) * 4];
        for x in 0..nw {
            let dx2 = x as f32 + 0.5 - cx_out;
            let dy2 = y as f32 + 0.5 - cy_out;
            let sx = ca * dx2 - sa * dy2 + cx_src;
            let sy = sa * dx2 + ca * dy2 + cy_src;
            let px = src.sample_bilinear(sx, sy).map(|v| v.round().clamp(0.0, 255.0) as u8);
            row[(x as usize) * 4..(x as usize) * 4 + 4].copy_from_slice(&px);
        }
        row
    }).collect();
    let mut out = Rgba8::new(nw.max(1), nh.max(1));
    for (y, row) in rows.into_iter().enumerate() {
        out.data[y * (nw as usize) * 4..(y + 1) * (nw as usize) * 4].copy_from_slice(&row);
    }
    out
}

/// Affine warp of a layer by matrix [[a, b, tx], [c, d, ty]] into nw x nh canvas.
pub fn affine_layer(doc: &mut Document, layer_id: u64, m: [f32; 6], nw: u32, nh: u32) {
    let Some(src) = doc.get_layer(layer_id).map(|l| (l.pixels.clone(), l.mask.clone())) else { return };
    let (src_img, src_mask) = src;
    let warped = warp_image(&src_img, m, nw, nh);
    let mask_warped = src_mask.map(|mk| rgba_to_gray(&warp_image(&gray_to_rgba(&mk), m, nw, nh)));
    if let Some(l) = doc.get_layer_mut(layer_id) {
        l.pixels = warped;
        l.mask = mask_warped;
    }
    doc.w = nw;
    doc.h = nh;
    doc.mark_dirty(Rect::new(0, 0, nw, nh));
}

/// Warp image with forward matrix m = [a b tx c d ty]; inverse-mapped, bilinear.
pub fn warp_image(src: &Rgba8, m: [f32; 6], nw: u32, nh: u32) -> Rgba8 {
    let nw = nw.max(1);
    let nh = nh.max(1);
    let (a, b, tx, c, d, ty) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let det = a * d - b * c;
    let mut out = Rgba8::new(nw, nh);
    if det.abs() < 1e-12 {
        return out;
    }
    let ia = d / det;
    let ib = -b / det;
    let ic = -c / det;
    let id = a / det;
    let rows: Vec<Vec<u8>> = (0..nh).into_par_iter().map(|y| {
        let mut row = vec![0u8; (nw as usize) * 4];
        for x in 0..nw {
            let px = x as f32 + 0.5 - tx;
            let py = y as f32 + 0.5 - ty;
            let sx = ia * px + ib * py;
            let sy = ic * px + id * py;
            let v = src.sample_bilinear(sx, sy).map(|v| v.round().clamp(0.0, 255.0) as u8);
            row[(x as usize) * 4..(x as usize) * 4 + 4].copy_from_slice(&v);
        }
        row
    }).collect();
    for (y, row) in rows.into_iter().enumerate() {
        out.data[y * (nw as usize) * 4..(y + 1) * (nw as usize) * 4].copy_from_slice(&row);
    }
    out
}

/// Perspective warp: map src rect corners to the 4 given destination points.
pub fn perspective_warp(doc: &mut Document, layer_id: u64, corners: [(f32, f32); 4], nw: u32, nh: u32) {
    let Some(src) = doc.get_layer(layer_id).map(|l| (l.pixels.w as f32, l.pixels.h as f32, l.pixels.clone(), l.mask.clone())) else { return };
    let (w, h, src_img, src_mask) = src;
    let dst = corners;
    let srcp = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    let Some(hm) = solve_homography(&srcp, &dst) else { return };
    let inv = invert_h(&hm);
    let warped = persp_image(&src_img, &inv, nw, nh);
    let mask_warped = src_mask.map(|mk| rgba_to_gray(&persp_image(&gray_to_rgba(&mk), &inv, nw, nh)));
    if let Some(l) = doc.get_layer_mut(layer_id) {
        l.pixels = warped;
        l.mask = mask_warped;
    }
    doc.w = nw;
    doc.h = nh;
    doc.mark_dirty(Rect::new(0, 0, nw, nh));
}

fn solve_homography(src: &[(f32, f32); 4], dst: &[(f32, f32); 4]) -> Option<[f64; 9]> {
    let mut m = [[0.0f64; 10]; 8];
    for i in 0..4 {
        let (x, y) = (src[i].0 as f64, src[i].1 as f64);
        let (u, v) = (dst[i].0 as f64, dst[i].1 as f64);
        m[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, 0.0, u];
        m[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, 0.0, v];
    }
    for col in 0..8 {
        let mut piv = col;
        for r in col + 1..8 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-10 {
            return None;
        }
        m.swap(col, piv);
        let pv = m[col][col];
        for r in 0..8 {
            if r != col && m[r][col].abs() > 1e-14 {
                let f = m[r][col] / pv;
                for c2 in col..10 {
                    m[r][c2] -= f * m[col][c2];
                }
            }
        }
    }
    let mut h = [0.0f64; 9];
    for i in 0..8 {
        h[i] = m[i][9] / m[i][i];
    }
    h[8] = 1.0;
    Some(h)
}

fn invert_h(h: &[f64; 9]) -> [f64; 9] {
    let [a, b, c, d, e, f, g, i, j] = *h;
    let det = a * (e * j - f * i) - b * (d * j - f * g) + c * (d * i - e * g);
    if det.abs() < 1e-12 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    }
    let id = 1.0 / det;
    [
        (e * j - f * i) * id, (c * i - b * j) * id, (b * f - c * e) * id,
        (f * g - d * j) * id, (a * j - c * g) * id, (c * d - a * f) * id,
        (d * i - e * g) * id, (b * g - a * i) * id, (a * e - b * d) * id,
    ]
}

fn persp_image(src: &Rgba8, inv: &[f64; 9], nw: u32, nh: u32) -> Rgba8 {
    let nw = nw.max(1);
    let nh = nh.max(1);
    let rows: Vec<Vec<u8>> = (0..nh).into_par_iter().map(|y| {
        let mut row = vec![0u8; (nw as usize) * 4];
        for x in 0..nw {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let k = inv[6] * px + inv[7] * py + inv[8];
            if k.abs() < 1e-12 {
                continue;
            }
            let sx = ((inv[0] * px + inv[1] * py + inv[2]) / k) as f32;
            let sy = ((inv[3] * px + inv[4] * py + inv[5]) / k) as f32;
            let v = src.sample_bilinear(sx, sy).map(|v| v.round().clamp(0.0, 255.0) as u8);
            row[(x as usize) * 4..(x as usize) * 4 + 4].copy_from_slice(&v);
        }
        row
    }).collect();
    let mut out = Rgba8::new(nw, nh);
    for (y, row) in rows.into_iter().enumerate() {
        out.data[y * (nw as usize) * 4..(y + 1) * (nw as usize) * 4].copy_from_slice(&row);
    }
    out
}

/// Rotate the whole document by 90° CW turns.
pub fn rotate_doc(doc: &mut Document, quarter_turns_cw: i32) {
    let turns = quarter_turns_cw.rem_euclid(4);
    if turns == 0 { return; }
    let (nw, nh) = if turns % 2 == 1 { (doc.h, doc.w) } else { (doc.w, doc.h) };
    let ids = doc.layer_ids();
    for id in ids {
        if let Some(l) = doc.get_layer_mut(id) {
            l.pixels = rotate90(&l.pixels, turns);
            if let Some(m) = &l.mask {
                let rgba = gray_to_rgba(m);
                l.mask = Some(rgba_to_gray(&rotate90(&rgba, turns)));
            }
        }
    }
    doc.w = nw;
    doc.h = nh;
    doc.selection = crate::layer::Selection::none(nw, nh);
    doc.composite_reset();
}

fn rotate90(src: &Rgba8, turns: i32) -> Rgba8 {
    match turns.rem_euclid(4) {
        0 => src.clone(),
        1 => {
            // CW 90: new(x', y') where x' = H-1-y, y' = x
            let nw = src.h;
            let nh = src.w;
            let mut out = Rgba8::new(nw, nh);
            for y in 0..src.h as i32 {
                for x in 0..src.w as i32 {
                    out.set(nh as i32 - 1 - y, x, src.get(x, y));
                }
            }
            out
        }
        2 => {
            let mut out = Rgba8::new(src.w, src.h);
            for y in 0..src.h as i32 {
                for x in 0..src.w as i32 {
                    out.set(src.w as i32 - 1 - x, src.h as i32 - 1 - y, src.get(x, y));
                }
            }
            out
        }
        _ => {
            // CCW 90
            let nw = src.h;
            let nh = src.w;
            let mut out = Rgba8::new(nw, nh);
            for y in 0..src.h as i32 {
                for x in 0..src.w as i32 {
                    out.set(y, src.w as i32 - 1 - x, src.get(x, y));
                }
            }
            out
        }
    }
}

/// Move layer content by (dx, dy).
pub fn move_layer(doc: &mut Document, layer_id: u64, dx: i32, dy: i32) {
    if dx == 0 && dy == 0 { return; }
    let Some(src) = doc.get_layer(layer_id).map(|l| (l.pixels.clone(), l.mask.clone())) else { return };
    let (src_img, src_mask) = src;
    let (dw, dh) = (doc.w, doc.h);
    let moved = translate_image(&src_img, dw, dh, dx, dy);
    let mask_moved = src_mask.map(|m| translate_image(&gray_to_rgba(&m), dw, dh, dx, dy));
    if let Some(l) = doc.get_layer_mut(layer_id) {
        l.pixels = moved;
        if let Some(mv) = mask_moved {
            l.mask = Some(rgba_to_gray(&mv));
        }
    }
    doc.mark_dirty(Rect::new(0, 0, doc.w, doc.h));
}

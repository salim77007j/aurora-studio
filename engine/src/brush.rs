//! Brush engine: stamp-based strokes with per-tile flow accumulation, pressure dynamics, eraser.
use crate::img::{Rect, Rgba8};
use crate::layer::Document;
const TILE: u32 = 256;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct BrushParams {
    pub size: f32,          // diameter in px (1..5000)
    pub hardness: f32,      // 0..1 (1 = hard anti-aliased edge)
    pub flow: f32,          // 0..1 paint amount per stamp
    pub opacity: f32,       // 0..1 stroke max opacity
    pub spacing: f32,       // 0..1 fraction of size between stamps
    pub eraser: bool,
    pub color: [u8; 4],
    pub size_pressure: bool,
    pub opacity_pressure: bool,
    pub pencil: bool,       // hard, aliased edge (pencil tool)
}

impl Default for BrushParams {
    fn default() -> Self {
        BrushParams {
            size: 30.0,
            hardness: 0.8,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.12,
            eraser: false,
            color: [0, 0, 0, 255],
            size_pressure: true,
            opacity_pressure: false,
            pencil: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey(i32, i32);

/// Per-tile accumulation buffer (premultiplied float accumulation within a flush window).
struct StrokeTile {
    x: i32, // tile origin in px
    y: i32,
    rgb: Vec<f32>,
    a: Vec<f32>,
}

/// Live stroke state. Stamps accumulate per-tile and are flushed into the layer
/// after every pointer event so the canvas shows live feedback.
pub struct Stroke {
    pub layer_id: u64,
    pub params: BrushParams,
    last_x: f32,
    last_y: f32,
    residue: f32,
    tiles: Vec<StrokeTile>,
    tile_keys: Vec<TileKey>,
    /// original bytes of each touched tile (first touch only) — for undo
    journal: HashMap<TileKey, Vec<u8>>,
    min_x: f32, min_y: f32, max_x: f32, max_y: f32,
}

impl Stroke {
    pub fn new(doc_w: u32, doc_h: u32, layer_id: u64, params: BrushParams, x: f32, y: f32, pressure: f32) -> Stroke {
        let _ = (doc_w, doc_h);
        let mut s = Stroke {
            layer_id,
            params,
            last_x: x,
            last_y: y,
            residue: 0.0,
            tiles: Vec::new(),
            tile_keys: Vec::new(),
            journal: HashMap::new(),
            min_x: x, min_y: y, max_x: x, max_y: y,
        };
        s.stamp_at(x, y, x, y, pressure);
        s
    }

    pub fn add_point(&mut self, x: f32, y: f32, pressure: f32) {
        self.stamp_at(self.last_x, self.last_y, x, y, pressure);
        self.last_x = x;
        self.last_y = y;
    }

    fn stamp_at(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, pressure: f32) {
        let p = self.params;
        let base = p.size.max(1.0);
        let pr = pressure.clamp(0.0, 1.0);
        let dia = if p.size_pressure {
            base * (0.15 + 0.85 * pr * pr).max(0.05)
        } else {
            base
        };
        let dx = x1 - x0;
        let dy = y1 - y0;
        let dist = (dx * dx + dy * dy).sqrt();
        let spacing_px = (dia * p.spacing.clamp(0.02, 4.0)).max(0.5);
        if dist < 1e-6 {
            if self.residue <= 0.0 {
                self.stamp(x1, y1, dia, pr);
                self.residue = spacing_px;
            }
            return;
        }
        let mut d = self.residue;
        while d <= dist {
            let t = d / dist;
            self.stamp(x0 + dx * t, y0 + dy * t, dia, pr);
            d += spacing_px;
        }
        self.residue = d - dist;
    }

    /// Add one stamp into accumulation tiles.
    fn stamp(&mut self, cx: f32, cy: f32, dia: f32, pressure: f32) {
        let p = self.params;
        let r = dia * 0.5;
        if r < 0.5 {
            return;
        }
        let r2 = r * r;
        let x0 = (cx - r).floor() as i32;
        let y0 = (cy - r).floor() as i32;
        let x1 = (cx + r).ceil() as i32;
        let y1 = (cy + r).ceil() as i32;
        let mut flow = p.flow.clamp(0.01, 1.0);
        if p.opacity_pressure {
            flow *= (0.15 + 0.85 * pressure).clamp(0.05, 1.0);
        }
        let col = if p.eraser {
            [0.0f32; 3]
        } else {
            [p.color[0] as f32 / 255.0, p.color[1] as f32 / 255.0, p.color[2] as f32 / 255.0]
        };
        for ty in y0.div_euclid(TILE as i32)..=(y1.div_euclid(TILE as i32)) {
            for tx in x0.div_euclid(TILE as i32)..=(x1.div_euclid(TILE as i32)) {
                let key = TileKey(tx, ty);
                if !self.tile_keys.contains(&key) {
                    self.tile_keys.push(key);
                    self.tiles.push(StrokeTile {
                        x: tx * TILE as i32,
                        y: ty * TILE as i32,
                        rgb: vec![0.0; (TILE * TILE) as usize * 3],
                        a: vec![0.0; (TILE * TILE) as usize],
                    });
                }
                let ti = self.tile_keys.iter().position(|&k| k == key).unwrap();
                let tile = &mut self.tiles[ti];
                let py_lo = y0.max(tile.y);
                let py_hi = y1.min(tile.y + TILE as i32 - 1);
                let px_lo = x0.max(tile.x);
                let px_hi = x1.min(tile.x + TILE as i32 - 1);
                for py in py_lo..=py_hi {
                    for px in px_lo..=px_hi {
                        let ddx = px as f32 + 0.5 - cx;
                        let ddy = py as f32 + 0.5 - cy;
                        let dd = ddx * ddx + ddy * ddy;
                        if dd > r2 { continue; }
                        let t = dd.sqrt() / r;
                        let a = if p.pencil {
                            if t <= 0.999 { 1.0 } else { 0.0 }
                        } else if p.hardness >= 0.999 {
                            ((1.0 - t) * r).clamp(0.0, 1.0)
                        } else {
                            let h = p.hardness.clamp(0.0, 0.999);
                            if t <= h {
                                1.0
                            } else {
                                let f = (t - h) / (1.0 - h);
                                (1.0 - f) * (1.0 - f)
                            }
                        };
                        if a <= 0.0 { continue; }
                        let sa = a * flow;
                        let li = ((py - tile.y) as u32 * TILE + (px - tile.x) as u32) as usize;
                        let dst_a = tile.a[li];
                        let out_a = sa + dst_a * (1.0 - sa);
                        if out_a > 1e-6 {
                            for c in 0..3 {
                                tile.rgb[li * 3 + c] = (col[c] * sa + tile.rgb[li * 3 + c] * dst_a * (1.0 - sa)) / out_a;
                            }
                        }
                        tile.a[li] = out_a;
                    }
                }
                self.min_x = self.min_x.min(cx - r);
                self.min_y = self.min_y.min(cy - r);
                self.max_x = self.max_x.max(cx + r);
                self.max_y = self.max_y.max(cy + r);
            }
        }
    }

    /// Merge accumulated tiles into the layer now (live feedback). Journaling happens here.
    pub fn flush_to_layer(&mut self, doc: &mut Document) {
        let params = self.params;
        let tiles = std::mem::take(&mut self.tiles);
        let keys = std::mem::take(&mut self.tile_keys);
        for (key, tile) in keys.into_iter().zip(tiles.into_iter()) {
            if !self.journal.contains_key(&key) {
                // capture original tile bytes (once)
                if let Some(l) = doc.get_layer(self.layer_id) {
                    let mut buf = vec![0u8; (TILE * TILE) as usize * 4];
                    let mut o = 0;
                    for py in 0..TILE as i32 {
                        for px in 0..TILE as i32 {
                            let v = l.pixels.get(tile.x + px, tile.y + py);
                            buf[o..o + 4].copy_from_slice(&v);
                            o += 4;
                        }
                    }
                    self.journal.insert(key, buf);
                }
            }
            if let Some(l) = doc.get_layer_mut(self.layer_id) {
                for py in 0..TILE as i32 {
                    for px in 0..TILE as i32 {
                        let li = (py as u32 * TILE + px as u32) as usize;
                        let a = tile.a[li];
                        if a <= 1e-6 { continue; }
                        let dx = tile.x + px;
                        let dy = tile.y + py;
                        let sa = (a * params.opacity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
                        let dpx = l.pixels.get(dx, dy);
                        let out = if params.eraser {
                            let na = (dpx[3] as f32 * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
                            [dpx[0], dpx[1], dpx[2], na]
                        } else {
                            // tile.rgb is already normalized 0..1
                            let sr = tile.rgb[li * 3];
                            let sg = tile.rgb[li * 3 + 1];
                            let sb = tile.rgb[li * 3 + 2];
                            let da = dpx[3] as f32 / 255.0;
                            let oa = sa + da * (1.0 - sa);
                            let mix = |sc: f32, dc: u8| -> u8 {
                                let v = (sc * sa + dc as f32 / 255.0 * da * (1.0 - sa)) / if oa > 1e-6 { oa } else { 1.0 };
                                (v * 255.0).round().clamp(0.0, 255.0) as u8
                            };
                            [mix(sr, dpx[0]), mix(sg, dpx[1]), mix(sb, dpx[2]), (oa * 255.0).round().clamp(0.0, 255.0) as u8]
                        };
                        l.pixels.set(dx, dy, out);
                    }
                }
            }
        }
    }

    pub fn touched_rect(&self) -> Rect {
        Rect::from_ltrb(
            self.min_x.floor() as i32,
            self.min_y.floor() as i32,
            (self.max_x + 1.0).ceil() as i32,
            (self.max_y + 1.0).ceil() as i32,
        )
    }

    /// Finish: build undo entry from the journal and mark doc dirty. Returns label for history.
    pub fn finish(mut self, doc: &mut Document, label: &str) -> Option<(Rect, Vec<u8>, Vec<u8>)> {
        self.flush_to_layer(doc);
        if self.journal.is_empty() {
            return None;
        }
        let rect = self.touched_rect().clip_to(doc.w, doc.h);
        if rect.is_empty() {
            return None;
        }
        // read "after" for the whole rect
        let n = (rect.w as usize) * (rect.h as usize) * 4;
        let mut after = vec![0u8; n];
        let mut before = vec![0u8; n];
        {
            let layer = doc.get_layer(self.layer_id)?;
            let mut o = 0;
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    let v = layer.pixels.get(x, y);
                    after[o..o + 4].copy_from_slice(&v);
                    o += 4;
                }
            }
        }
        before.copy_from_slice(&after);
        // overwrite before with journaled tile bytes
        for (key, bytes) in &self.journal {
            let tx = key.0 * TILE as i32;
            let ty = key.1 * TILE as i32;
            // intersect tile with rect
            let x0 = tx.max(rect.x);
            let y0 = ty.max(rect.y);
            let x1 = (tx + TILE as i32).min(rect.right());
            let y1 = (ty + TILE as i32).min(rect.bottom());
            for y in y0..y1 {
                for x in x0..x1 {
                    let ti = ((y - ty) as u32 * TILE + (x - tx) as u32) as usize * 4;
                    let ri = ((y - rect.y) as u32 * rect.w + (x - rect.x) as u32) as usize * 4;
                    before[ri..ri + 4].copy_from_slice(&bytes[ti..ti + 4]);
                }
            }
        }
        // mark dirty
        doc.mark_dirty(rect);
        let _ = label;
        Some((rect, before, after))
    }
}

/// Begin a stroke on a document.
pub fn begin(layer_id: u64, params: BrushParams, x: f32, y: f32, pressure: f32) -> Stroke {
    Stroke::new(0, 0, layer_id, params, x, y, pressure)
}

/// Rgba8 helper used by other modules.
pub type Image = Rgba8;

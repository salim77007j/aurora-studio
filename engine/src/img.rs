//! Aurora Engine — core pixel image types.
use serde::{Deserialize, Serialize};

/// Integer rectangle. x/y can be negative during transforms; w/h >= 0.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn from_ltrb(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        Rect { x: x0, y: y0, w: (x1 - x0) as u32, h: (y1 - y0) as u32 }
    }
    pub fn right(&self) -> i32 { self.x + self.w as i32 }
    pub fn bottom(&self) -> i32 { self.y + self.h as i32 }
    pub fn is_empty(&self) -> bool { self.w == 0 || self.h == 0 }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        if x1 > x0 && y1 > y0 { Some(Rect::from_ltrb(x0, y0, x1, y1)) } else { None }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() { return *o; }
        if o.is_empty() { return *self; }
        Rect::from_ltrb(self.x.min(o.x), self.y.min(o.y), self.right().max(o.right()), self.bottom().max(o.bottom()))
    }
    /// Clip to bounds (0,0,w,h).
    pub fn clip_to(&self, w: u32, h: u32) -> Rect {
        self.intersect(&Rect::new(0, 0, w, h)).unwrap_or_default()
    }
}

/// 8-bit straight-alpha RGBA image, row-major, 4 bytes/px.
#[derive(Clone, Serialize, Deserialize)]
pub struct Rgba8 {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Rgba8 {
    pub fn new(w: u32, h: u32) -> Self {
        Rgba8 { w, h, data: vec![0; (w as usize) * (h as usize) * 4] }
    }
    pub fn filled(w: u32, h: u32, rgba: [u8; 4]) -> Self {
        let mut img = Rgba8::new(w, h);
        for px in img.data.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
        img
    }
    pub fn from_raw(w: u32, h: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() == (w as usize) * (h as usize) * 4 {
            Some(Rgba8 { w, h, data })
        } else {
            None
        }
    }
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return [0; 4];
        }
        let o = ((y as u32 * self.w + x as u32) * 4) as usize;
        [self.data[o], self.data[o + 1], self.data[o + 2], self.data[o + 3]]
    }
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, px: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let o = ((y as u32 * self.w + x as u32) * 4) as usize;
        self.data[o..o + 4].copy_from_slice(&px);
    }
    pub fn fill(&mut self, rgba: [u8; 4]) {
        for px in self.data.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
    }
    /// Bilinear sample in continuous coords (center-of-pixel convention).
    pub fn sample_bilinear(&self, fx: f32, fy: f32) -> [f32; 4] {
        let w = self.w as i32;
        let h = self.h as i32;
        if w == 0 || h == 0 {
            return [0.0; 4];
        }
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
                if wgt <= 0.0 {
                    continue;
                }
                let px = if cx < 0 || cy < 0 || cx >= w || cy >= h {
                    [0u8; 4]
                } else {
                    self.get(cx, cy)
                };
                for c in 0..4 {
                    acc[c] += wgt * px[c] as f32;
                }
            }
        }
        acc
    }
    /// Bicubic (Catmull-Rom) sample.
    pub fn sample_bicubic(&self, fx: f32, fy: f32) -> [f32; 4] {
        let w = self.w as i32;
        let h = self.h as i32;
        if w == 0 || h == 0 {
            return [0.0; 4];
        }
        let x = fx - 0.5;
        let y = fy - 0.5;
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let tx = x - xi as f32;
        let ty = y - yi as f32;
        let wgt = |t: f32| -> [f32; 4] {
            [
                catmull_rom(t + 1.0),
                catmull_rom(t),
                catmull_rom(t - 1.0),
                catmull_rom(t - 2.0),
            ]
        };
        let wx = wgt(tx);
        let wy = wgt(ty);
        let mut acc = [0.0f32; 4];
        for (j, wyv) in wy.iter().enumerate() {
            for (i, wxv) in wx.iter().enumerate() {
                let wgt2 = wxv * wyv;
                if wgt2.abs() < 1e-6 {
                    continue;
                }
                let (cx, cy) = (xi + i as i32, yi + j as i32);
                let px = if cx < 0 || cy < 0 || cx >= w || cy >= h {
                    [0u8; 4]
                } else {
                    self.get(cx, cy)
                };
                for c in 0..4 {
                    acc[c] += wgt2 * px[c] as f32;
                }
            }
        }
        acc.map(|v| v.clamp(0.0, 255.0))
    }
    /// Downscale by simple area-average (for thumbnails).
    pub fn thumbnail(&self, max_w: u32, max_h: u32) -> Rgba8 {
        if self.w == 0 || self.h == 0 {
            return Rgba8::new(1, 1);
        }
        let scale = (max_w as f32 / self.w as f32).min(max_h as f32 / self.h as f32).min(1.0);
        let tw = ((self.w as f32 * scale).round() as u32).max(1);
        let th = ((self.h as f32 * scale).round() as u32).max(1);
        let mut out = Rgba8::new(tw, th);
        let (sw, sh) = (self.w as f32, self.h as f32);
        for ty in 0..th {
            let y0 = ty as f32 / scale;
            let y1 = ((ty + 1) as f32 / scale).min(sh);
            for tx in 0..tw {
                let x0 = tx as f32 / scale;
                let x1 = ((tx + 1) as f32 / scale).min(sw);
                let mut acc = [0.0f32; 4];
                let mut n = 0.0f32;
                let iy0 = y0 as u32;
                let iy1 = (y1.ceil() as u32).min(self.h);
                let ix0 = x0 as u32;
                let ix1 = (x1.ceil() as u32).min(self.w);
                for y in iy0..iy1 {
                    let fy = overlap(y as f32, y as f32 + 1.0, y0, y1);
                    for x in ix0..ix1 {
                        let fx = overlap(x as f32, x as f32 + 1.0, x0, x1);
                        let wgt = fx * fy;
                        if wgt <= 0.0 {
                            continue;
                        }
                        let px = self.get(x as i32, y as i32);
                        for c in 0..4 {
                            acc[c] += wgt * px[c] as f32;
                        }
                        n += wgt;
                    }
                }
                if n > 0.0 {
                    let px: [u8; 4] = acc.map(|v| (v / n).round().clamp(0.0, 255.0) as u8);
                    out.set(tx as i32, ty as i32, px);
                }
            }
        }
        out
    }
}

#[inline]
fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Catmull-Rom kernel weight for cubic resampling.
#[inline]
pub fn catmull_rom(x: f32) -> f32 {
    let a = x.abs();
    if a <= 1.0 {
        1.5 * a * a * a - 2.5 * a * a + 1.0
    } else if a < 2.0 {
        -0.5 * a * a * a + 2.5 * a * a - 4.0 * a + 2.0
    } else {
        0.0
    }
}

/// 8-bit grayscale image (masks).
#[derive(Clone, Serialize, Deserialize)]
pub struct Gray8 {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Gray8 {
    pub fn new(w: u32, h: u32) -> Self {
        Gray8 { w, h, data: vec![255; (w as usize) * (h as usize)] }
    }
    pub fn zeros(w: u32, h: u32) -> Self {
        Gray8 { w, h, data: vec![0; (w as usize) * (h as usize)] }
    }
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return 0;
        }
        self.data[(y as u32 * self.w + x as u32) as usize]
    }
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        self.data[(y as u32 * self.w + x as u32) as usize] = v;
    }
    pub fn bounds(&self) -> Rect {
        // bounding box of nonzero pixels
        let mut minx = self.w as i32;
        let mut miny = self.h as i32;
        let mut maxx = -1;
        let mut maxy = -1;
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                if self.get(x, y) > 0 {
                    if x < minx { minx = x; }
                    if x > maxx { maxx = x; }
                    if y < miny { miny = y; }
                    if y > maxy { maxy = y; }
                }
            }
        }
        if maxx < 0 {
            Rect::default()
        } else {
            Rect::from_ltrb(minx, miny, maxx + 1, maxy + 1)
        }
    }
}

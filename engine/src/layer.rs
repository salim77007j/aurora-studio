//! Layer tree, document model and compositing.
use crate::blend::{composite_region, BlendMode};
use crate::img::{Gray8, Rgba8, Rect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: f32, // 0..1
    pub blend: BlendMode,
    pub pixels: Rgba8,
    pub mask: Option<Gray8>,
    #[serde(default = "default_true")]
    pub mask_enabled: bool,
    #[serde(default)]
    pub locked: bool,
}

fn default_true() -> bool { true }

impl Layer {
    pub fn new(id: u64, name: &str, w: u32, h: u32) -> Self {
        Layer {
            id,
            name: name.to_string(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            pixels: Rgba8::new(w, h),
            mask: None,
            mask_enabled: true,
            locked: false,
        }
    }
    pub fn filled(id: u64, name: &str, w: u32, h: u32, rgba: [u8; 4]) -> Self {
        let mut l = Layer::new(id, name, w, h);
        l.pixels.fill(rgba);
        l
    }
    /// Effective alpha of a pixel after mask application.
    #[inline]
    pub fn masked_alpha(&self, x: i32, y: i32) -> u8 {
        let a = self.pixels.get(x, y)[3];
        if let (Some(m), true) = (&self.mask, self.mask_enabled) {
            let mv = m.get(x, y) as u32;
            ((a as u32 * mv) / 255) as u8
        } else {
            a
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    #[serde(default = "default_true")]
    pub expanded: bool,
    pub children: Vec<Node>,
}

impl Group {
    pub fn new(id: u64, name: &str) -> Self {
        Group { id, name: name.to_string(), visible: true, opacity: 1.0, blend: BlendMode::Normal, expanded: true, children: Vec::new() }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Node {
    Layer(Layer),
    Group(Group),
}

impl Node {
    pub fn id(&self) -> u64 {
        match self { Node::Layer(l) => l.id, Node::Group(g) => g.id }
    }
    pub fn name(&self) -> &str {
        match self { Node::Layer(l) => &l.name, Node::Group(g) => &g.name }
    }
    pub fn set_name(&mut self, s: &str) {
        match self { Node::Layer(l) => l.name = s.to_string(), Node::Group(g) => g.name = s.to_string() }
    }
    pub fn visible(&self) -> bool {
        match self { Node::Layer(l) => l.visible, Node::Group(g) => g.visible }
    }
    pub fn set_visible(&mut self, v: bool) {
        match self { Node::Layer(l) => l.visible = v, Node::Group(g) => g.visible = v }
    }
    pub fn opacity(&self) -> f32 {
        match self { Node::Layer(l) => l.opacity, Node::Group(g) => g.opacity }
    }
    pub fn set_opacity(&mut self, o: f32) {
        match self { Node::Layer(l) => l.opacity = o, Node::Group(g) => g.opacity = o }
    }
    pub fn blend(&self) -> BlendMode {
        match self { Node::Layer(l) => l.blend, Node::Group(g) => g.blend }
    }
    pub fn set_blend(&mut self, b: BlendMode) {
        match self { Node::Layer(l) => l.blend = b, Node::Group(g) => g.blend = b }
    }
    pub fn as_layer_mut(&mut self) -> Option<&mut Layer> {
        match self { Node::Layer(l) => Some(l), _ => None }
    }
    pub fn as_group_mut(&mut self) -> Option<&mut Group> {
        match self { Node::Group(g) => Some(g), _ => None }
    }
    pub fn as_group(&self) -> Option<&Group> {
        match self { Node::Group(g) => Some(g), _ => None }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct NodePath {
    /// chain of child indices from root.children down to the node
    pub chain: Vec<u32>,
}

/// Selection mask.
#[derive(Clone, Serialize, Deserialize)]
pub struct Selection {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    #[serde(default)]
    pub active: bool,
}

impl Selection {
    pub fn none(w: u32, h: u32) -> Self {
        Selection { w, h, data: vec![0; (w as usize) * (h as usize)], active: false }
    }
    pub fn all(w: u32, h: u32) -> Self {
        Selection { w, h, data: vec![255; (w as usize) * (h as usize)], active: true }
    }
    pub fn as_gray(&self) -> Option<Gray8> {
        if !self.active { None } else { Some(Gray8 { w: self.w, h: self.h, data: self.data.clone() }) }
    }
    #[inline]
    pub fn factor(&self, x: i32, y: i32) -> f32 {
        if !self.active { return 1.0; }
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 { return 0.0; }
        self.data[(y as u32 * self.w + x as u32) as usize] as f32 / 255.0
    }
    pub fn bounds(&self) -> Option<Rect> {
        if !self.active { return None; }
        let mut g = Gray8 { w: self.w, h: self.h, data: self.data.clone() };
        let b = g.bounds();
        if b.is_empty() { None } else { Some(b) }
    }
    /// Combine with a new mask (mode: 0=replace, 1=add, 2=subtract, 3=intersect).
    pub fn combine(&mut self, other: &Gray8, mode: i32) {
        if mode == 0 || !self.active {
            self.data = other.data.clone();
            self.active = true;
            return;
        }
        match mode {
            1 => for (d, s) in self.data.iter_mut().zip(other.data.iter()) { *d = (*d as u32).max(*s as u32) as u8; },
            2 => for (d, s) in self.data.iter_mut().zip(other.data.iter()) { *d = d.saturating_sub(*s); },
            3 => for (d, s) in self.data.iter_mut().zip(other.data.iter()) { *d = (*d as u32).min(*s as u32) as u8; },
            _ => {}
        }
        self.active = self.data.iter().any(|&v| v > 0);
    }
}

/// The document: canvas size, layer tree, selection, history, composite cache.
pub struct Document {
    pub w: u32,
    pub h: u32,
    pub root: Group,
    pub active_id: u64,
    pub selection: Selection,
    pub next_id: u64,
    pub history: crate::undo::History,
    pub name: String,
    composite: Rgba8,
    dirty: Rect,
    dirty_any: bool,
    /// bumped on every composite change (dirty processing or reset) — used by the UI
    /// to detect changes even when the dirty region was already consumed.
    pub version: u64,
}

impl Document {
    pub fn new(w: u32, h: u32, bg: [u8; 4], name: &str) -> Self {
        let mut root = Group::new(0, "root");
        if bg[3] > 0 {
            let bg_layer = Layer::filled(1, "Background", w, h, bg);
            root.children.push(Node::Layer(bg_layer));
        }
        let mut doc = Document {
            w, h,
            root,
            active_id: if bg[3] > 0 { 1 } else { 0 },
            selection: Selection::none(w, h),
            next_id: 2,
            history: crate::undo::History::new(),
            name: name.to_string(),
            composite: Rgba8::new(w, h),
            dirty: Rect::new(0, 0, w, h),
            dirty_any: true,
            version: 0,
        };
        doc.recomposite_all();
        doc
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty = Rect::new(0, 0, self.w, self.h);
        self.dirty_any = true;
    }
    pub fn mark_dirty(&mut self, r: Rect) {
        let r = r.clip_to(self.w, self.h);
        if r.is_empty() { return; }
        self.dirty = if self.dirty_any { self.dirty.union(&r) } else { r };
        self.dirty_any = true;
    }

    // ---------- tree helpers ----------

    pub fn find_path(&self, id: u64) -> Option<NodePath> {
        fn walk(g: &Group, id: u64, chain: &mut Vec<u32>) -> Option<NodePath> {
            for (i, n) in g.children.iter().enumerate() {
                chain.push(i as u32);
                if n.id() == id {
                    return Some(NodePath { chain: chain.clone() });
                }
                if let Node::Group(cg) = n {
                    if let Some(p) = walk(cg, id, chain) {
                        return Some(p);
                    }
                }
                chain.pop();
            }
            None
        }
        walk(&self.root, id, &mut Vec::new())
    }
    pub fn node_at(&self, path: &NodePath) -> Option<&Node> {
        let mut g: &Group = &self.root;
        for (i, step) in path.chain.iter().enumerate() {
            let last = i == path.chain.len() - 1;
            let child = g.children.get(*step as usize)?;
            if last { return Some(child); }
            match child {
                Node::Group(cg) => g = cg,
                _ => return None,
            }
        }
        None
    }
    pub fn node_at_mut(&mut self, path: &NodePath) -> Option<&mut Node> {
        let mut g: &mut Group = &mut self.root;
        for (i, step) in path.chain.iter().enumerate() {
            let last = i == path.chain.len() - 1;
            let child = g.children.get_mut(*step as usize)?;
            if last { return Some(child); }
            match child {
                Node::Group(cg) => g = cg,
                _ => return None,
            }
        }
        None
    }
    pub fn get_node(&self, id: u64) -> Option<&Node> {
        self.find_path(id).and_then(|p| self.node_at(&p))
    }
    pub fn get_node_mut(&mut self, id: u64) -> Option<&mut Node> {
        let p = self.find_path(id);
        p.and_then(|p| self.node_at_mut(&p))
    }
    pub fn get_layer_mut(&mut self, id: u64) -> Option<&mut Layer> {
        self.get_node_mut(id).and_then(|n| n.as_layer_mut())
    }
    pub fn get_layer(&self, id: u64) -> Option<&Layer> {
        self.get_node(id).and_then(|n| match n {
            Node::Layer(l) => Some(l),
            _ => None,
        })
    }

    /// Depth-first ordered list of layer ids (bottom → top drawing order).
    pub fn layer_ids(&self) -> Vec<u64> {
        fn walk(g: &Group, out: &mut Vec<u64>) {
            for n in &g.children {
                match n {
                    Node::Layer(l) => out.push(l.id),
                    Node::Group(cg) => walk(cg, out),
                }
            }
        }
        let mut v = Vec::new();
        walk(&self.root, &mut v);
        v
    }

    /// Flattened, visible composite of the whole document (fresh buffer).
    pub fn flatten(&self) -> Rgba8 {
        let mut out = Rgba8::new(self.w.max(1), self.h.max(1));
        blend_children(&self.root.children, &mut out, Rect::new(0, 0, self.w, self.h), (0, 0));
        out
    }

    // ---------- compositing ----------

    pub fn recomposite_all(&mut self) {
        self.mark_all_dirty();
        self.process_dirty();
    }

    /// Composite the dirty region. Returns the region that was updated.
    pub fn process_dirty(&mut self) -> Option<Rect> {
        if !self.dirty_any { return None; }
        let region = self.dirty.clip_to(self.w, self.h);
        if region.is_empty() {
            self.dirty_any = false;
            return None;
        }
        // clear region
        let stride = self.composite.w as usize * 4;
        for y in region.y..region.bottom() {
            let off = y as usize * stride + region.x as usize * 4;
            let len = region.w as usize * 4;
            self.composite.data[off..off + len].fill(0);
        }
        {
            // disjoint borrows: root (immutable) + composite (mutable)
            let root = &self.root;
            let comp = &mut self.composite;
            blend_children(&root.children, comp, region, (0, 0));
        }
        self.dirty = Rect::default();
        self.dirty_any = false;
        self.version = self.version.wrapping_add(1);
        Some(region)
    }

    pub fn composite_image(&self) -> &Rgba8 { &self.composite }

    /// Reset composite cache after canvas size change.
    pub fn composite_reset(&mut self) {
        self.composite = Rgba8::new(self.w.max(1), self.h.max(1));
        self.mark_all_dirty();
        self.version = self.version.wrapping_add(1);
    }

    /// Copy composite region into an RGBA output buffer (4 bytes/px, straight alpha).
    pub fn read_composite(&self, region: Rect, out: &mut [u8]) -> bool {
        let region = region.clip_to(self.w, self.h);
        if region.is_empty() || out.len() < (region.w as usize) * (region.h as usize) * 4 {
            return false;
        }
        let ci = self.composite_image();
        let mut o = 0usize;
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let px = ci.get(x, y);
                out[o] = px[0];
                out[o + 1] = px[1];
                out[o + 2] = px[2];
                out[o + 3] = px[3];
                o += 4;
            }
        }
        true
    }
}

/// Blend a list of nodes into `out` (bottom → top).
/// `region` is in out-local coordinates; `src_origin` is the doc-space position of out's (0,0).
fn blend_children(children: &[Node], out: &mut Rgba8, region: Rect, src_origin: (i32, i32)) {
    for n in children {
        match n {
            Node::Layer(l) => {
                if !l.visible || l.opacity <= 0.0 { continue; }
                let mask = if l.mask_enabled { l.mask.as_ref() } else { None };
                composite_region(out, &l.pixels, l.blend, l.opacity, region, src_origin, mask);
            }
            Node::Group(cg) => {
                if !cg.visible || cg.opacity <= 0.0 { continue; }
                // flatten nested group into region-sized temp, then blend as a unit
                let mut temp = Rgba8::new(region.w.max(1), region.h.max(1));
                blend_children(&cg.children, &mut temp, Rect::new(0, 0, region.w, region.h), (src_origin.0 + region.x, src_origin.1 + region.y));
                composite_region(out, &temp, cg.blend, cg.opacity, region, (0, 0), None);
            }
        }
    }
}

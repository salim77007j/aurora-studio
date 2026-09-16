//! Aurora Engine — C ABI for P/Invoke.
#![allow(clippy::missing_safety_doc)]

mod adjust;
mod blend;
mod brush;
mod filters;
mod img;
mod io;
mod layer;
mod paint;
mod selection;
mod transform;
mod undo;

use blend::BlendMode;
use img::{Rect, Rgba8};
use layer::{Document, Node};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock};

static DOCS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Document>>>>> = OnceLock::new();
static NEXT: OnceLock<Mutex<u64>> = OnceLock::new();
static STROKES: OnceLock<Mutex<HashMap<u64, Option<brush::Stroke>>>> = OnceLock::new();

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn docs() -> &'static Mutex<HashMap<u64, Arc<Mutex<Document>>>> {
    DOCS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn next_id() -> u64 {
    let mut n = NEXT.get_or_init(|| Mutex::new(1000)).lock().unwrap();
    *n += 1;
    *n
}
fn strokes() -> &'static Mutex<HashMap<u64, Option<brush::Stroke>>> {
    STROKES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_err<S: Into<String>>(s: S) {
    LAST_ERROR.with(|e| *e.borrow_mut() = s.into());
}

fn take_err() -> String {
    LAST_ERROR.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn get_doc(h: u64) -> Arc<Mutex<Document>> {
    match docs().lock().unwrap().get(&h).cloned() {
        Some(d) => d,
        None => {
            set_err(format!("invalid document handle {h}"));
            panic!("invalid document handle");
        }
    }
}

/// Run a closure, catching panics; returns 0/negative error code convention handled by caller.
fn guard<T>(f: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(p) => {
            let msg = if let Some(s) = p.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = p.downcast_ref::<String>() {
                s.clone()
            } else {
                "engine panic".to_string()
            };
            set_err(format!("internal error: {msg}"));
            None
        }
    }
}

// ---------- strings ----------

fn read_str(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p).to_string_lossy().to_string() }
}

/// Copy a string into caller buffer. Returns needed length (excluding NUL), or -1 if buffer too small.
fn put_str(buf: *mut u8, cap: u32, s: &str) -> i32 {
    let bytes = s.as_bytes();
    let needed = bytes.len();
    if cap as usize >= needed + 1 && !buf.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, needed);
            *buf.add(needed) = 0;
        }
    }
    needed as i32
}

// ---------- Brush FFI struct ----------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrushFFI {
    pub size: f32,
    pub hardness: f32,
    pub flow: f32,
    pub opacity: f32,
    pub spacing: f32,
    pub eraser: i32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub size_pressure: i32,
    pub opacity_pressure: i32,
    pub pencil: i32,
}

fn conv_brush(f: &BrushFFI) -> brush::BrushParams {
    brush::BrushParams {
        size: f.size,
        hardness: f.hardness,
        flow: f.flow,
        opacity: f.opacity,
        spacing: f.spacing,
        eraser: f.eraser != 0,
        color: [f.r, f.g, f.b, f.a],
        size_pressure: f.size_pressure != 0,
        opacity_pressure: f.opacity_pressure != 0,
        pencil: f.pencil != 0,
    }
}

// ===================== lifecycle =====================

#[no_mangle]
pub extern "C" fn aurora_version(buf: *mut u8, cap: u32) -> i32 {
    put_str(buf, cap, concat!("Aurora Engine ", env!("CARGO_PKG_VERSION")))
}

#[no_mangle]
pub extern "C" fn aurora_last_error(buf: *mut u8, cap: u32) -> i32 {
    put_str(buf, cap, &take_err())
}

#[no_mangle]
pub extern "C" fn aurora_doc_new(w: u32, h: u32, bg: i32, name: *const c_char) -> u64 {
    guard(|| {
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            set_err("canvas size out of range (1..16384)");
            return 0;
        }
        let bgc = if bg == 1 { [255u8, 255, 255, 255] } else if bg == 2 { [0u8, 0, 0, 0] } else { [0, 0, 0, 0] };
        let nm = read_str(name);
        let name = if nm.is_empty() { "Untitled".to_string() } else { nm };
        let doc = Document::new(w, h, bgc, &name);
        let handle = next_id();
        docs().lock().unwrap().insert(handle, Arc::new(Mutex::new(doc)));
        handle
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn aurora_doc_open(path: *const c_char) -> u64 {
    guard(|| {
        let p = read_str(path);
        match io::open(&p) {
            Ok(doc) => {
                let handle = next_id();
                docs().lock().unwrap().insert(handle, Arc::new(Mutex::new(doc)));
                handle
            }
            Err(e) => {
                set_err(e);
                0
            }
        }
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn aurora_doc_clone(h: u64) -> u64 {
    guard(|| {
        let d = get_doc(h);
        let clone_src = d.lock().unwrap();
        // deep clone via serialize-free manual clone
        let mut nd = Document::new(clone_src.w.max(1), clone_src.h.max(1), [0, 0, 0, 0], &clone_src.name);
        nd.root = clone_src.root.clone();
        nd.active_id = clone_src.active_id;
        nd.selection = clone_src.selection.clone();
        nd.next_id = clone_src.next_id;
        // replace root's first background layer if empty doc
        if clone_src.root.children.is_empty() {
            nd.root.children.clear();
        }
        nd.recomposite_all();
        drop(clone_src);
        let handle = next_id();
        docs().lock().unwrap().insert(handle, Arc::new(Mutex::new(nd)));
        handle
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn aurora_doc_free(h: u64) -> i32 {
    guard(|| {
        let removed = docs().lock().unwrap().remove(&h);
        strokes().lock().unwrap().remove(&h);
        if removed.is_some() {
            0
        } else {
            set_err("unknown handle");
            -1
        }
    })
    .unwrap_or(-1)
}

// ===================== state =====================

#[no_mangle]
pub extern "C" fn aurora_doc_json(h: u64, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        put_str(buf, cap, &doc_state_json(&doc))
    })
    .unwrap_or(-1)
}

fn doc_state_json(doc: &Document) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(4096);
    let _ = write!(s, "{{\"w\":{},\"h\":{},\"name\":{},\"activeId\":{},\"layers\":[", doc.w, doc.h, json_str(&doc.name), doc.active_id);
    let mut first = true;
    fn node_json(n: &Node, s: &mut String, first: &mut bool) {
        if !*first {
            let _ = write!(s, ",");
        }
        *first = false;
        match n {
            Node::Layer(l) => {
                let _ = write!(
                    s,
                    "{{\"id\":{},\"name\":{},\"kind\":\"layer\",\"visible\":{},\"opacity\":{:.3},\"blend\":{},\"hasMask\":{},\"locked\":{}",
                    l.id, json_str(&l.name), l.visible, l.opacity, l.blend.index(), l.mask.is_some(), l.locked
                );
                let _ = write!(s, "}}");
            }
            Node::Group(g) => {
                let _ = write!(
                    s,
                    "{{\"id\":{},\"name\":{},\"kind\":\"group\",\"visible\":{},\"opacity\":{:.3},\"blend\":{},\"expanded\":{},\"children\":[",
                    g.id, json_str(&g.name), g.visible, g.opacity, g.blend.index(), g.expanded
                );
                let mut f2 = true;
                for c in &g.children {
                    node_json(c, s, &mut f2);
                }
                let _ = write!(s, "]}}");
            }
        }
    }
    // top-first for UI display
    for n in doc.root.children.iter().rev() {
        node_json(n, &mut s, &mut first);
    }
    let _ = write!(s, "]}}");
    s
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

#[no_mangle]
pub extern "C" fn aurora_history_json(h: u64, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        let (index, names) = doc.history.labels();
        use std::fmt::Write;
        let mut s = String::new();
        let _ = write!(s, "{{\"index\":{},\"entries\":[", index);
        for (i, n) in names.iter().enumerate() {
            if i > 0 {
                let _ = write!(s, ",");
            }
            let _ = write!(s, "{}", json_str(n));
        }
        let _ = write!(s, "]}}");
        put_str(buf, cap, &s)
    })
    .unwrap_or(-1)
}

/// Poll composite dirty region (composites it). Returns 1 with region, 0 if clean, -1 error.
/// `version` always receives the current composite version (bump on ANY composite change).
#[no_mangle]
pub extern "C" fn aurora_poll(h: u64, x: *mut i32, y: *mut i32, w: *mut u32, hh: *mut u32, version: *mut u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        unsafe {
            if !version.is_null() { *version = doc.version; }
        }
        match doc.process_dirty() {
            Some(r) => unsafe {
                if !x.is_null() { *x = r.x; }
                if !y.is_null() { *y = r.y; }
                if !w.is_null() { *w = r.w; }
                if !hh.is_null() { *hh = r.h; }
                1
            },
            None => 0,
        }
    })
    .unwrap_or(-1)
}

/// Read composite region into RGBA buffer (straight alpha).
#[no_mangle]
pub extern "C" fn aurora_composite_read(h: u64, x: i32, y: i32, w: u32, hh: u32, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        let need = (w as usize) * (hh as usize) * 4;
        if (cap as usize) < need || buf.is_null() {
            set_err("composite buffer too small");
            return -1;
        }
        let region = Rect::new(x, y, w, hh).clip_to(doc.w, doc.h);
        let slice = unsafe { std::slice::from_raw_parts_mut(buf, need) };
        if doc.read_composite(region, slice) {
            0
        } else {
            set_err("composite read failed");
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_selection_bounds(h: u64, x: *mut i32, y: *mut i32, w: *mut u32, hh: *mut u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        match doc.selection.bounds() {
            Some(r) => unsafe {
                if !x.is_null() { *x = r.x; }
                if !y.is_null() { *y = r.y; }
                if !w.is_null() { *w = r.w; }
                if !hh.is_null() { *hh = r.h; }
                1
            },
            None => 0,
        }
    })
    .unwrap_or(-1)
}

// ===================== layers =====================

#[no_mangle]
pub extern "C" fn aurora_layer_add(h: u64, parent_id: i64, name: *const c_char, below_active: i32) -> u64 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let id = doc.alloc_id();
        let nm = read_str(name);
        let nm = if nm.is_empty() { format!("Layer {}", id) } else { nm };
        let layer = Node::Layer(layer::Layer::new(id, &nm, doc.w, doc.h));
        let (parent, index) = insert_position(&mut doc, parent_id, below_active);
        undo::insert_at(&mut doc, parent, index, layer);
        doc.active_id = id;
        let node_clone = doc_get_node(&doc, id).unwrap().clone();
        doc.history.push(undo::Command::Insert {
            parent_id: parent,
            index,
            node: Box::new(node_clone),
            label: "New Layer".into(),
        });
        doc.recomposite_all();
        id
    })
    .unwrap_or(0)
}

fn doc_get_node<'a>(doc: &'a Document, id: u64) -> Option<&'a Node> {
    doc.get_node(id)
}

fn insert_position(doc: &mut Document, parent_id: i64, below_active: i32) -> (u64, usize) {
    // returns (parent_id, index) to insert so that the new node sits above the active layer
    let active = doc.active_id;
    if below_active == 0 || active == 0 {
        // top of root (children list is bottom→top; top = end)
        return (0, doc.root.children.len());
    }
    // find active node path
    if let Some(path) = doc.find_path(active) {
        if path.chain.is_empty() {
            return (0, doc.root.children.len());
        }
        let top_index = *path.chain.last().unwrap() as usize;
        // check whether parent chain matches requested parent
        if parent_id >= 0 && path.chain.len() >= 1 {
            // parent id of active node:
            let mut pid = 0u64;
            if path.chain.len() > 1 {
                let mut g: &layer::Group = &doc.root;
                for step in &path.chain[..path.chain.len() - 1] {
                    match g.children.get(*step as usize) {
                        Some(Node::Group(cg)) => g = cg,
                        _ => break,
                    }
                }
                pid = g.id;
            }
            if pid == parent_id as u64 {
                // insert just above active
                return (parent_id as u64, top_index + 1);
            }
        }
        return (0, doc.root.children.len());
    }
    (0, doc.root.children.len())
}

#[no_mangle]
pub extern "C" fn aurora_layer_add_group(h: u64, name: *const c_char) -> u64 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let id = doc.alloc_id();
        let nm = read_str(name);
        let nm = if nm.is_empty() { format!("Group {id}") } else { nm };
        let group = Node::Group(layer::Group::new(id, &nm));
        let (parent, index) = insert_position(&mut doc, -1, 1);
        undo::insert_at(&mut doc, parent, index, group);
        let node_clone = doc_get_node(&doc, id).unwrap().clone();
        doc.history.push(undo::Command::Insert {
            parent_id: parent,
            index,
            node: Box::new(node_clone),
            label: "New Group".into(),
        });
        doc.recomposite_all();
        id
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn aurora_layer_delete(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if doc.get_node(id).is_none() {
            set_err("layer not found");
            return -1;
        }
        if doc.root.children.len() == 1 && doc.get_node(id).map(|n| n.id()) == Some(doc.root.children[0].id()) {
            // keep at least one layer: delete then add empty
        }
        let Some(path) = doc.find_path(id) else { return -1 };
        let parent_of = parent_id_of(&doc, id);
        let index = *path.chain.last().unwrap_or(&0) as usize;
        let node = undo::remove_at(&mut doc, parent_of, index);
        if let Some(n) = node {
            doc.history.push(undo::Command::Remove {
                parent_id: parent_of,
                index,
                node: Box::new(n),
                label: "Delete Layer".into(),
            });
        }
        // fix active
        let ids = doc.layer_ids();
        if !ids.contains(&doc.active_id) {
            doc.active_id = ids.last().copied().unwrap_or(0);
            if doc.active_id == 0 {
                // create fresh layer
                let nid = doc.alloc_id();
                let l = layer::Layer::new(nid, "Layer", doc.w, doc.h);
                doc.root.children.push(Node::Layer(l));
                doc.active_id = nid;
                let node_clone = doc_get_node(&doc, nid).unwrap().clone();
                let ins_idx = doc.root.children.len() - 1;
                doc.history.push(undo::Command::Insert {
                    parent_id: 0,
                    index: ins_idx,
                    node: Box::new(node_clone),
                    label: "New Layer".into(),
                });
            }
        }
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

fn parent_id_of(doc: &Document, id: u64) -> u64 {
    if let Some(path) = doc.find_path(id) {
        if path.chain.len() <= 1 {
            return 0;
        }
        let mut g: &layer::Group = &doc.root;
        for step in &path.chain[..path.chain.len() - 1] {
            match g.children.get(*step as usize) {
                Some(Node::Group(cg)) => g = cg,
                _ => return 0,
            }
        }
        return g.id;
    }
    0
}

#[no_mangle]
pub extern "C" fn aurora_layer_duplicate(h: u64, id: u64) -> u64 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(node) = doc.get_node(id).cloned() else {
            set_err("layer not found");
            return 0;
        };
        let new_id = doc.alloc_id();
        let mut clone = node;
        match &mut clone {
            Node::Layer(l) => {
                l.id = new_id;
                l.name = format!("{} copy", l.name);
            }
            Node::Group(g) => {
                assign_new_ids(g, &mut doc.next_id_local());
                g.id = new_id;
                g.name = format!("{} copy", g.name);
            }
        }
        let (parent, index) = insert_position(&mut doc, -1, 1);
        undo::insert_at(&mut doc, parent, index, clone);
        doc.active_id = new_id;
        let node_clone = doc_get_node(&doc, new_id).unwrap().clone();
        doc.history.push(undo::Command::Insert {
            parent_id: parent,
            index,
            node: Box::new(node_clone),
            label: "Duplicate Layer".into(),
        });
        doc.recomposite_all();
        new_id
    })
    .unwrap_or(0)
}

impl Document {
    pub fn next_id_local(&mut self) -> &mut u64 {
        &mut self.next_id
    }
}

fn assign_new_ids(g: &mut layer::Group, next: &mut u64) {
    for c in &mut g.children {
        match c {
            Node::Layer(l) => {
                *next += 1;
                l.id = *next;
            }
            Node::Group(cg) => {
                *next += 1;
                cg.id = *next;
                assign_new_ids(cg, next);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn aurora_layer_merge_down(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let ids = doc.layer_ids();
        let pos = match ids.iter().position(|&x| x == id) {
            Some(p) => p,
            None => {
                set_err("layer not found");
                return -1;
            }
        };
        if pos == 0 {
            set_err("no layer below");
            return -1;
        }
        let below_id = ids[pos - 1];
        // composite the two into below
        let snapshot_before = {
            let (a, b) = (doc.get_layer(below_id).cloned(), doc.get_layer(id).cloned());
            match (a, b) {
                (Some(mut below), Some(upper)) => {
                    let region = Rect::new(0, 0, doc.w, doc.h);
                    let mask = if upper.mask_enabled { upper.mask.clone() } else { None };
                    blend::composite_region(&mut below.pixels, &upper.pixels, upper.blend, upper.opacity, region, (0, 0), mask.as_ref());
                    (below, upper)
                }
                _ => return -1,
            }
        };
        let (mut merged, upper) = snapshot_before;
        merged.name = format!("{} merged", upper.name);
        // replace below's pixels; remove upper
        if let Some(Node::Layer(l)) = doc.get_node_mut(below_id) {
            l.pixels = merged.pixels.clone();
            l.name = merged.name.clone();
        }
        let Some(path) = doc.find_path(id) else { return -1 };
        let parent = parent_id_of(&doc, id);
        let index = *path.chain.last().unwrap_or(&0) as usize;
        if let Some(node) = undo::remove_at(&mut doc, parent, index) {
            doc.history.push(undo::Command::Remove {
                parent_id: parent,
                index,
                node: Box::new(node),
                label: "Merge Down".into(),
            });
        }
        doc.active_id = below_id;
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_visible(h: u64, id: u64, v: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(n) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        let old = n.visible();
        let nv = v != 0;
        if old == nv {
            return 0;
        }
        n.set_visible(nv);
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::Visible(old, nv),
            label: "Toggle Visibility".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_opacity(h: u64, id: u64, o: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(n) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        let old = n.opacity();
        let nv = o.clamp(0.0, 1.0);
        if (old - nv).abs() < 1e-6 {
            return 0;
        }
        n.set_opacity(nv);
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::Opacity(old, nv),
            label: "Layer Opacity".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_blend(h: u64, id: u64, blend: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(n) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        let old = n.blend().index();
        let bm = BlendMode::from_i32(blend);
        if n.blend() == bm {
            return 0;
        }
        n.set_blend(bm);
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::Blend(old, bm.index()),
            label: "Blend Mode".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_name(h: u64, id: u64, name: *const c_char) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let nm = read_str(name);
        let Some(n) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        let old = n.name().to_string();
        if old == nm {
            return 0;
        }
        n.set_name(&nm);
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::Name(old, nm),
            label: "Rename Layer".into(),
        });
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_active(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if doc.get_node(id).is_none() {
            set_err("layer not found");
            return -1;
        }
        let old = doc.active_id;
        doc.active_id = id;
        if old != id {
            doc.history.push(undo::Command::Prop {
                id,
                kind: undo::PropKind::Active(old, id),
                label: "Select Layer".into(),
            });
        }
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_set_locked(h: u64, id: u64, v: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(n) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        if let Node::Layer(l) = n {
            l.locked = v != 0;
        }
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_move_node(h: u64, id: u64, new_parent: i64, new_index: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(path) = doc.find_path(id) else {
            set_err("layer not found");
            return -1;
        };
        let parent = parent_id_of(&doc, id);
        let index = *path.chain.last().unwrap_or(&0) as usize;
        let target_parent = if new_parent < 0 { 0 } else { new_parent as u64 };
        let target_index = if new_index < 0 { usize::MAX } else { new_index as usize };
        if target_parent != parent || target_index != index {
            if let Some(node) = undo::remove_at(&mut doc, parent, index) {
                let idx = target_index.min(doc_group_len(&doc, target_parent));
                undo::insert_at(&mut doc, target_parent, idx, node);
                doc.history.push(undo::Command::Move {
                    from_parent: parent,
                    from_index: index,
                    to_parent: target_parent,
                    to_index: idx,
                    label: "Reorder Layer".into(),
                });
            }
        }
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_move_up(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(path) = doc.find_path(id) else { set_err("layer not found"); return -1; };
        let parent = parent_id_of(&doc, id);
        let index = *path.chain.last().unwrap_or(&0) as usize;
        let len = doc_group_len(&doc, parent);
        if index + 1 >= len {
            set_err("already top");
            return -1;
        }
        if let Some(node) = undo::remove_at(&mut doc, parent, index) {
            undo::insert_at(&mut doc, parent, index + 1, node);
            doc.history.push(undo::Command::Move {
                from_parent: parent,
                from_index: index,
                to_parent: parent,
                to_index: index + 1,
                label: "Move Layer Up".into(),
            });
        }
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_move_down(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(path) = doc.find_path(id) else { set_err("layer not found"); return -1; };
        let parent = parent_id_of(&doc, id);
        let index = *path.chain.last().unwrap_or(&0) as usize;
        if index == 0 {
            set_err("already bottom");
            return -1;
        }
        if let Some(node) = undo::remove_at(&mut doc, parent, index) {
            undo::insert_at(&mut doc, parent, index - 1, node);
            doc.history.push(undo::Command::Move {
                from_parent: parent,
                from_index: index,
                to_parent: parent,
                to_index: index - 1,
                label: "Move Layer Down".into(),
            });
        }
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

fn doc_group_len(doc: &Document, parent: u64) -> usize {
    if parent == 0 {
        return doc.root.children.len();
    }
    doc.get_node(parent).and_then(|n| n.as_group()).map(|g| g.children.len()).unwrap_or(0)
}

// ===================== masks =====================

#[no_mangle]
pub extern "C" fn aurora_layer_mask_from_selection(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if !doc.selection.active {
            set_err("no active selection");
            return -1;
        }
        let mask = doc.selection.as_gray().unwrap();
        let Some(Node::Layer(l)) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        if l.mask.is_some() {
            set_err("layer already has a mask");
            return -1;
        }
        l.mask = Some(mask);
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::MaskAdded(id),
            label: "Add Layer Mask".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_mask_apply(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = {
            let Some(Node::Layer(l)) = doc.get_node_mut(id) else {
                set_err("layer not found");
                return -1;
            };
            if l.mask.is_none() {
                set_err("no mask on layer");
                return -1;
            }
            l.pixels.clone()
        };
        let after = {
            let layer = doc.get_layer(id).unwrap().clone();
            let mut px = layer.pixels.clone();
            if let Some(m) = &layer.mask {
                for y in 0..layer.pixels.h as i32 {
                    for x in 0..layer.pixels.w as i32 {
                        let mv = m.get(x, y);
                        let p = px.get(x, y);
                        px.set(x, y, [p[0], p[1], p[2], ((p[3] as u32 * mv as u32) / 255) as u8]);
                    }
                }
            }
            px
        };
        if let Some(Node::Layer(l)) = doc.get_node_mut(id) {
            l.pixels = after;
            l.mask = None;
        }
        let after_data = doc.get_layer(id).unwrap().pixels.data.clone();
        let full = Rect::new(0, 0, doc.w, doc.h);
        doc.history.push(undo::Command::Pixels {
            layer_id: id,
            rect: full,
            before: before.data,
            after: after_data,
            label: "Apply Mask".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_mask_delete(h: u64, id: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let Some(Node::Layer(l)) = doc.get_node_mut(id) else {
            set_err("layer not found");
            return -1;
        };
        if l.mask.is_none() {
            set_err("no mask on layer");
            return -1;
        }
        let m = l.mask.take().unwrap();
        doc.history.push(undo::Command::Prop {
            id,
            kind: undo::PropKind::MaskRemoved(id, Box::new(m)),
            label: "Delete Mask".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

// ===================== thumbnails =====================

#[no_mangle]
pub extern "C" fn aurora_layer_thumbnail(h: u64, id: u64, size: u32, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        let Some(node) = doc.get_node(id) else {
            set_err("layer not found");
            return -1;
        };
        let img: Rgba8 = match node {
            Node::Layer(l) => l.pixels.clone(),
            Node::Group(g) => {
                let mut out = Rgba8::new(doc.w.max(1), doc.h.max(1));
                blend_children_pub(&g.children, &mut out);
                out
            }
        };
        let t = img.thumbnail(size, size);
        let need = (t.w * t.h * 4) as usize;
        if (cap as usize) < need || buf.is_null() {
            return need as i32;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(t.data.as_ptr(), buf, need);
        }
        need as i32
    })
    .unwrap_or(-1)
}

fn blend_children_pub(children: &[Node], out: &mut Rgba8) {
    for n in children {
        if let Node::Layer(l) = n {
            if !l.visible {
                continue;
            }
            blend::composite_region(out, &l.pixels, l.blend, l.opacity, Rect::new(0, 0, out.w, out.h), (0, 0), None);
        }
    }
}

// ===================== selection =====================

fn sel_mode_from(doc: &Document, mode: i32) -> (bool, i32) {
    // mode: 0 replace, 1 add, 2 subtract — set active accordingly
    let _ = doc;
    (mode >= 0, mode.clamp(0, 3))
}

fn apply_select(doc: &mut Document, mask: img::Gray8, mode: i32, label: &str) {
    let before = undo::capture_selection(doc);
    if mode == 0 && !doc.selection.active {
        // nothing
    }
    doc.selection.combine(&mask, mode);
    let after = undo::capture_selection(doc);
    doc.history.push(undo::Command::Prop {
        id: 0,
        kind: undo::PropKind::SelectionChanged { before, after },
        label: label.into(),
    });
}

#[no_mangle]
pub extern "C" fn aurora_select_rect(h: u64, x: i32, y: i32, w: i32, hh: i32, mode: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let m = selection::rect_mask(&doc, x, y, w, hh);
        apply_select(&mut doc, m, mode, "Rectangular Selection");
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_ellipse(h: u64, x: i32, y: i32, w: i32, hh: i32, mode: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let m = selection::ellipse_mask(&doc, x, y, w, hh);
        apply_select(&mut doc, m, mode, "Elliptical Selection");
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_lasso(h: u64, pts: *const f32, n: i32, mode: i32) -> i32 {
    guard(|| {
        if pts.is_null() || n < 3 {
            set_err("lasso needs >= 3 points");
            return -1;
        }
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let slice = unsafe { std::slice::from_raw_parts(pts, (n * 2) as usize) };
        let points: Vec<(f32, f32)> = slice.chunks(2).map(|c| (c[0], c[1])).collect();
        let m = selection::polygon_mask(&doc, &points);
        apply_select(&mut doc, m, mode, "Lasso Selection");
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_wand(h: u64, x: i32, y: i32, tolerance: i32, contiguous: i32, sample_layer: i32, mode: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let m = selection::wand_mask(&doc, x, y, tolerance, contiguous != 0, sample_layer != 0);
        apply_select(&mut doc, m, mode, "Magic Wand");
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_all(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let (w, hh) = (doc.w, doc.h);
        let before = undo::capture_selection(&doc);
        doc.selection = layer::Selection::all(w, hh);
        let after = undo::capture_selection(&doc);
        doc.history.push(undo::Command::Prop {
            id: 0,
            kind: undo::PropKind::SelectionChanged { before, after },
            label: "Select All".into(),
        });
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_none(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::capture_selection(&doc);
        let (w, hh) = (doc.w, doc.h);
        doc.selection = layer::Selection::none(w, hh);
        let after = undo::capture_selection(&doc);
        if before.is_some() {
            doc.history.push(undo::Command::Prop {
                id: 0,
                kind: undo::PropKind::SelectionChanged { before, after },
                label: "Deselect".into(),
            });
        }
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_invert(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::capture_selection(&doc);
        selection::invert(&mut doc);
        let after = undo::capture_selection(&doc);
        doc.history.push(undo::Command::Prop {
            id: 0,
            kind: undo::PropKind::SelectionChanged { before, after },
            label: "Invert Selection".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_feather(h: u64, radius: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if !doc.selection.active {
            set_err("no active selection");
            return -1;
        }
        let before = undo::capture_selection(&doc);
        let mut m = img::Gray8 { w: doc.selection.w, h: doc.selection.h, data: doc.selection.data.clone() };
        selection::feather(&mut m, radius.max(0.1));
        doc.selection.data = m.data;
        doc.selection.active = doc.selection.data.iter().any(|&v| v > 0);
        let after = undo::capture_selection(&doc);
        doc.history.push(undo::Command::Prop {
            id: 0,
            kind: undo::PropKind::SelectionChanged { before, after },
            label: "Feather Selection".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_select_contour(h: u64, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        let loops = selection::contour(&doc);
        let mut s = String::from("[");
        for (i, lp) in loops.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push('[');
            for (j, (x, y)) in lp.iter().enumerate() {
                if j > 0 {
                    s.push(',');
                }
                s.push_str(&format!("{:.1},{:.1}", x, y));
            }
            s.push(']');
        }
        s.push(']');
        put_str(buf, cap, &s)
    })
    .unwrap_or(-1)
}

// ===================== brush =====================

#[no_mangle]
pub extern "C" fn aurora_brush_begin(h: u64, layer_id: u64, params: *const BrushFFI, x: f32, y: f32, pressure: f32) -> i32 {
    guard(|| {
        if params.is_null() {
            set_err("null brush params");
            return -1;
        }
        let d = get_doc(h);
        let bp = conv_brush(unsafe { &*params });
        let mut doc = d.lock().unwrap();
        if doc.get_layer(layer_id).is_none() {
            set_err("brush: layer not found");
            return -1;
        }
        if let Some(Node::Layer(l)) = doc.get_node(layer_id) {
            if l.locked {
                set_err("layer is locked");
                return -1;
            }
        }
        LAST_STROKE_LAYER.with(|l| *l.borrow_mut() = layer_id);
        LAST_STROKE_ERASER.with(|e| *e.borrow_mut() = bp.eraser);
        let s = brush::begin(layer_id, bp, x, y, pressure);
        strokes().lock().unwrap().insert(h, Some(s));
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_brush_move(h: u64, x: f32, y: f32, pressure: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let mut sg = strokes().lock().unwrap();
        let mut stroke_opt = sg.get_mut(&h).and_then(|s| s.take());
        let result = match stroke_opt.as_mut() {
            Some(s) => {
                s.add_point(x, y, pressure);
                s.flush_to_layer(&mut doc);
                let r = s.touched_rect().clip_to(doc.w, doc.h);
                doc.mark_dirty(r);
                0
            }
            None => {
                set_err("no active stroke");
                -1
            }
        };
        // put the stroke back for subsequent move calls
        if let Some(s) = stroke_opt.take() {
            sg.insert(h, Some(s));
        }
        result
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_brush_end(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let stroke = strokes().lock().unwrap().remove(&h).and_then(|s| s);
        match stroke {
            Some(s) => {
                let layer_id = LAST_STROKE_LAYER.with(|l| l.borrow().to_owned());
                let eraser = LAST_STROKE_ERASER.with(|e| e.borrow().to_owned());
                let lid = if layer_id != 0 { layer_id } else { doc.active_id };
                if let Some((rect, before, after)) = s.finish(&mut doc, "Brush") {
                    doc.history.push(undo::Command::Pixels {
                        layer_id: lid,
                        rect,
                        before,
                        after,
                        label: if eraser { "Eraser" } else { "Brush Stroke" }.to_string(),
                    });
                }
                doc.process_dirty();
                0
            }
            None => {
                set_err("no active stroke");
                -1
            }
        }
    })
    .unwrap_or(-1)
}

thread_local! {
    static LAST_STROKE_LAYER: RefCell<u64> = const { RefCell::new(0) };
    static LAST_STROKE_ERASER: RefCell<bool> = const { RefCell::new(false) };
}

// ===================== fill tools =====================

#[no_mangle]
pub extern "C" fn aurora_fill(h: u64, r: u8, g: u8, b: u8, a: u8) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let color = [r, g, b, a];
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        if let Some(region) = paint::fill(&mut doc, color) {
            let after = pixel_region_after(&mut doc, layer_id, region);
            doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Fill".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

fn pixel_region_before(doc: &mut Document, layer_id: u64, region: Rect) -> Vec<u8> {
    let rect = doc.selection.bounds().unwrap_or(region).clip_to(doc.w, doc.h);
    let n = (rect.w as usize) * (rect.h as usize) * 4;
    let mut buf = vec![0u8; n];
    if rect.is_empty() {
        return buf;
    }
    let layer = doc.get_layer(layer_id).unwrap();
    let mut o = 0;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            buf[o..o + 4].copy_from_slice(&layer.pixels.get(x, y));
            o += 4;
        }
    }
    buf
}

fn pixel_region_after(doc: &mut Document, layer_id: u64, region: Rect) -> Vec<u8> {
    pixel_region_before(doc, layer_id, region)
}

#[no_mangle]
pub extern "C" fn aurora_bucket(h: u64, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8, tolerance: i32, contiguous: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        if let Some(region) = paint::bucket(&mut doc, x, y, [r, g, b, a], tolerance, contiguous != 0) {
            let after = pixel_region_after(&mut doc, layer_id, region);
            doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Paint Bucket".into() });
                        doc.process_dirty();
0
        } else {
            set_err("bucket: no pixels changed");
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_gradient(h: u64, x0: f32, y0: f32, x1: f32, y1: f32, kind: i32, fill: i32,
                                  fr: u8, fgn: u8, fb: u8, fa: u8, br: u8, bgc: u8, bb: u8, ba: u8, dither: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        let kind = if kind == 1 { paint::GradientKind::Radial } else { paint::GradientKind::Linear };
        let fillm = if fill == 1 { paint::GradientFill::FgTransparent } else { paint::GradientFill::FgBg };
        if let Some(region) = paint::gradient(&mut doc, x0, y0, x1, y1, kind, fillm,
            [fr, fgn, fb, fa], [br, bgc, bb, ba], dither != 0) {
            let after = pixel_region_after(&mut doc, layer_id, region);
            doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Gradient".into() });
                        doc.process_dirty();
0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

/// Paste RGBA pixels onto a layer at (x, y) — used by text/shape commit & clipboard paste.
#[no_mangle]
pub extern "C" fn aurora_paste_pixels(h: u64, layer_id: u64, x: i32, y: i32, w: u32, hh: u32, buf: *const u8, cap: u32) -> i32 {
    guard(|| {
        if buf.is_null() {
            set_err("null pixel buffer");
            return -1;
        }
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let need = (w as usize) * (hh as usize) * 4;
        if (cap as usize) < need {
            set_err("paste buffer too small");
            return -1;
        }
        let slice = unsafe { std::slice::from_raw_parts(buf, need) };
        let region = Rect::new(x, y, w, hh).clip_to(doc.w, doc.h);
        if region.is_empty() {
            return 0;
        }
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        {
            let Some(layer) = doc.get_layer_mut(layer_id) else {
                set_err("layer not found");
                return -1;
            };
            let mut o = 0usize;
            for py in 0..hh as i32 {
                for px in 0..w as i32 {
                    let dx = x + px;
                    let dy = y + py;
                    if region.contains(dx, dy) {
                        let p = [slice[o], slice[o + 1], slice[o + 2], slice[o + 3]];
                        layer.pixels.set(dx, dy, p);
                    }
                    o += 4;
                }
            }
        }
        let after = pixel_region_after(&mut doc, layer_id, region);
        doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Paste".into() });
        doc.mark_dirty(region);
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

/// Paste a PNG (decoded in-engine) onto a layer at (x, y). Used for Skia-rendered text/shapes.
#[no_mangle]
pub extern "C" fn aurora_paste_png(h: u64, layer_id: u64, x: i32, y: i32, buf: *const u8, cap: u32) -> i32 {
    guard(|| {
        if buf.is_null() || cap == 0 {
            set_err("null png buffer");
            return -1;
        }
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let slice = unsafe { std::slice::from_raw_parts(buf, cap as usize) };
        let img = match image::load_from_memory(slice) {
            Ok(i) => i.to_rgba8(),
            Err(e) => {
                set_err(format!("png decode: {e}"));
                return -1;
            }
        };
        let (w, h) = (img.width(), img.height());
        let region = Rect::new(x, y, w, h).clip_to(doc.w, doc.h);
        if region.is_empty() {
            return 0;
        }
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        {
            let Some(layer) = doc.get_layer_mut(layer_id) else {
                set_err("layer not found");
                return -1;
            };
            for py in 0..h as i32 {
                for px in 0..w as i32 {
                    let dx = x + px;
                    let dy = y + py;
                    if region.contains(dx, dy) {
                        layer.pixels.set(dx, dy, img.get_pixel(px as u32, py as u32).0);
                    }
                }
            }
        }
        let after = pixel_region_after(&mut doc, layer_id, region);
        doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Paste".into() });
        doc.mark_dirty(region);
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

/// Clear selection content (or whole layer if no selection) on the active layer.
#[no_mangle]
pub extern "C" fn aurora_layer_clear(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h));
        let full = Rect::new(0, 0, doc.w, doc.h);
        let before = pixel_region_before(&mut doc, layer_id, full);
        {
            let sel = doc.selection.as_gray();
            let Some(layer) = doc.get_layer_mut(layer_id) else {
                set_err("layer not found");
                return -1;
            };
            for y in region.y..region.bottom() {
                for x in region.x..region.right() {
                    if let Some(s) = &sel {
                        if s.get(x, y) == 0 { continue; }
                    }
                    layer.pixels.set(x, y, [0, 0, 0, 0]);
                }
            }
        }
        let after = pixel_region_after(&mut doc, layer_id, region);
        doc.history.push(undo::Command::Pixels { layer_id, rect: region, before, after, label: "Clear".into() });
        doc.mark_dirty(region);
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

// ===================== adjustments =====================

/// Curves via JSON: {"rgb":[[x,y],...],"r":[...],"g":[...],"b":[...]} — points 0..1.
#[no_mangle]
pub extern "C" fn aurora_adj_curves(h: u64, json: *const c_char) -> i32 {
    guard(|| {
        let js = read_str(json);
        let params: adjust::CurvesParams = match serde_json::from_str(&js) {
            Ok(v) => v,
            Err(e) => {
                set_err(format!("curves: {e}"));
                return -1;
            }
        };
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = adjust::curves(&mut doc, &params) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Curves".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_adj_levels(h: u64, in_black: f32, in_white: f32, gamma: f32, out_black: f32, out_white: f32, channel: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let p = adjust::LevelsParams { in_black, in_white, gamma, out_black, out_white, channel };
        if let Some((rect, before, after)) = adjust::levels(&mut doc, p) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Levels".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_adj_bc(h: u64, brightness: f32, contrast: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = adjust::brightness_contrast(&mut doc, adjust::BcParams { brightness, contrast }) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Brightness/Contrast".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_adj_hsl(h: u64, hue: f32, sat: f32, light: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = adjust::hue_saturation(&mut doc, adjust::HslParams { hue, saturation: sat, lightness: light }) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Hue/Saturation".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

// ===================== filters =====================

#[no_mangle]
pub extern "C" fn aurora_filter_gauss(h: u64, radius: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::gaussian_blur(&mut doc, radius) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Gaussian Blur".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_sharpen(h: u64, amount: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::sharpen(&mut doc, amount) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Sharpen".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_noise(h: u64, amount: i32, mono: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::add_noise(&mut doc, amount, mono != 0) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Add Noise".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_pixelate(h: u64, size: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::pixelate(&mut doc, size) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Pixelate".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_twirl(h: u64, cx: f32, cy: f32, radius: f32, angle: f32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::twirl(&mut doc, cx, cy, radius, angle) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Twirl".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_wave(h: u64, amplitude: f32, wavelength: f32, vertical: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::wave(&mut doc, amplitude, wavelength, vertical != 0) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Wave".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_filter_emboss(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        if let Some((rect, before, after)) = filters::emboss(&mut doc) {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: "Emboss".into() });
            doc.process_dirty();
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

// ══════════════════════════════════════════════════════════════════════
// v3.0: professional adjustments & effects — FFI surface
// ══════════════════════════════════════════════════════════════════════

/// Commit a pixel op returned by filters/adjust (selection-aware, undo-integrated).
fn commit_pixop(doc: &mut Document, layer_id: u64, result: Option<(Rect, Vec<u8>, Vec<u8>)>, label: &str) -> i32 {
    match result {
        Some((rect, before, after)) => {
            doc.history.push(undo::Command::Pixels { layer_id, rect, before, after, label: label.into() });
            doc.process_dirty();
            0
        }
        None => {
            set_err(format!("{}: nothing to apply (empty layer or selection)", label));
            -1
        }
    }
}

fn apply_f<F>(h: u64, label: &str, f: F) -> i32
where
    F: FnOnce(&mut Document) -> Option<(Rect, Vec<u8>, Vec<u8>)>,
{
    guard(move || {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let layer_id = doc.active_id;
        let result = f(&mut doc);
        commit_pixop(&mut doc, layer_id, result, label)
    })
    .unwrap_or(-1)
}

// ---------- new adjustments ----------

#[no_mangle]
pub extern "C" fn aurora_adj_exposure(h: u64, stops: f32, gamma: f32) -> i32 {
    apply_f(h, "Exposure", |doc| adjust::exposure_gamma(doc, stops, gamma))
}

#[no_mangle]
pub extern "C" fn aurora_adj_vibrance(h: u64, amount: i32) -> i32 {
    apply_f(h, "Vibrance", |doc| adjust::vibrance(doc, amount))
}

#[no_mangle]
pub extern "C" fn aurora_adj_white_balance(h: u64, temperature: i32, tint: i32) -> i32 {
    apply_f(h, "White Balance", |doc| adjust::white_balance(doc, temperature, tint))
}

#[no_mangle]
pub extern "C" fn aurora_adj_shadows_highlights(h: u64, shadows: i32, highlights: i32) -> i32 {
    apply_f(h, "Shadows/Highlights", |doc| adjust::shadows_highlights(doc, shadows, highlights))
}

#[no_mangle]
pub extern "C" fn aurora_adj_color_balance(h: u64, cr: i32, mg: i32, yb: i32) -> i32 {
    apply_f(h, "Color Balance", |doc| adjust::color_balance(doc, cr, mg, yb))
}

#[no_mangle]
pub extern "C" fn aurora_adj_black_white(h: u64, rw: i32, gw: i32, bw: i32) -> i32 {
    apply_f(h, "Black & White", |doc| adjust::black_white(doc, rw, gw, bw))
}

#[no_mangle]
pub extern "C" fn aurora_adj_desaturate(h: u64) -> i32 {
    apply_f(h, "Desaturate", |doc| adjust::desaturate(doc))
}

#[no_mangle]
pub extern "C" fn aurora_adj_invert(h: u64) -> i32 {
    apply_f(h, "Invert", |doc| adjust::invert(doc))
}

#[no_mangle]
pub extern "C" fn aurora_adj_threshold(h: u64, level: i32) -> i32 {
    apply_f(h, "Threshold", |doc| adjust::threshold(doc, level))
}

#[no_mangle]
pub extern "C" fn aurora_adj_posterize(h: u64, levels: i32) -> i32 {
    apply_f(h, "Posterize", |doc| adjust::posterize(doc, levels))
}

#[no_mangle]
pub extern "C" fn aurora_adj_photo_filter(h: u64, tr: u8, tg: u8, tb: u8, density: i32, preserve: i32) -> i32 {
    apply_f(h, "Photo Filter", |doc| adjust::photo_filter(doc, tr, tg, tb, density, preserve != 0))
}

#[no_mangle]
pub extern "C" fn aurora_adj_gradient_map(h: u64, r0: u8, g0: u8, b0: u8, r1: u8, g1: u8, b1: u8) -> i32 {
    apply_f(h, "Gradient Map", |doc| adjust::gradient_map(doc, r0, g0, b0, r1, g1, b1))
}

#[no_mangle]
pub extern "C" fn aurora_adj_auto_tone(h: u64) -> i32 {
    apply_f(h, "Auto Tone", |doc| adjust::auto_tone(doc))
}

#[no_mangle]
pub extern "C" fn aurora_adj_auto_contrast(h: u64) -> i32 {
    apply_f(h, "Auto Contrast", |doc| adjust::auto_contrast(doc))
}

#[no_mangle]
pub extern "C" fn aurora_adj_auto_color(h: u64) -> i32 {
    apply_f(h, "Auto Color", |doc| adjust::auto_color(doc))
}

#[no_mangle]
pub extern "C" fn aurora_adj_clarity(h: u64, amount: i32) -> i32 {
    apply_f(h, "Clarity", |doc| adjust::clarity(doc, amount))
}

// ---------- new effects ----------

#[no_mangle]
pub extern "C" fn aurora_filter_box_blur(h: u64, radius: i32) -> i32 {
    apply_f(h, "Box Blur", |doc| filters::box_blur(doc, radius))
}

#[no_mangle]
pub extern "C" fn aurora_filter_motion_blur(h: u64, length: i32, angle: f32) -> i32 {
    apply_f(h, "Motion Blur", |doc| filters::motion_blur(doc, length, angle))
}

#[no_mangle]
pub extern "C" fn aurora_filter_zoom_blur(h: u64, amount: i32) -> i32 {
    apply_f(h, "Zoom Blur", |doc| filters::zoom_blur(doc, amount))
}

#[no_mangle]
pub extern "C" fn aurora_filter_unsharp(h: u64, radius: f32, strength: f32, threshold: i32) -> i32 {
    apply_f(h, "Unsharp Mask", |doc| filters::unsharp_mask(doc, radius, strength, threshold))
}

#[no_mangle]
pub extern "C" fn aurora_filter_find_edges(h: u64, invert: i32) -> i32 {
    apply_f(h, "Find Edges", |doc| filters::find_edges(doc, invert != 0))
}

#[no_mangle]
pub extern "C" fn aurora_filter_oil_paint(h: u64, radius: i32) -> i32 {
    apply_f(h, "Oil Paint", |doc| filters::oil_paint(doc, radius))
}

#[no_mangle]
pub extern "C" fn aurora_filter_halftone(h: u64, cell: i32) -> i32 {
    apply_f(h, "Halftone", |doc| filters::halftone(doc, cell))
}

#[no_mangle]
pub extern "C" fn aurora_filter_charcoal(h: u64, detail: i32) -> i32 {
    apply_f(h, "Charcoal", |doc| filters::charcoal(doc, detail))
}

#[no_mangle]
pub extern "C" fn aurora_filter_pencil(h: u64, strength: i32) -> i32 {
    apply_f(h, "Pencil Sketch", |doc| filters::pencil_sketch(doc, strength))
}

#[no_mangle]
pub extern "C" fn aurora_filter_median(h: u64) -> i32 {
    apply_f(h, "Noise Reduction", |doc| filters::median_denoise(doc))
}

#[no_mangle]
pub extern "C" fn aurora_filter_vignette(h: u64, amount: i32, roundness: i32) -> i32 {
    apply_f(h, "Vignette", |doc| filters::vignette(doc, amount, roundness))
}

#[no_mangle]
pub extern "C" fn aurora_filter_bloom(h: u64, radius: f32, intensity: i32) -> i32 {
    apply_f(h, "Bloom", |doc| filters::bloom(doc, radius, intensity))
}

#[no_mangle]
pub extern "C" fn aurora_filter_grain(h: u64, amount: i32, size: i32) -> i32 {
    apply_f(h, "Film Grain", |doc| filters::film_grain(doc, amount, size))
}

#[no_mangle]
pub extern "C" fn aurora_filter_scanlines(h: u64, spacing: i32, intensity: i32) -> i32 {
    apply_f(h, "Scanlines", |doc| filters::scanlines(doc, spacing, intensity))
}

#[no_mangle]
pub extern "C" fn aurora_filter_glitch(h: u64, strength: i32) -> i32 {
    apply_f(h, "Glitch", |doc| filters::glitch(doc, strength))
}

#[no_mangle]
pub extern "C" fn aurora_filter_chromatic(h: u64, amount: i32) -> i32 {
    apply_f(h, "Chromatic Aberration", |doc| filters::chromatic_aberration(doc, amount))
}

#[no_mangle]
pub extern "C" fn aurora_filter_duotone(h: u64, sr: u8, sg: u8, sb: u8, hr: u8, hg: u8, hb: u8) -> i32 {
    apply_f(h, "Duotone", |doc| filters::duotone(doc, sr, sg, sb, hr, hg, hb))
}

#[no_mangle]
pub extern "C" fn aurora_filter_ripple(h: u64, amplitude: f32, wavelength: f32, cx: f32, cy: f32) -> i32 {
    apply_f(h, "Ripple", |doc| filters::ripple(doc, amplitude, wavelength, cx, cy))
}

#[no_mangle]
pub extern "C" fn aurora_filter_pinch(h: u64, amount: i32, cx: f32, cy: f32, radius: f32) -> i32 {
    apply_f(h, "Pinch", |doc| filters::pinch(doc, amount, cx, cy, radius))
}

#[no_mangle]
pub extern "C" fn aurora_filter_clouds(h: u64, scale: f32, seed: u32, opacity: i32) -> i32 {
    apply_f(h, "Clouds", |doc| filters::clouds(doc, scale, seed, opacity))
}

/// Luminance histogram of the flattened composite (or selection region).
/// Writes 256 bins; returns needed byte count (256) or negative on error.
#[no_mangle]
pub extern "C" fn aurora_histogram(h: u64, buf: *mut u8, cap: u32) -> i32 {
    guard(|| {
        if buf.is_null() || (cap as usize) < 256 {
            set_err("histogram buffer too small");
            return -1;
        }
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        let region = doc.selection.bounds().unwrap_or(Rect::new(0, 0, doc.w, doc.h)).clip_to(doc.w, doc.h);
        let mut bins = [0u32; 256];
        // composite already accounts for visibility/opacity/blend
        let comp = doc.composite_image();
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let px = comp.get(x, y);
                if px[3] == 0 { continue; }
                let lum = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as usize;
                bins[lum.min(255)] += 1;
            }
        }
        let max = bins.iter().copied().max().unwrap_or(1).max(1) as f32;
        let out = unsafe { std::slice::from_raw_parts_mut(buf, 256) };
        for (i, b) in bins.iter().enumerate() {
            // normalize to 0..255 for compact transport; C# can rescale
            let v = (*b as f32 / max * 255.0) as u8;
            out[i] = if *b > 0 { v.max(2) } else { 0 };
        }
        256
    })
    .unwrap_or(-1)
}

// ===================== undo =====================

#[no_mangle]
pub extern "C" fn aurora_undo(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if undo::undo(&mut doc) { 0 } else { -1 }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_redo(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if undo::redo(&mut doc) { 0 } else { -1 }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_history_set(h: u64, index: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        undo::set_depth(&mut doc, index as usize);
        0
    })
    .unwrap_or(-1)
}

// ===================== doc ops =====================

#[no_mangle]
pub extern "C" fn aurora_image_resize(h: u64, nw: u32, nh: u32, interp: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::resize_doc_image(&mut doc, nw, nh, interp_to(interp));
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Image Size".into(),
        });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

fn interp_to(v: i32) -> transform::Interp {
    match v {
        0 => transform::Interp::Nearest,
        2 => transform::Interp::Bicubic,
        _ => transform::Interp::Bilinear,
    }
}

#[no_mangle]
pub extern "C" fn aurora_canvas_resize(h: u64, nw: u32, nh: u32, anchor: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::resize_canvas(&mut doc, nw, nh, anchor);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Canvas Size".into(),
        });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_doc_crop(h: u64, x: i32, y: i32, w: u32, hh: u32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::crop(&mut doc, x, y, w, hh);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Crop".into(),
        });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_image_rotate(h: u64, turns: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::rotate_doc(&mut doc, turns);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Rotate Canvas".into(),
        });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

/// Flip whole document (all layers). axis: 0 = horizontal, 1 = vertical.
#[no_mangle]
pub extern "C" fn aurora_image_flip(h: u64, axis: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        let ids = doc.layer_ids();
        for id in ids {
            transform::flip_layer(&mut doc, id, axis);
        }
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Flip Canvas".into(),
        });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_doc_flatten(h: u64) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        if doc.root.children.len() <= 1 {
            if matches!(doc.root.children.first(), Some(Node::Layer(_))) {
                return 0;
            }
        }
        let flat = doc.flatten();
        let before = undo::DocSnapshot::capture(&doc);
        let id = doc.alloc_id();
        let mut l = layer::Layer::new(id, "Flattened", doc.w, doc.h);
        l.pixels = flat;
        doc.root.children.clear();
        doc.root.children.push(Node::Layer(l));
        doc.active_id = id;
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot {
            before: Box::new(before),
            after: Box::new(after),
            label: "Flatten Image".into(),
        });
        doc.recomposite_all();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_doc_set_name(h: u64, name: *const c_char) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        doc.name = read_str(name);
        0
    })
    .unwrap_or(-1)
}

// ===================== layer transforms =====================

#[no_mangle]
pub extern "C" fn aurora_layer_flip(h: u64, id: u64, axis: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::flip_layer(&mut doc, id, axis);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot { before: Box::new(before), after: Box::new(after), label: "Flip Layer".into() });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_rotate(h: u64, id: u64, degrees: f32, expand: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::rotate_layer(&mut doc, id, degrees, expand != 0);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot { before: Box::new(before), after: Box::new(after), label: "Rotate Layer".into() });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn aurora_layer_move_by(h: u64, id: u64, dx: i32, dy: i32) -> i32 {
    guard(|| {
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        transform::move_layer(&mut doc, id, dx, dy);
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot { before: Box::new(before), after: Box::new(after), label: "Move Layer".into() });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

/// Free-transform commit. mode: 0 = affine (m: 6 floats), 1 = perspective (corners: 8 floats).
#[no_mangle]
pub extern "C" fn aurora_layer_warp(h: u64, id: u64, mode: i32, m: *const f32, nw: u32, nh: u32) -> i32 {
    guard(|| {
        if m.is_null() {
            set_err("null transform matrix");
            return -1;
        }
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        let before = undo::DocSnapshot::capture(&doc);
        if mode == 0 {
            let mm = unsafe { std::slice::from_raw_parts(m, 6) };
            let mat: [f32; 6] = [mm[0], mm[1], mm[2], mm[3], mm[4], mm[5]];
            transform::affine_layer(&mut doc, id, mat, nw, nh);
        } else {
            let mm = unsafe { std::slice::from_raw_parts(m, 8) };
            let corners = [
                (mm[0], mm[1]),
                (mm[2], mm[3]),
                (mm[4], mm[5]),
                (mm[6], mm[7]),
            ];
            transform::perspective_warp(&mut doc, id, corners, nw, nh);
        }
        let after = undo::DocSnapshot::capture(&doc);
        doc.history.push(undo::Command::DocSnapshot { before: Box::new(before), after: Box::new(after), label: "Transform".into() });
        doc.process_dirty();
        0
    })
    .unwrap_or(-1)
}

/// Current active layer id (source of truth — avoids stale cached state in the UI).
#[no_mangle]
pub extern "C" fn aurora_active_layer(h: u64) -> u64 {
    guard(|| {
        let d = get_doc(h);
        let doc = d.lock().unwrap();
        doc.active_id
    })
    .unwrap_or(0)
}

// ===================== io =====================

#[no_mangle]
pub extern "C" fn aurora_doc_export(h: u64, path: *const c_char, format: *const c_char,
                                    quality: i32, compression: i32, lossless: i32, frames: i32) -> i32 {
    guard(|| {
        let p = read_str(path);
        let f = read_str(format);
        let fmt: &'static str = match f.as_str() {
            "png" => "png", "jpeg" | "jpg" => "jpeg", "webp" => "webp", "gif" => "gif",
            "bmp" => "bmp", "tiff" => "tiff", "svg" => "svg", "ora" => "ora", "psd" => "psd",
            other => {
                set_err(format!("unsupported format: {other}"));
                return -1;
            }
        };
        let params = io::ExportParams {
            path: p,
            format: fmt,
            quality,
            compression,
            lossless: lossless != 0,
            layers_as_frames: frames != 0,
        };
        let d = get_doc(h);
        let mut doc = d.lock().unwrap();
        match io::export(&mut doc, &params) {
            Ok(()) => 0,
            Err(e) => {
                set_err(e);
                -1
            }
        }
    })
    .unwrap_or(-1)
}

// ===================== self test =====================

/// Internal sanity checks; returns 0 on pass, negative on failure (also sets last error).
#[no_mangle]
pub extern "C" fn aurora_selftest() -> i32 {
    guard(|| {
        // 1. document + composite
        let h = aurora_doc_new(64, 64, 2, c"test".as_ptr());
        if h == 0 { set_err("selftest: doc_new failed"); return -1; }
        // 2. brush stroke
        let layer = aurora_layer_add(h, -1, c"L".as_ptr(), 0);
        if layer == 0 { set_err("selftest: layer_add failed"); return -2; }
        let bp = BrushFFI { size: 10.0, hardness: 0.8, flow: 1.0, opacity: 1.0, spacing: 0.1, eraser: 0, r: 255, g: 0, b: 0, a: 255, size_pressure: 0, opacity_pressure: 0, pencil: 0 };
        if aurora_brush_begin(h, layer, &bp, 10.0, 10.0, 1.0) != 0 { set_err(format!("selftest: brush_begin: {}", take_err())); return -3; }
        for i in 0..30 {
            if aurora_brush_move(h, 10.0 + i as f32, 30.0, 1.0) != 0 { set_err(format!("selftest: brush_move: {}", take_err())); return -4; }
        }
        if aurora_brush_end(h) != 0 { set_err(format!("selftest: brush_end: {}", take_err())); return -5; }
        // verify red pixels exist
        {
            let d = get_doc(h);
            let doc = d.lock().unwrap();
            let found = doc.get_layer(layer).unwrap().pixels.data.chunks(4).any(|c| c[0] > 200 && c[3] > 100);
            if !found { set_err("selftest: brush produced no pixels"); return -6; }
        }
        // 3. selection
        if aurora_select_rect(h, 0, 0, 20, 20, 0) != 0 { set_err(format!("selftest: select: {}", take_err())); return -7; }
        // 4. filter
        if aurora_filter_gauss(h, 2.0) != 0 { set_err(format!("selftest: filter: {}", take_err())); return -8; }
        // 5. adjustment
        if aurora_adj_bc(h, 10.0, 5.0) != 0 { set_err(format!("selftest: adj: {}", take_err())); return -9; }
        // 6. export png
        let cpath = |p: &std::path::Path| -> std::ffi::CString {
            std::ffi::CString::new(p.to_string_lossy().as_bytes()).unwrap()
        };
        let path = std::env::temp_dir().join(format!("aurora_selftest_{}.png", std::process::id()));
        let cp = cpath(&path);
        if aurora_doc_export(h, cp.as_ptr(), c"png".as_ptr(), 90, 6, 0, 0) != 0 {
            set_err(format!("selftest: export: {}", take_err()));
            return -10;
        }
        if !path.exists() { set_err("selftest: png not written"); return -11; }
        // 7. reopen
        let h2 = aurora_doc_open(cp.as_ptr());
        if h2 == 0 { set_err(format!("selftest: reopen: {}", take_err())); return -12; }
        // 8. undo all the way
        for _ in 0..30 {
            aurora_undo(h);
        }
        // 9. psd roundtrip
        let psd = std::env::temp_dir().join(format!("aurora_selftest_{}.psd", std::process::id()));
        let cpsd = cpath(&psd);
        if aurora_doc_export(h2, cpsd.as_ptr(), c"psd".as_ptr(), 90, 6, 0, 0) != 0 {
            set_err(format!("selftest: psd export: {}", take_err()));
            return -13;
        }
        let h3 = aurora_doc_open(cpsd.as_ptr());
        if h3 == 0 { set_err(format!("selftest: psd reopen: {}", take_err())); return -14; }
        // 10. ora roundtrip
        let ora = std::env::temp_dir().join(format!("aurora_selftest_{}.ora", std::process::id()));
        let cora = cpath(&ora);
        if aurora_doc_export(h3, cora.as_ptr(), c"ora".as_ptr(), 90, 6, 0, 0) != 0 {
            set_err(format!("selftest: ora export: {}", take_err()));
            return -15;
        }
        let _h4 = aurora_doc_open(cora.as_ptr());
        // cleanup
        aurora_doc_free(h);
        aurora_doc_free(h2);
        aurora_doc_free(h3);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&psd);
        let _ = std::fs::remove_file(&ora);
        0
    })
    .unwrap_or(-99)
}

/// v3.0 exhaustive ops audit: every new adjustment & effect must run without
/// error on a real document and remain undoable. Returns 0 on success.
#[no_mangle]
pub extern "C" fn aurora_ops_audit() -> i32 {
    guard(|| {
        let h = aurora_doc_new(96, 96, 2, c"audit".as_ptr());
        if h == 0 { set_err("audit: doc_new failed"); return -1; }
        let layer = aurora_layer_add(h, -1, c"L".as_ptr(), 0);
        // paint a base image so ops have real data to chew on:
        // a brush stroke + a full-canvas gradient guarantees a wide tonal range
        let bp = BrushFFI { size: 24.0, hardness: 0.9, flow: 1.0, opacity: 1.0, spacing: 0.1, eraser: 0, r: 90, g: 140, b: 220, a: 255, size_pressure: 0, opacity_pressure: 0, pencil: 0 };
        if aurora_brush_begin(h, layer, &bp, 10.0, 40.0, 1.0) != 0 { set_err("audit: brush_begin"); return -2; }
        for i in 0..40 { let _ = aurora_brush_move(h, 10.0 + i as f32, 40.0 + (i as f32 * 0.7).sin() * 12.0, 1.0); }
        let _ = aurora_brush_end(h);
        let _ = aurora_gradient(h, 0.0, 0.0, 96.0, 96.0, 0, 0, 245, 240, 230, 255, 15, 18, 30, 255, 0);

        // (fn pointer, name) pairs — each must return 0
        type Op = (fn(u64) -> i32, &'static str);
        let ops: Vec<Op> = vec![
            (|h| aurora_adj_exposure(h, 0.3, 1.1), "exposure"),
            (|h| aurora_adj_vibrance(h, 40), "vibrance"),
            (|h| aurora_adj_white_balance(h, 25, -10), "white_balance"),
            (|h| aurora_adj_shadows_highlights(h, 35, -20), "shadows_highlights"),
            (|h| aurora_adj_color_balance(h, 10, -5, 8), "color_balance"),
            (|h| aurora_adj_black_white(h, 20, 0, -10), "black_white"),
            (|h| aurora_adj_desaturate(h), "desaturate"),
            (|h| aurora_adj_invert(h), "invert"),
            (|h| aurora_adj_threshold(h, 128), "threshold"),
            (|h| aurora_adj_posterize(h, 6), "posterize"),
            (|h| aurora_adj_photo_filter(h, 255, 160, 60, 35, 1), "photo_filter"),
            (|h| aurora_adj_gradient_map(h, 20, 10, 60, 250, 240, 200), "gradient_map"),
            (|h| aurora_adj_auto_tone(h), "auto_tone"),
            (|h| aurora_adj_auto_contrast(h), "auto_contrast"),
            (|h| aurora_adj_auto_color(h), "auto_color"),
            (|h| aurora_adj_clarity(h, 30), "clarity"),
            (|h| aurora_filter_box_blur(h, 4), "box_blur"),
            (|h| aurora_filter_motion_blur(h, 12, 30.0), "motion_blur"),
            (|h| aurora_filter_zoom_blur(h, 25), "zoom_blur"),
            (|h| aurora_filter_unsharp(h, 3.0, 1.2, 4), "unsharp"),
            (|h| aurora_filter_find_edges(h, 0), "find_edges"),
            (|h| aurora_filter_oil_paint(h, 3), "oil_paint"),
            (|h| aurora_filter_halftone(h, 8), "halftone"),
            (|h| aurora_filter_charcoal(h, 6), "charcoal"),
            (|h| aurora_filter_pencil(h, 6), "pencil"),
            (|h| aurora_filter_median(h), "median"),
            (|h| aurora_filter_vignette(h, 60, 50), "vignette"),
            (|h| aurora_filter_bloom(h, 8.0, 40), "bloom"),
            (|h| aurora_filter_grain(h, 30, 2), "grain"),
            (|h| aurora_filter_scanlines(h, 6, 50), "scanlines"),
            (|h| aurora_filter_glitch(h, 20), "glitch"),
            (|h| aurora_filter_chromatic(h, 30), "chromatic"),
            (|h| aurora_filter_duotone(h, 30, 20, 80, 250, 230, 180), "duotone"),
            (|h| aurora_filter_ripple(h, 8.0, 40.0, -1.0, -1.0), "ripple"),
            (|h| aurora_filter_pinch(h, 40, -1.0, -1.0, 0.0), "pinch"),
            (|h| aurora_filter_clouds(h, 8.0, 42, 60), "clouds"),
            (|h| hist_ok(h), "histogram"),
        ];
        for (op, name) in ops {
            // undo to a common base between ops so each sees real content
            if op(h) != 0 {
                set_err(format!("audit: {} failed: {}", name, take_err()));
                return -3;
            }
            // undo the op so the next one also has content (first op needs content too)
            if aurora_undo(h) != 0 {
                set_err(format!("audit: {} undo failed", name));
                return -4;
            }
        }
        // histogram sanity: needs at least one non-zero bin
        unsafe {
            let mut buf = [0u8; 256];
            if aurora_histogram(h, buf.as_mut_ptr(), 256) != 256 {
                set_err("audit: histogram call failed");
                return -5;
            }
            if buf.iter().all(|&b| b == 0) {
                set_err("audit: histogram all-zero");
                return -6;
            }
        }
        aurora_doc_free(h);
        0
    })
    .unwrap_or(-99)
}

fn hist_ok(h: u64) -> i32 {
    let mut buf = [0u8; 256];
    let rc = unsafe { aurora_histogram(h, buf.as_mut_ptr(), 256) };
    if rc == 256 { 0 } else { -1 }
}

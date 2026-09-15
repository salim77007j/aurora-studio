//! Undo / redo — command pattern with self-inverting deltas.
use crate::img::{Rgba8, Rect};
use crate::layer::{Document, Node};

pub struct History {
    pub entries: Vec<Command>,
    pub index: usize, // number of applied commands
    pub limit: usize,
}

#[derive(Clone)]
pub enum Command {
    /// Pixel region change on one layer. before/after: RGBA bytes of the rect.
    Pixels { layer_id: u64, rect: Rect, before: Vec<u8>, after: Vec<u8>, label: String },
    /// A node was inserted (undo removes it).
    Insert { parent_id: u64, index: usize, node: Box<Node>, label: String },
    /// A node was removed (undo re-inserts it).
    Remove { parent_id: u64, index: usize, node: Box<Node>, label: String },
    /// Node moved between parent/index.
    Move { from_parent: u64, from_index: usize, to_parent: u64, to_index: usize, label: String },
    /// Property change.
    Prop { id: u64, kind: PropKind, label: String },
    /// Full-document structural state (resize/canvas/rotate). before/after snapshots.
    DocSnapshot { before: Box<DocSnapshot>, after: Box<DocSnapshot>, label: String },
}

#[derive(Clone)]
pub enum PropKind {
    Visible(bool, bool),   // old, new
    Opacity(f32, f32),
    Blend(i32, i32),
    Name(String, String),
    MaskAdded(u64),
    MaskRemoved(u64, Box<crate::img::Gray8>),
    MaskApplied(u64),
    Active(u64, u64),
    SelectionChanged { before: Option<(u32, u32, Vec<u8>, bool)>, after: Option<(u32, u32, Vec<u8>, bool)> },
}

#[derive(Clone)]
pub struct DocSnapshot {
    pub w: u32,
    pub h: u32,
    pub root: crate::layer::Group,
    pub active_id: u64,
    pub next_id: u64,
    pub selection: crate::layer::Selection,
    pub layers_rgba: Vec<(u64, Rgba8, Option<crate::img::Gray8>)>,
}

impl DocSnapshot {
    pub fn capture(doc: &Document) -> DocSnapshot {
        let mut layers = Vec::new();
        collect_layers(&doc.root, &mut layers);
        DocSnapshot {
            w: doc.w,
            h: doc.h,
            root: doc.root.clone(),
            active_id: doc.active_id,
            next_id: doc.next_id,
            selection: doc.selection.clone(),
            layers_rgba: layers,
        }
    }
}

fn collect_layers(g: &crate::layer::Group, out: &mut Vec<(u64, Rgba8, Option<crate::img::Gray8>)>) {
    for n in &g.children {
        match n {
            crate::layer::Node::Layer(l) => out.push((l.id, l.pixels.clone(), l.mask.clone())),
            crate::layer::Node::Group(cg) => collect_layers(cg, out),
        }
    }
}

impl History {
    pub fn new() -> Self {
        History { entries: Vec::new(), index: 0, limit: 80 }
    }

    pub fn push(&mut self, cmd: Command) {
        if self.entries.len() > self.index {
            self.entries.truncate(self.index);
        }
        // memory guard: drop oldest entries if too many pixel edits
        while self.entries.len() >= self.limit {
            self.entries.remove(0);
            self.index -= 1;
        }
        self.entries.push(cmd);
        self.index = self.entries.len();
    }

    pub fn can_undo(&self) -> bool { self.index > 0 }
    pub fn can_redo(&self) -> bool { self.index < self.entries.len() }

    pub fn undo_label(&self) -> Option<&str> {
        self.entries.get(self.index.wrapping_sub(1)).map(label_of)
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.entries.get(self.index).map(label_of)
    }

    /// List of entry names up to index for the history panel (returns (position, names)).
    pub fn labels(&self) -> (usize, Vec<String>) {
        let names: Vec<String> = self.entries.iter().map(|c| label_of(c).to_string()).collect();
        (self.index, names)
    }
}

fn label_of(c: &Command) -> &str {
    match c {
        Command::Pixels { label, .. } => label,
        Command::Insert { label, .. } => label,
        Command::Remove { label, .. } => label,
        Command::Move { label, .. } => label,
        Command::Prop { label, .. } => label,
        Command::DocSnapshot { label, .. } => label,
    }
}

/// Apply undo of the last applied command.
pub fn undo(doc: &mut Document) -> bool {
    if !doc.history.can_undo() {
        return false;
    }
    doc.history.index -= 1;
    let cmd = doc.history.entries[doc.history.index].clone();
    apply_inverse(doc, &cmd, true);
    doc.recomposite_all();
    true
}

pub fn redo(doc: &mut Document) -> bool {
    if !doc.history.can_redo() {
        return false;
    }
    let cmd = doc.history.entries[doc.history.index].clone();
    apply_inverse(doc, &cmd, false);
    doc.history.index += 1;
    doc.recomposite_all();
    true
}

/// Jump history to a specific position (history panel).
pub fn set_depth(doc: &mut Document, mut target: usize) -> bool {
    target = target.min(doc.history.entries.len());
    while doc.history.index > target {
        if !undo(doc) { break; }
    }
    while doc.history.index < target {
        if !redo(doc) { break; }
    }
    true
}

fn apply_inverse(doc: &mut Document, cmd: &Command, inverse: bool) {
    match cmd {
        Command::Pixels { layer_id, rect, before, after, .. } => {
            let data = if inverse { before } else { after };
            if let Some(l) = doc.get_layer_mut(*layer_id) {
                let mut o = 0usize;
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        let px = [data[o], data[o + 1], data[o + 2], data[o + 3]];
                        l.pixels.set(x, y, px);
                        o += 4;
                    }
                }
            }
        }
        Command::Insert { parent_id, index, node, .. } => {
            if inverse {
                remove_at(doc, *parent_id, *index);
            } else {
                insert_at(doc, *parent_id, *index, (**node).clone());
            }
        }
        Command::Remove { parent_id, index, node, .. } => {
            if inverse {
                insert_at(doc, *parent_id, *index, (**node).clone());
            } else {
                remove_at(doc, *parent_id, *index);
            }
        }
        Command::Move { from_parent, from_index, to_parent, to_index, .. } => {
            let (ap, ai, bp, bi) = if inverse {
                (*to_parent, *to_index, *from_parent, *from_index)
            } else {
                (*from_parent, *from_index, *to_parent, *to_index)
            };
            if let Some(node) = remove_at(doc, ap, ai) {
                insert_at(doc, bp, bi, node);
            }
        }
        Command::Prop { id, kind, .. } => apply_prop(doc, *id, kind, inverse),
        Command::DocSnapshot { before, after, .. } => {
            let snap = if inverse { &**before } else { &**after };
            restore_snapshot(doc, snap);
        }
    }
}

fn apply_prop(doc: &mut Document, id: u64, kind: &PropKind, inverse: bool) {
    match kind {
        PropKind::Visible(old, new) => {
            let v = if inverse { *old } else { *new };
            if let Some(n) = doc.get_node_mut(id) { n.set_visible(v); }
        }
        PropKind::Opacity(old, new) => {
            let v = if inverse { *old } else { *new };
            if let Some(n) = doc.get_node_mut(id) { n.set_opacity(v); }
        }
        PropKind::Blend(old, new) => {
            let v = if inverse { *old } else { *new };
            if let Some(n) = doc.get_node_mut(id) { n.set_blend(crate::blend::BlendMode::from_i32(v)); }
        }
        PropKind::Name(old, new) => {
            let v = if inverse { old.clone() } else { new.clone() };
            if let Some(n) = doc.get_node_mut(id) { n.set_name(&v); }
        }
        PropKind::MaskAdded(_) => {
            if inverse {
                if let Some(l) = doc.get_layer_mut(id) { l.mask = None; }
            }
        }
        PropKind::MaskRemoved(_, mask) => {
            if inverse {
                if let Some(l) = doc.get_layer_mut(id) { l.mask = Some((**mask).clone()); }
            }
        }
        PropKind::MaskApplied(_) => { /* mask application is undone via Pixels entries */ }
        PropKind::Active(old, new) => {
            let v = if inverse { *old } else { *new };
            doc.active_id = v;
        }
        PropKind::SelectionChanged { before, after } => {
            let s = if inverse { before } else { after };
            match s {
                Some((w, h, data, active)) => {
                    doc.selection = crate::layer::Selection { w: *w, h: *h, data: data.clone(), active: *active };
                }
                None => {
                    doc.selection = crate::layer::Selection::none(doc.w, doc.h);
                }
            }
        }
    }
}

/// Remove child at index from group; returns the removed node.
pub fn remove_at(doc: &mut Document, parent_id: u64, index: usize) -> Option<Node> {
    if parent_id == 0 {
        if index < doc.root.children.len() {
            let n = doc.root.children.remove(index);
            return Some(n);
        }
        return None;
    }
    let path = doc.find_path(parent_id)?;
    let g = doc.node_at_mut(&path)?.as_group_mut()?;
    if index < g.children.len() {
        Some(g.children.remove(index))
    } else {
        None
    }
}

/// Insert node into group at index.
pub fn insert_at(doc: &mut Document, parent_id: u64, index: usize, node: Node) {
    if parent_id == 0 {
        let idx = index.min(doc.root.children.len());
        doc.root.children.insert(idx, node);
        return;
    }
    let path = doc.find_path(parent_id);
    if let Some(p) = path {
        if let Some(n) = doc.node_at_mut(&p) {
            if let Some(g) = n.as_group_mut() {
                let idx = index.min(g.children.len());
                g.children.insert(idx, node);
            }
        }
    }
}

fn restore_snapshot(doc: &mut Document, snap: &DocSnapshot) {
    doc.w = snap.w;
    doc.h = snap.h;
    doc.root = snap.root.clone();
    doc.active_id = snap.active_id;
    doc.next_id = snap.next_id;
    doc.selection = snap.selection.clone();
    // write pixel buffers back (root was cloned before edits for these snapshots)
    for (id, rgba, mask) in &snap.layers_rgba {
        if let Some(l) = doc.get_layer_mut(*id) {
            l.pixels = rgba.clone();
            l.mask = mask.clone();
        }
    }
}

/// Convenience: capture selection state for undo.
pub fn capture_selection(doc: &Document) -> Option<(u32, u32, Vec<u8>, bool)> {
    if doc.selection.active {
        Some((doc.selection.w, doc.selection.h, doc.selection.data.clone(), true))
    } else {
        None
    }
}

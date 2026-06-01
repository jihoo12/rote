// graph.rs — Force-directed graph view of the workspace.
//
// Nodes  = files  (circles, blue)
// Groups = dirs   (rounded squares, amber)
// Edges  = [[link]] references (arrows)
// Parent-child containment edges are shown as thin grey lines.
//
// Interactions:
//   - Drag canvas  : pan
//   - Scroll wheel : zoom
//   - Click node   : select (emits path via on_node_click callback)
//   - Ctrl+G       : toggle graph / editor (handled in main.rs)

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{
    gdk, glib,
    graphene,
    prelude::*,
    DrawingArea,
};

use crate::object::{extract_links, Group, Node, ObjectKind};

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GraphNode {
    pub path:     PathBuf,
    pub label:    String,
    pub is_group: bool,
    pub x:        f64,
    pub y:        f64,
}

#[derive(Clone)]
pub struct GraphEdge {
    pub from: usize, // index into nodes vec
    pub to:   usize,
    pub kind: EdgeKind,
}

#[derive(Clone, PartialEq)]
pub enum EdgeKind {
    Link,      // [[link]] reference  — solid coloured arrow
    Contains,  // parent dir → child  — thin grey line
}

// ── Layout state ──────────────────────────────────────────────────────────────

struct LayoutState {
    nodes:    Vec<GraphNode>,
    edges:    Vec<GraphEdge>,
    // view transform
    pan_x:    f64,
    pan_y:    f64,
    zoom:     f64,
    // interaction
    drag_start: Option<(f64, f64)>,  // canvas coords when pan started
    pan_start:  Option<(f64, f64)>,  // pan offset at drag start
    selected:   Option<usize>,
}

impl LayoutState {
    fn new() -> Self {
        Self {
            nodes:      Vec::new(),
            edges:      Vec::new(),
            pan_x:      0.0,
            pan_y:      0.0,
            zoom:       1.0,
            drag_start: None,
            pan_start:  None,
            selected:   None,
        }
    }

    /// Convert canvas (widget) coords → world coords.
    fn to_world(&self, cx: f64, cy: f64) -> (f64, f64) {
        ((cx - self.pan_x) / self.zoom, (cy - self.pan_y) / self.zoom)
    }

    /// Pick the node index under world point (wx, wy).
    fn hit_test(&self, wx: f64, wy: f64) -> Option<usize> {
        for (i, n) in self.nodes.iter().enumerate().rev() {
            let r = node_radius(n);
            if (wx - n.x).powi(2) + (wy - n.y).powi(2) <= r * r {
                return Some(i);
            }
        }
        None
    }

    /// Run one iteration of force-directed layout.
    fn step(&mut self) {
        let n = self.nodes.len();
        if n == 0 { return; }

        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];

        // Repulsion between every pair
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.nodes[i].x - self.nodes[j].x;
                let dy = self.nodes[i].y - self.nodes[j].y;
                let dist2 = (dx * dx + dy * dy).max(1.0);
                let dist  = dist2.sqrt();
                let force = 8000.0 / dist2;
                let (fdx, fdy) = (force * dx / dist, force * dy / dist);
                fx[i] += fdx;  fy[i] += fdy;
                fx[j] -= fdx;  fy[j] -= fdy;
            }
        }

        // Attraction along edges
        for e in &self.edges {
            let dx = self.nodes[e.to].x - self.nodes[e.from].x;
            let dy = self.nodes[e.to].y - self.nodes[e.from].y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let rest  = match e.kind { EdgeKind::Link => 200.0, EdgeKind::Contains => 150.0 };
            let force = 0.05 * (dist - rest);
            let (fdx, fdy) = (force * dx / dist, force * dy / dist);
            fx[e.from] += fdx;  fy[e.from] += fdy;
            fx[e.to]   -= fdx;  fy[e.to]   -= fdy;
        }

        // Centre gravity
        let (cx, cy) = self.nodes.iter().fold((0.0, 0.0), |(ax, ay), n| (ax + n.x, ay + n.y));
        let (cx, cy) = (cx / n as f64, cy / n as f64);
        for i in 0..n {
            fx[i] += (cx - self.nodes[i].x) * 0.01;
            fy[i] += (cy - self.nodes[i].y) * 0.01;
        }

        // Apply with damping
        let damping = 0.85;
        for i in 0..n {
            self.nodes[i].x += fx[i].clamp(-30.0, 30.0) * damping;
            self.nodes[i].y += fy[i].clamp(-30.0, 30.0) * damping;
        }
    }
}

fn node_radius(n: &GraphNode) -> f64 {
    if n.is_group { 28.0 } else { 20.0 }
}

// ── GraphView public struct ───────────────────────────────────────────────────

pub struct GraphView {
    pub widget:        DrawingArea,
    state:             Rc<RefCell<LayoutState>>,
    on_node_click:     Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>>,
}

impl GraphView {
    pub fn new() -> Self {
        let area = DrawingArea::builder()
            .vexpand(true)
            .hexpand(true)
            .build();

        let state          = Rc::new(RefCell::new(LayoutState::new()));
        let on_node_click: Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>> =
            Rc::new(RefCell::new(None));

        // ── Drawing ──────────────────────────────────────────────────────────
        {
            let state = state.clone();
            area.set_draw_func(move |_, cr, w, h| {
                draw(&state.borrow(), cr, w, h);
            });
        }

        // ── Animation tick (runs layout steps) ───────────────────────────────
        {
            let state  = state.clone();
            let area2  = area.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                state.borrow_mut().step();
                area2.queue_draw();
                glib::ControlFlow::Continue
            });
        }

        // ── Scroll → zoom ────────────────────────────────────────────────────
        {
            let state = state.clone();
            let area2 = area.clone();
            let scroll = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL,
            );
            scroll.connect_scroll(move |_, _dx, dy| {
                let mut s = state.borrow_mut();
                let factor = if dy < 0.0 { 1.1 } else { 0.9 };
                s.zoom = (s.zoom * factor).clamp(0.1, 5.0);
                area2.queue_draw();
                glib::Propagation::Stop
            });
            area.add_controller(scroll);
        }

        // ── Drag → pan / click ───────────────────────────────────────────────
        {
            let state2        = state.clone();
            let on_click      = on_node_click.clone();
            let area2         = area.clone();

            let gesture = gtk::GestureDrag::new();

            gesture.connect_drag_begin({
                let state2 = state2.clone();
                move |_, x, y| {
                    let mut s = state2.borrow_mut();
                    s.drag_start = Some((x, y));
                    s.pan_start  = Some((s.pan_x, s.pan_y));
                }
            });

            gesture.connect_drag_update({
                let state2 = state2.clone();
                let area2  = area2.clone();
                move |_, dx, dy| {
                    let mut s = state2.borrow_mut();
                    if let (Some(_), Some((px, py))) = (s.drag_start, s.pan_start) {
                        s.pan_x = px + dx;
                        s.pan_y = py + dy;
                        area2.queue_draw();
                    }
                }
            });

            gesture.connect_drag_end({
                let state2   = state2.clone();
                let on_click = on_click.clone();
                move |_, dx, dy| {
                    let mut s = state2.borrow_mut();
                    // If the drag was tiny treat it as a click
                    if dx.abs() < 4.0 && dy.abs() < 4.0 {
                        if let Some((sx, sy)) = s.drag_start {
                            let (wx, wy) = s.to_world(sx, sy);
                            if let Some(idx) = s.hit_test(wx, wy) {
                                s.selected = Some(idx);
                                let path = s.nodes[idx].path.clone();
                                drop(s);
                                if let Some(cb) = on_click.borrow().as_ref() {
                                    cb(path);
                                }
                                return;
                            }
                        }
                    }
                    s.drag_start = None;
                    s.pan_start  = None;
                }
            });

            area.add_controller(gesture);
        }

        Self { widget: area, state, on_node_click }
    }

    /// Register a callback invoked when the user clicks a node.
    pub fn on_node_click<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.on_node_click.borrow_mut() = Some(Box::new(f));
    }

    /// Rebuild the graph from `root` directory.
    pub fn load(&self, root: &Path) {
        let mut state  = self.state.borrow_mut();
        state.nodes.clear();
        state.edges.clear();

        // Index: path → node index
        let mut idx_map: HashMap<PathBuf, usize> = HashMap::new();

        // First pass: collect all objects
        collect_objects(root, &mut state.nodes, &mut idx_map, 0);

        // Second pass: build edges
        //   a) containment (dir → child)
        //   b) [[link]] references
        for i in 0..state.nodes.len() {
            let node = state.nodes[i].clone();

            // containment: parent dir → this node
            if let Some(parent) = node.path.parent() {
                if let Some(&pi) = idx_map.get(parent) {
                    state.edges.push(GraphEdge { from: pi, to: i, kind: EdgeKind::Contains });
                }
            }

            // [[links]] in content
            let content = if node.is_group {
                // read .group meta file
                let meta = node.path.join(".group");
                std::fs::read_to_string(meta).unwrap_or_default()
            } else {
                std::fs::read_to_string(&node.path).unwrap_or_default()
            };
            for link in extract_links(&content) {
                // Try to resolve the link name to a known path
                if let Some(target_idx) = idx_map.iter().find_map(|(p, &ti)| {
                    let name = p.file_name()?.to_string_lossy();
                    // match by exact name or name without extension
                    if name == link.as_str()
                        || p.file_stem().map(|s| s.to_string_lossy().to_string()).as_deref() == Some(&link)
                    {
                        Some(ti)
                    } else {
                        None
                    }
                }) {
                    state.edges.push(GraphEdge { from: i, to: target_idx, kind: EdgeKind::Link });
                }
            }
        }

        // Initial scatter so nodes don't all start at origin
        let n = state.nodes.len();
        for (i, node) in state.nodes.iter_mut().enumerate() {
            let angle = (i as f64 / n as f64) * std::f64::consts::TAU;
            let r     = 150.0 + (i % 3) as f64 * 60.0;
            node.x = angle.cos() * r;
            node.y = angle.sin() * r;
        }

        state.pan_x = 0.0;
        state.pan_y = 0.0;
        state.zoom  = 1.0;
    }

    /// Highlight the node corresponding to `path`.
    pub fn select_path(&self, path: &Path) {
        let mut s = self.state.borrow_mut();
        s.selected = s.nodes.iter().position(|n| n.path == path);
    }
}

// ── Recursive object collector ────────────────────────────────────────────────

fn collect_objects(
    dir: &Path,
    nodes: &mut Vec<GraphNode>,
    idx_map: &mut HashMap<PathBuf, usize>,
    depth: usize,
) {
    // Add the directory itself as a group node (skip if it's the root at depth 0
    // so the root doesn't appear as an extra isolated node — keep it so the
    // containment edges work; just give it a lighter style if depth == 0).
    let group_idx = nodes.len();
    idx_map.insert(dir.to_path_buf(), group_idx);
    nodes.push(GraphNode {
        path:     dir.to_path_buf(),
        label:    dir.file_name()
                     .map(|n| n.to_string_lossy().to_string())
                     .unwrap_or_else(|| dir.to_string_lossy().to_string()),
        is_group: true,
        x: 0.0, y: 0.0,
    });

    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let p = entry.path();
        // Skip hidden files / meta files
        if p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        if p.is_dir() {
            collect_objects(&p, nodes, idx_map, depth + 1);
        } else {
            let file_idx = nodes.len();
            idx_map.insert(p.clone(), file_idx);
            nodes.push(GraphNode {
                path:     p.clone(),
                label:    p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                is_group: false,
                x: 0.0, y: 0.0,
            });
        }
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(s: &LayoutState, cr: &gtk::cairo::Context, w: i32, h: i32) {
    // Background
    cr.set_source_rgb(0.12, 0.12, 0.14);
    cr.paint().ok();

    cr.save().ok();
    // Centre the origin, then apply pan + zoom
    cr.translate(w as f64 / 2.0 + s.pan_x, h as f64 / 2.0 + s.pan_y);
    cr.scale(s.zoom, s.zoom);

    // ── Edges ────────────────────────────────────────────────────────────────
    for e in &s.edges {
        let a = &s.nodes[e.from];
        let b = &s.nodes[e.to];
        match e.kind {
            EdgeKind::Contains => {
                cr.set_source_rgba(0.5, 0.5, 0.55, 0.35);
                cr.set_line_width(1.0);
                cr.set_dash(&[4.0, 4.0], 0.0);
            }
            EdgeKind::Link => {
                cr.set_source_rgba(0.4, 0.75, 1.0, 0.8);
                cr.set_line_width(1.8);
                cr.set_dash(&[], 0.0);
            }
        }
        cr.move_to(a.x, a.y);
        cr.line_to(b.x, b.y);
        cr.stroke().ok();

        // Arrow head for link edges
        if e.kind == EdgeKind::Link {
            draw_arrow(cr, a.x, a.y, b.x, b.y, node_radius(b));
        }
    }
    cr.set_dash(&[], 0.0);

    // ── Nodes ────────────────────────────────────────────────────────────────
    for (i, n) in s.nodes.iter().enumerate() {
        let r        = node_radius(n);
        let selected = s.selected == Some(i);

        // Shadow
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
        if n.is_group {
            rounded_rect(cr, n.x - r, n.y - r, r * 2.0, r * 2.0, 8.0);
        } else {
            cr.arc(n.x + 2.0, n.y + 2.0, r, 0.0, std::f64::consts::TAU);
        }
        cr.fill().ok();

        // Fill
        if n.is_group {
            // Amber for groups
            if selected {
                cr.set_source_rgb(1.0, 0.85, 0.3);
            } else {
                cr.set_source_rgb(0.85, 0.6, 0.1);
            }
            rounded_rect(cr, n.x - r, n.y - r, r * 2.0, r * 2.0, 8.0);
        } else {
            // Blue for nodes
            if selected {
                cr.set_source_rgb(0.4, 0.8, 1.0);
            } else {
                cr.set_source_rgb(0.2, 0.55, 0.9);
            }
            cr.arc(n.x, n.y, r, 0.0, std::f64::consts::TAU);
        }
        cr.fill_preserve().ok();

        // Stroke ring
        cr.set_source_rgba(1.0, 1.0, 1.0, if selected { 0.9 } else { 0.25 });
        cr.set_line_width(if selected { 2.5 } else { 1.0 });
        cr.stroke().ok();

        // Label
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_font_size(11.0 / s.zoom.max(0.5));
        let label = truncate(&n.label, 18);
        let ext   = cr.text_extents(&label).unwrap();
        cr.move_to(n.x - ext.width() / 2.0, n.y + r + 14.0 / s.zoom.max(0.5));
        cr.show_text(&label).ok();
    }

    cr.restore().ok();

    // HUD: zoom level
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
    cr.set_font_size(11.0);
    cr.move_to(8.0, h as f64 - 8.0);
    cr.show_text(&format!("{:.0}%", s.zoom * 100.0)).ok();
}

fn draw_arrow(cr: &gtk::cairo::Context, x1: f64, y1: f64, x2: f64, y2: f64, target_r: f64) {
    let dx  = x2 - x1;
    let dy  = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let ux  = dx / len;
    let uy  = dy / len;

    // Tip just outside the target circle
    let tip_x = x2 - ux * (target_r + 4.0);
    let tip_y = y2 - uy * (target_r + 4.0);

    let head  = 9.0;
    let angle = 0.45_f64;
    let lx = tip_x - head * (ux * angle.cos() - uy * angle.sin());
    let ly = tip_y - head * (ux * angle.sin() + uy * angle.cos());
    let rx = tip_x - head * (ux * angle.cos() + uy * angle.sin());
    let ry = tip_y - head * ((-ux) * angle.sin() + uy * angle.cos());

    cr.set_source_rgba(0.4, 0.75, 1.0, 0.9);
    cr.set_line_width(0.0);
    cr.move_to(tip_x, tip_y);
    cr.line_to(lx, ly);
    cr.line_to(rx, ry);
    cr.close_path();
    cr.fill().ok();
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.move_to(x + r, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.line_to(x + r, y + h);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, std::f64::consts::TAU * 1.5 / 2.0);
    cr.close_path();
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(s.len())])
    }
}
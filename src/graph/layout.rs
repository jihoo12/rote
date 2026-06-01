// graph/layout.rs — Force-directed simulation state and stepping.
//
// Responsible for:
//   - Holding node positions and edge topology
//   - Running repulsion / attraction / gravity each tick
//   - View-transform helpers (pan, zoom, hit-testing)

use std::path::PathBuf;

// ── Public data types ─────────────────────────────────────────────────────────

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
    pub from: usize,
    pub to:   usize,
    pub kind: EdgeKind,
}

#[derive(Clone, PartialEq)]
pub enum EdgeKind {
    /// `[[link]]` reference — solid coloured arrow.
    Link,
    /// Parent directory → child — thin grey line.
    Contains,
}

// ── Layout state ──────────────────────────────────────────────────────────────

pub struct LayoutState {
    pub nodes:    Vec<GraphNode>,
    pub edges:    Vec<GraphEdge>,

    // View transform
    pub pan_x:    f64,
    pub pan_y:    f64,
    pub zoom:     f64,

    // Interaction
    pub drag_start: Option<(f64, f64)>,  // canvas coords when pan started
    pub pan_start:  Option<(f64, f64)>,  // pan offset at drag start
    pub selected:   Option<usize>,
}

impl LayoutState {
    pub fn new() -> Self {
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
    pub fn to_world(&self, cx: f64, cy: f64) -> (f64, f64) {
        ((cx - self.pan_x) / self.zoom, (cy - self.pan_y) / self.zoom)
    }

    /// Return the index of the node under world point `(wx, wy)`, if any.
    pub fn hit_test(&self, wx: f64, wy: f64) -> Option<usize> {
        for (i, n) in self.nodes.iter().enumerate().rev() {
            let r = node_radius(n);
            if (wx - n.x).powi(2) + (wy - n.y).powi(2) <= r * r {
                return Some(i);
            }
        }
        None
    }

    /// Run one iteration of force-directed layout.
    pub fn step(&mut self) {
        let n = self.nodes.len();
        if n == 0 { return; }

        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];

        // Repulsion between every pair of nodes
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.nodes[i].x - self.nodes[j].x;
                let dy = self.nodes[i].y - self.nodes[j].y;
                let dist2 = (dx * dx + dy * dy).max(1.0);
                let dist  = dist2.sqrt();
                let force = REPULSION / dist2;
                let (fdx, fdy) = (force * dx / dist, force * dy / dist);
                fx[i] += fdx;  fy[i] += fdy;
                fx[j] -= fdx;  fy[j] -= fdy;
            }
        }

        // Spring attraction along edges
        for e in &self.edges {
            let dx = self.nodes[e.to].x - self.nodes[e.from].x;
            let dy = self.nodes[e.to].y - self.nodes[e.from].y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let rest  = match e.kind {
                EdgeKind::Link     => REST_LINK,
                EdgeKind::Contains => REST_CONTAINS,
            };
            let force = SPRING_K * (dist - rest);
            let (fdx, fdy) = (force * dx / dist, force * dy / dist);
            fx[e.from] += fdx;  fy[e.from] += fdy;
            fx[e.to]   -= fdx;  fy[e.to]   -= fdy;
        }

        // Weak gravity towards centroid (keeps graph from drifting)
        let (cx, cy) = self.nodes.iter()
            .fold((0.0, 0.0), |(ax, ay), node| (ax + node.x, ay + node.y));
        let (cx, cy) = (cx / n as f64, cy / n as f64);
        for i in 0..n {
            fx[i] += (cx - self.nodes[i].x) * GRAVITY;
            fy[i] += (cy - self.nodes[i].y) * GRAVITY;
        }

        // Apply forces with velocity damping
        for i in 0..n {
            self.nodes[i].x += fx[i].clamp(-MAX_STEP, MAX_STEP) * DAMPING;
            self.nodes[i].y += fy[i].clamp(-MAX_STEP, MAX_STEP) * DAMPING;
        }
    }
}

// ── Physics constants ─────────────────────────────────────────────────────────

const REPULSION:    f64 = 8_000.0;
const SPRING_K:     f64 = 0.05;
const REST_LINK:    f64 = 200.0;
const REST_CONTAINS:f64 = 150.0;
const GRAVITY:      f64 = 0.01;
const DAMPING:      f64 = 0.85;
const MAX_STEP:     f64 = 30.0;

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Visual radius for a graph node.
pub fn node_radius(n: &GraphNode) -> f64 {
    if n.is_group { 28.0 } else { 20.0 }
}
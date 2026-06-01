// graph/mod.rs — Public API for the graph view widget.
//
// GraphView owns the GTK DrawingArea and glues together:
//   layout  — force simulation state
//   collect — workspace traversal and edge building
//   draw    — Cairo rendering
//
// Consumers only need to call:
//   GraphView::new()
//   .load(root)
//   .select_path(path)
//   .on_node_click(callback)

mod collect;
mod draw;
pub mod layout;

use std::{cell::RefCell, path::{Path, PathBuf}, rc::Rc};

use gtk::{glib, prelude::*, DrawingArea};

use layout::LayoutState;



// ── GraphView ─────────────────────────────────────────────────────────────────

pub struct GraphView {
    pub widget:    DrawingArea,
    state:         Rc<RefCell<LayoutState>>,
    on_node_click: Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>>,
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

        wire_draw(&area, &state);
        wire_animation(&area, &state);
        wire_zoom(&area, &state);
        wire_drag(&area, &state, &on_node_click);

        Self { widget: area, state, on_node_click }
    }

    /// Register a callback invoked when the user clicks a node.
    pub fn on_node_click<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.on_node_click.borrow_mut() = Some(Box::new(f));
    }

    /// Rebuild the graph from the `root` workspace directory.
    pub fn load(&self, root: &Path) {
        collect::load_workspace(&mut self.state.borrow_mut(), root);
    }

    /// Highlight the node whose path matches `path`.
    pub fn select_path(&self, path: &Path) {
        let mut s = self.state.borrow_mut();
        s.selected = s.nodes.iter().position(|n| n.path == path);
    }
}

// ── GTK wiring helpers ────────────────────────────────────────────────────────

/// Connect the draw function.
fn wire_draw(area: &DrawingArea, state: &Rc<RefCell<LayoutState>>) {
    let state = state.clone();
    area.set_draw_func(move |_, cr, w, h| {
        draw::draw_frame(&state.borrow(), cr, w, h);
    });
}

/// 60 fps animation tick that advances the simulation.
fn wire_animation(area: &DrawingArea, state: &Rc<RefCell<LayoutState>>) {
    let state = state.clone();
    let area  = area.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        state.borrow_mut().step();
        area.queue_draw();
        glib::ControlFlow::Continue
    });
}

/// Scroll wheel → zoom.
fn wire_zoom(area: &DrawingArea, state: &Rc<RefCell<LayoutState>>) {
    let state = state.clone();
    let area2 = area.clone();
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL,
    );
    scroll.connect_scroll(move |_, _dx, dy| {
        let mut s  = state.borrow_mut();
        let factor = if dy < 0.0 { 1.1 } else { 0.9 };
        s.zoom     = (s.zoom * factor).clamp(0.1, 5.0);
        area2.queue_draw();
        glib::Propagation::Stop
    });
    area.add_controller(scroll);
}

/// Drag gesture: pan the canvas or click a node.
fn wire_drag(
    area: &DrawingArea,
    state: &Rc<RefCell<LayoutState>>,
    on_node_click: &Rc<RefCell<Option<Box<dyn Fn(PathBuf)>>>>,
) {
    let state2   = state.clone();
    let on_click = on_node_click.clone();
    let area2    = area.clone();
    let gesture  = gtk::GestureDrag::new();

    gesture.connect_drag_begin({
        let state2 = state2.clone();
        move |_, x, y| {
            let mut s    = state2.borrow_mut();
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
            // If the drag was tiny, treat it as a click.
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
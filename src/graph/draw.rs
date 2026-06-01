// graph/draw.rs — Cairo rendering for the graph view.
//
// Receives an immutable &LayoutState snapshot and produces the frame.
// No filesystem access, no layout mutation — pure drawing.

use super::layout::{EdgeKind, GraphNode, LayoutState, node_radius};

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw_frame(s: &LayoutState, cr: &gtk::cairo::Context, w: i32, h: i32) {
    draw_background(cr);

    cr.save().ok();
    // Centre origin, then apply pan + zoom.
    cr.translate(w as f64 / 2.0 + s.pan_x, h as f64 / 2.0 + s.pan_y);
    cr.scale(s.zoom, s.zoom);

    draw_edges(cr, s);
    draw_nodes(cr, s);

    cr.restore().ok();

    draw_hud(cr, s, h);
}

// ── Background ────────────────────────────────────────────────────────────────

fn draw_background(cr: &gtk::cairo::Context) {
    cr.set_source_rgb(0.12, 0.12, 0.14);
    cr.paint().ok();
}

// ── Edges ─────────────────────────────────────────────────────────────────────

fn draw_edges(cr: &gtk::cairo::Context, s: &LayoutState) {
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

        if e.kind == EdgeKind::Link {
            draw_arrowhead(cr, a.x, a.y, b.x, b.y, node_radius(b));
        }
    }

    // Reset dash after edge pass.
    cr.set_dash(&[], 0.0);
}

// ── Nodes ─────────────────────────────────────────────────────────────────────

fn draw_nodes(cr: &gtk::cairo::Context, s: &LayoutState) {
    for (i, n) in s.nodes.iter().enumerate() {
        let r        = node_radius(n);
        let selected = s.selected == Some(i);

        draw_node_shadow(cr, n, r);
        draw_node_fill(cr, n, r, selected);
        draw_node_stroke(cr, n, r, selected);
        draw_node_label(cr, n, r, s.zoom);
    }
}

fn draw_node_shadow(cr: &gtk::cairo::Context, n: &GraphNode, r: f64) {
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
    if n.is_group {
        rounded_rect(cr, n.x - r + 2.0, n.y - r + 2.0, r * 2.0, r * 2.0, 8.0);
    } else {
        cr.arc(n.x + 2.0, n.y + 2.0, r, 0.0, std::f64::consts::TAU);
    }
    cr.fill().ok();
}

fn draw_node_fill(cr: &gtk::cairo::Context, n: &GraphNode, r: f64, selected: bool) {
    if n.is_group {
        if selected {
            cr.set_source_rgb(1.0, 0.85, 0.3);   // bright amber
        } else {
            cr.set_source_rgb(0.85, 0.6, 0.1);   // amber
        }
        rounded_rect(cr, n.x - r, n.y - r, r * 2.0, r * 2.0, 8.0);
    } else {
        if selected {
            cr.set_source_rgb(0.4, 0.8, 1.0);    // bright blue
        } else {
            cr.set_source_rgb(0.2, 0.55, 0.9);   // blue
        }
        cr.arc(n.x, n.y, r, 0.0, std::f64::consts::TAU);
    }
    cr.fill_preserve().ok();
}

fn draw_node_stroke(cr: &gtk::cairo::Context, _n: &GraphNode, _r: f64, selected: bool) {
    cr.set_source_rgba(1.0, 1.0, 1.0, if selected { 0.9 } else { 0.25 });
    cr.set_line_width(if selected { 2.5 } else { 1.0 });
    cr.stroke().ok();
}

fn draw_node_label(cr: &gtk::cairo::Context, n: &GraphNode, r: f64, zoom: f64) {
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let font_size = 11.0 / zoom.max(0.5);
    cr.set_font_size(font_size);
    let label = truncate(&n.label, 18);
    if let Ok(ext) = cr.text_extents(&label) {
        cr.move_to(n.x - ext.width() / 2.0, n.y + r + 14.0 / zoom.max(0.5));
        cr.show_text(&label).ok();
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(cr: &gtk::cairo::Context, s: &LayoutState, h: i32) {
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
    cr.set_font_size(11.0);
    cr.move_to(8.0, h as f64 - 8.0);
    cr.show_text(&format!("{:.0}%", s.zoom * 100.0)).ok();
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Draw an arrowhead at the `(x2, y2)` end of the segment, offset by `target_r`.
fn draw_arrowhead(
    cr: &gtk::cairo::Context,
    x1: f64, y1: f64,
    x2: f64, y2: f64,
    target_r: f64,
) {
    let dx  = x2 - x1;
    let dy  = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (ux, uy) = (dx / len, dy / len);

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

/// Cairo path for a rounded rectangle.
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

/// Truncate a string to `max` characters, appending `…` if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(s.len());
        format!("{}…", &s[..cut])
    }
}
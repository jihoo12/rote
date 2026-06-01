// graph/collect.rs — Walks the workspace and builds graph nodes + edges.
//
// Separated from layout so the force simulation has no filesystem concerns,
// and from draw.rs so the Cairo dependency stays isolated.

use std::{collections::HashMap, path::Path};

use crate::object::extract_links;

use super::layout::{EdgeKind, GraphEdge, GraphNode, LayoutState};

/// Populate `state` with nodes and edges derived from `root`.
/// Clears any previous content first.
pub fn load_workspace(state: &mut LayoutState, root: &Path) {
    state.nodes.clear();
    state.edges.clear();

    // Build the node list and a path→index map in a single DFS pass.
    let mut idx_map: HashMap<std::path::PathBuf, usize> = HashMap::new();
    collect_objects(root, &mut state.nodes, &mut idx_map, 0);

    // Second pass: edges.
    build_edges(state, &idx_map);

    // Scatter nodes so they don't all start at the origin.
    scatter_initial_positions(&mut state.nodes);

    // Reset view.
    state.pan_x = 0.0;
    state.pan_y = 0.0;
    state.zoom  = 1.0;
}

// ── Object collection (DFS) ───────────────────────────────────────────────────

fn collect_objects(
    dir: &Path,
    nodes: &mut Vec<GraphNode>,
    idx_map: &mut HashMap<std::path::PathBuf, usize>,
    depth: usize,
) {
    // Add the directory itself as a group node.
    let group_idx = nodes.len();
    idx_map.insert(dir.to_path_buf(), group_idx);
    nodes.push(GraphNode {
        path:     dir.to_path_buf(),
        label:    dir.file_name()
                     .map(|n| n.to_string_lossy().to_string())
                     .unwrap_or_else(|| dir.to_string_lossy().to_string()),
        is_group: true,
        x: 0.0,
        y: 0.0,
    });

    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let p = entry.path();

        // Skip hidden/meta files.
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
                x: 0.0,
                y: 0.0,
            });
        }
    }
}

// ── Edge building ─────────────────────────────────────────────────────────────

fn build_edges(
    state: &mut LayoutState,
    idx_map: &HashMap<std::path::PathBuf, usize>,
) {
    for i in 0..state.nodes.len() {
        let node = state.nodes[i].clone();

        // Containment edge: parent directory → this node.
        if let Some(parent) = node.path.parent() {
            if let Some(&pi) = idx_map.get(parent) {
                state.edges.push(GraphEdge { from: pi, to: i, kind: EdgeKind::Contains });
            }
        }

        // Link edges: [[wikilink]] references in file/group content.
        let content = read_content(&node);
        for link in extract_links(&content) {
            if let Some(target_idx) = resolve_link(&link, idx_map) {
                state.edges.push(GraphEdge { from: i, to: target_idx, kind: EdgeKind::Link });
            }
        }
    }
}

/// Read the content relevant for link extraction.
/// Groups use their `.group` meta file; nodes use the file directly.
fn read_content(node: &GraphNode) -> String {
    if node.is_group {
        std::fs::read_to_string(node.path.join(".group")).unwrap_or_default()
    } else {
        std::fs::read_to_string(&node.path).unwrap_or_default()
    }
}

/// Try to find a node index matching a wikilink name (with or without extension).
fn resolve_link(
    link: &str,
    idx_map: &HashMap<std::path::PathBuf, usize>,
) -> Option<usize> {
    idx_map.iter().find_map(|(p, &ti)| {
        let name = p.file_name()?.to_string_lossy();
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string());
        if name == link || stem.as_deref() == Some(link) {
            Some(ti)
        } else {
            None
        }
    })
}

// ── Initial placement ─────────────────────────────────────────────────────────

/// Spread nodes evenly on a rough spiral so the first layout step isn't degenerate.
fn scatter_initial_positions(nodes: &mut Vec<GraphNode>) {
    let n = nodes.len();
    for (i, node) in nodes.iter_mut().enumerate() {
        let angle = (i as f64 / n as f64) * std::f64::consts::TAU;
        let r     = 150.0 + (i % 3) as f64 * 60.0;
        node.x = angle.cos() * r;
        node.y = angle.sin() * r;
    }
}
// app/mod.rs — Application state and top-level entry point.

pub mod actions;
pub mod ui;

use std::path::PathBuf;

use crate::object::{Group, Node, ObjectKind};

// ── AppState ──────────────────────────────────────────────────────────────────

pub struct AppState {
    pub workspace:    Option<PathBuf>,
    pub current_path: Option<PathBuf>,
    pub is_modified:  bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            workspace:    None,
            current_path: None,
            is_modified:  false,
        }
    }

    /// Construct the current object from `current_path`, if set.
    pub fn current_object(&self) -> Option<ObjectKind> {
        let path = self.current_path.as_ref()?;
        if path.is_dir() {
            Some(ObjectKind::Group(Group::new(path.clone())))
        } else {
            Some(ObjectKind::Node(Node::new(path.clone())))
        }
    }
}
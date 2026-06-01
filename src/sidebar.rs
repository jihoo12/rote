use std::path::{Path, PathBuf};

use gtk::{
    prelude::*,
    Box, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow,
    SelectionMode,
};

use crate::object::{Group, ObjectKind};

// ── SidebarTree ───────────────────────────────────────────────────────────────
//
// Shows the workspace directory as a flat list of indented rows.
// Clicking a row emits the path so the editor can open it.

pub struct SidebarTree {
    pub widget: Box,           // top-level widget to embed
    pub list:   ListBox,
    pub paths:  std::cell::RefCell<Vec<PathBuf>>,
}

impl SidebarTree {
    pub fn new() -> Self {
        let list = ListBox::builder()
            .selection_mode(SelectionMode::Single)
            .build();
        list.add_css_class("navigation-sidebar");

        let scrolled = ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hexpand(false)
            .min_content_width(200)
            .build();

        let widget = Box::builder()
            .orientation(Orientation::Vertical)
            .build();
        widget.append(&scrolled);

        Self {
            widget,
            list,
            paths: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Populate the sidebar from `root`. Clears previous entries.
    pub fn load(&self, root: &Path) {
        // Remove all existing rows
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut paths = self.paths.borrow_mut();
        paths.clear();

        let group = Group::new(root.to_path_buf());
        self.add_group_recursive(&group, 0, &mut paths);
    }

    fn add_group_recursive(
        &self,
        group: &Group,
        depth: u32,
        paths: &mut Vec<PathBuf>,
    ) {
        // Add the group row itself (skip the root at depth 0 — shown in title)
        if depth > 0 {
            self.push_row(&group.path, &group.name, depth - 1, true, paths);
        }

        for child in group.children() {
            match child {
                ObjectKind::Node(n) => {
                    self.push_row(&n.path, &n.name, depth, false, paths);
                }
                ObjectKind::Group(g) => {
                    self.add_group_recursive(&g, depth + 1, paths);
                }
            }
        }
    }

    fn push_row(
        &self,
        path: &Path,
        name: &str,
        indent: u32,
        is_group: bool,
        paths: &mut Vec<PathBuf>,
    ) {
        let icon = if is_group { "📁 " } else { "📄 " };
        let label_text = format!("{icon}{name}");

        let label = Label::builder()
            .label(&label_text)
            .halign(gtk::Align::Start)
            .margin_start((indent * 16 + 8) as i32)
            .margin_top(4)
            .margin_bottom(4)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let row = ListBoxRow::new();
        row.set_child(Some(&label));
        self.list.append(&row);
        paths.push(path.to_path_buf());
    }

    /// Returns the path for the row at `index`.
    pub fn path_at(&self, index: usize) -> Option<PathBuf> {
        self.paths.borrow().get(index).cloned()
    }
}
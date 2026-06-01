// app/ui.rs — GTK widget construction for the NoteGraph window.
//
// build_ui creates every widget, wires them together via actions.rs helpers,
// and presents the window.  It contains no business logic beyond layout.

use std::{cell::RefCell, rc::Rc};

use gtk::{
    prelude::*,
    Application, ApplicationWindow, Box, Button, HeaderBar, Label,
    Orientation, Paned, ScrolledWindow, Stack, TextView, ToggleButton,
};

use crate::{
    graph::GraphView,
    sidebar::SidebarTree,
};

use super::{AppState, actions};

// ── Shared widget handles ─────────────────────────────────────────────────────
//
// Passed to action wiring functions so they can close over exactly the widgets
// they need without dragging the entire build_ui scope into every closure.

pub struct Widgets {
    pub window:        ApplicationWindow,
    pub buffer:        gtk::TextBuffer,
    pub links_label:   Label,
    pub status_label:  Label,
    pub stack:         Stack,
    pub btn_graph:     ToggleButton,
    pub btn_open_ws:   Button,
    pub btn_new_note:  Button,
    pub btn_new_group: Button,
    pub btn_save:      Button,
    pub btn_undo:      Button,
    pub btn_redo:      Button,
    pub sidebar:       Rc<SidebarTree>,
    pub graph_view:    Rc<GraphView>,
    pub state:         Rc<RefCell<AppState>>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState::new()));

    // ── Editor ────────────────────────────────────────────────────────────────
    let text_view = TextView::builder()
        .vexpand(true).hexpand(true)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12).bottom_margin(12)
        .left_margin(16).right_margin(16)
        .build();
    let buffer = text_view.buffer();
    buffer.set_enable_undo(true);

    let editor_scroll = ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true).hexpand(true)
        .build();

    // ── Labels ────────────────────────────────────────────────────────────────
    let links_label = Label::builder()
        .label("Links: ---")
        .halign(gtk::Align::Start)
        .margin_start(16).margin_end(16).margin_top(4).margin_bottom(4)
        .wrap(true)
        .build();
    links_label.add_css_class("dim-label");

    let status_label = Label::builder()
        .label("No file open")
        .halign(gtk::Align::Start)
        .margin_start(16).margin_end(16).margin_top(2).margin_bottom(4)
        .build();
    status_label.add_css_class("dim-label");

    // ── Editor panel ──────────────────────────────────────────────────────────
    let editor_panel = Box::builder().orientation(Orientation::Vertical).build();
    editor_panel.append(&editor_scroll);
    editor_panel.append(&links_label);
    editor_panel.append(&status_label);

    // ── Graph view ────────────────────────────────────────────────────────────
    let graph_view = Rc::new(GraphView::new());

    // ── Stack ─────────────────────────────────────────────────────────────────
    let stack = Stack::builder().vexpand(true).hexpand(true).build();
    stack.add_named(&editor_panel,      Some("editor"));
    stack.add_named(&graph_view.widget, Some("graph"));
    stack.set_visible_child_name("editor");

    // ── Sidebar ───────────────────────────────────────────────────────────────
    let sidebar = Rc::new(SidebarTree::new());

    // ── Paned layout ──────────────────────────────────────────────────────────
    let paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .start_child(&sidebar.widget)
        .end_child(&stack)
        .position(220)
        .build();

    // ── Header buttons ────────────────────────────────────────────────────────
    let btn_open_ws = Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Open workspace folder (Ctrl+O)").build();
    let btn_new_note = Button::builder()
        .icon_name("document-new-symbolic")
        .tooltip_text("New note in current group (Ctrl+N)").build();
    let btn_new_group = Button::builder()
        .icon_name("folder-new-symbolic")
        .tooltip_text("New group (sub-directory)").build();
    let btn_save = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save (Ctrl+S)").build();
    let btn_undo = Button::builder()
        .icon_name("edit-undo-symbolic")
        .tooltip_text("Undo (Ctrl+Z)").build();
    let btn_redo = Button::builder()
        .icon_name("edit-redo-symbolic")
        .tooltip_text("Redo (Ctrl+Y)").build();
    let btn_graph = ToggleButton::builder()
        .icon_name("view-grid-symbolic")
        .tooltip_text("Toggle graph view (Ctrl+G)").build();

    let header = HeaderBar::new();
    header.pack_start(&btn_open_ws);
    header.pack_start(&btn_new_note);
    header.pack_start(&btn_new_group);
    header.pack_start(&btn_save);
    header.pack_end(&btn_redo);
    header.pack_end(&btn_undo);
    header.pack_end(&btn_graph);

    // ── Window ────────────────────────────────────────────────────────────────
    let window = ApplicationWindow::builder()
        .application(app)
        .title("NoteGraph")
        .titlebar(&header)
        .default_width(1100)
        .default_height(680)
        .child(&paned)
        .build();

    // ── Wire all actions ──────────────────────────────────────────────────────
    let w = Widgets {
        window: window.clone(),
        buffer,
        links_label,
        status_label,
        stack,
        btn_graph,
        btn_open_ws,
        btn_new_note,
        btn_new_group,
        btn_save,
        btn_undo,
        btn_redo,
        sidebar,
        graph_view,
        state,
    };

    actions::wire_all(w);

    window.present();
}
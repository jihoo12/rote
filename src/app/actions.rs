// app/actions.rs — Event handlers and button wiring for NoteGraph.
//
// All closures that respond to user gestures live here.
// The module receives a `Widgets` bundle and attaches every handler,
// keeping build_ui() free of business logic.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{gio, glib, prelude::*};

use crate::{
    graph::GraphView,
    object::{self, Group, Node, ObjectKind},
    sidebar::SidebarTree,
};

use super::{AppState, ui::Widgets};

// ── Public entry point ────────────────────────────────────────────────────────

/// Attach every action handler to the provided widgets.
pub fn wire_all(w: Widgets) {
    let Widgets {
        window, buffer, links_label, status_label,
        stack, btn_graph, btn_open_ws, btn_new_note, btn_new_group,
        btn_save, btn_undo, btn_redo, sidebar, graph_view, state,
    } = w;

    // Shared helper: update the window title from current state.
    let update_title = make_update_title(window.clone(), state.clone());

    // Shared helper: load an object into the editor.
    let load_object = make_load_object(
        buffer.clone(), state.clone(),
        links_label.clone(), status_label.clone(),
        update_title.clone(), graph_view.clone(),
        stack.clone(), btn_graph.clone(),
    );

    wire_graph_toggle(&btn_graph, &stack, &state, &graph_view);
    wire_graph_node_click(&graph_view, load_object.clone());
    wire_sidebar_click(&sidebar, load_object.clone());
    wire_open_workspace(&btn_open_ws, &window, &state, &sidebar, &graph_view, update_title.clone());
    wire_save(&btn_save, &buffer, &state, update_title.clone());
    wire_new_note(&btn_new_note, &window, &state, &sidebar, load_object.clone());
    wire_new_group(&btn_new_group, &window, &state, &sidebar, load_object.clone());
    wire_undo_redo(&btn_undo, &btn_redo, &buffer);
    wire_buffer_changed(&buffer, &state, &links_label, update_title.clone());
    wire_keyboard_shortcuts(
        &window, &btn_open_ws, &btn_save, &btn_new_note,
        &btn_undo, &btn_redo, &btn_graph,
    );
}

// ── Title helper ──────────────────────────────────────────────────────────────

type TitleFn = Rc<dyn Fn()>;

fn make_update_title(window: gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) -> TitleFn {
    Rc::new(move || {
        let s = state.borrow();
        let ws = s.workspace.as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "NoteGraph".into());
        let obj = s.current_path.as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());
        let modified = if s.is_modified { "* " } else { "" };
        let title = match obj {
            Some(name) => format!("{modified}{name} -- {ws}"),
            None       => ws,
        };
        window.set_title(Some(&title));
    })
}

// ── Load-object helper ────────────────────────────────────────────────────────

type LoadFn = Rc<dyn Fn(PathBuf)>;

#[allow(clippy::too_many_arguments)]
fn make_load_object(
    buffer:       gtk::TextBuffer,
    state:        Rc<RefCell<AppState>>,
    links_label:  gtk::Label,
    status_label: gtk::Label,
    update_title: TitleFn,
    graph_view:   Rc<GraphView>,
    stack:        gtk::Stack,
    btn_graph:    gtk::ToggleButton,
) -> LoadFn {
    Rc::new(move |path: PathBuf| {
        let obj = if path.is_dir() {
            ObjectKind::Group(Group::new(path.clone()))
        } else {
            ObjectKind::Node(Node::new(path.clone()))
        };

        buffer.set_text(&obj.content());
        buffer.set_enable_undo(true);

        let lnks = obj.links();
        links_label.set_label(&if lnks.is_empty() {
            "Links: ---".into()
        } else {
            format!("Links: {}", lnks.join("  *  "))
        });

        let kind = if obj.is_group() { "group" } else { "node" };
        status_label.set_label(&format!("{kind}  *  {}", path.display()));

        graph_view.select_path(&path);

        // Switch back to editor when a node is opened while graph is active.
        if !obj.is_group() && btn_graph.is_active() {
            btn_graph.set_active(false);
            stack.set_visible_child_name("editor");
        }

        let mut s = state.borrow_mut();
        s.current_path = Some(path);
        s.is_modified  = false;
        drop(s);
        update_title();
    })
}

// ── Individual handlers ───────────────────────────────────────────────────────

fn wire_graph_toggle(
    btn:        &gtk::ToggleButton,
    stack:      &gtk::Stack,
    state:      &Rc<RefCell<AppState>>,
    graph_view: &Rc<GraphView>,
) {
    let stack      = stack.clone();
    let state      = state.clone();
    let graph_view = graph_view.clone();
    btn.connect_toggled(move |btn| {
        if btn.is_active() {
            if let Some(ws) = state.borrow().workspace.clone() {
                graph_view.load(&ws);
            }
            stack.set_visible_child_name("graph");
        } else {
            stack.set_visible_child_name("editor");
        }
    });
}

fn wire_graph_node_click(graph_view: &Rc<GraphView>, load_object: LoadFn) {
    graph_view.on_node_click(move |path| load_object(path));
}

fn wire_sidebar_click(sidebar: &Rc<SidebarTree>, load_object: LoadFn) {
    let sidebar = sidebar.clone();
    let list    = sidebar.list.clone();
    list.connect_row_activated(move |_, row| {
        let idx = row.index() as usize;
        if let Some(path) = sidebar.path_at(idx) {
            load_object(path);
        }
    });
}

fn wire_open_workspace(
    btn:          &gtk::Button,
    window:       &gtk::ApplicationWindow,
    state:        &Rc<RefCell<AppState>>,
    sidebar:      &Rc<SidebarTree>,
    graph_view:   &Rc<GraphView>,
    update_title: TitleFn,
) {
    let window       = window.clone();
    let state        = state.clone();
    let sidebar      = sidebar.clone();
    let graph_view   = graph_view.clone();

    btn.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder()
            .title("Open Workspace Folder")
            .modal(true)
            .build();
        let window       = window.clone();
        let state        = state.clone();
        let sidebar      = sidebar.clone();
        let graph_view   = graph_view.clone();
        let update_title = update_title.clone();
        dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    state.borrow_mut().workspace = Some(path.clone());
                    sidebar.load(&path);
                    graph_view.load(&path);
                    update_title();
                }
            }
        });
    });
}

fn wire_save(
    btn:          &gtk::Button,
    buffer:       &gtk::TextBuffer,
    state:        &Rc<RefCell<AppState>>,
    update_title: TitleFn,
) {
    let buffer = buffer.clone();
    let state  = state.clone();
    btn.connect_clicked(move |_| {
        let s = state.borrow();
        let Some(obj) = s.current_object() else { return };
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false);
        drop(s);
        if obj.save_content(&text).is_ok() {
            state.borrow_mut().is_modified = false;
            update_title();
        }
    });
}

fn wire_new_note(
    btn:         &gtk::Button,
    window:      &gtk::ApplicationWindow,
    state:       &Rc<RefCell<AppState>>,
    sidebar:     &Rc<SidebarTree>,
    load_object: LoadFn,
) {
    let window  = window.clone();
    let state   = state.clone();
    let sidebar = sidebar.clone();
    btn.connect_clicked(move |_| {
        let dir = initial_dir(&state);
        let Some(dir) = dir else { return };
        let dialog = gtk::FileDialog::builder()
            .title("New Note").modal(true)
            .initial_folder(&gio::File::for_path(&dir))
            .build();
        let window      = window.clone();
        let sidebar     = sidebar.clone();
        let state       = state.clone();
        let load_object = load_object.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let _ = std::fs::write(&path, "");
                    reload_sidebar(&sidebar, &state);
                    load_object(path);
                }
            }
        });
    });
}

fn wire_new_group(
    btn:         &gtk::Button,
    window:      &gtk::ApplicationWindow,
    state:       &Rc<RefCell<AppState>>,
    sidebar:     &Rc<SidebarTree>,
    load_object: LoadFn,
) {
    let window  = window.clone();
    let state   = state.clone();
    let sidebar = sidebar.clone();
    btn.connect_clicked(move |_| {
        let dir = initial_dir(&state);
        let Some(dir) = dir else { return };
        let dialog = gtk::FileDialog::builder()
            .title("Create Group").modal(true)
            .initial_folder(&gio::File::for_path(&dir))
            .build();
        let window      = window.clone();
        let sidebar     = sidebar.clone();
        let state       = state.clone();
        let load_object = load_object.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let _ = std::fs::create_dir_all(&path);
                    reload_sidebar(&sidebar, &state);
                    load_object(path);
                }
            }
        });
    });
}

fn wire_undo_redo(
    btn_undo: &gtk::Button,
    btn_redo: &gtk::Button,
    buffer:   &gtk::TextBuffer,
) {
    { let b = buffer.clone(); btn_undo.connect_clicked(move |_| { b.undo(); }); }
    { let b = buffer.clone(); btn_redo.connect_clicked(move |_| { b.redo(); }); }
}

fn wire_buffer_changed(
    buffer:       &gtk::TextBuffer,
    state:        &Rc<RefCell<AppState>>,
    links_label:  &gtk::Label,
    update_title: TitleFn,
) {
    let state       = state.clone();
    let links_label = links_label.clone();
    buffer.connect_changed(move |buf| {
        state.borrow_mut().is_modified = true;
        update_title();
        let (s, e) = buf.bounds();
        let text   = buf.text(&s, &e, false);
        let lnks   = object::extract_links(&text);
        links_label.set_label(&if lnks.is_empty() {
            "Links: ---".into()
        } else {
            format!("Links: {}", lnks.join("  *  "))
        });
    });
}

fn wire_keyboard_shortcuts(
    window:       &gtk::ApplicationWindow,
    btn_open_ws:  &gtk::Button,
    btn_save:     &gtk::Button,
    btn_new_note: &gtk::Button,
    btn_undo:     &gtk::Button,
    btn_redo:     &gtk::Button,
    btn_graph:    &gtk::ToggleButton,
) {
    let controller   = gtk::EventControllerKey::new();
    let btn_open_ws  = btn_open_ws.clone();
    let btn_save     = btn_save.clone();
    let btn_new_note = btn_new_note.clone();
    let btn_undo     = btn_undo.clone();
    let btn_redo     = btn_redo.clone();
    let btn_graph    = btn_graph.clone();

    controller.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        match (ctrl, key) {
            (true, gtk::gdk::Key::o) => { btn_open_ws.emit_clicked();                                glib::Propagation::Stop }
            (true, gtk::gdk::Key::s) => { btn_save.emit_clicked();                                   glib::Propagation::Stop }
            (true, gtk::gdk::Key::n) => { btn_new_note.emit_clicked();                               glib::Propagation::Stop }
            (true, gtk::gdk::Key::z) => { btn_undo.emit_clicked();                                   glib::Propagation::Stop }
            (true, gtk::gdk::Key::y) => { btn_redo.emit_clicked();                                   glib::Propagation::Stop }
            (true, gtk::gdk::Key::g) => { btn_graph.set_active(!btn_graph.is_active());              glib::Propagation::Stop }
            _                        => glib::Propagation::Proceed,
        }
    });

    window.add_controller(controller);
}

// ── Small utilities ───────────────────────────────────────────────────────────

/// Directory to use as the initial location for new-file dialogs.
fn initial_dir(state: &Rc<RefCell<AppState>>) -> Option<PathBuf> {
    let s = state.borrow();
    s.current_path.as_ref().map(|p| {
        if p.is_dir() { p.clone() } else { p.parent().unwrap().to_path_buf() }
    }).or_else(|| s.workspace.clone())
}

/// Reload the sidebar from the current workspace root.
fn reload_sidebar(sidebar: &Rc<SidebarTree>, state: &Rc<RefCell<AppState>>) {
    if let Some(ws) = state.borrow().workspace.clone() {
        sidebar.load(&ws);
    }
}
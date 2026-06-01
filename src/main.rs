mod object;
mod sidebar;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{
    gio, glib,
    prelude::*,
    Application, ApplicationWindow, Box, Button, HeaderBar, Label,
    Orientation, Paned, ScrolledWindow, TextView,
};

use object::{Group, ObjectKind, Node};
use sidebar::SidebarTree;

const APP_ID: &str = "org.gtk_rs.NoteGraph";

// ── App state ─────────────────────────────────────────────────────────────────

struct AppState {
    workspace:    Option<PathBuf>,   // open directory (root Group)
    current_path: Option<PathBuf>,   // currently edited object
    is_modified:  bool,
}

impl AppState {
    fn new() -> Self {
        Self { workspace: None, current_path: None, is_modified: false }
    }

    fn current_object(&self) -> Option<ObjectKind> {
        let path = self.current_path.as_ref()?;
        if path.is_dir() {
            Some(ObjectKind::Group(Group::new(path.clone())))
        } else {
            Some(ObjectKind::Node(Node::new(path.clone())))
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState::new()));

    // ── Editor (TextView) ─────────────────────────────────────────────────────
    let text_view = TextView::builder()
        .vexpand(true)
        .hexpand(true)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(16)
        .right_margin(16)
        .build();

    let buffer = text_view.buffer();
    buffer.set_enable_undo(true);

    let editor_scroll = ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .hexpand(true)
        .build();

    // ── Links bar (shows [[outgoing links]] for the open object) ──────────────
    let links_label = Label::builder()
        .label("Links: —")
        .halign(gtk::Align::Start)
        .margin_start(16)
        .margin_end(16)
        .margin_top(4)
        .margin_bottom(4)
        .wrap(true)
        .build();
    links_label.add_css_class("dim-label");

    // ── Status bar ────────────────────────────────────────────────────────────
    let status_label = Label::builder()
        .label("No file open")
        .halign(gtk::Align::Start)
        .margin_start(16)
        .margin_end(16)
        .margin_top(2)
        .margin_bottom(4)
        .build();
    status_label.add_css_class("dim-label");

    // ── Right panel (editor + links + status) ─────────────────────────────────
    let right_panel = Box::builder()
        .orientation(Orientation::Vertical)
        .build();
    right_panel.append(&editor_scroll);
    right_panel.append(&links_label);
    right_panel.append(&status_label);

    // ── Sidebar ───────────────────────────────────────────────────────────────
    let sidebar = Rc::new(SidebarTree::new());

    // ── Paned layout ──────────────────────────────────────────────────────────
    let paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .start_child(&sidebar.widget)
        .end_child(&right_panel)
        .position(220)
        .build();

    // ── Header bar ────────────────────────────────────────────────────────────
    let header = HeaderBar::new();

    let btn_open_ws = Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Open workspace folder (Ctrl+O)")
        .build();
    let btn_new_note = Button::builder()
        .icon_name("document-new-symbolic")
        .tooltip_text("New note in current group (Ctrl+N)")
        .build();
    let btn_new_group = Button::builder()
        .icon_name("folder-new-symbolic")
        .tooltip_text("New group (sub-directory)")
        .build();
    let btn_save = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save (Ctrl+S)")
        .build();
    let btn_undo = Button::builder()
        .icon_name("edit-undo-symbolic")
        .tooltip_text("Undo (Ctrl+Z)")
        .build();
    let btn_redo = Button::builder()
        .icon_name("edit-redo-symbolic")
        .tooltip_text("Redo (Ctrl+Y)")
        .build();

    header.pack_start(&btn_open_ws);
    header.pack_start(&btn_new_note);
    header.pack_start(&btn_new_group);
    header.pack_start(&btn_save);
    header.pack_end(&btn_redo);
    header.pack_end(&btn_undo);

    // ── Window ────────────────────────────────────────────────────────────────
    let window = ApplicationWindow::builder()
        .application(app)
        .title("NoteGraph")
        .titlebar(&header)
        .default_width(1100)
        .default_height(680)
        .child(&paned)
        .build();

    // ── Helpers ───────────────────────────────────────────────────────────────

    // Update window title
    let update_title = {
        let window = window.clone();
        let state  = state.clone();
        move || {
            let s = state.borrow();
            let ws = s.workspace.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "NoteGraph".into());
            let obj = s.current_path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string());
            let modified = if s.is_modified { "• " } else { "" };
            let title = match obj {
                Some(name) => format!("{modified}{name} — {ws}"),
                None       => ws,
            };
            window.set_title(Some(&title));
        }
    };

    // Load an object into the editor
    let load_object = {
        let buffer       = buffer.clone();
        let state        = state.clone();
        let links_label  = links_label.clone();
        let status_label = status_label.clone();
        let update_title = update_title.clone();
        Rc::new(move |path: PathBuf| {
            let obj = if path.is_dir() {
                ObjectKind::Group(Group::new(path.clone()))
            } else {
                ObjectKind::Node(Node::new(path.clone()))
            };

            let content = obj.content();
            buffer.set_text(&content);
            buffer.set_enable_undo(true); // reset undo stack on load

            // Links
            let lnks = obj.links();
            if lnks.is_empty() {
                links_label.set_label("Links: —");
            } else {
                links_label.set_label(&format!("Links: {}", lnks.join("  ·  ")));
            }

            // Status
            let kind = if obj.is_group() { "group" } else { "node" };
            status_label.set_label(&format!(
                "{kind}  ·  {}",
                path.display()
            ));

            let mut s = state.borrow_mut();
            s.current_path = Some(path);
            s.is_modified  = false;
            drop(s);
            update_title();
        })
    };

    // ── Sidebar row click → open object ──────────────────────────────────────
    {
        let sidebar     = sidebar.clone();
        let load_object = load_object.clone();
        // Grab the list widget *before* the move so we don't borrow + move sidebar simultaneously.
        let list = sidebar.list.clone();
        list.connect_row_activated(move |_, row| {
            let idx = row.index() as usize;
            if let Some(path) = sidebar.path_at(idx) {
                load_object(path);
            }
        });
    }

    // ── Open workspace ────────────────────────────────────────────────────────
    {
        let window      = window.clone();
        let state       = state.clone();
        let sidebar     = sidebar.clone();
        let update_title = update_title.clone();
        btn_open_ws.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Open Workspace Folder")
                .modal(true)
                .build();
            let window       = window.clone();
            let state        = state.clone();
            let sidebar      = sidebar.clone();
            let update_title = update_title.clone();
            dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        state.borrow_mut().workspace = Some(path.clone());
                        sidebar.load(&path);
                        update_title();
                    }
                }
            });
        });
    }

    // ── Save ──────────────────────────────────────────────────────────────────
    {
        let buffer       = buffer.clone();
        let state        = state.clone();
        let update_title = update_title.clone();
        btn_save.connect_clicked(move |_| {
            let s = state.borrow();
            let Some(obj) = s.current_object() else { return };
            let (start, end) = buffer.bounds();
            let text = buffer.text(&start, &end, false);
            drop(s);
            if obj.save_content(&text).is_ok() {
                let mut s = state.borrow_mut();
                s.is_modified = false;
                drop(s);
                update_title();
            }
        });
    }

    // ── New note ──────────────────────────────────────────────────────────────
    {
        let window   = window.clone();
        let state    = state.clone();
        let sidebar  = sidebar.clone();
        let load_object = load_object.clone();
        btn_new_note.connect_clicked(move |_| {
            // Determine target directory
            let dir = {
                let s = state.borrow();
                s.current_path.as_ref().map(|p| {
                    if p.is_dir() { p.clone() } else { p.parent().unwrap().to_path_buf() }
                }).or_else(|| s.workspace.clone())
            };
            let Some(dir) = dir else { return };

            let dialog = gtk::FileDialog::builder()
                .title("New Note")
                .modal(true)
                .initial_folder(&gio::File::for_path(&dir))
                .build();
            let window  = window.clone();
            let sidebar = sidebar.clone();
            let state   = state.clone();
            let load_object = load_object.clone();
            dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        // Create empty file
                        let _ = std::fs::write(&path, "");
                        // Refresh sidebar
                        if let Some(ws) = state.borrow().workspace.clone() {
                            sidebar.load(&ws);
                        }
                        load_object(path);
                    }
                }
            });
        });
    }

    // ── New group ─────────────────────────────────────────────────────────────
    {
        let window  = window.clone();
        let state   = state.clone();
        let sidebar = sidebar.clone();
        let load_object = load_object.clone();
        btn_new_group.connect_clicked(move |_| {
            let dir = {
                let s = state.borrow();
                s.current_path.as_ref().map(|p| {
                    if p.is_dir() { p.clone() } else { p.parent().unwrap().to_path_buf() }
                }).or_else(|| s.workspace.clone())
            };
            let Some(dir) = dir else { return };

            // Use FileDialog: the chosen save path becomes the new group directory.
            let dialog = gtk::FileDialog::builder()
                .title("Create Group (choose location + name)")
                .modal(true)
                .initial_folder(&gio::File::for_path(&dir))
                .build();
            let window  = window.clone();
            let sidebar = sidebar.clone();
            let state   = state.clone();
            let load_object = load_object.clone();
            dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let _ = std::fs::create_dir_all(&path);
                        if let Some(ws) = state.borrow().workspace.clone() {
                            sidebar.load(&ws);
                        }
                        load_object(path);
                    }
                }
            });
        });
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────
    { let b = buffer.clone(); btn_undo.connect_clicked(move |_| { b.undo(); }); }
    { let b = buffer.clone(); btn_redo.connect_clicked(move |_| { b.redo(); }); }

    // ── Track edits (links + modified flag) ───────────────────────────────────
    {
        let state        = state.clone();
        let links_label  = links_label.clone();
        let update_title = update_title.clone();
        buffer.connect_changed(move |buf| {
            state.borrow_mut().is_modified = true;
            update_title();

            // Re-parse links on every change
            let (s, e) = buf.bounds();
            let text   = buf.text(&s, &e, false);
            let lnks   = object::extract_links(&text);
            if lnks.is_empty() {
                links_label.set_label("Links: —");
            } else {
                links_label.set_label(&format!("Links: {}", lnks.join("  ·  ")));
            }
        });
    }

    // ── Keyboard shortcuts ────────────────────────────────────────────────────
    {
        let controller  = gtk::EventControllerKey::new();
        let btn_open_ws = btn_open_ws.clone();
        let btn_save    = btn_save.clone();
        let btn_new_note = btn_new_note.clone();
        let btn_undo    = btn_undo.clone();
        let btn_redo    = btn_redo.clone();
        controller.connect_key_pressed(move |_, key, _, mods| {
            let ctrl  = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            match (ctrl, key) {
                (true, gtk::gdk::Key::o) => { btn_open_ws.emit_clicked();  glib::Propagation::Stop }
                (true, gtk::gdk::Key::s) => { btn_save.emit_clicked();     glib::Propagation::Stop }
                (true, gtk::gdk::Key::n) => { btn_new_note.emit_clicked(); glib::Propagation::Stop }
                (true, gtk::gdk::Key::z) => { btn_undo.emit_clicked();     glib::Propagation::Stop }
                (true, gtk::gdk::Key::y) => { btn_redo.emit_clicked();     glib::Propagation::Stop }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(controller);
    }

    window.present();
}
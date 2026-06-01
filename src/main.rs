mod app;
mod graph;
mod object;
mod sidebar;

use gtk::{Application, gio::prelude::{ApplicationExt, ApplicationExtManual}, glib};

use app::ui::build_ui;

const APP_ID: &str = "org.gtk_rs.NoteGraph";

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id(APP_ID)
        .build();
    app.connect_activate(build_ui);
    app.run()
}
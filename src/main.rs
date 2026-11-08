mod app;
mod document_view;
mod files;
mod images;
mod memory;
mod native;
mod picker;
mod prototype;
mod reading;
mod render;
mod search;
mod state;
mod window;

fn main() -> gtk::glib::ExitCode {
    app::run()
}

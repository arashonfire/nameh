//! Bundled preview using the production model-driven renderer.
use std::{path::Path, rc::Rc};
pub const FIXTURE: &str = include_str!("../fixtures/prototype.md");

pub fn build() -> gtk::ScrolledWindow {
    let markdown = format!(
        "{FIXTURE}\n\n{}\n\n{}",
        include_str!("../fixtures/basic.md"),
        include_str!("../fixtures/rendering.md")
    );
    let view = render(&markdown);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build()
}

pub fn render(markdown: &str) -> crate::document_view::DocumentView {
    crate::native::render(
        &mdview::document::parse(markdown),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
        Rc::new(|_| {}),
    )
}

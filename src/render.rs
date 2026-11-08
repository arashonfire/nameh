//! Empty-state presentation. Model-driven documents are rendered by native.rs.

use gtk::prelude::*;

pub fn empty_state() -> (gtk::Box, gtk::Label) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(24);
    content.set_margin_bottom(24);

    let title = gtk::Label::new(Some("Markdown Viewer"));
    title.add_css_class("title-1");
    content.append(&title);

    let status = gtk::Label::new(Some("Choose a Markdown file to get started."));
    status.set_wrap(true);
    status.set_justify(gtk::Justification::Center);
    status.set_max_width_chars(60);
    status.set_selectable(true);
    content.append(&status);

    let open = gtk::Button::builder()
        .label("Open File…")
        .action_name("win.open")
        .halign(gtk::Align::Center)
        .build();
    content.append(&open);
    (content, status)
}

//! Native document text view with embedded blocks and lossless plain-text copying.

use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Default)]
    pub struct DocumentView {
        pub blocks: RefCell<Vec<(gtk::TextChildAnchor, String)>>,
        pub scrollers: RefCell<Vec<gtk::ScrolledWindow>>,
        pub fitted: RefCell<Vec<gtk::Widget>>,
        pub headings: RefCell<std::collections::HashMap<String, gtk::TextMark>>,
        disposed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DocumentView {
        const NAME: &'static str = "MdviewDocumentView";
        type Type = super::DocumentView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for DocumentView {
        fn dispose(&self) {
            if !self.disposed.replace(true)
                && (self.obj().buffer().char_count() >= 1_000_000
                    || self
                        .blocks
                        .borrow()
                        .iter()
                        .map(|(_, s)| s.len())
                        .sum::<usize>()
                        >= 1_000_000)
            {
                crate::memory::reclaim_after_retirement();
            }
        }
    }

    impl WidgetImpl for DocumentView {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                // An unwrapped embedded line must not widen the document viewport.
                (80, 800, -1, -1)
            } else {
                self.parent_measure(orientation, for_size)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let margin = ((width - 900) / 2).max(24);
            self.obj().set_left_margin(margin);
            self.obj().set_right_margin(margin);
            let available = (width - margin * 2).max(1);
            for block in self.scrollers.borrow().iter() {
                if block.width_request() != available {
                    block.set_size_request(available, -1);
                }
            }
            for child in self.fitted.borrow().iter() {
                if child.width_request() != available {
                    child.set_size_request(available, -1);
                }
            }
            self.parent_size_allocate(width, height, baseline);
        }
    }

    impl TextViewImpl for DocumentView {
        fn copy_clipboard(&self) {
            let view = self.obj();
            if let Some((start, end)) = view.buffer().selection_bounds() {
                view.clipboard().set_text(&view.plain_text(&start, &end));
            }
        }
    }
}

glib::wrapper! {
    pub struct DocumentView(ObjectSubclass<imp::DocumentView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for DocumentView {
    fn default() -> Self {
        let view: Self = glib::Object::builder()
            .property("editable", false)
            .property("cursor-visible", true)
            .property("wrap-mode", gtk::WrapMode::WordChar)
            .property("accepts-tab", false)
            .property("left-margin", 24)
            .property("right-margin", 24)
            .property("top-margin", 20)
            .property("bottom-margin", 20)
            .build();
        // This is a viewer: retaining undo records duplicates large documents.
        view.buffer().set_enable_undo(false);
        view.update_property(&[gtk::accessible::Property::Label("Markdown document")]);
        view
    }
}

pub(crate) struct ReadingPosition {
    heading: Option<(String, i32)>,
    fraction: f64,
}

impl DocumentView {
    pub(crate) fn reading_position(&self) -> ReadingPosition {
        let rect = self.visible_rect();
        let line = self.line_at_y(rect.y()).0.line();
        let buffer = self.buffer();
        let heading = self
            .imp()
            .headings
            .borrow()
            .iter()
            .map(|(id, mark)| (id.clone(), buffer.iter_at_mark(mark).line()))
            .filter(|(_, at)| *at <= line)
            .max_by_key(|(_, at)| *at)
            .map(|(id, at)| (id, line - at));
        let fraction = self
            .vadjustment()
            .map_or(0.0, |a| a.value() / (a.upper() - a.page_size()).max(1.0));
        ReadingPosition { heading, fraction }
    }

    pub(crate) fn restore_position(&self, position: ReadingPosition) {
        // Wait for the replacement scroller's first allocation. A scroll request
        // made while reparenting can otherwise be clamped to its initial zero.
        let weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            let Some(view) = weak.upgrade() else {
                return;
            };
            if !view.is_mapped() {
                return;
            }
            if let Some((id, delta)) = position.heading
                && let Some(mark) = view.imp().headings.borrow().get(&id)
            {
                let buffer = view.buffer();
                let mut at = buffer.iter_at_mark(mark);
                at.forward_lines(delta);
                buffer.place_cursor(&at);
                view.scroll_to_iter(&mut at, 0.0, true, 0.0, 0.0);
                return;
            }
            if let Some(a) = view.vadjustment() {
                a.set_value(position.fraction * (a.upper() - a.page_size()).max(0.0));
            }
        });
    }

    pub(crate) fn embedded(&self) -> Vec<(gtk::TextChildAnchor, String)> {
        self.imp().blocks.borrow().clone()
    }

    pub fn resize_blocks(&self) {
        for scroller in self.imp().scrollers.borrow().iter() {
            if let Some(block) = scroller.child().and_downcast::<gtk::TextView>() {
                let context = block.pango_context();
                let mut font = context.font_description().unwrap_or_default();
                font.set_family("monospace");
                let metrics = context.metrics(Some(&font), None);
                let line = (metrics.ascent() + metrics.descent() + gtk::pango::SCALE - 1)
                    / gtk::pango::SCALE;
                scroller.set_min_content_height(
                    (block.buffer().line_count().min(240) * line + 28).min(240),
                );
                if let Some(view) = block.downcast_ref::<DocumentView>() {
                    view.resize_blocks();
                }
            }
        }
    }

    pub fn append(&self, text: &str, tag: Option<&gtk::TextTag>) {
        let buffer = self.buffer();
        let mut end = buffer.end_iter();
        if let Some(tag) = tag {
            buffer.insert_with_tags(&mut end, text, &[tag]);
        } else {
            buffer.insert(&mut end, text);
        }
    }

    pub fn fit(&self, child: &impl IsA<gtk::Widget>) {
        child.set_size_request(1, -1);
        self.imp().fitted.borrow_mut().push(child.clone().upcast());
    }

    pub fn embed(&self, child: &impl IsA<gtk::Widget>, plain_text: String) {
        let buffer = self.buffer();
        let anchor = buffer.create_child_anchor(&mut buffer.end_iter());
        self.add_child_at_anchor(child, &anchor);
        self.imp().blocks.borrow_mut().push((anchor, plain_text));
    }

    pub fn heading(&self, id: &str) {
        let buffer = self.buffer();
        let mark = buffer.create_mark(None, &buffer.end_iter(), true);
        self.imp().headings.borrow_mut().insert(id.to_owned(), mark);
    }

    pub fn go_to_heading(&self, id: &str) -> bool {
        if id.is_empty() {
            let buffer = self.buffer();
            let mut start = buffer.start_iter();
            buffer.place_cursor(&start);
            self.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
            return true;
        }
        let Some(mark) = self.imp().headings.borrow().get(id).cloned() else {
            return false;
        };
        self.scroll_to_mark(&mark, 0.1, true, 0.0, 0.0);
        self.buffer()
            .place_cursor(&self.buffer().iter_at_mark(&mark));
        true
    }

    pub fn scrollable_block(&self, text: &str, label: &str) {
        let block = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(true)
            .accepts_tab(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::None)
            .left_margin(8)
            .right_margin(8)
            .top_margin(8)
            .bottom_margin(8)
            .build();
        block.buffer().set_enable_undo(false);
        block.buffer().set_text(text);
        // A text tag preserves fixed-width columns even when desktop CSS
        // overrides the TextView's monospace style class.
        let monospace = gtk::TextTag::builder().family("monospace").build();
        let block_buffer = block.buffer();
        block_buffer.tag_table().add(&monospace);
        block_buffer.apply_tag(
            &monospace,
            &block_buffer.start_iter(),
            &block_buffer.end_iter(),
        );
        self.scrollable_view(&block, text, label);
    }

    pub fn scrollable_view(&self, block: &impl IsA<gtk::TextView>, text: &str, label: &str) {
        let block = block.as_ref();
        let context = block.pango_context();
        let mut font = context.font_description().unwrap_or_default();
        font.set_family("monospace");
        let metrics = context.metrics(Some(&font), None);
        let line_height =
            (metrics.ascent() + metrics.descent() + gtk::pango::SCALE - 1) / gtk::pango::SCALE;
        let content_height =
            (text.split('\n').take(240).count().max(1) as i32 * line_height + 28).min(240);
        block.update_property(&[gtk::accessible::Property::Label(label)]);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .min_content_height(content_height)
            .max_content_height(240)
            .has_frame(true)
            .child(block)
            .build();
        scroller.set_size_request(320, -1);
        self.embed(&scroller, text.to_owned());
        self.imp().scrollers.borrow_mut().push(scroller);
    }

    pub fn plain_text(&self, start: &gtk::TextIter, end: &gtk::TextIter) -> String {
        let buffer = self.buffer();
        let blocks = self.imp().blocks.borrow();
        let replacements: std::collections::BTreeMap<_, _> = blocks
            .iter()
            .map(|(anchor, text)| (buffer.iter_at_child_anchor(anchor).offset(), text))
            .collect();
        let mut text = String::new();
        for (offset, character) in buffer.slice(start, end, false).chars().enumerate() {
            let position = start.offset() + offset as i32;
            if let Some(replacement) = replacements.get(&position) {
                text.push_str(replacement);
            } else {
                text.push(character);
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn embedded_content_copy_layout_and_focus() {
        gtk::init().expect("GTK needs a display for this integration check");
        glib::set_prgname(Some("mdview-tests"));
        glib::set_application_name("mdview tests");
        // Chooser tests must neither read nor update the user's recent files.
        gtk::Settings::default()
            .unwrap()
            .set_gtk_recent_files_enabled(false);
        let context = glib::MainContext::default();
        let view = crate::prototype::render(crate::prototype::FIXTURE);
        let outer = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&view)
            .build();
        let window = gtk::Window::builder()
            .title("mdview prototype verification")
            .default_width(400)
            .default_height(600)
            .resizable(false)
            .child(&outer)
            .build();
        window.present();
        context.block_on(glib::timeout_future(Duration::from_millis(200)));
        let original_clipboard = context
            .block_on(view.clipboard().read_text_future())
            .ok()
            .flatten();

        let buffer = view.buffer();
        let (start, end) = buffer.bounds();
        buffer.select_range(&start, &end);
        view.emit_by_name::<()>("copy-clipboard", &[]);
        let copied = context
            .block_on(view.clipboard().read_text_future())
            .expect("clipboard readable")
            .expect("clipboard contains text");
        for expected in [
            "Alpha",
            "Beta",
            "300",
            "[Image: A blue and yellow square]",
            "    Indentation and spaces must be preserved.",
            "GTK documentation (https://docs.gtk.org/gtk4/)",
            "Café — Ελληνικά — فارسی — 日本語 — 🐧",
        ] {
            assert!(
                copied.contains(expected),
                "missing copied content: {expected}"
            );
        }
        assert!(!copied.contains('\u{fffc}'), "embedded placeholders leaked");
        assert!(copied.find("Alpha").unwrap() < copied.find("[Image:").unwrap());
        assert!(copied.find("[Image:").unwrap() < copied.find("Indentation").unwrap());

        // A partial main-buffer selection crossing a table and image must expand
        // just those blocks and preserve the selected prose on each side.
        let begin = start
            .forward_search(
                "Select this paragraph",
                gtk::TextSearchFlags::TEXT_ONLY,
                None,
            )
            .unwrap()
            .0;
        let finish = start
            .forward_search(
                "This paragraph follows the image.",
                gtk::TextSearchFlags::TEXT_ONLY,
                None,
            )
            .unwrap()
            .1;
        buffer.select_range(&begin, &finish);
        view.emit_by_name::<()>("copy-clipboard", &[]);
        let partial = context
            .block_on(view.clipboard().read_text_future())
            .unwrap()
            .unwrap();
        assert!(partial.starts_with("Select this paragraph"));
        assert!(partial.ends_with("This paragraph follows the image."));
        assert!(partial.contains("Alpha") && partial.contains("[Image:"));
        assert!(!partial.contains("Indentation"));

        // Literal U+FFFC text is not an anchor and must survive copying.
        view.append("Literal \u{fffc} survives", None);
        assert!(
            view.plain_text(&buffer.start_iter(), &buffer.end_iter())
                .ends_with("Literal \u{fffc} survives")
        );

        let scrollers = view.imp().scrollers.borrow();
        assert_eq!(scrollers.len(), 2, "table and code each have a scroller");
        for scroller in scrollers.iter() {
            eprintln!(
                "layout: document={} block={} scroll_upper={} scroll_page={}",
                view.width(),
                scroller.width(),
                scroller.hadjustment().upper(),
                scroller.hadjustment().page_size()
            );
            assert!(
                scroller.width() <= view.width() - 48,
                "embedded block exceeds viewport"
            );
            let adjustment = scroller.hadjustment();
            assert!(
                adjustment.upper() > adjustment.page_size(),
                "long block does not scroll horizontally"
            );
            adjustment.set_value(40.0);
            assert!(adjustment.value() > 0.0);
            let vertical = scroller.vadjustment();
            assert!(
                vertical.upper() <= vertical.page_size() + 1.0,
                "short embedded content was vertically clipped"
            );
            let block = scroller
                .child()
                .unwrap()
                .downcast::<gtk::TextView>()
                .unwrap();
            assert!(!block.is_editable());
            assert!(!block.accepts_tab());
            assert_eq!(block.accessible_role(), gtk::AccessibleRole::TextBox);
            assert!(
                block.grab_focus(),
                "embedded text must accept keyboard focus"
            );
            let block_buffer = block.buffer();
            block_buffer.select_range(&block_buffer.start_iter(), &block_buffer.end_iter());
            block.emit_by_name::<()>("copy-clipboard", &[]);
            let copied = context
                .block_on(block.clipboard().read_text_future())
                .unwrap()
                .unwrap();
            assert_eq!(
                copied.as_str(),
                block_buffer
                    .text(&block_buffer.start_iter(), &block_buffer.end_iter(), false)
                    .as_str()
            );
        }
        assert!(view.grab_focus());
        let mut focused = Vec::new();
        for _ in 0..12 {
            window.child_focus(gtk::DirectionType::TabForward);
            if let Some(widget) = gtk::prelude::RootExt::focus(&window) {
                focused.push(widget);
            }
        }
        for scroller in scrollers.iter() {
            assert!(
                focused.contains(&scroller.child().unwrap()),
                "Tab traversal skipped an embedded text block"
            );
        }
        assert!(
            focused.iter().any(|widget| widget.is::<gtk::LinkButton>()),
            "Tab traversal skipped the link"
        );
        drop(scrollers);
        assert_eq!(view.accessible_role(), gtk::AccessibleRole::TextBox);
        assert!(!view.is_editable());
        assert!(
            view.width() <= outer.width(),
            "document widened the outer viewport"
        );
        window.set_default_size(900, 600);
        context.block_on(glib::timeout_future(Duration::from_millis(200)));
        assert_eq!(view.width(), 900, "test window did not resize wider");
        for scroller in view.imp().scrollers.borrow().iter() {
            assert!(scroller.width() <= 852);
            assert!(
                scroller.width() > 800,
                "block failed to grow with the viewport"
            );
        }
        window.set_default_size(360, 600);
        context.block_on(glib::timeout_future(Duration::from_millis(200)));
        assert_eq!(view.width(), 360, "test window did not resize narrower");
        for scroller in view.imp().scrollers.borrow().iter() {
            assert!(
                scroller.width() <= 312,
                "block failed to shrink with the viewport"
            );
        }
        window.close();
        crate::native::verify_rendering();
        crate::images::verify_loading_queue();
        crate::window::verify_file_opening();
        crate::app::verify_open_signals();
        if let Some(text) = original_clipboard {
            view.clipboard().set_text(&text);
        }
    }
}

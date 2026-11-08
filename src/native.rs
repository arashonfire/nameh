//! Model-driven GTK rendering; target dispatch is supplied by the caller.
use crate::{document_view::DocumentView, images};
use gtk::{glib, prelude::*};
use mdview::document::{Alignment, Document, LinkTarget, Node, NodeKind};
use std::{path::Path, rc::Rc};

type Activate = Rc<dyn Fn(&LinkTarget)>;

pub fn render(document: &Document, base: &Path, activate: Activate) -> DocumentView {
    glib::MainContext::default().block_on(render_async(document, base, activate))
}

/// Cooperative rendering keeps GTK work on its owning thread. The caller checks
/// its request generation before rendering and aborts this future on a newer request.
pub async fn render_async(document: &Document, base: &Path, activate: Activate) -> DocumentView {
    let view = DocumentView::default();
    let weak = view.downgrade();
    let dispatch: Activate = Rc::new(move |target| {
        if let LinkTarget::Heading(fragment) = target {
            if let (Some(view), Some(id)) = (weak.upgrade(), images::decode_component(fragment))
                && view.go_to_heading(&id)
            {
                return;
            }
            activate(target);
        } else {
            activate(target);
        }
    });
    nodes(&view, &document.children, base, &dispatch, 0).await;
    view.buffer().place_cursor(&view.buffer().start_iter());
    view
}

async fn styled(view: &DocumentView, tag: gtk::TextTag, f: impl std::future::Future<Output = ()>) {
    let buffer = view.buffer();
    buffer.tag_table().add(&tag);
    let start = buffer.char_count();
    f.await;
    buffer.apply_tag(&tag, &buffer.iter_at_offset(start), &buffer.end_iter());
}

fn nodes<'a>(
    view: &'a DocumentView,
    children: &'a [Node],
    base: &'a Path,
    activate: &'a Activate,
    depth: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
    Box::pin(async move {
        let mut slice = std::time::Instant::now();
        for node in children {
            node_view(view, node, base, activate, depth).await;
            if slice.elapsed() >= std::time::Duration::from_millis(4) {
                glib::timeout_future(std::time::Duration::from_millis(1)).await;
                slice = std::time::Instant::now();
            }
        }
    })
}

async fn append(view: &DocumentView, text: &str) {
    let mut rest = text;
    let mut slice = std::time::Instant::now();
    while !rest.is_empty() {
        let mut end = rest.len().min(8192);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        view.append(&rest[..end], None);
        rest = &rest[end..];
        if slice.elapsed() >= std::time::Duration::from_millis(4) {
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            slice = std::time::Instant::now();
        }
    }
}

async fn node_view(
    view: &DocumentView,
    node: &Node,
    base: &Path,
    activate: &Activate,
    depth: usize,
) {
    let children = || nodes(view, &node.children, base, activate, depth);
    match &node.kind {
        NodeKind::Text(text) => append(view, text).await,
        NodeKind::Code(text) => {
            styled(
                view,
                gtk::TextTag::builder().family("monospace").build(),
                append(view, text),
            )
            .await
        }
        NodeKind::SoftBreak => view.append(" ", None),
        NodeKind::HardBreak => view.append("\n", None),
        NodeKind::Paragraph => {
            children().await;
            view.append("\n\n", None);
        }
        NodeKind::Heading { level, id } => {
            view.heading(id);
            styled(
                view,
                gtk::TextTag::builder()
                    .scale(match level {
                        1 => 1.8,
                        2 => 1.5,
                        3 => 1.3,
                        _ => 1.1,
                    })
                    .weight(700)
                    .build(),
                children(),
            )
            .await;
            view.append("\n\n", None);
        }
        NodeKind::Emphasis => {
            styled(
                view,
                gtk::TextTag::builder()
                    .style(gtk::pango::Style::Italic)
                    .build(),
                children(),
            )
            .await
        }
        NodeKind::Strong => {
            styled(
                view,
                gtk::TextTag::builder().weight(700).build(),
                children(),
            )
            .await
        }
        NodeKind::Strikethrough => {
            styled(
                view,
                gtk::TextTag::builder().strikethrough(true).build(),
                children(),
            )
            .await
        }
        NodeKind::BlockQuote => {
            styled(
                view,
                gtk::TextTag::builder()
                    .left_margin(20 + depth as i32 * 20)
                    .style(gtk::pango::Style::Italic)
                    .build(),
                children(),
            )
            .await;
            view.append("\n", None);
        }
        NodeKind::List { start } => {
            if view.buffer().end_iter().line_offset() > 0 {
                view.append("\n", None);
            }
            let mut slice = std::time::Instant::now();
            for (index, item) in node.children.iter().enumerate() {
                if slice.elapsed() >= std::time::Duration::from_millis(4) {
                    glib::timeout_future(std::time::Duration::from_millis(1)).await;
                    slice = std::time::Instant::now();
                }
                view.append(&"  ".repeat(depth), None);
                view.append(
                    &start
                        .map(|n| format!("{}. ", n.saturating_add(index as u64)))
                        .unwrap_or_else(|| "• ".into()),
                    None,
                );
                nodes(view, &item.children, base, activate, depth + 1).await;
                if view.buffer().end_iter().line_offset() > 0 {
                    view.append("\n", None);
                }
            }
            if depth == 0 {
                view.append("\n", None);
            }
        }
        NodeKind::TaskMarker(checked) => view.append(if *checked { "☑ " } else { "☐ " }, None),
        NodeKind::Rule => {
            let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
            view.fit(&rule);
            view.embed(&rule, "────────".into());
            view.append("\n\n", None);
        }
        NodeKind::CodeBlock { info } => {
            // Node::text intentionally flattens breaks; code children are Text
            // nodes whose original whitespace is retained verbatim.
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
            let text = node.text();
            // Insert code cooperatively too; a single fenced block can be 5 MB.
            let mut rest = text.as_str();
            while !rest.is_empty() {
                let mut end = rest.len().min(8192);
                while !rest.is_char_boundary(end) {
                    end -= 1;
                }
                block
                    .buffer()
                    .insert(&mut block.buffer().end_iter(), &rest[..end]);
                rest = &rest[end..];
                glib::timeout_future(std::time::Duration::from_millis(1)).await;
            }
            let tag = gtk::TextTag::builder().family("monospace").build();
            block.buffer().tag_table().add(&tag);
            block.buffer().apply_tag(
                &tag,
                &block.buffer().start_iter(),
                &block.buffer().end_iter(),
            );
            view.scrollable_view(&block, &text, &format!("Code block: {info}"));
            view.append("\n\n", None);
        }
        NodeKind::Table { alignments } => {
            table(view, node, alignments, base, activate).await;
            view.append("\n\n", None);
        }
        NodeKind::Image { source, title } => {
            let alt = node.text();
            let image = images::widget(base, source.as_ref(), &alt);
            image.set_tooltip_text((!title.is_empty()).then_some(title));
            view.fit(&image);
            view.embed(&image, format!("[Image: {alt}]"));
        }
        NodeKind::Link {
            target: Some(target),
            title,
        } => {
            let label = node.text();
            let uri = match target {
                LinkTarget::Heading(id) => format!("#{id}"),
                LinkTarget::Local(local) => format!(
                    "{}{}",
                    local.path,
                    local
                        .fragment
                        .as_ref()
                        .map(|s| format!("#{s}"))
                        .unwrap_or_default()
                ),
                LinkTarget::External { uri, .. } => uri.clone(),
            };
            let button = gtk::LinkButton::with_label(&uri, &label);
            if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                label.set_markup(&inline_markup(node));
                if view.wrap_mode() != gtk::WrapMode::None {
                    // Child anchors allocate a wrapped label's minimum width;
                    // give the button an explicit readable width before wrapping.
                    let width = (label.layout().pixel_size().0 + 34).clamp(36, 280);
                    button.set_size_request(width, -1);
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                }
            }
            button.set_tooltip_text(Some(if title.is_empty() { &uri } else { title }));
            // Always intercept GTK's URI launcher, including for LocalTarget.
            let target = target.clone();
            let activate = activate.clone();
            button.connect_activate_link(move |_| {
                activate(&target);
                glib::Propagation::Stop
            });
            view.embed(&button, format!("{label} ({uri})"));
        }
        NodeKind::Item
        | NodeKind::TableHead
        | NodeKind::TableRow
        | NodeKind::TableCell
        | NodeKind::Link { target: None, .. } => children().await,
    }
}

fn inline_markup(node: &Node) -> String {
    let children = || node.children.iter().map(inline_markup).collect::<String>();
    match &node.kind {
        NodeKind::Text(text) => glib::markup_escape_text(text).to_string(),
        NodeKind::Code(text) => format!("<tt>{}</tt>", glib::markup_escape_text(text)),
        NodeKind::Strong => format!("<b>{}</b>", children()),
        NodeKind::Emphasis => format!("<i>{}</i>", children()),
        NodeKind::Strikethrough => format!("<s>{}</s>", children()),
        NodeKind::SoftBreak | NodeKind::HardBreak => " ".into(),
        _ => children(),
    }
}

async fn table(
    view: &DocumentView,
    node: &Node,
    alignments: &[Alignment],
    base: &Path,
    activate: &Activate,
) {
    let mut widths = vec![0; alignments.len()];
    let mut slice = std::time::Instant::now();
    for row in &node.children {
        for (width, cell) in widths.iter_mut().zip(&row.children) {
            *width = (*width).max(cell.text().chars().count());
        }
        if slice.elapsed() >= std::time::Duration::from_millis(4) {
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            slice = std::time::Instant::now();
        }
    }
    let block = DocumentView::default();
    block.set_wrap_mode(gtk::WrapMode::None);
    block.set_left_margin(8);
    block.set_right_margin(8);
    block.set_top_margin(8);
    block.set_bottom_margin(8);
    let mut slice = std::time::Instant::now();
    for (row_index, row) in node.children.iter().enumerate() {
        if slice.elapsed() >= std::time::Duration::from_millis(4) {
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            slice = std::time::Instant::now();
        }
        for (i, cell) in row.children.iter().enumerate() {
            if i > 0 {
                block.append(" | ", None);
            }
            let padding = widths
                .get(i)
                .copied()
                .unwrap_or(0)
                .saturating_sub(cell.text().chars().count());
            let left = match alignments.get(i) {
                Some(Alignment::Right) => padding,
                Some(Alignment::Center) => padding / 2,
                _ => 0,
            };
            block.append(&" ".repeat(left), None);
            if row_index == 0 {
                styled(
                    &block,
                    gtk::TextTag::builder().weight(700).build(),
                    nodes(&block, &cell.children, base, activate, 0),
                )
                .await;
            } else {
                nodes(&block, &cell.children, base, activate, 0).await;
            }
            block.append(&" ".repeat(padding - left), None);
        }
        if row_index + 1 < node.children.len() {
            block.append("\n", None);
        }
    }
    let buffer = block.buffer();
    let tag = gtk::TextTag::builder().family("monospace").build();
    buffer.tag_table().add(&tag);
    buffer.apply_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
    let text = block.plain_text(&buffer.start_iter(), &buffer.end_iter());
    view.scrollable_view(&block, &text, "Markdown table");
}

/// Runs on the existing GTK test thread; GTK must only initialize once.
#[cfg(test)]
pub fn verify_rendering() {
    use std::{cell::RefCell, time::Duration};
    fn descendants(widget: &gtk::Widget, result: &mut Vec<gtk::Widget>) {
        let mut child = widget.first_child();
        while let Some(widget) = child {
            descendants(&widget, result);
            child = widget.next_sibling();
            result.push(widget);
        }
    }
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let markdown = format!(
        "{}\n\n{}\n\n# Café\n\n[Encoded](#caf%C3%A9) [Inert](javascript:alert)\n\n| Link |\n| --- |\n| [Table heading](#caf%C3%A9) |\n\n7. Seven\n8. Eight\n\n![blocked](https://example.org/x.png)\n",
        include_str!("../fixtures/basic.md"),
        include_str!("../fixtures/rendering.md")
    );
    let targets = Rc::new(RefCell::new(Vec::new()));
    let saved = targets.clone();
    let view = render(
        &mdview::document::parse(&markdown),
        &base,
        Rc::new(move |target| saved.borrow_mut().push(target.clone())),
    );
    let outer = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&view)
        .build();
    let window = gtk::Window::builder()
        .title("mdview complete renderer verification")
        .default_width(360)
        .default_height(700)
        .resizable(false)
        .child(&outer)
        .build();
    window.present();
    let context = glib::MainContext::default();
    context.block_on(glib::timeout_future(Duration::from_millis(400)));
    let buffer = view.buffer();
    let plain = view.plain_text(&buffer.start_iter(), &buffer.end_iter());
    for text in [
        "bold",
        "italic",
        "strikethrough",
        "☑ Completed task",
        "☐ Pending task",
        "7. Seven",
        "8. Eight",
        "\n  • Nested item",
        "[Image: blocked]",
        "Inert",
    ] {
        assert!(plain.contains(text), "missing {text}");
    }
    assert!(!plain.contains("javascript:"));
    assert!(!plain.contains('\u{fffc}'));
    for id in [
        "basic-markdown",
        "lists",
        "duplicate",
        "duplicate-1",
        "café",
    ] {
        assert!(view.go_to_heading(id), "missing heading {id}");
    }
    assert!(!view.go_to_heading("unknown"));
    let mut widgets = Vec::new();
    descendants(view.upcast_ref(), &mut widgets);
    let links: Vec<_> = widgets
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::LinkButton>())
        .collect();
    assert_eq!(links.iter().filter(|b| b.uri() == "#caf%C3%A9").count(), 2);
    for button in links {
        assert!(button.height() < 60, "short link collapsed into a column");
        assert!(
            button.emit_by_name::<bool>("activate-link", &[]),
            "default URI launching must be stopped"
        );
        if button.uri() == "#caf%C3%A9" {
            let iter = buffer.iter_at_offset(buffer.cursor_position());
            assert!(
                buffer
                    .text(&iter, &buffer.end_iter(), false)
                    .starts_with("Café"),
                "heading in a table must target the main document"
            );
        }
    }
    assert!(
        targets
            .borrow()
            .iter()
            .all(|t| matches!(t, LinkTarget::Local(_))),
        "headings should be handled by renderer"
    );
    assert!(!targets.borrow().is_empty());
    for width in [360, 1000, 400] {
        window.set_default_size(width, 700);
        context.block_on(glib::timeout_future(Duration::from_millis(200)));
        assert!(view.width() <= outer.width());
        for widget in &widgets {
            if let Some(block) = widget.downcast_ref::<gtk::ScrolledWindow>() {
                assert!(block.width() <= view.width() - view.left_margin() - view.right_margin());
            }
        }
        if width == 1000 {
            assert_eq!(view.left_margin(), 50);
        }
    }
    let labels: Vec<_> = widgets
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Label>())
        .map(|l| l.text().to_string())
        .collect();
    assert!(
        labels
            .iter()
            .any(|s| s.contains("Missing local image") && s.contains("unavailable")),
        "missing image must settle to an error"
    );
    assert!(
        labels
            .iter()
            .any(|s| s.contains("blocked") && s.contains("blocked or unavailable"))
    );
    assert!(
        widgets.iter().any(|w| w.is::<gtk::Picture>()),
        "local image did not finish loading"
    );
    window.close();
}

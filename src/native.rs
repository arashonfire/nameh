//! Model-driven GTK rendering; target dispatch is supplied by the caller.
use crate::{
    document_view::{BlockHeight, DocumentView},
    images,
};
use gtk::{glib, prelude::*};
use nameh::document::{Alignment, Document, LinkTarget, Node, NodeKind};
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
            view.scrollable_view(
                &block,
                &text,
                &format!("Code block: {info}"),
                BlockHeight::Bounded,
            );
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
            let uri = link_uri(target);
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

/// Tables with more cells than this keep the monospace layout: a widget per
/// cell would make multi-megabyte documents slow to lay out and resize.
const GRID_CELL_LIMIT: usize = 3000;

async fn table(
    view: &DocumentView,
    node: &Node,
    alignments: &[Alignment],
    base: &Path,
    activate: &Activate,
) {
    let cells: usize = node.children.iter().map(|row| row.children.len()).sum();
    if cells > GRID_CELL_LIMIT {
        monospace_table(view, node, alignments, base, activate).await;
    } else {
        grid_table(view, node, alignments, activate).await;
    }
}

/// A table of wrapping cells: long cells wrap within their column instead of
/// widening the table, and the document scrolls the table with the page.
async fn grid_table(
    view: &DocumentView,
    node: &Node,
    alignments: &[Alignment],
    activate: &Activate,
) {
    install_table_style();
    let columns = node
        .children
        .iter()
        .map(|row| row.children.len())
        .max()
        .unwrap_or(0)
        .max(alignments.len());
    // A column's minimum is its longest word, capped so one long path or URL
    // wraps instead of forcing the whole table to scroll sideways.
    // Its natural width follows its typical cell (90th percentile), capped at
    // a readable line, so a rare long cell wraps instead of widening it.
    let mut minimum = vec![1; columns];
    let mut lengths = vec![Vec::with_capacity(node.children.len()); columns];
    for row in &node.children {
        for (column, cell) in row.children.iter().enumerate() {
            let text = cell.text();
            let longest = text
                .split_whitespace()
                .map(|word| word.chars().count())
                .max()
                .unwrap_or(0);
            minimum[column] = minimum[column].max(longest.min(14));
            lengths[column].push(text.chars().count());
        }
    }
    let natural: Vec<usize> = lengths
        .iter_mut()
        .zip(&minimum)
        .map(|(lengths, &minimum)| {
            lengths.sort_unstable();
            let typical = lengths.get(lengths.len() * 9 / 10).copied().unwrap_or(1);
            typical.clamp(minimum, 80)
        })
        .collect();
    // Text columns share any width left over after every column is natural.
    let widest = natural.iter().copied().max().unwrap_or(0);
    let expands: Vec<bool> = natural
        .iter()
        .map(|&chars| chars > 24 || chars == widest)
        .collect();
    let grid = gtk::Grid::new();
    grid.add_css_class("mdview-table");
    grid.update_property(&[gtk::accessible::Property::Label("Markdown table")]);
    let mut plain: Vec<Vec<String>> = Vec::with_capacity(node.children.len());
    let mut cells = Vec::with_capacity(node.children.len() * columns);
    let mut has_links = Vec::with_capacity(node.children.len() * columns);
    let mut slice = std::time::Instant::now();
    for (row_index, row) in node.children.iter().enumerate() {
        if slice.elapsed() >= std::time::Duration::from_millis(4) {
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            slice = std::time::Instant::now();
        }
        let mut row_plain = Vec::with_capacity(columns);
        for column in 0..columns {
            let cell = row.children.get(column);
            let mut links = Vec::new();
            let mut markup = cell.map(|c| cell_markup(c, &mut links)).unwrap_or_default();
            if row_index == 0 {
                markup = format!("<b>{markup}</b>");
            }
            row_plain.push(cell.map(cell_plain).unwrap_or_default());
            let label = gtk::Label::new(None);
            label.set_markup(&markup);
            label.set_wrap(true);
            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            label.set_width_chars(minimum[column] as i32);
            label.set_max_width_chars(natural[column] as i32);
            label.set_hexpand(expands[column]);
            label.set_yalign(0.0);
            let (xalign, justify) = match alignments.get(column) {
                Some(Alignment::Right) => (1.0, gtk::Justification::Right),
                Some(Alignment::Center) => (0.5, gtk::Justification::Center),
                _ => (0.0, gtk::Justification::Left),
            };
            label.set_xalign(xalign);
            label.set_justify(justify);
            label.set_selectable(true);
            label.add_css_class("mdview-cell");
            if row_index == 0 {
                label.add_css_class("mdview-head");
            } else if row_index % 2 == 0 {
                label.add_css_class("mdview-even");
            }
            if column > 0 {
                label.add_css_class("mdview-after");
            }
            if row_index > 0 {
                card_classes(&label, row_index - 1, column, columns);
            }
            // Keep Tab usable past long tables: the first cell and cells with
            // links are stops; any other cell takes focus when clicked.
            if (row_index, column) != (0, 0) && links.is_empty() {
                set_tab_stop(&label, false);
            }
            if !links.is_empty() {
                let activate = activate.clone();
                label.connect_activate_link(move |_, uri| {
                    if let Some((_, target)) = links.iter().find(|(link, _)| link == uri) {
                        activate(target);
                    }
                    // Never let GTK launch the URI itself.
                    glib::Propagation::Stop
                });
            }
            grid.attach(&label, column as i32, row_index as i32, 1, 1);
            has_links.push(label.is_focusable() && (row_index, column) != (0, 0));
            cells.push(label);
        }
        plain.push(row_plain);
    }
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let promoted: std::cell::RefCell<Option<gtk::Label>> = Default::default();
    click.connect_pressed(move |gesture, _, x, y| {
        let Some(grid) = gesture.widget() else {
            return;
        };
        let Some(label) = grid
            .pick(x, y, gtk::PickFlags::DEFAULT)
            .and_then(|w| {
                if w.is::<gtk::Label>() {
                    Some(w)
                } else {
                    w.ancestor(gtk::Label::static_type())
                }
            })
            .and_downcast::<gtk::Label>()
            .filter(|label| label.has_css_class("mdview-cell"))
        else {
            return;
        };
        if !label.is_focusable() {
            if let Some(previous) = promoted.replace(Some(label.clone()))
                && !previous.has_focus()
            {
                set_tab_stop(&previous, false);
            }
            set_tab_stop(&label, true);
        }
    });
    grid.add_controller(click);
    // Neither direction scrolls: narrow windows switch to cards instead. A
    // sideways-scrolling frame would measure the wrapped cells' height at the
    // table's narrowest width, leaving a long empty space below the table.
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .has_frame(true)
        .child(&grid)
        .build();
    let headers = plain.first().cloned().unwrap_or_default();
    let layout = TableLayout {
        grid,
        scroller: scroller.clone(),
        cells,
        has_links,
        columns,
        xalign: (0..columns)
            .map(|column| match alignments.get(column) {
                Some(Alignment::Right) => 1.0,
                Some(Alignment::Center) => 0.5,
                _ => 0.0,
            })
            .collect(),
        headers,
        fields: Default::default(),
        cards: Default::default(),
    };
    view.embed_wide(
        &scroller,
        table_text(&plain, alignments),
        Some(Rc::new(move |width| layout.fit(width))),
    );
}

/// Switches a grid table between columns and stacked cards. The same cell
/// widgets are rearranged, so search, copying, links and focus keep working.
struct TableLayout {
    grid: gtk::Grid,
    scroller: gtk::ScrolledWindow,
    /// Every cell, header row first, in reading order.
    cells: Vec<gtk::Label>,
    has_links: Vec<bool>,
    columns: usize,
    xalign: Vec<f32>,
    headers: Vec<String>,
    /// Column names repeated beside each card's values; made on first use.
    fields: std::cell::RefCell<Vec<gtk::Label>>,
    cards: std::cell::Cell<bool>,
}

impl TableLayout {
    /// Uses cards when `width` is narrower than the columns need.
    fn fit(&self, width: i32) {
        let rows = self.cells.len() / self.columns.max(1);
        if self.columns < 2 || rows < 2 {
            return;
        }
        // The first body row is visible in both layouts, so its minimum does
        // not depend on the current one; the margin covers the bold header.
        let needed = self.cells[self.columns..self.columns * 2]
            .iter()
            .map(|cell| cell.measure(gtk::Orientation::Horizontal, -1).0)
            .sum::<i32>() as f32
            * 1.08
            + 4.0;
        let cards = (width as f32) < needed;
        if cards == self.cards.replace(cards) {
            return;
        }
        if cards && self.fields.borrow().is_empty() {
            let mut fields = self.fields.borrow_mut();
            for card in 0..rows - 1 {
                for (column, header) in self.headers.iter().enumerate() {
                    let field = gtk::Label::new(Some(header));
                    field.set_wrap(true);
                    field.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    field.set_max_width_chars(16);
                    field.set_xalign(0.0);
                    field.set_yalign(0.0);
                    field.add_css_class("mdview-field");
                    card_classes(&field, card, column, self.columns);
                    self.grid.attach(&field, 0, 0, 1, 1);
                    fields.push(field);
                }
            }
        }
        let place = |widget: &gtk::Label, column: usize, row: usize| {
            if let Some(child) = self
                .grid
                .layout_manager()
                .map(|manager| manager.layout_child(widget))
                .and_downcast::<gtk::GridLayoutChild>()
            {
                child.set_column(column as i32);
                child.set_row(row as i32);
            }
        };
        for (index, cell) in self.cells.iter().enumerate() {
            let (row, column) = (index / self.columns, index % self.columns);
            if cards {
                cell.set_visible(row > 0);
                if row > 0 {
                    place(cell, 1, (row - 1) * self.columns + column);
                }
                cell.set_xalign(0.0);
            } else {
                cell.set_visible(true);
                place(cell, column, row);
                cell.set_xalign(self.xalign[column]);
            }
        }
        for (index, field) in self.fields.borrow().iter().enumerate() {
            field.set_visible(cards);
            if cards {
                place(field, 0, index);
            }
        }
        // The hidden header cell is the table's Tab stop; the first card's
        // first value takes over while cards are shown.
        let first = &self.cells[self.columns];
        set_tab_stop(first, cards || self.has_links[self.columns]);
        if cards {
            self.grid.add_css_class("mdview-cards");
        } else {
            self.grid.remove_css_class("mdview-cards");
        }
        self.scroller.set_has_frame(!cards);
    }
}

/// Classes that frame a card's first and last lines in the card layout.
fn card_classes(label: &gtk::Label, card: usize, column: usize, columns: usize) {
    if column == 0 {
        label.add_css_class("mdview-card-top");
        if card > 0 {
            label.add_css_class("mdview-card-gap");
        }
    }
    if column + 1 == columns {
        label.add_css_class("mdview-card-bottom");
    }
}

/// A label that is not a tab stop must also refuse focus moves: GtkLabel's
/// focus handler otherwise reports success without taking focus, which
/// would trap Tab on the previous cell.
fn set_tab_stop(label: &gtk::Label, stop: bool) {
    label.set_focusable(stop);
    label.set_can_focus(stop);
}

/// Plain text for copying a whole table: one row per line, cells padded into
/// columns and separated by " | ".
fn table_text(rows: &[Vec<String>], alignments: &[Alignment]) -> String {
    let mut widths = Vec::new();
    for row in rows {
        widths.resize(widths.len().max(row.len()), 0);
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(i, cell)| {
                    let padding = widths[i].saturating_sub(cell.chars().count());
                    let left = match alignments.get(i) {
                        Some(Alignment::Right) => padding,
                        Some(Alignment::Center) => padding / 2,
                        _ => 0,
                    };
                    format!("{}{cell}{}", " ".repeat(left), " ".repeat(padding - left))
                })
                .collect::<Vec<_>>()
                .join(" | ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn link_uri(target: &LinkTarget) -> String {
    match target {
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
    }
}

/// Pango markup for a table cell. Links become label links whose targets are
/// collected so activation goes through the viewer, never GTK's launcher.
fn cell_markup(node: &Node, links: &mut Vec<(String, LinkTarget)>) -> String {
    let children = |links: &mut Vec<(String, LinkTarget)>| {
        node.children
            .iter()
            .map(|child| cell_markup(child, links))
            .collect::<String>()
    };
    match &node.kind {
        NodeKind::Text(text) => glib::markup_escape_text(text).to_string(),
        NodeKind::Code(text) => format!("<tt>{}</tt>", glib::markup_escape_text(text)),
        NodeKind::Strong => format!("<b>{}</b>", children(links)),
        NodeKind::Emphasis => format!("<i>{}</i>", children(links)),
        NodeKind::Strikethrough => format!("<s>{}</s>", children(links)),
        NodeKind::SoftBreak => " ".into(),
        NodeKind::HardBreak => "\n".into(),
        NodeKind::TaskMarker(checked) => (if *checked { "☑ " } else { "☐ " }).into(),
        NodeKind::Image { .. } => {
            glib::markup_escape_text(&format!("[Image: {}]", node.text())).to_string()
        }
        NodeKind::Link {
            target: Some(target),
            title,
        } => {
            let uri = link_uri(target);
            let tooltip = if title.is_empty() { &uri } else { title };
            let markup = format!(
                "<a href=\"{}\" title=\"{}\">{}</a>",
                glib::markup_escape_text(&uri),
                glib::markup_escape_text(tooltip),
                children(links)
            );
            links.push((uri, target.clone()));
            markup
        }
        _ => children(links),
    }
}

/// A cell's copied text, matching how links and images copy from prose.
fn cell_plain(node: &Node) -> String {
    let children = || node.children.iter().map(cell_plain).collect::<String>();
    match &node.kind {
        NodeKind::Text(text) | NodeKind::Code(text) => text.clone(),
        NodeKind::SoftBreak | NodeKind::HardBreak => " ".into(),
        NodeKind::TaskMarker(checked) => (if *checked { "☑ " } else { "☐ " }).into(),
        NodeKind::Image { .. } => format!("[Image: {}]", node.text()),
        NodeKind::Link {
            target: Some(target),
            ..
        } => format!("{} ({})", children(), link_uri(target)),
        _ => children(),
    }
}

/// Table rules that apply in every appearance, installed once per display.
fn install_table_style() {
    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSTALLED.replace(true) {
        return;
    }
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".mdview-table label.mdview-cell { padding: 6px 10px; border-bottom: 1px solid alpha(currentColor, 0.15); }
         .mdview-table label.mdview-after { border-left: 1px solid alpha(currentColor, 0.15); }
         .mdview-table label.mdview-head { background-color: alpha(currentColor, 0.07); border-bottom: 2px solid alpha(currentColor, 0.3); }
         .mdview-table label.mdview-even { background-color: alpha(currentColor, 0.03); }
         .mdview-table.mdview-cards label.mdview-cell, .mdview-table.mdview-cards label.mdview-field { border: none; padding: 3px 12px; background-color: alpha(currentColor, 0.05); }
         .mdview-table.mdview-cards label.mdview-field { font-weight: bold; padding-right: 4px; }
         .mdview-table.mdview-cards label.mdview-card-top { padding-top: 10px; border-top: 1px solid alpha(currentColor, 0.2); }
         .mdview-table.mdview-cards label.mdview-card-bottom { padding-bottom: 10px; border-bottom: 1px solid alpha(currentColor, 0.2); }
         .mdview-table.mdview-cards label.mdview-card-gap { margin-top: 12px; }",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Monospace table for very large tables; cells stay on one line.
async fn monospace_table(
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
    view.scrollable_view(&block, &text, "Markdown table", BlockHeight::Full);
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
        &nameh::document::parse(&markdown),
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
    assert_eq!(links.iter().filter(|b| b.uri() == "#caf%C3%A9").count(), 1);
    // The table's link is a link inside its cell's label.
    let cell = widgets
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Label>())
        .find(|l| l.text() == "Table heading" && l.has_css_class("mdview-cell"))
        .expect("table link cell");
    assert!(cell.is_focusable(), "a cell with a link must be a Tab stop");
    assert!(cell.label().contains("href=\"#caf%C3%A9\""));
    buffer.place_cursor(&buffer.start_iter());
    assert!(
        cell.emit_by_name::<bool>("activate-link", &[&"#caf%C3%A9"]),
        "default URI launching must be stopped"
    );
    let iter = buffer.iter_at_offset(buffer.cursor_position());
    assert!(
        buffer
            .text(&iter, &buffer.end_iter(), false)
            .starts_with("Café"),
        "heading in a table must target the main document"
    );
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
                let limit = if crate::document_view::table_cells(widget).is_some() {
                    // Tables may use the window's width beside a 24px gutter.
                    view.width() - 48
                } else {
                    view.width() - view.left_margin() - view.right_margin()
                };
                assert!(block.width() <= limit);
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
    // A table wider than a narrow window becomes cards, and returns to
    // columns when the window widens.
    let wide = render(
        &nameh::document::parse(
            "| ID | Decision | Status | Date | Rationale |\n| --- | --- | --- | --- | --- |\n| F1 | Arch Linux base | Accepted | 2026-10-10 | Rolling release |\n| F2 | Hyprland compositor | Accepted | 2026-10-10 | Tiling |\n",
        ),
        &base,
        Rc::new(|_| {}),
    );
    outer.set_child(Some(&wide));
    let table = wide
        .embedded()
        .into_iter()
        .flat_map(|(anchor, _)| anchor.widgets())
        .find(|w| crate::document_view::table_cells(w).is_some())
        .unwrap();
    let grid = crate::document_view::table_cells(&table).unwrap()[0]
        .parent()
        .unwrap();
    for (width, cards) in [(360, true), (1000, false), (360, true)] {
        window.set_default_size(width, 700);
        context.block_on(glib::timeout_future(Duration::from_millis(300)));
        assert_eq!(grid.has_css_class("mdview-cards"), cards, "width {width}");
        let cells = crate::document_view::table_cells(&table).unwrap();
        // Cards hide the header row; its names label every card instead.
        assert_eq!(cells.len(), if cards { 10 } else { 15 });
        assert_eq!(cells[0].text(), if cards { "F1" } else { "ID" });
        assert!(cells[0].is_focusable(), "the table lost its Tab stop");
        let horizontal = table
            .downcast_ref::<gtk::ScrolledWindow>()
            .unwrap()
            .hadjustment();
        assert!(
            horizontal.upper() <= horizontal.page_size() + 1.0,
            "width {width}: table scrolls sideways"
        );
        // The table ends at its last row, with no empty space below it.
        let last = cells.last().unwrap();
        let bottom = last
            .compute_point(
                &table,
                &gtk::graphene::Point::new(0.0, last.height() as f32),
            )
            .unwrap();
        assert!(
            table.height() as f32 - bottom.y() < 24.0,
            "width {width}: table is {} tall but its rows end at {}",
            table.height(),
            bottom.y()
        );
        if cards {
            let id = cells[0].compute_point(&grid, &gtk::graphene::Point::new(0.0, 0.0));
            let date = cells[3].compute_point(&grid, &gtk::graphene::Point::new(0.0, 0.0));
            let (id, date) = (id.unwrap(), date.unwrap());
            assert_eq!(id.x(), date.x(), "a card's values share one column");
            assert!(date.y() > id.y(), "a card's values are stacked");
        }
    }
    window.close();
}

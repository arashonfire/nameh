//! Search rendered text in reading order, mapping hits back to native buffers.
use crate::document_view::DocumentView;
use gtk::{gio, glib, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Where a run of searchable text lives.
#[derive(Clone)]
enum Target {
    /// Characters of a text buffer.
    Text(gtk::TextView),
    /// An embedded widget standing for one anchor character of a buffer.
    Widget(gtk::TextView, gtk::Widget),
    /// A table cell; offsets are characters of the label's text.
    Cell(gtk::Label),
    /// A separator between table cells; never highlighted.
    Gap,
}

#[derive(Clone)]
struct Span {
    start: i32,
    end: i32,
    target: Target,
    offset: i32,
    parents: Vec<(gtk::TextView, i32)>,
}

#[derive(Clone)]
struct Piece {
    target: Target,
    start: i32,
    end: i32,
    parents: Vec<(gtk::TextView, i32)>,
}

#[derive(Default)]
pub struct Search {
    hits: Vec<(i32, i32)>,
    spans: Vec<Span>,
    buffers: Vec<gtk::TextBuffer>,
    /// Highlighted ranges of table cells: (cell, start, end, active).
    cells: Vec<(gtk::Label, i32, i32, bool)>,
    active: usize,
    indexed: Option<(DocumentView, String)>,
    cancelled: Arc<AtomicBool>,
}

fn push(text: &mut String, spans: &mut Vec<Span>, value: &str, mut span: Span) {
    span.start = spans.last().map_or(0, |s| s.end);
    span.end = span.start + value.chars().count() as i32;
    text.push_str(value);
    spans.push(span);
}

fn collect<'a>(
    view: &'a gtk::TextView,
    parents: &'a [(gtk::TextView, i32)],
    text: &'a mut String,
    spans: &'a mut Vec<Span>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
    Box::pin(async move {
        let buffer = view.buffer();
        let blocks = view
            .downcast_ref::<DocumentView>()
            .map(DocumentView::embedded)
            .unwrap_or_default();
        let mut offset = 0;
        let mut slice = std::time::Instant::now();
        for (anchor, replacement) in blocks {
            if slice.elapsed() >= std::time::Duration::from_millis(4) {
                glib::timeout_future(std::time::Duration::from_millis(1)).await;
                slice = std::time::Instant::now();
            }
            let at = buffer.iter_at_child_anchor(&anchor).offset();
            collect_text(view, parents, offset, at, text, spans).await;
            let widget = anchor.widgets().into_iter().next();
            let child = widget
                .as_ref()
                .and_then(|w| w.downcast_ref::<gtk::ScrolledWindow>())
                .and_then(|s| s.child())
                .and_downcast::<gtk::TextView>();
            let cells = widget.as_ref().and_then(crate::document_view::table_cells);
            if let Some(child) = child {
                let mut parents = parents.to_vec();
                parents.push((view.clone(), at));
                collect(&child, &parents, text, spans).await;
            } else if let Some(cells) = cells {
                let mut parents = parents.to_vec();
                parents.push((view.clone(), at));
                for (index, cell) in cells.into_iter().enumerate() {
                    if index > 0 {
                        // Keep matches from running across cells.
                        push(text, spans, "\n", span(Target::Gap, 0, &parents));
                    }
                    push(
                        text,
                        spans,
                        &cell.text(),
                        span(Target::Cell(cell), 0, &parents),
                    );
                }
            } else {
                // Link destinations are copy metadata, not rendered text.
                let label = widget
                    .as_ref()
                    .and_then(|w| w.downcast_ref::<gtk::LinkButton>())
                    .and_then(|b| b.child().and_downcast::<gtk::Label>())
                    .map(|label| label.text());
                push(
                    text,
                    spans,
                    label.as_deref().unwrap_or(&replacement),
                    span(
                        match widget {
                            Some(widget) => Target::Widget(view.clone(), widget),
                            None => Target::Text(view.clone()),
                        },
                        at,
                        parents,
                    ),
                );
            }
            offset = at + 1;
        }
        collect_text(view, parents, offset, buffer.char_count(), text, spans).await;
    })
}

fn span(target: Target, offset: i32, parents: &[(gtk::TextView, i32)]) -> Span {
    Span {
        start: 0,
        end: 0,
        target,
        offset,
        parents: parents.to_vec(),
    }
}

async fn collect_text(
    view: &gtk::TextView,
    parents: &[(gtk::TextView, i32)],
    mut offset: i32,
    end: i32,
    text: &mut String,
    spans: &mut Vec<Span>,
) {
    let buffer = view.buffer();
    let mut slice = std::time::Instant::now();
    while offset < end {
        let next = (offset + 8192).min(end);
        push(
            text,
            spans,
            &buffer.text(
                &buffer.iter_at_offset(offset),
                &buffer.iter_at_offset(next),
                false,
            ),
            span(Target::Text(view.clone()), offset, parents),
        );
        offset = next;
        if slice.elapsed() >= std::time::Duration::from_millis(4) {
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            slice = std::time::Instant::now();
        }
    }
}

impl Search {
    pub fn clear(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(hit) = self.hits.get(self.active) {
            self.tag(&self.pieces(*hit), "mdview-search-active", false);
        }
        for hit in self.hits.iter().take(1000) {
            for piece in self.pieces(*hit) {
                if let Target::Widget(_, widget) = &piece.target {
                    widget.remove_css_class("mdview-search-hit");
                    widget.remove_css_class("mdview-search-active");
                }
            }
        }
        for (cell, ..) in std::mem::take(&mut self.cells) {
            cell.set_attributes(None);
        }
        for buffer in &self.buffers {
            if let Some(mark) = buffer.mark("mdview-search-reveal") {
                buffer.delete_mark(&mark);
            }
            for name in ["mdview-search", "mdview-search-active"] {
                if let Some(tag) = buffer.tag_table().lookup(name) {
                    buffer.remove_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
                }
            }
        }
        // Hiding search also releases capacity from dense multi-megabyte hits.
        self.hits = Vec::new();
        self.spans = Vec::new();
        self.buffers = Vec::new();
        self.active = 0;
        self.indexed = None;
    }

    // GTK 4.10 has no replacement for resolving named theme colors for TextTags.
    #[allow(deprecated)]
    pub async fn find_async(&mut self, view: &DocumentView, query: &str, reveal: bool) {
        if self
            .indexed
            .as_ref()
            .is_some_and(|(old_view, old_query)| old_view == view && old_query == query)
        {
            return;
        }
        self.clear();
        self.indexed = Some((view.clone(), query.into()));
        self.cancelled = Arc::new(AtomicBool::new(false));
        if query.is_empty() {
            return;
        }
        let mut text = String::new();
        let mut spans = Vec::new();
        collect(view.upcast_ref(), &[], &mut text, &mut spans).await;
        let query = query.to_owned();
        let cancelled = self.cancelled.clone();
        self.hits = gio::spawn_blocking(move || match_ranges(&text, &query, &cancelled))
            .await
            .unwrap_or_default();
        for span in &spans {
            let (Target::Text(view) | Target::Widget(view, _)) = &span.target else {
                continue;
            };
            let buffer = view.buffer();
            if self.buffers.contains(&buffer) {
                continue;
            }
            let (color, active_fg) = highlight_colors(view);
            for (name, alpha) in [("mdview-search", 0.28), ("mdview-search-active", 1.0)] {
                let tag = buffer.tag_table().lookup(name).unwrap_or_else(|| {
                    let tag = gtk::TextTag::builder().name(name).build();
                    buffer.tag_table().add(&tag);
                    tag
                });
                let mut color = color;
                color.set_alpha(alpha);
                tag.set_background_rgba(Some(&color));
                if name.ends_with("active") {
                    tag.set_underline(gtk::pango::Underline::Double);
                    tag.set_foreground_rgba(Some(&active_fg));
                }
            }
            self.buffers.push(buffer);
        }
        self.spans = spans;
        // Bound native tag segments. Count/navigation still include every hit.
        for index in 0..self.hits.len().min(1000) {
            self.tag(&self.pieces(self.hits[index]), "mdview-search", true);
            if index % 100 == 99 {
                glib::timeout_future(std::time::Duration::from_millis(1)).await;
            }
        }
        if reveal {
            self.reveal();
        } else if let Some(&hit) = self.hits.first() {
            self.tag(&self.pieces(hit), "mdview-search-active", true);
        }
    }

    fn pieces(&self, (start, end): (i32, i32)) -> Vec<Piece> {
        let begin = self.spans.partition_point(|s| s.end <= start);
        self.spans[begin..]
            .iter()
            .take_while(|s| s.start < end)
            .filter(|s| !matches!(s.target, Target::Gap))
            .map(|s| {
                let (from, to) = if matches!(s.target, Target::Widget(..)) {
                    (0, 1)
                } else {
                    ((start - s.start).max(0), (end.min(s.end) - s.start).max(0))
                };
                Piece {
                    target: s.target.clone(),
                    start: s.offset + from,
                    end: s.offset + to,
                    parents: s.parents.clone(),
                }
            })
            .collect()
    }

    fn tag(&mut self, hit: &[Piece], name: &str, apply: bool) {
        let active = name.ends_with("active");
        for piece in hit {
            let view = match &piece.target {
                Target::Text(view) => view,
                Target::Widget(view, widget) => {
                    let class = if active {
                        "mdview-search-active"
                    } else {
                        "mdview-search-hit"
                    };
                    if apply {
                        widget.add_css_class(class);
                    } else {
                        widget.remove_css_class(class);
                    }
                    view
                }
                Target::Cell(cell) => {
                    let mark = (cell.clone(), piece.start, piece.end, active);
                    if apply {
                        self.cells.push(mark);
                    } else if let Some(index) = self.cells.iter().position(|m| *m == mark) {
                        self.cells.swap_remove(index);
                    }
                    self.paint(cell);
                    continue;
                }
                Target::Gap => continue,
            };
            let buffer = view.buffer();
            let tag = buffer.tag_table().lookup(name).unwrap();
            let (start, end) = (
                buffer.iter_at_offset(piece.start),
                buffer.iter_at_offset(piece.end),
            );
            if apply {
                buffer.apply_tag(&tag, &start, &end);
            } else {
                buffer.remove_tag(&tag, &start, &end);
            }
        }
    }

    /// Redraws a cell's highlights from its recorded ranges.
    fn paint(&self, cell: &gtk::Label) {
        let marks: Vec<_> = self.cells.iter().filter(|m| &m.0 == cell).collect();
        if marks.is_empty() {
            cell.set_attributes(None);
            return;
        }
        let (background, foreground) = highlight_colors(cell);
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0) as u16;
        let text = cell.text();
        let byte = |offset: i32| {
            text.char_indices()
                .nth(offset as usize)
                .map_or(text.len(), |(index, _)| index) as u32
        };
        let list = gtk::pango::AttrList::new();
        for (_, start, end, active) in &marks {
            // An active mark replaces the plain highlight of the same range.
            if !active && marks.iter().any(|m| m.3 && (m.1, m.2) == (*start, *end)) {
                continue;
            }
            let (start, end) = (byte(*start), byte(*end));
            let mut attributes: Vec<gtk::pango::Attribute> = vec![
                gtk::pango::AttrColor::new_background(
                    channel(background.red()),
                    channel(background.green()),
                    channel(background.blue()),
                )
                .into(),
                gtk::pango::AttrInt::new_background_alpha(if *active {
                    65535
                } else {
                    channel(0.28)
                })
                .into(),
            ];
            if *active {
                attributes
                    .push(gtk::pango::AttrInt::new_underline(gtk::pango::Underline::Double).into());
                attributes.push(
                    gtk::pango::AttrColor::new_foreground(
                        channel(foreground.red()),
                        channel(foreground.green()),
                        channel(foreground.blue()),
                    )
                    .into(),
                );
            }
            for mut attribute in attributes {
                attribute.set_start_index(start);
                attribute.set_end_index(end);
                list.insert(attribute);
            }
        }
        cell.set_attributes(Some(&list));
    }

    fn reveal(&mut self) {
        let Some(&hit) = self.hits.get(self.active) else {
            return;
        };
        let pieces = self.pieces(hit);
        self.tag(&pieces, "mdview-search-active", true);
        if let Some(piece) = pieces.first() {
            for (view, offset) in &piece.parents {
                reveal_offset(view, *offset);
            }
            let outer = piece.parents.first().map(|(view, _)| view.clone());
            match &piece.target {
                Target::Cell(cell) => reveal_in(cell.upcast_ref(), None, outer),
                Target::Text(view) | Target::Widget(view, _) => {
                    reveal_offset(view, piece.start);
                    // A full-height block cannot scroll itself; the document must.
                    let full = view
                        .parent()
                        .and_downcast::<gtk::ScrolledWindow>()
                        .is_some_and(|s| s.vscrollbar_policy() == gtk::PolicyType::Never);
                    if full {
                        reveal_in(view.upcast_ref(), Some(piece.start), outer);
                    }
                }
                Target::Gap => {}
            }
        }
    }

    pub fn step(&mut self, backwards: bool) {
        if self.hits.is_empty() {
            return;
        }
        self.tag(
            &self.pieces(self.hits[self.active]),
            "mdview-search-active",
            false,
        );
        self.active =
            (self.active + if backwards { self.hits.len() - 1 } else { 1 }) % self.hits.len();
        self.reveal();
    }

    pub fn restore_active(&mut self, index: usize) {
        if self.hits.is_empty() {
            return;
        }
        self.tag(
            &self.pieces(self.hits[self.active]),
            "mdview-search-active",
            false,
        );
        self.active = index.min(self.hits.len() - 1);
        self.tag(
            &self.pieces(self.hits[self.active]),
            "mdview-search-active",
            true,
        );
    }

    pub fn position(&self) -> (usize, usize) {
        (
            if self.hits.is_empty() {
                0
            } else {
                self.active + 1
            },
            self.hits.len(),
        )
    }
}

fn reveal_offset(view: &gtk::TextView, offset: i32) {
    let buffer = view.buffer();
    let at = buffer.iter_at_offset(offset);
    let mark = buffer
        .mark("mdview-search-reveal")
        .unwrap_or_else(|| buffer.create_mark(Some("mdview-search-reveal"), &at, true));
    buffer.move_mark(&mark, &at);
    // Marks let GTK validate lines that have not been laid out yet. Iter-based
    // scrolling can settle at an estimated location in multi-megabyte buffers.
    view.scroll_to_mark(&mark, 0.1, true, 0.5, 0.5);
}

/// Scrolls the document `outer` so `widget` (or, for a text view, the
/// character at `offset`) is centred, once layout has settled. Used for
/// content that cannot scroll itself: table cells and full-height blocks.
fn reveal_in(widget: &gtk::Widget, offset: Option<i32>, outer: Option<gtk::TextView>) {
    let Some(outer) = outer else {
        return;
    };
    let widget = widget.clone();
    let last = std::cell::Cell::new(None);
    let ticks = std::cell::Cell::new(0);
    outer.add_tick_callback(move |outer, _| {
        ticks.set(ticks.get() + 1);
        let (x, y, height) = match (offset, widget.downcast_ref::<gtk::TextView>()) {
            (Some(offset), Some(view)) => {
                let rect = view.iter_location(&view.buffer().iter_at_offset(offset));
                let (x, y) =
                    view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
                (x as f32, y as f32, rect.height())
            }
            _ => (0.0, 0.0, widget.height()),
        };
        let point = widget.compute_point(outer, &gtk::graphene::Point::new(x, y));
        let value = outer.vadjustment().map_or(0, |a| a.value() as i64);
        let now = point.map(|p| (p.y() as i32, value));
        // Wait until the document has applied its own pending scroll.
        if now.is_none() || now != last.replace(now) {
            return if ticks.get() < 40 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            };
        }
        let point = point.unwrap();
        // A wide table scrolls sideways inside its own frame.
        if offset.is_none()
            && let Some(scroller) = widget
                .ancestor(gtk::ScrolledWindow::static_type())
                .and_downcast::<gtk::ScrolledWindow>()
            && let Some(content) = scroller.child()
            && let Some(at) = widget.compute_point(&content, &gtk::graphene::Point::new(0.0, 0.0))
        {
            let a = scroller.hadjustment();
            let (left, right) = (at.x() as f64, (at.x() + widget.width() as f32) as f64);
            if left < a.value() || right > a.value() + a.page_size() {
                a.set_value((left - 24.0).clamp(a.lower(), a.upper() - a.page_size()));
            }
        }
        if let Some(a) = outer.vadjustment() {
            let target = a.value() + point.y() as f64 - (a.page_size() - height as f64) / 2.0;
            a.set_value(target.clamp(a.lower(), (a.upper() - a.page_size()).max(a.lower())));
        }
        glib::ControlFlow::Break
    });
}

/// Unicode case folding on a worker, retaining original character offsets even
/// for expansions such as ß -> ss. GTK objects never cross the thread boundary.
/// The opaque hit background and the text color for the active hit on it.
///
/// Prefers the theme's solid accent pair. Some themes define selection as a
/// translucent tint whose foreground is the tint's own hue, which becomes
/// invisible once the tint is made opaque, so the pair is also checked for
/// contrast and falls back to black or white text.
// GTK 4.10 has no replacement for resolving named theme colors.
#[allow(deprecated)]
pub(crate) fn highlight_colors(widget: &impl IsA<gtk::Widget>) -> (gtk::gdk::RGBA, gtk::gdk::RGBA) {
    let context = widget.style_context();
    let pair = |bg: &str, fg: &str| Some((context.lookup_color(bg)?, context.lookup_color(fg)));
    let (mut background, foreground) = pair("accent_bg_color", "accent_fg_color")
        .or_else(|| pair("theme_selected_bg_color", "theme_selected_fg_color"))
        .unwrap_or_else(|| (widget.as_ref().color(), None));
    background.set_alpha(1.0);
    let foreground = foreground
        .map(|mut color| {
            color.set_alpha(1.0);
            color
        })
        .filter(|color| contrast(color, &background) >= 4.5)
        .unwrap_or_else(|| {
            let (black, white) = (gtk::gdk::RGBA::BLACK, gtk::gdk::RGBA::WHITE);
            if contrast(&black, &background) >= contrast(&white, &background) {
                black
            } else {
                white
            }
        });
    (background, foreground)
}

/// WCAG contrast ratio between two opaque colors.
fn contrast(a: &gtk::gdk::RGBA, b: &gtk::gdk::RGBA) -> f32 {
    let luminance = |color: &gtk::gdk::RGBA| {
        let linear = |value: f32| {
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.red())
            + 0.7152 * linear(color.green())
            + 0.0722 * linear(color.blue())
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn match_ranges(text: &str, query: &str, cancelled: &AtomicBool) -> Vec<(i32, i32)> {
    let query = glib::casefold(query);
    if query.is_empty() {
        return Vec::new();
    }
    // Stream folded characters through KMP instead of duplicating the entire
    // document and retaining an offset for every folded character. Only the
    // query-sized history is needed to map a completed match back to GTK.
    let query: Vec<char> = query.chars().collect();
    let mut prefix = vec![0; query.len()];
    let mut matched = 0;
    for index in 1..query.len() {
        while matched > 0 && query[index] != query[matched] {
            matched = prefix[matched - 1];
        }
        if query[index] == query[matched] {
            matched += 1;
        }
        prefix[index] = matched;
    }
    let mut offsets = vec![0; query.len()];
    let (mut matched, mut folded_count, mut previous_end) = (0, 0, 0);
    let mut result = Vec::new();
    let mut feed = |ch: char, original: i32| {
        offsets[folded_count % query.len()] = original;
        folded_count += 1;
        while matched > 0 && ch != query[matched] {
            matched = prefix[matched - 1];
        }
        if ch == query[matched] {
            matched += 1;
        }
        if matched == query.len() {
            let start = offsets[(folded_count - query.len()) % query.len()];
            let end = original + 1;
            if start >= previous_end {
                result.push((start, end));
                previous_end = end;
            }
            // Literal search is nonoverlapping in the folded text as well as
            // the original text (including partial case-fold expansions).
            matched = 0;
        }
    };
    for (index, ch) in text.chars().enumerate() {
        if index % 8192 == 0 && cancelled.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if ch.is_ascii() {
            feed(ch.to_ascii_lowercase(), index as i32);
        } else {
            for folded in glib::casefold(ch.to_string()).chars() {
                feed(folded, index as i32);
            }
        }
    }
    result
}

impl Drop for Search {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn same_hue_selection_text_fails_contrast() {
        let rgba = |s: &str| gtk::gdk::RGBA::parse(s).unwrap();
        // A theme's translucent-tint selection: text in the tint's own hue.
        assert!(super::contrast(&rgba("#ffb3b6"), &rgba("#ffb3b6")) < 1.01);
        assert!(super::contrast(&rgba("#67001e"), &rgba("#ffb3b6")) >= 4.5);
        assert!(
            (super::contrast(&gtk::gdk::RGBA::BLACK, &gtk::gdk::RGBA::WHITE) - 21.0).abs() < 0.01
        );
    }

    fn match_ranges(text: &str, query: &str) -> Vec<(i32, i32)> {
        super::match_ranges(text, query, &std::sync::atomic::AtomicBool::new(false))
    }

    #[test]
    fn unicode_fold_offsets_and_dense_results() {
        assert_eq!(
            match_ranges("Café ß STRASSE فارسی 日本語", "CAFÉ"),
            [(0, 4)]
        );
        assert_eq!(match_ranges("ß STRASSE", "ss"), [(0, 1), (6, 8)]);
        assert_eq!(match_ranges("ßs", "s"), [(0, 1), (1, 2)]);
        assert_eq!(match_ranges("aaaaa", "aa"), [(0, 2), (2, 4)]);
        assert_eq!(match_ranges("ababababac", "ababac"), [(4, 10)]);
        assert_eq!(match_ranges("aaaabaaaab", "aaab"), [(1, 5), (6, 10)]);
        assert_eq!(match_ranges("sßß", "ss"), [(0, 2)]);
        assert_eq!(match_ranges("İi", "i"), [(0, 1), (1, 2)]);
        assert!(match_ranges("short", "longer query").is_empty());
        assert!(match_ranges("abc", "").is_empty());
        assert_eq!(match_ranges(&"a ".repeat(20_000), "A").len(), 20_000);
    }
}

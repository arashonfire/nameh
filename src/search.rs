//! Search rendered text in reading order, mapping hits back to native buffers.
use crate::document_view::DocumentView;
use gtk::{gio, glib, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct Span {
    start: i32,
    end: i32,
    view: gtk::TextView,
    offset: i32,
    atomic: bool,
    widget: Option<gtk::Widget>,
    parents: Vec<(gtk::TextView, i32)>,
}

#[derive(Clone)]
struct Piece {
    widget: Option<gtk::Widget>,
    view: gtk::TextView,
    start: i32,
    end: i32,
    parents: Vec<(gtk::TextView, i32)>,
}

#[derive(Default)]
pub struct Search {
    hits: Vec<(i32, i32)>,
    spans: Vec<Span>,
    buffers: Vec<gtk::TextBuffer>,
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
            if let Some(child) = child {
                let mut parents = parents.to_vec();
                parents.push((view.clone(), at));
                collect(&child, &parents, text, spans).await;
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
                    Span {
                        start: 0,
                        end: 0,
                        view: view.clone(),
                        offset: at,
                        atomic: true,
                        widget: widget.clone(),
                        parents: parents.to_vec(),
                    },
                );
            }
            offset = at + 1;
        }
        collect_text(view, parents, offset, buffer.char_count(), text, spans).await;
    })
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
            Span {
                start: 0,
                end: 0,
                view: view.clone(),
                offset,
                atomic: false,
                widget: None,
                parents: parents.to_vec(),
            },
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
            Self::tag(&self.pieces(*hit), "mdview-search-active", false);
        }
        for hit in self.hits.iter().take(1000) {
            for piece in self.pieces(*hit) {
                if let Some(widget) = &piece.widget {
                    widget.remove_css_class("mdview-search-hit");
                    widget.remove_css_class("mdview-search-active");
                }
            }
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
            let buffer = span.view.buffer();
            if self.buffers.contains(&buffer) {
                continue;
            }
            let context = span.view.style_context();
            let color = context
                .lookup_color("theme_selected_bg_color")
                .or_else(|| context.lookup_color("accent_bg_color"))
                .unwrap_or_else(|| span.view.color());
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
                    tag.set_foreground_rgba(
                        context.lookup_color("theme_selected_fg_color").as_ref(),
                    );
                }
            }
            self.buffers.push(buffer);
        }
        self.spans = spans;
        // Bound native tag segments. Count/navigation still include every hit.
        for (index, hit) in self.hits.iter().take(1000).enumerate() {
            Self::tag(&self.pieces(*hit), "mdview-search", true);
            if index % 100 == 99 {
                glib::timeout_future(std::time::Duration::from_millis(1)).await;
            }
        }
        if reveal {
            self.reveal();
        } else if let Some(hit) = self.hits.first() {
            Self::tag(&self.pieces(*hit), "mdview-search-active", true);
        }
    }

    fn pieces(&self, (start, end): (i32, i32)) -> Vec<Piece> {
        let begin = self.spans.partition_point(|s| s.end <= start);
        self.spans[begin..]
            .iter()
            .take_while(|s| s.start < end)
            .map(|s| Piece {
                widget: s.widget.clone(),
                view: s.view.clone(),
                start: s.offset
                    + if s.atomic {
                        0
                    } else {
                        (start - s.start).max(0)
                    },
                end: s.offset
                    + if s.atomic {
                        1
                    } else {
                        (end.min(s.end) - s.start).max(0)
                    },
                parents: s.parents.clone(),
            })
            .collect()
    }

    fn tag(hit: &[Piece], name: &str, apply: bool) {
        for piece in hit {
            if let Some(widget) = &piece.widget {
                let class = if name.ends_with("active") {
                    "mdview-search-active"
                } else {
                    "mdview-search-hit"
                };
                if apply {
                    widget.add_css_class(class);
                } else {
                    widget.remove_css_class(class);
                }
            }
            let buffer = piece.view.buffer();
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

    fn reveal(&self) {
        let Some(hit) = self.hits.get(self.active) else {
            return;
        };
        let pieces = self.pieces(*hit);
        Self::tag(&pieces, "mdview-search-active", true);
        if let Some(piece) = pieces.first() {
            for (view, offset) in &piece.parents {
                reveal_offset(view, *offset);
            }
            reveal_offset(&piece.view, piece.start);
        }
    }

    pub fn step(&mut self, backwards: bool) {
        if self.hits.is_empty() {
            return;
        }
        Self::tag(
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
        Self::tag(
            &self.pieces(self.hits[self.active]),
            "mdview-search-active",
            false,
        );
        self.active = index.min(self.hits.len() - 1);
        Self::tag(
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

/// Unicode case folding on a worker, retaining original character offsets even
/// for expansions such as ß -> ss. GTK objects never cross the thread boundary.
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

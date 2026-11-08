//! Owned Markdown tree with UTF-8 byte ranges into the original source.
//!
//! Only tables, task lists and strikethrough extensions are enabled. Tight list
//! items may contain inline nodes directly; loose items contain paragraphs.
//! HTML events are discarded, including the contents of raw HTML blocks.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use std::{collections::HashSet, ops::Range};

mod links;
pub use links::{ExternalScheme, LinkTarget, LocalTarget, classify_link};

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub source: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Half-open byte range, not a character or rendered-text offset.
    pub source: Range<usize>,
    pub kind: NodeKind,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Paragraph,
    Heading {
        level: u8,
        id: String,
    },
    BlockQuote,
    List {
        start: Option<u64>,
    },
    Item,
    CodeBlock {
        info: String,
    },
    Table {
        alignments: Vec<Alignment>,
    },
    /// Header cells are direct children; body rows use TableRow.
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    /// None means inert label text. Never dispatch the original source slice.
    Link {
        target: Option<LinkTarget>,
        title: String,
    },
    /// Only local paths can be image sources. Children contain the alt text.
    Image {
        source: Option<LocalTarget>,
        title: String,
    },
    Text(String),
    Code(String),
    SoftBreak,
    HardBreak,
    Rule,
    TaskMarker(bool),
}

impl Node {
    /// Inline text for heading names and image descriptions, without markup.
    pub fn text(&self) -> String {
        let mut text = String::new();
        let mut pending = vec![self];
        while let Some(node) = pending.pop() {
            match &node.kind {
                NodeKind::Text(value) | NodeKind::Code(value) => text.push_str(value),
                NodeKind::SoftBreak | NodeKind::HardBreak => text.push(' '),
                _ => {}
            }
            pending.extend(node.children.iter().rev());
        }
        text
    }
}

/// Parse without GTK, filesystem access, URI dispatch or network activity.
pub fn parse(source: &str) -> Document {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut roots = Vec::new();
    // A None frame discards an HTML block, including all of its descendants.
    let mut stack: Vec<Option<Node>> = Vec::new();
    let mut heading_ids = HashSet::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                stack.push(kind_for_tag(tag).map(|kind| Node {
                    source: range,
                    kind,
                    // Most block containers contain a single text node. Avoid
                    // Vec's four-node first allocation for every paragraph in
                    // multi-megabyte documents; it fragments worker arenas.
                    children: Vec::with_capacity(1),
                }));
            }
            Event::End(_) => {
                if let Some(Some(mut node)) = stack.pop() {
                    node.source.end = range.end;
                    if matches!(node.kind, NodeKind::Heading { .. }) {
                        let id = unique_heading_id(&node.text(), &mut heading_ids);
                        if let NodeKind::Heading { id: heading_id, .. } = &mut node.kind {
                            *heading_id = id;
                        }
                    }
                    append(node, &mut stack, &mut roots);
                }
            }
            event => {
                let kind = match event {
                    Event::Text(text) => NodeKind::Text(text.into_string()),
                    Event::Code(text) => NodeKind::Code(text.into_string()),
                    Event::SoftBreak => NodeKind::SoftBreak,
                    Event::HardBreak => NodeKind::HardBreak,
                    Event::Rule => NodeKind::Rule,
                    Event::TaskListMarker(checked) => NodeKind::TaskMarker(checked),
                    // HTML has no representation. Other extensions are disabled.
                    _ => continue,
                };
                append(
                    Node {
                        source: range,
                        kind,
                        children: Vec::new(),
                    },
                    &mut stack,
                    &mut roots,
                );
            }
        }
    }
    Document {
        source: source.to_owned(),
        children: roots,
    }
}

fn append(node: Node, stack: &mut [Option<Node>], roots: &mut Vec<Node>) {
    match stack.last_mut() {
        Some(Some(parent)) => parent.children.push(node),
        Some(None) => {}
        None => roots.push(node),
    }
}

fn kind_for_tag(tag: Tag<'_>) -> Option<NodeKind> {
    Some(match tag {
        Tag::Paragraph => NodeKind::Paragraph,
        Tag::Heading { level, .. } => NodeKind::Heading {
            level: level as u8,
            id: String::new(),
        },
        Tag::BlockQuote(_) => NodeKind::BlockQuote,
        Tag::List(start) => NodeKind::List { start },
        Tag::Item => NodeKind::Item,
        Tag::CodeBlock(kind) => NodeKind::CodeBlock {
            info: match kind {
                CodeBlockKind::Indented => String::new(),
                CodeBlockKind::Fenced(info) => info.into_string(),
            },
        },
        Tag::Table(alignments) => NodeKind::Table {
            alignments: alignments
                .into_iter()
                .map(|value| match value {
                    pulldown_cmark::Alignment::None => Alignment::None,
                    pulldown_cmark::Alignment::Left => Alignment::Left,
                    pulldown_cmark::Alignment::Center => Alignment::Center,
                    pulldown_cmark::Alignment::Right => Alignment::Right,
                })
                .collect(),
        },
        Tag::TableHead => NodeKind::TableHead,
        Tag::TableRow => NodeKind::TableRow,
        Tag::TableCell => NodeKind::TableCell,
        Tag::Emphasis => NodeKind::Emphasis,
        Tag::Strong => NodeKind::Strong,
        Tag::Strikethrough => NodeKind::Strikethrough,
        Tag::Link {
            dest_url, title, ..
        } => NodeKind::Link {
            target: classify_link(&dest_url),
            title: title.into_string(),
        },
        Tag::Image {
            dest_url, title, ..
        } => NodeKind::Image {
            source: match classify_link(&dest_url) {
                Some(LinkTarget::Local(local)) => Some(local),
                _ => None,
            },
            title: title.into_string(),
        },
        _ => return None,
    })
}

/// Lowercase Unicode alphanumerics, preserve '_' and '-', drop punctuation,
/// collapse whitespace to '-', use 'section' when empty. IDs are globally
/// unique in source order, suffixing '-1', '-2', etc. (even for literal suffixes).
fn unique_heading_id(text: &str, used: &mut HashSet<String>) -> String {
    let mut base = String::new();
    let mut space = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_whitespace() {
            space = !base.is_empty();
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            if space {
                base.push('-');
            }
            space = false;
            base.push(ch);
        }
    }
    if base.is_empty() {
        base.push_str("section");
    }
    let mut candidate = base.clone();
    let mut suffix = 1;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

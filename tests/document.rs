use mdview::document::{
    Alignment, Document, ExternalScheme, LinkTarget, Node, NodeKind, classify_link, parse,
};

fn nodes(document: &Document) -> Vec<&Node> {
    let mut result = Vec::new();
    let mut pending: Vec<_> = document.children.iter().rev().collect();
    while let Some(node) = pending.pop() {
        result.push(node);
        pending.extend(node.children.iter().rev());
    }
    result
}

#[test]
fn nesting_lists_quotes_and_tasks() {
    let doc = parse(
        "> 3. **Bold and *italic***\n>    - [x] ~~done~~\n>    - [ ] pending\n>\n>    second paragraph\n",
    );
    let all = nodes(&doc);
    assert!(matches!(doc.children[0].kind, NodeKind::BlockQuote));
    let list = &doc.children[0].children[0];
    assert!(matches!(list.kind, NodeKind::List { start: Some(3) }));
    assert!(matches!(list.children[0].kind, NodeKind::Item));
    let strong = all.iter().find(|n| n.kind == NodeKind::Strong).unwrap();
    assert!(strong.children.iter().any(|n| n.kind == NodeKind::Emphasis));
    assert!(all.iter().any(|n| n.kind == NodeKind::List { start: None }));
    assert!(all.iter().any(|n| n.kind == NodeKind::Strikethrough));
    assert!(all.iter().any(|n| n.kind == NodeKind::TaskMarker(true)));
    assert!(all.iter().any(|n| n.kind == NodeKind::TaskMarker(false)));
}

#[test]
fn tables_preserve_cells_alignment_and_inline_formatting() {
    let doc =
        parse("| A | B | C | D |\n| :--- | :---: | ---: | --- |\n| **x** | `y` | [z](#a) | |\n");
    let table = &doc.children[0];
    assert_eq!(
        table.kind,
        NodeKind::Table {
            alignments: vec![
                Alignment::Left,
                Alignment::Center,
                Alignment::Right,
                Alignment::None
            ]
        }
    );
    assert_eq!(table.children[0].kind, NodeKind::TableHead);
    assert_eq!(table.children[0].children.len(), 4);
    let row = &table.children[1];
    assert_eq!(row.kind, NodeKind::TableRow);
    assert_eq!(row.children.len(), 4);
    assert!(row.children.iter().all(|n| n.kind == NodeKind::TableCell));
    assert_eq!(row.children[0].children[0].kind, NodeKind::Strong);
    assert_eq!(row.children[1].children[0].kind, NodeKind::Code("y".into()));
    assert!(row.children[3].children.is_empty());
}

#[test]
fn headings_are_deterministic_unicode_and_collision_safe() {
    let source = "# Hello *WORLD*!\n# Hello world\n# Hello world-1\n# Hello world\n# !!!\n# 🐧\n# Café 日本語 فارسی\n# [A](#b) `B` ![C](x.png)\nTitle\n=====\n";
    let doc = parse(source);
    let headings: Vec<_> = doc
        .children
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Heading { level, id } => (*level, id.as_str()),
            _ => panic!("expected heading"),
        })
        .collect();
    assert_eq!(
        headings,
        vec![
            (1, "hello-world"),
            (1, "hello-world-1"),
            (1, "hello-world-1-1"),
            (1, "hello-world-2"),
            (1, "section"),
            (1, "section-1"),
            (1, "café-日本語-فارسی"),
            (1, "a-b-c"),
            (1, "title")
        ]
    );
    assert_eq!(doc, parse(source));
    let levels = parse("# a\n## b\n### c\n#### d\n##### e\n###### f\n");
    for (index, node) in levels.children.iter().enumerate() {
        assert!(
            matches!(node.kind, NodeKind::Heading { level, .. } if usize::from(level) == index + 1)
        );
    }
}

#[test]
fn code_breaks_rules_and_source_byte_ranges() {
    let source = "é &amp; *日本語*\nsoft  \nhard\n\n---\n\n```rust extra\n  <script>x</script>\n```\n\n    indented\n";
    let doc = parse(source);
    let all = nodes(&doc);
    assert!(all.iter().any(|n| n.kind == NodeKind::SoftBreak));
    assert!(all.iter().any(|n| n.kind == NodeKind::HardBreak));
    assert!(all.iter().any(|n| n.kind == NodeKind::Rule));
    let code = all
        .iter()
        .find(|n| {
            n.kind
                == NodeKind::CodeBlock {
                    info: "rust extra".into(),
                }
        })
        .unwrap();
    assert_eq!(code.text(), "  <script>x</script>\n");
    assert!(all.iter().any(|n| n.kind
        == NodeKind::CodeBlock {
            info: String::new()
        }
        && n.text() == "indented\n"));
    let emphasis = all.iter().find(|n| n.kind == NodeKind::Emphasis).unwrap();
    assert_eq!(&doc.source[emphasis.source.clone()], "*日本語*");
    for node in all {
        assert!(doc.source.get(node.source.clone()).is_some());
        for child in &node.children {
            assert!(node.source.start <= child.source.start && child.source.end <= node.source.end);
        }
    }
}

#[test]
fn html_is_removed_without_losing_surrounding_markdown() {
    let doc = parse(
        "before <b>ordinary **text**</b> after\n\n<script>\nalert('gone');\n</script>\n\n![hidden](javascript:evil)\n\nend\n",
    );
    assert_eq!(doc.children[0].text(), "before ordinary text after");
    assert_eq!(doc.children.last().unwrap().text(), "end");
    assert!(!nodes(&doc).iter().any(|n| n.text().contains("alert")));
    assert!(
        nodes(&doc)
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Image { source: None, .. }))
    );
}

#[test]
fn allowed_links_keep_encoded_paths_and_fragments() {
    for uri in [
        "https://example.org/a#b",
        "HTTP://example.org",
        "mailto:user@example.org?subject=Hello",
    ] {
        assert!(
            matches!(classify_link(uri), Some(LinkTarget::External { .. })),
            "{uri}"
        );
    }
    assert!(matches!(
        classify_link("HTTPS://example.org"),
        Some(LinkTarget::External {
            scheme: ExternalScheme::Https,
            ..
        })
    ));
    assert_eq!(
        classify_link("#café"),
        Some(LinkTarget::Heading("café".into()))
    );
    for path in [
        "../notes/Unicode%20caf%C3%A9.md",
        "/tmp/a.md",
        "notes/a b.md",
        "./name:part.md",
        "",
        "%6aavascript%3aevil",
    ] {
        let Some(LinkTarget::Local(local)) = classify_link(path) else {
            panic!("{path}");
        };
        assert_eq!(local.path, path);
    }
    let Some(LinkTarget::Local(local)) = classify_link("notes.md#hello%20world") else {
        panic!();
    };
    assert_eq!(local.path, "notes.md");
    assert_eq!(local.fragment.as_deref(), Some("hello%20world"));
}

#[test]
fn unsafe_and_ambiguous_links_are_rejected() {
    for uri in [
        "javascript:alert(1)",
        "JaVaScRiPt:evil",
        "data:text/html,x",
        "file:///tmp/a",
        "ftp://host",
        "tel:123",
        "unknown:stuff",
        "//host/image",
        "\\\\host\\image",
        "https:\\host",
        " https://host",
        "https://host ",
        "java\nscript:x",
        "https://host/\0x",
        "https://",
        "http:host",
        "mailto:",
        "https://host/a b",
        "a%",
        "a%gg",
        "a%0a",
        "a%00",
        "a%5c",
    ] {
        assert!(classify_link(uri).is_none(), "accepted {uri:?}");
    }
}

#[test]
fn parser_classifies_references_autolinks_entities_and_images() {
    let doc = parse(
        "[safe][ref] <https://example.org> [bad](jav&#x61;script:evil) [bad2](data:text/html,x)\n\n[ref]: notes.md#title \"Title\"\n\n![local *alt*](assets/x.png \"Image\") ![remote](https://host/x.png) ![network](//host/x.png) ![data](data:image/png,x)\n",
    );
    let all = nodes(&doc);
    let links: Vec<_> = all
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Link { target, title } => Some((target, title)),
            _ => None,
        })
        .collect();
    assert_eq!(links.len(), 4);
    assert!(matches!(links[0].0, Some(LinkTarget::Local(_))));
    assert_eq!(links[0].1, "Title");
    assert!(matches!(links[1].0, Some(LinkTarget::External { .. })));
    assert!(links[2].0.is_none() && links[3].0.is_none());
    let images: Vec<_> = all
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Image { .. }))
        .collect();
    assert_eq!(images.len(), 4);
    assert_eq!(images[0].text(), "local alt");
    assert!(
        matches!(&images[0].kind, NodeKind::Image { source: Some(_), title } if title == "Image")
    );
    assert!(
        images[1..]
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Image { source: None, .. }))
    );
}

#[test]
fn malformed_markdown_and_fixtures_remain_parseable() {
    for source in [
        "",
        "[unfinished](",
        "**unclosed *text",
        "```rust\nunfinished\n",
        "|bad|\n|--",
        "![broken",
        "<script>unfinished",
        include_str!("../fixtures/basic.md"),
        include_str!("../fixtures/rendering.md"),
        include_str!("../fixtures/prototype.md"),
    ] {
        let doc = parse(source);
        assert_eq!(doc.source, source);
        for node in nodes(&doc) {
            assert!(source.get(node.source.clone()).is_some());
        }
    }
    let code = parse("```rust\nunfinished\n");
    assert_eq!(code.children[0].text(), "unfinished\n");
    assert_eq!(parse("[unfinished](").children[0].text(), "[unfinished](");
}

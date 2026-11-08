//! Runs on the existing single GTK test thread with real GIO notifications.
use super::*;
use std::{
    cell::Cell,
    fs,
    time::{Duration, Instant},
};

fn pump(ms: u64) {
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(ms)));
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "reload did not settle");
        pump(10);
    }
}

fn text(controller: &DocumentWindow) -> String {
    let view = controller.state.borrow().view.clone().unwrap();
    let buffer = view.buffer();
    let mut text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    for (_, block) in view.embedded() {
        text.push_str(&block);
    }
    text
}

pub fn verify_reload(application: &gtk::Application) {
    let dir = std::env::temp_dir().join(format!("mdview-reload-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("watched café.md");
    let source: String = (0..120)
        .map(|i| format!("# Section {i}\n\nParagraph number {i}.\n\n"))
        .collect();
    fs::write(&path, &source).unwrap();
    let controller = DocumentWindow::new(
        application,
        false,
        Rc::new(files::launch),
        crate::reading::ReadingSettings::new(Some(dir.join("prefs.ini"))),
    );
    controller.open_file(gio::File::for_path(&path));
    wait_for_open(&controller);
    pump(100);
    let old = controller.state.borrow().view.clone().unwrap();
    controller
        .window
        .lookup_action("find")
        .unwrap()
        .activate(None);
    controller.search_entry.set_text("Paragraph");
    until(|| controller.search_count.text() != "Searching…");
    old.go_to_heading("section-65");
    pump(350);
    let top = old.line_at_y(old.visible_rect().y()).0.line();
    assert!(top > 100, "test must start well into the document");
    let old_monitor = controller.monitor.borrow().clone().unwrap();
    fs::write(
        &path,
        format!("# Inserted\n\nextra\n\n{source}\nordinary-save"),
    )
    .unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("ordinary-save"));
    pump(350);
    let view = controller.state.borrow().view.clone().unwrap();
    let now = view.line_at_y(view.visible_rect().y()).0.line();
    assert!(
        (now - top - 4).abs() <= 3,
        "heading-relative viewport lost: {top} -> {now}"
    );
    until(|| controller.search_count.text() != "Searching…");
    assert_eq!(controller.search.borrow().position().1, 120);
    controller
        .window
        .lookup_action("hide-search")
        .unwrap()
        .activate(None);
    assert!(old_monitor.is_cancelled(), "old monitors must be cancelled");
    drop(view);
    drop(old);
    let before_burst = controller.state.borrow().view.clone().unwrap();
    for i in 0..20 {
        let temp = dir.join("replacement.tmp");
        fs::write(&temp, format!("{source}\natomic-{i}")).unwrap();
        fs::rename(temp, &path).unwrap();
    }
    pump(75);
    assert_eq!(
        controller.state.borrow().view.as_ref(),
        Some(&before_burst),
        "burst must wait for its quiet period"
    );
    until(|| !controller.state.borrow().loading && text(&controller).contains("atomic-19"));
    let retired = before_burst.downgrade();
    drop(before_burst);
    pump(300);
    assert!(
        retired.upgrade().is_none(),
        "retired document resources must be released"
    );
    pump(200);
    assert!(
        controller.reload_timer.borrow().is_none(),
        "burst timer must stop at idle"
    );
    fs::remove_file(&path).unwrap();
    until(|| !controller.state.borrow().loading && controller.retry.is_enabled());
    assert!(
        text(&controller).contains("atomic-19"),
        "deletion must preserve the view"
    );
    fs::write(&path, [0xff, 0xfe]).unwrap();
    until(|| controller.status.text().contains("UTF-8"));
    assert!(text(&controller).contains("atomic-19"));
    fs::write(&path, "# Recreated\n\nrecovery").unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("recovery"));
    assert!(!controller.notice.reveals_child());
    fs::rename(&path, dir.join("moved.md")).unwrap();
    until(|| controller.retry.is_enabled());
    fs::write(&path, "# Retry\n\nretry recovery").unwrap();
    controller.retry.activate(None);
    until(|| !controller.state.borrow().loading && text(&controller).contains("retry recovery"));
    assert!(text(&controller).contains("retry recovery"));

    // Symlinks retain logical replacement monitoring and follow target saves.
    let target_dir = dir.join("targets");
    fs::create_dir_all(&target_dir).unwrap();
    let target = target_dir.join("target.md");
    fs::write(&target, "symlink initial").unwrap();
    let linked = dir.join("linked.md");
    std::os::unix::fs::symlink(&target, &linked).unwrap();
    controller.open_file(gio::File::for_path(&linked));
    wait_for_open(&controller);
    assert_eq!(controller.target_monitors.borrow().len(), 1);
    fs::write(&target, "symlink target update").unwrap();
    until(|| {
        !controller.state.borrow().loading && text(&controller).contains("symlink target update")
    });
    let replacement = dir.join("logical replacement");
    fs::write(&replacement, "logical replacement update").unwrap();
    fs::rename(replacement, &linked).unwrap();
    until(|| {
        !controller.state.borrow().loading
            && text(&controller).contains("logical replacement update")
    });
    assert!(controller.target_monitors.borrow().is_empty());
    controller.open_file(gio::File::for_path(&path));
    wait_for_open(&controller);
    // Relative navigation retains '..' semantics while watching the actual entry.
    fs::create_dir_all(dir.join("sub")).unwrap();
    controller.perform(Request::Follow {
        document_path: path.clone(),
        target: mdview::document::classify_link("sub/../watched%20caf%C3%A9.md").unwrap(),
    });
    wait_for_open(&controller);
    fs::write(&path, "relative parent save").unwrap();
    until(|| {
        !controller.state.borrow().loading && text(&controller).contains("relative parent save")
    });

    // Removing all heading IDs exercises proportional fallback.
    fs::write(&path, &source).unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("Section 119"));
    let view = controller.state.borrow().view.clone().unwrap();
    view.go_to_heading("section-60");
    pump(350);
    let adjust = view.vadjustment().unwrap();
    let fraction = adjust.value() / (adjust.upper() - adjust.page_size());
    let no_headings = source.replace("# Section", "Plain section");
    fs::write(&path, &no_headings).unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("Plain section 119"));
    pump(350);
    let view = controller.state.borrow().view.clone().unwrap();
    let adjust = view.vadjustment().unwrap();
    let restored = adjust.value() / (adjust.upper() - adjust.page_size());
    assert!(
        (fraction - restored).abs() < 0.04,
        "proportional fallback drifted: {fraction} -> {restored}"
    );
    // An explicit open must start at the top even after a restored reload
    // viewport, with a document long enough to keep that previous offset.
    controller.open_file(gio::File::for_path(&path));
    wait_for_open(&controller);
    pump(200);
    assert!(
        controller
            .state
            .borrow()
            .view
            .as_ref()
            .unwrap()
            .vadjustment()
            .unwrap()
            .value()
            < 1.0,
        "explicit opening must start at the top"
    );
    // Subsequent navigation assertions use this known small source again.
    fs::write(&path, "# Retry\n\nretry recovery").unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("retry recovery"));

    let beats = Rc::new(Cell::new(0_u32));
    let worst = Rc::new(Cell::new(Duration::ZERO));
    let last = Rc::new(Cell::new(Instant::now()));
    let (b, w, l) = (beats.clone(), worst.clone(), last.clone());
    let heartbeat = glib::timeout_add_local(Duration::from_millis(10), move || {
        let now = Instant::now();
        w.set(w.get().max(now.duration_since(l.replace(now))));
        b.set(b.get() + 1);
        glib::ControlFlow::Continue
    });
    let large = dir.join("large.md");
    let paragraph =
        "A responsive document with café, فارسی and 日本語 for searching and reading.\n\n";
    let large_source = format!(
        "# Large\n\n{}\nlarge-end",
        paragraph.repeat(5_000_000 / paragraph.len() + 1)
    );
    assert!(large_source.len() >= 5_000_000);
    fs::write(&large, &large_source).unwrap();
    let before = beats.get();
    let input = Rc::new(Cell::new(false));
    let input_seen = input.clone();
    let weak = Rc::downgrade(&controller);
    glib::timeout_add_local_once(Duration::from_millis(30), move || {
        let this = weak.upgrade().unwrap();
        assert!(this.state.borrow().loading);
        this.window.lookup_action("find").unwrap().activate(None);
        this.search_entry.set_text("input while loading");
        this.window
            .lookup_action("hide-search")
            .unwrap()
            .activate(None);
        input_seen.set(true);
    });
    controller.open_file(gio::File::for_path(&large));
    until(|| !controller.state.borrow().loading);
    assert!(text(&controller).contains("large-end"));
    assert!(
        input.get(),
        "reading controls must handle input during loading"
    );
    assert!(
        beats.get() > before + 5,
        "loading must yield to the main loop"
    );
    controller
        .window
        .lookup_action("find")
        .unwrap()
        .activate(None);
    controller.search_entry.set_text("CAFÉ");
    until(|| controller.search_count.text() != "Searching…");
    assert_eq!(
        controller.search.borrow().position().1,
        large_source.matches("café").count()
    );
    // A new query cancels a pending search and can never apply stale matches.
    controller.search_entry.set_text("a");
    controller.search_entry.set_text("large-end");
    until(|| controller.search_count.text() != "Searching…");
    assert_eq!(controller.search.borrow().position(), (1, 1));
    pump(400);
    let view = controller.state.borrow().view.clone().unwrap();
    let top_line = view.line_at_y(view.visible_rect().y()).0.line();
    assert!(
        view.buffer().line_count() - top_line < 50,
        "last search hit must be near the viewport: top line {top_line} of {}",
        view.buffer().line_count()
    );
    drop(view);
    controller
        .window
        .lookup_action("hide-search")
        .unwrap()
        .activate(None);
    pump(150);
    eprintln!(
        "5 MB prose/search: heartbeat={} worst-main-loop-gap={:?}",
        beats.get() - before,
        worst.get()
    );
    assert!(
        worst.get() < Duration::from_millis(250),
        "main loop stalled: {:?}",
        worst.get()
    );

    worst.set(Duration::ZERO);
    last.set(Instant::now());
    let retired_prose = controller.state.borrow().view.as_ref().unwrap().downgrade();
    let retired_prose_buffer = controller
        .state
        .borrow()
        .view
        .as_ref()
        .unwrap()
        .buffer()
        .downgrade();
    let code_source = format!(
        "# Code\n\n```text\n{}code-end\n```",
        "code line café and preserved spaces     \n".repeat(5_000_000 / 40 + 1)
    );
    fs::write(&large, &code_source).unwrap();
    until(|| !controller.state.borrow().loading && text(&controller).contains("code-end"));
    pump(100);
    assert!(retired_prose.upgrade().is_none());
    assert!(retired_prose_buffer.upgrade().is_none());
    eprintln!("5 MB fenced code: worst-main-loop-gap={:?}", worst.get());
    assert!(
        worst.get() < Duration::from_millis(250),
        "code stalled main loop: {:?}",
        worst.get()
    );
    // Switching while a reload is queued must cancel it and release its monitor.
    let monitor = controller.monitor.borrow().clone().unwrap();
    let retired_code = controller.state.borrow().view.as_ref().unwrap().downgrade();
    fs::write(&large, "stale reload").unwrap();
    pump(30);
    controller.open_file(gio::File::for_path(&path));
    wait_for_open(&controller);
    pump(250);
    assert!(text(&controller).contains("retry recovery"));
    assert!(retired_code.upgrade().is_none());
    assert!(monitor.is_cancelled());
    let monitor = controller.monitor.borrow().clone().unwrap();
    controller.window.close();
    assert!(monitor.is_cancelled());
    assert!(controller.reload_timer.borrow().is_none());
    heartbeat.remove();
    fs::remove_dir_all(dir).unwrap();
}

use super::*;
use std::{cell::Cell, fs, time::Duration};

fn find_link(widget: &gtk::Widget, uri: &str) -> Option<gtk::LinkButton> {
    if let Some(button) = widget.downcast_ref::<gtk::LinkButton>()
        && button.uri() == uri
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(button) = find_link(&widget, uri) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

fn activate(controller: &Rc<DocumentWindow>, uri: &str) {
    let button = find_link(controller.window.upcast_ref(), uri)
        .unwrap_or_else(|| panic!("missing link {uri}"));
    assert!(button.emit_by_name::<bool>("activate-link", &[]));
    wait_for_open(controller);
}

fn copied(controller: &DocumentWindow) -> String {
    let state = controller.state.borrow();
    let view = state.view.as_ref().unwrap();
    view.plain_text(&view.buffer().start_iter(), &view.buffer().end_iter())
}

fn chooser(controller: &DocumentWindow) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|dialog| {
            dialog.title().as_deref() == Some("Open a Markdown file")
                && dialog.transient_for().as_ref() == Some(controller.window.upcast_ref())
        })
        .expect("Open must present a native file picker")
}

fn picker_path(widget: &gtk::Widget) -> Option<gtk::Entry> {
    if widget.widget_name() == "mdview-file-path" {
        return widget.clone().downcast().ok();
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(entry) = picker_path(&widget) {
            return Some(entry);
        }
        child = widget.next_sibling();
    }
    None
}

fn find_widget(
    root: &gtk::Widget,
    predicate: &dyn Fn(&gtk::Widget) -> bool,
) -> Option<gtk::Widget> {
    if predicate(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget(&widget, predicate) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn button(root: &gtk::Widget, label: &str) -> gtk::Button {
    find_widget(root, &|widget| {
        widget
            .downcast_ref::<gtk::Button>()
            .is_some_and(|button| button.label().as_deref() == Some(label))
    })
    .unwrap()
    .downcast()
    .unwrap()
}

fn rows(list: &gtk::ListView) -> Vec<String> {
    let model = list.model().unwrap();
    (0..model.n_items())
        .map(|i| {
            model
                .item(i)
                .and_downcast::<gtk::StringObject>()
                .unwrap()
                .string()
                .to_string()
        })
        .collect()
}

fn until(condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "picker operation did not settle"
        );
        glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(10)));
    }
}

pub fn verify_file_opening() {
    let application = gtk::Application::builder()
        .application_id("io.github.arash.mdview.TestWindow")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(gio::Cancellable::NONE).unwrap();
    let launched = Rc::new(RefCell::new(Vec::new()));
    let records = launched.clone();
    let fail = Rc::new(Cell::new(false));
    let failures = fail.clone();
    let controller = DocumentWindow::new(
        &application,
        false,
        Rc::new(move |_, target, done| {
            records.borrow_mut().push(target);
            done(if failures.get() {
                Err("Test desktop handler failed".into())
            } else {
                Ok(())
            });
        }),
        crate::reading::ReadingSettings::new(None),
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let fixtures = [
        root.join("basic.md"),
        root.join("rendering.md"),
        root.join("notes/Unicode café.md"),
    ];
    let original: Vec<_> = fixtures.iter().map(|p| fs::read(p).unwrap()).collect();
    // Exercise the actual Open action, chooser selection and callback for each
    // fixture. GTK must stay on this test thread, as with the renderer checks.
    for fixture in &fixtures {
        controller
            .window
            .lookup_action("open")
            .unwrap()
            .activate(None);
        let dialog = chooser(&controller);
        let path = picker_path(dialog.upcast_ref()).unwrap();
        path.set_text(&fixture.to_string_lossy());
        let context = glib::MainContext::default();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let generation = controller.state.borrow().generation;
        path.emit_activate();
        while controller.state.borrow().generation == generation {
            assert!(
                std::time::Instant::now() < deadline,
                "picker did not accept fixture"
            );
            context.block_on(glib::timeout_future(Duration::from_millis(10)));
        }
        wait_for_open(&controller);
        assert_eq!(
            controller.state.borrow().current_path.as_ref(),
            Some(fixture)
        );
    }
    let before = controller.state.borrow().view.clone();
    controller
        .window
        .lookup_action("open")
        .unwrap()
        .activate(None);
    let dialog = chooser(&controller);
    dialog.close();
    assert_eq!(controller.state.borrow().view, before);
    controller
        .window
        .lookup_action("open")
        .unwrap()
        .activate(None);
    let dialog = chooser(&controller);
    let path = picker_path(dialog.upcast_ref()).unwrap();
    let list = find_widget(dialog.upcast_ref(), &|widget| widget.is::<gtk::ListView>())
        .unwrap()
        .downcast::<gtk::ListView>()
        .unwrap();
    // The last opened fixture lives under notes; Up must return to fixtures.
    button(dialog.upcast_ref(), "Up").emit_clicked();
    until(|| rows(&list).contains(&"notes/".to_owned()));
    let notes = rows(&list)
        .iter()
        .position(|name| name == "notes/")
        .unwrap() as u32;
    list.emit_by_name::<()>("activate", &[&notes]);
    until(|| rows(&list).contains(&"Unicode café.md".to_owned()));
    button(dialog.upcast_ref(), "Up").emit_clicked();
    until(|| rows(&list).contains(&"assets/".to_owned()));
    let assets = rows(&list)
        .iter()
        .position(|name| name == "assets/")
        .unwrap() as u32;
    list.emit_by_name::<()>("activate", &[&assets]);
    until(|| path.text().ends_with("assets") && rows(&list).is_empty());
    let all = find_widget(dialog.upcast_ref(), &|widget| {
        widget.is::<gtk::CheckButton>()
    })
    .unwrap()
    .downcast::<gtk::CheckButton>()
    .unwrap();
    all.set_active(true);
    assert_eq!(rows(&list), ["sample.ppm"]);
    path.set_text(&root.join("not-a-folder-or-file").to_string_lossy());
    path.emit_activate();
    until(|| {
        find_widget(dialog.upcast_ref(), &|widget| {
            widget
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text().starts_with("Cannot open"))
        })
        .is_some()
    });
    assert_eq!(controller.state.borrow().view, before);
    button(dialog.upcast_ref(), "Cancel").emit_clicked();
    assert!(!dialog.is_visible());
    controller.open_file(gio::File::for_path(&fixtures[0]));
    wait_for_open(&controller);
    assert_eq!(
        controller.state.borrow().current_path.as_ref(),
        Some(&fixtures[0])
    );
    assert!(copied(&controller).contains("☑ Completed task"));
    assert!(
        launched.borrow().is_empty(),
        "rendering must never launch a handler"
    );
    activate(&controller, "rendering.md");
    assert_eq!(
        controller.state.borrow().current_path.as_ref(),
        Some(&fixtures[1])
    );
    activate(&controller, "notes/Unicode%20caf%C3%A9.md");
    assert_eq!(
        controller.state.borrow().current_path.as_ref(),
        Some(&fixtures[2])
    );
    activate(&controller, "../rendering.md#unicode");
    {
        let state = controller.state.borrow();
        let buffer = state.view.as_ref().unwrap().buffer();
        assert!(
            buffer
                .text(
                    &buffer.iter_at_offset(buffer.cursor_position()),
                    &buffer.end_iter(),
                    false
                )
                .starts_with("Unicode")
        );
    }
    assert!(launched.borrow().is_empty());

    // Deliver the actual GDK FileList drop signal to the installed target.
    let controllers = controller.window.observe_controllers();
    let target = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .find_map(|c| c.downcast::<gtk::DropTarget>().ok())
        .unwrap();
    for fixture in &fixtures {
        let list = gdk::FileList::from_array(&[gio::File::for_path(fixture)]);
        assert!(target.emit_by_name::<bool>(
            "drop",
            &[&glib::BoxedValue(list.to_value()), &0.0_f64, &0.0_f64]
        ));
        wait_for_open(&controller);
        assert_eq!(
            controller.state.borrow().current_path.as_ref(),
            Some(fixture)
        );
    }
    let list = gdk::FileList::from_array(&[
        gio::File::for_path(&fixtures[0]),
        gio::File::for_path(&fixtures[1]),
    ]);
    assert!(!target.emit_by_name::<bool>(
        "drop",
        &[&glib::BoxedValue(list.to_value()), &0.0_f64, &0.0_f64]
    ));
    assert!(controller.status.text().contains("one Markdown file"));

    let temp = std::env::temp_dir().join(format!("mdview-window-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let missing = temp.join("retry café.md");
    let saved_view = controller.state.borrow().view.clone().unwrap();
    let saved_path = controller.state.borrow().current_path.clone();
    for path in [missing.clone(), temp.clone()] {
        controller.open_file(gio::File::for_path(&path));
        wait_for_open(&controller);
        assert_eq!(controller.state.borrow().view.as_ref(), Some(&saved_view));
        assert_eq!(controller.state.borrow().current_path, saved_path);
        assert!(controller.retry.is_enabled());
        assert!(controller.notice.reveals_child());
    }
    controller.open_file(gio::File::for_uri("https://example.org/remote.md"));
    wait_for_open(&controller);
    assert!(controller.status.text().contains("Only local files"));
    assert_eq!(controller.state.borrow().view.as_ref(), Some(&saved_view));
    let invalid = temp.join("invalid.md");
    fs::write(&invalid, [0xff]).unwrap();
    controller.open_file(gio::File::for_path(&invalid));
    wait_for_open(&controller);
    assert!(controller.status.text().contains("UTF-8"));
    assert_eq!(controller.state.borrow().view.as_ref(), Some(&saved_view));
    controller.open_file(gio::File::for_path(&missing));
    wait_for_open(&controller);
    fs::write(&missing, "# Retry succeeded\n").unwrap();
    controller
        .window
        .lookup_action("retry")
        .unwrap()
        .activate(None);
    wait_for_open(&controller);
    assert!(copied(&controller).contains("Retry succeeded"));
    assert!(!controller.notice.reveals_child());
    assert!(!controller.retry.is_enabled());

    let source = temp.join("links.md");
    let text = "# Links\n\n[Web](https://example.org/) [Mail](mailto:test@example.org) [File](report.pdf) [Script](run.sh) [Bad](javascript:alert) [Missing](#unknown) [Top](#)";
    fs::write(&source, text).unwrap();
    fs::write(temp.join("report.pdf"), b"%PDF-1.4").unwrap();
    fs::write(temp.join("run.sh"), b"echo bad").unwrap();
    controller.open_file(gio::File::for_path(&source));
    wait_for_open(&controller);
    assert!(launched.borrow().is_empty());
    assert!(find_link(controller.window.upcast_ref(), "javascript:alert").is_none());
    activate(&controller, "https://example.org/");
    activate(&controller, "mailto:test@example.org");
    activate(&controller, "report.pdf");
    assert_eq!(
        *launched.borrow(),
        vec![
            LaunchTarget::Uri("https://example.org/".into()),
            LaunchTarget::Uri("mailto:test@example.org".into()),
            LaunchTarget::File(temp.join("report.pdf").canonicalize().unwrap())
        ]
    );
    let count = launched.borrow().len();
    activate(&controller, "run.sh");
    assert_eq!(launched.borrow().len(), count);
    assert!(controller.status.text().contains("scripts"));
    activate(&controller, "#unknown");
    assert!(controller.status.text().contains("Heading not found"));
    assert_eq!(
        controller.state.borrow().current_path.as_ref(),
        Some(&source)
    );
    activate(&controller, "#");
    assert_eq!(
        controller
            .state
            .borrow()
            .view
            .as_ref()
            .unwrap()
            .buffer()
            .cursor_position(),
        0
    );
    fail.set(true);
    activate(&controller, "https://example.org/");
    assert!(
        controller
            .status
            .text()
            .contains("Test desktop handler failed")
    );
    assert!(controller.retry.is_enabled());
    fail.set(false);
    controller.retry.activate(None);
    wait_for_open(&controller);
    assert!(!controller.notice.reveals_child());
    assert_eq!(fs::read_to_string(&source).unwrap(), text);
    assert_eq!(fs::read(temp.join("report.pdf")).unwrap(), b"%PDF-1.4");
    for (fixture, bytes) in fixtures.iter().zip(&original) {
        assert_eq!(fs::read(fixture).unwrap(), *bytes);
    }

    // A later open must win even when older work completes afterwards.
    controller.open_file(gio::File::for_path(&fixtures[0]));
    controller.open_file(gio::File::for_path(&fixtures[2]));
    wait_for_open(&controller);
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(50)));
    assert_eq!(
        controller.state.borrow().current_path.as_ref(),
        Some(&fixtures[2])
    );
    controller.window.close();
    fs::remove_dir_all(temp).unwrap();
}

pub fn verify_reading(controller: &Rc<DocumentWindow>, application: &gtk::Application) {
    let dir = std::env::temp_dir().join(format!("mdview-reading-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reading.md");
    let code = (0..40)
        .map(|i| format!("line {i} {}\n", "padding ".repeat(30)))
        .collect::<String>();
    // Filler rows put the matching row far below the table's top.
    let rows = (0..80)
        .map(|i| format!("| row {i} | value {i} |\n"))
        .collect::<String>();
    let source = format!(
        "# Search\n\nCafé فارسی 日本語 café.\n\nbefore [**Label**](https://example.org/hidden-destination) after\n\n| Name | Value |\n| --- | --- |\n{rows}| Café | فارسی |\n\n```text\n{code}Café فارسی 日本語 target\n```\n"
    );
    fs::write(&path, &source).unwrap();
    controller.window.set_resizable(false);
    controller.window.set_default_size(420, 320);
    controller.open_file(gio::File::for_path(&path));
    wait_for_open(controller);
    let view = controller.state.borrow().view.clone().unwrap();
    let buffer = view.buffer();
    buffer.select_range(&buffer.start_iter(), &buffer.iter_at_offset(3));
    let selection = buffer.selection_bounds().unwrap();
    controller
        .window
        .lookup_action("find")
        .unwrap()
        .activate(None);
    assert!(controller.search_bar.reveals_child());
    for (query, expected) in [
        ("CAFÉ", 4),
        ("فارسی", 3),
        ("日本語", 2),
        ("before Label after", 1),
        ("hidden-destination", 0),
        ("not present", 0),
        ("", 0),
    ] {
        controller.search_entry.set_text(query);
        controller.update_search();
        wait_for_search(controller);
        assert_eq!(
            controller.search.borrow().position().1,
            expected,
            "query {query}"
        );
    }
    let link = find_link(
        controller.window.upcast_ref(),
        "https://example.org/hidden-destination",
    )
    .unwrap();
    controller.search_entry.set_text("before Label after");
    controller.update_search();
    wait_for_search(controller);
    assert!(link.has_css_class("mdview-search-active"));
    // Table cells are highlighted, and the active hit in a cell far down a
    // full-height table is scrolled into view by the document.
    let cell = view
        .embedded()
        .into_iter()
        .flat_map(|(a, _)| a.widgets())
        .find_map(|w| crate::document_view::table_cells(&w))
        .unwrap()
        .into_iter()
        .find(|cell| cell.text() == "فارسی")
        .unwrap();
    controller.search_entry.set_text("فارسی");
    controller.update_search();
    wait_for_search(controller);
    assert!(
        cell.attributes().is_some(),
        "table cell hit not highlighted"
    );
    controller
        .window
        .lookup_action("find-next")
        .unwrap()
        .activate(None);
    assert_eq!(controller.search_count.text(), "2 of 3");
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(400)));
    let at = cell
        .compute_point(&view, &gtk::graphene::Point::new(0.0, 0.0))
        .unwrap();
    assert!(
        at.y() >= 0.0 && at.y() + cell.height() as f32 <= view.height() as f32,
        "active table cell not revealed: y={} view={}",
        at.y(),
        view.height()
    );
    controller.search_entry.set_text("target");
    controller.update_search();
    wait_for_search(controller);
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(150)));
    let code_scroll = view
        .embedded()
        .into_iter()
        .flat_map(|(a, _)| a.widgets())
        .filter_map(|w| w.downcast::<gtk::ScrolledWindow>().ok())
        .last()
        .unwrap();
    assert!(
        code_scroll.vadjustment().value() > 0.0,
        "active hit must reveal the bottom of code"
    );
    controller.search_entry.set_text("café");
    controller.update_search();
    wait_for_search(controller);
    assert_eq!(controller.search_count.text(), "1 of 4");
    controller
        .window
        .lookup_action("find-previous")
        .unwrap()
        .activate(None);
    assert_eq!(controller.search_count.text(), "4 of 4");
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(150)));
    assert!(
        buffer.selection_bounds().is_some_and(
            |(a, b)| a.offset() == selection.0.offset() && b.offset() == selection.1.offset()
        ),
        "search must preserve copy selection"
    );
    controller
        .window
        .lookup_action("find-next")
        .unwrap()
        .activate(None);
    assert_eq!(controller.search_count.text(), "1 of 4");
    let controllers = controller.window.observe_controllers();
    let keys = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .filter_map(|c| c.downcast::<gtk::EventControllerKey>().ok())
        .find(|c| c.name().as_deref() == Some("mdview-reading-keys"))
        .unwrap();
    for key in [gdk::Key::G, gdk::Key::g] {
        assert!(keys.emit_by_name::<bool>(
            "key-pressed",
            &[
                &key,
                &0_u32,
                &(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK)
            ]
        ));
    }
    assert_eq!(controller.search_count.text(), "3 of 4");
    assert!(
        !keys.emit_by_name::<bool>(
            "key-pressed",
            &[&gdk::Key::c, &0_u32, &gdk::ModifierType::CONTROL_MASK]
        ),
        "copy must remain a native shortcut"
    );
    assert!(keys.emit_by_name::<bool>(
        "key-pressed",
        &[&gdk::Key::Escape, &0_u32, &gdk::ModifierType::empty()]
    ));
    controller
        .window
        .lookup_action("hide-search")
        .unwrap()
        .activate(None);
    assert!(!controller.search_bar.reveals_child());
    assert_eq!(controller.search.borrow().position(), (0, 0));
    assert!(
        cell.attributes().is_none(),
        "hiding search kept a cell highlight"
    );
    assert!(
        gtk::prelude::RootExt::focus(&controller.window)
            .is_some_and(|w| w == view.clone().upcast::<gtk::Widget>())
    );

    let context = glib::MainContext::default();
    application.activate_action("zoom-reset", None);
    context.block_on(glib::timeout_future(Duration::from_millis(80)));
    let initial_font = view.pango_context().font_description().unwrap().size();
    for _ in 0..20 {
        application.activate_action("zoom-in", None);
    }
    assert_eq!(controller.settings.values.borrow().zoom, 200);
    for window in application.windows() {
        assert!(
            find_widget(window.upcast_ref(), &|w| w
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text() == "200%"))
            .is_some(),
            "zoom must update every window"
        );
    }
    context.block_on(glib::timeout_future(Duration::from_millis(80)));
    assert!(view.pango_context().font_description().unwrap().size() > initial_font);
    for _ in 0..20 {
        application.activate_action("zoom-out", None);
    }
    assert_eq!(controller.settings.values.borrow().zoom, 50);
    application.activate_action("zoom-reset", None);
    assert_eq!(controller.settings.values.borrow().zoom, 100);

    let system = view.color();
    application.activate_action("theme", Some(&"light".to_variant()));
    context.block_on(glib::timeout_future(Duration::from_millis(100)));
    let light = view.color();
    application.activate_action("theme", Some(&"dark".to_variant()));
    context.block_on(glib::timeout_future(Duration::from_millis(100)));
    let dark = view.color();
    assert_ne!(light, dark, "appearance must change native text colors");
    application.activate_action("theme", Some(&"system".to_variant()));
    context.block_on(glib::timeout_future(Duration::from_millis(100)));
    assert_eq!(view.color(), system, "System must restore desktop CSS");
    assert_eq!(
        application
            .lookup_action("theme")
            .unwrap()
            .state()
            .unwrap()
            .str(),
        Some("system")
    );
    application.activate_action("theme", Some(&"invalid".to_variant()));
    assert_eq!(controller.settings.values.borrow().theme, "system");
    assert_eq!(application.accels_for_action("win.find"), ["<Control>f"]);
    assert!(!application.accels_for_action("app.zoom-in").is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
    let preferences_path = dir.join("preferences.ini");
    let saved = nameh::preferences::Preferences {
        theme: "dark".into(),
        zoom: 140,
        width: 960,
        height: 720,
    };
    saved.save(&preferences_path).unwrap();
    let restarted_settings = crate::reading::ReadingSettings::new(Some(preferences_path));
    assert_eq!(*restarted_settings.values.borrow(), saved);
    let restarted = DocumentWindow::new(
        application,
        false,
        Rc::new(files::launch),
        restarted_settings,
    );
    assert_eq!(
        restarted.window.default_size(),
        (960, 720),
        "persisted size must be requested before the compositor allocates"
    );
    assert_eq!(restarted.zoom_label.text(), "140%");
    restarted.window.close();
    verify_accessibility(controller, application);
    fs::remove_dir_all(dir).unwrap();
}

fn wait_for_search(controller: &DocumentWindow) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while controller.search_count.text() == "Searching…" {
        assert!(
            std::time::Instant::now() < deadline,
            "search did not finish"
        );
        glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(10)));
    }
}

// Exercise GTK settings in this process only; preserve the desktop settings.
#[allow(deprecated)] // GTK 4.10 baseline: palette lookup has no older replacement.
fn verify_accessibility(controller: &Rc<DocumentWindow>, application: &gtk::Application) {
    let context = glib::MainContext::default();
    let settings = gtk::Settings::default().unwrap();
    let font = settings.gtk_font_name();
    let theme = settings.gtk_theme_name();
    let animations = settings.is_gtk_enable_animations();
    let view = controller.state.borrow().view.clone().unwrap();
    let original_size = view.pango_context().font_description().unwrap().size();
    settings.set_gtk_font_name(Some("Sans 22"));
    settings.set_gtk_enable_animations(false);
    context.block_on(glib::timeout_future(Duration::from_millis(200)));
    assert!(view.pango_context().font_description().unwrap().size() > original_size);
    assert_eq!(
        controller.notice.transition_type(),
        gtk::RevealerTransitionType::None
    );
    assert_eq!(
        controller.search_bar.transition_type(),
        gtk::RevealerTransitionType::None
    );
    assert_eq!(
        controller.search_count.accessible_role(),
        gtk::AccessibleRole::Status
    );
    assert_eq!(
        controller.status.accessible_role(),
        gtk::AccessibleRole::Status
    );
    settings.set_gtk_theme_name(Some("HighContrast"));
    context.block_on(glib::timeout_future(Duration::from_millis(200)));
    let native_foreground = view.color();
    let native_background = view
        .style_context()
        .lookup_color("theme_base_color")
        .unwrap();
    application.activate_action("theme", Some(&"dark".to_variant()));
    context.block_on(glib::timeout_future(Duration::from_millis(200)));
    let style = view.style_context();
    let foreground = view.color();
    let background = style.lookup_color("theme_base_color").unwrap();
    assert_eq!(
        foreground, native_foreground,
        "manual Dark must preserve high-contrast text"
    );
    assert_eq!(
        background, native_background,
        "manual Dark must preserve high-contrast background"
    );
    fn luminance(color: &gdk::RGBA) -> f64 {
        let linear = |v: f32| {
            let v = f64::from(v);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.red())
            + 0.7152 * linear(color.green())
            + 0.0722 * linear(color.blue())
    }
    let (a, b) = (luminance(&foreground), luminance(&background));
    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    eprintln!("HighContrast document contrast: {contrast:.2}:1");
    assert!(
        contrast >= 7.0,
        "high contrast must survive a manual appearance override"
    );
    settings.set_gtk_font_name(font.as_deref());
    settings.set_gtk_theme_name(theme.as_deref());
    settings.set_gtk_enable_animations(animations);
    application.activate_action("theme", Some(&"system".to_variant()));
    context.block_on(glib::timeout_future(Duration::from_millis(200)));
}

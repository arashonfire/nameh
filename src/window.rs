//! One-document window controller and shared opening/navigation pipeline.
use crate::{
    files::{self, LaunchTarget, Launcher},
    native, prototype, render,
    state::{AppState, Request},
};
use gtk::{gdk, gio, glib, prelude::*};
use mdview::{
    document::LinkTarget,
    navigation::{self, Destination, LoadedDocument},
};
use std::{cell::RefCell, path::Path, rc::Rc};

pub struct DocumentWindow {
    pub window: gtk::ApplicationWindow,
    pub state: RefCell<AppState>,
    pub preview: bool,
    body: gtk::Box,
    status: gtk::Label,
    notice: gtk::Revealer,
    retry: gio::SimpleAction,
    launcher: Launcher,
    settings: Rc<crate::reading::ReadingSettings>,
    search_bar: gtk::Revealer,
    search_entry: gtk::Entry,
    search_count: gtk::Label,
    search: RefCell<crate::search::Search>,
    search_pending: RefCell<Option<glib::JoinHandle<()>>>,
    zoom_style: gtk::CssProvider,
    zoom_label: gtk::Label,
    monitor: RefCell<Option<gio::FileMonitor>>,
    reload_dirty: std::cell::Cell<bool>,
    target_monitors: RefCell<Vec<gio::FileMonitor>>,
    reload_timer: RefCell<Option<glib::SourceId>>,
    pending: RefCell<Option<glib::JoinHandle<()>>>,
}

enum Prepared {
    Document(LoadedDocument, Option<String>),
    Heading(String),
    Launch(LaunchTarget),
}

impl DocumentWindow {
    pub fn new(
        application: &gtk::Application,
        preview: bool,
        launcher: Launcher,
        settings: Rc<crate::reading::ReadingSettings>,
    ) -> Rc<Self> {
        let prefs = settings.values.borrow().clone();
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("mdview")
            .default_width(prefs.width)
            .default_height(prefs.height)
            .build();
        window.add_css_class("mdview-window");
        let header = gtk::HeaderBar::new();
        let open_button = gtk::Button::builder()
            .icon_name("document-open-symbolic")
            .tooltip_text("Open a Markdown file (Ctrl+O)")
            .action_name("win.open")
            .build();
        open_button.update_property(&[gtk::accessible::Property::Label("Open a Markdown file")]);
        header.pack_start(&open_button);
        let new_button = gtk::Button::builder()
            .icon_name("window-new-symbolic")
            .tooltip_text("New window (Ctrl+N)")
            .action_name("app.new-window")
            .build();
        new_button.update_property(&[gtk::accessible::Property::Label("Open a new window")]);
        header.pack_end(&new_button);
        let find = gtk::Button::builder()
            .icon_name("edit-find-symbolic")
            .tooltip_text("Find in document (Ctrl+F)")
            .action_name("win.find")
            .build();
        find.update_property(&[gtk::accessible::Property::Label("Find in document")]);
        header.pack_start(&find);
        let menu = gio::Menu::new();
        menu.append(Some("Zoom in"), Some("app.zoom-in"));
        menu.append(Some("Zoom out"), Some("app.zoom-out"));
        menu.append(Some("Reset zoom"), Some("app.zoom-reset"));
        let appearance = gio::Menu::new();
        for (label, value) in [("System", "system"), ("Light", "light"), ("Dark", "dark")] {
            let item = gio::MenuItem::new(Some(label), None);
            item.set_action_and_target_value(Some("app.theme"), Some(&value.to_variant()));
            appearance.append_item(&item);
        }
        menu.append_section(Some("Appearance"), &appearance);
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Reading preferences")
            .menu_model(&menu)
            .build();
        menu_button.update_property(&[gtk::accessible::Property::Label("Reading preferences")]);
        header.pack_end(&menu_button);
        let zoom_label = gtk::Label::new(None);
        header.pack_end(&zoom_label);
        window.set_titlebar(Some(&header));

        let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let search_bar = gtk::Revealer::new();
        search_bar.set_transition_type(gtk::RevealerTransitionType::None);
        search_bar.set_widget_name("mdview-searchbar");
        let search_entry = gtk::Entry::builder()
            .placeholder_text("Find in document")
            .hexpand(true)
            .build();
        search_entry.set_width_chars(12);
        search_entry.set_max_width_chars(24);
        search_entry.update_property(&[gtk::accessible::Property::Label("Find in document")]);
        let search_count = gtk::Label::new(Some("0 matches"));
        search_count.set_accessible_role(gtk::AccessibleRole::Status);
        search_count
            .update_property(&[gtk::accessible::Property::Description("Search match count")]);
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        search_row.append(&search_entry);
        search_row.append(&search_count);
        for (icon, tip, action) in [
            (
                "go-up-symbolic",
                "Previous match (Ctrl+Shift+G)",
                "win.find-previous",
            ),
            ("go-down-symbolic", "Next match (Ctrl+G)", "win.find-next"),
            (
                "window-close-symbolic",
                "Close search (Esc)",
                "win.hide-search",
            ),
        ] {
            let button = gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .action_name(action)
                .build();
            button.update_property(&[gtk::accessible::Property::Label(tip)]);
            search_row.append(&button);
        }
        search_bar.set_child(Some(&search_row));

        layout.append(&search_bar);
        let notice = gtk::Revealer::new();
        notice.set_transition_type(gtk::RevealerTransitionType::None);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_start(12);
        row.set_margin_end(12);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        let status = gtk::Label::new(None);
        status.set_accessible_role(gtk::AccessibleRole::Status);
        status.set_wrap(true);
        status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        status.set_selectable(true);
        status.set_hexpand(true);
        status.set_xalign(0.0);
        row.append(&status);
        row.append(
            &gtk::Button::builder()
                .label("Retry")
                .action_name("win.retry")
                .build(),
        );
        let dismiss = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Dismiss message")
            .action_name("win.dismiss-error")
            .build();
        dismiss.update_property(&[gtk::accessible::Property::Label("Dismiss message")]);
        row.append(&dismiss);
        notice.set_child(Some(&row));
        layout.append(&notice);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_vexpand(true);
        body.set_hexpand(true);
        let mut initial_state = AppState::default();
        if preview {
            let scroll = prototype::build();
            initial_state.view = scroll
                .child()
                .and_downcast::<crate::document_view::DocumentView>();
            body.append(&scroll);
        } else {
            let (empty, _) = render::empty_state();
            empty.set_vexpand(true);
            body.append(&empty);
        }
        layout.append(&body);
        window.set_child(Some(&layout));
        let retry = gio::SimpleAction::new("retry", None);
        retry.set_enabled(false);
        static STYLE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        body.set_widget_name(&format!(
            "mdview-reading-{}",
            STYLE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let zoom_style = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(&window),
            &zoom_style,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let controller = Rc::new(Self {
            window,
            state: RefCell::new(initial_state),
            preview,
            body,
            status,
            notice,
            retry,
            launcher,
            settings: settings.clone(),
            search_bar,
            search_entry,
            search_count,
            search: RefCell::new(crate::search::Search::default()),
            search_pending: RefCell::new(None),
            zoom_style,
            zoom_label,
            monitor: RefCell::new(None),
            reload_dirty: std::cell::Cell::new(false),
            target_monitors: RefCell::new(Vec::new()),
            reload_timer: RefCell::new(None),
            pending: RefCell::new(None),
        });
        controller.install_actions(preview);
        settings.register(&controller);
        controller.install_reading();
        controller.apply_reading();
        controller.window.present();
        controller
    }

    fn install_actions(self: &Rc<Self>, preview: bool) {
        let open = gio::SimpleAction::new("open", None);
        open.set_enabled(!preview);
        let weak = Rc::downgrade(self);
        open.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let weak = Rc::downgrade(&this);
            let directory = this
                .state
                .borrow()
                .current_path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| "/".into());
            files::choose_markdown(&this.window, &directory, move |file| {
                if let Some(this) = weak.upgrade() {
                    this.open_file(file);
                }
            });
        });
        self.window.add_action(&open);
        let weak = Rc::downgrade(self);
        self.retry.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                let request = this.state.borrow().retry.clone();
                if let Some(request) = request {
                    this.perform(request);
                }
            }
        });
        self.window.add_action(&self.retry);
        let dismiss = gio::SimpleAction::new("dismiss-error", None);
        let weak = Rc::downgrade(self);
        dismiss.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.notice.set_reveal_child(false);
            }
        });
        self.window.add_action(&dismiss);
        let close = gio::SimpleAction::new("close", None);
        let weak = self.window.downgrade();
        close.connect_activate(move |_, _| {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        });
        self.window.add_action(&close);
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        drop.connect_drop(move |_, value, _, _| {
            let (Some(this), Ok(files)) = (weak.upgrade(), value.get::<gdk::FileList>()) else {
                return false;
            };
            this.accept_drop(&files)
        });
        if !preview {
            self.window.add_controller(drop);
        }
    }

    /// A window accepts one file per drop; never silently discard extra files.
    fn accept_drop(self: &Rc<Self>, files: &gdk::FileList) -> bool {
        let files = files.files();
        if files.len() != 1 {
            self.message("Drop one Markdown file at a time.", None);
            return false;
        }
        self.open_file(files[0].clone());
        true
    }

    pub fn open_file(self: &Rc<Self>, file: gio::File) {
        self.perform(Request::Open {
            file,
            fragment: None,
        });
    }

    fn follow(self: &Rc<Self>, target: &LinkTarget) {
        let path = self.state.borrow().current_path.clone();
        if let Some(document_path) = path {
            self.perform(Request::Follow {
                document_path,
                target: target.clone(),
            });
        }
    }

    fn message(&self, text: &str, retry: Option<Request>) {
        self.status.set_text(text);
        self.retry.set_enabled(retry.is_some());
        self.state.borrow_mut().retry = retry;
        self.notice.set_reveal_child(true);
    }

    fn perform(self: &Rc<Self>, request: Request) {
        if let Some(pending) = self.pending.borrow_mut().take() {
            pending.abort();
        }
        let reloading = matches!(request, Request::Reload { .. });
        if let Some(timer) = self.reload_timer.borrow_mut().take() {
            timer.remove();
            self.reload_dirty.set(true);
        }
        if reloading {
            self.reload_dirty.set(false);
        }
        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.loading = true;
            state.reloading = reloading;
            state.retry = None;
            state.generation
        };
        if !reloading {
            self.message("Opening…", None);
        }
        let destination = match &request {
            Request::Open { file, fragment } => file
                .path()
                .map(|path| Destination::Markdown {
                    path,
                    fragment: fragment.clone(),
                })
                .ok_or_else(|| {
                    "Only local files can be opened. Remote document loading is disabled."
                        .to_owned()
                }),
            Request::Reload { file } => Ok(Destination::Markdown {
                path: file.path().expect("reload paths are local"),
                fragment: None,
            }),
            Request::Follow {
                document_path,
                target,
            } => navigation::resolve(document_path, target),
        };
        let weak = Rc::downgrade(self);
        let pending = glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                Ok(match destination? {
                    Destination::Markdown { path, fragment } => {
                        Prepared::Document(navigation::load_document(&path)?, fragment)
                    }
                    Destination::Heading(id) => Prepared::Heading(id),
                    Destination::OtherFile(path) => {
                        Prepared::Launch(LaunchTarget::File(navigation::check_other_file(&path)?))
                    }
                    Destination::ExternalUri(uri) => Prepared::Launch(LaunchTarget::Uri(uri)),
                })
            })
            .await
            .unwrap_or_else(|_| Err("Could not finish opening the file".to_owned()));
            let Some(this) = weak.upgrade() else {
                return;
            };
            if !this.window.is_visible() || this.state.borrow().generation != generation {
                return;
            }
            match result {
                Err(error) => this.message(&error, Some(request)),
                Ok(Prepared::Document(loaded, fragment)) => {
                    this.show_document(loaded, fragment, request, reloading)
                        .await
                }
                Ok(Prepared::Heading(id)) => {
                    let found = this
                        .state
                        .borrow()
                        .view
                        .as_ref()
                        .is_some_and(|view| view.go_to_heading(&id));
                    if found {
                        this.notice.set_reveal_child(false);
                    } else {
                        this.message(&format!("Heading not found: {id}"), Some(request));
                    }
                }
                Ok(Prepared::Launch(target)) => {
                    let weak = Rc::downgrade(&this);
                    (this.launcher)(
                        &this.window,
                        target,
                        Box::new(move |result| {
                            let Some(this) = weak.upgrade() else {
                                return;
                            };
                            if this.state.borrow().generation != generation {
                                return;
                            }
                            match result {
                                Ok(()) => this.notice.set_reveal_child(false),
                                Err(error) => this.message(
                                    &format!("Could not open the link: {error}"),
                                    Some(request),
                                ),
                            }
                        }),
                    );
                }
            }
            this.state.borrow_mut().loading = false;
            if this.reload_dirty.replace(false) {
                this.queue_reload();
            }
        });
        *self.pending.borrow_mut() = Some(pending);
    }

    async fn show_document(
        self: &Rc<Self>,
        loaded: LoadedDocument,
        fragment: Option<String>,
        request: Request,
        reloading: bool,
    ) {
        let weak = Rc::downgrade(self);
        let view = native::render_async(
            &loaded.document,
            loaded.path.parent().unwrap_or(Path::new("/")),
            Rc::new(move |target| {
                if let Some(this) = weak.upgrade() {
                    this.follow(target);
                }
            }),
        )
        .await;
        // Capture at replacement time so reading during loading is respected.
        let position = reloading
            .then(|| {
                self.state
                    .borrow()
                    .view
                    .as_ref()
                    .map(|v| v.reading_position())
            })
            .flatten();
        self.cancel_search();
        self.search.borrow_mut().clear();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&view)
            .build();
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        self.body.append(&scroll);
        let filename = loaded
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        self.window.set_title(Some(&format!("{filename} — mdview")));
        self.window
            .set_tooltip_text(Some(&loaded.path.to_string_lossy()));
        {
            let mut state = self.state.borrow_mut();
            state.current_path = Some(loaded.path.clone());
            state.view = Some(view.clone());
        }
        self.notice.set_reveal_child(false);
        self.reload_dirty.set(false);
        self.watch(&loaded.path, &loaded.watch_paths);
        self.apply_reading();
        if self.search_bar.reveals_child() {
            self.update_search_with_reveal(!reloading);
            self.search_entry.grab_focus();
        } else {
            view.grab_focus();
        }
        if let Some(position) = position {
            view.restore_position(position);
        }
        if let Some(id) = fragment {
            // TextView keeps scroll-to-mark requests pending until allocation.
            if !view.go_to_heading(&id) {
                self.message(
                    &format!("Opened the document, but heading not found: {id}"),
                    Some(request),
                );
            }
        }
        // Monitoring starts after the first read. Recheck identity/version on a
        // worker after installing it, so edits made during loading are not lost.
        let path = loaded.path;
        let stamp = loaded.stamp;
        let document = loaded.document;
        let changed = gio::spawn_blocking(move || {
            drop(document);
            navigation::FileStamp::read(&path).ok() != Some(stamp)
        })
        .await
        .unwrap_or(true);
        if changed {
            self.reload_dirty.set(true);
        }
    }
}

mod reading;
mod reload;
#[cfg(test)]
mod reload_tests;
#[cfg(test)]
pub use reload_tests::verify_reload;

#[cfg(test)]
pub fn wait_for_open(controller: &DocumentWindow) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while controller.state.borrow().loading {
        assert!(
            std::time::Instant::now() < deadline,
            "opening did not finish"
        );
        context.block_on(glib::timeout_future(std::time::Duration::from_millis(10)));
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub use tests::{verify_file_opening, verify_reading};

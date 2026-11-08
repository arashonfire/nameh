//! Lightweight local-file picker with a virtualized native GTK file list.
use gtk::{gio, glib, prelude::*};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    directory: bool,
}
struct State {
    directory: PathBuf,
    entries: Vec<Entry>,
    visible: Vec<Entry>,
    generation: u64,
}

#[derive(Clone)]
struct Browser {
    state: Rc<RefCell<State>>,
    model: gtk::StringList,
    selection: gtk::SingleSelection,
    path: glib::WeakRef<gtk::Entry>,
    status: glib::WeakRef<gtk::Label>,
    window: glib::WeakRef<gtk::Window>,
    all: glib::WeakRef<gtk::CheckButton>,
}

impl Browser {
    fn message(&self, message: &str) {
        if let Some(status) = self.status.upgrade() {
            status.set_text(message);
        }
    }

    fn refresh(&self) {
        let show_all = self.all.upgrade().is_some_and(|all| all.is_active());
        self.selection.set_selected(gtk::INVALID_LIST_POSITION);
        let mut state = self.state.borrow_mut();
        state.visible = state
            .entries
            .iter()
            .filter(|entry| {
                entry.directory || show_all || mdview::navigation::is_markdown(&entry.path)
            })
            .cloned()
            .collect();
        let names: Vec<_> = state
            .visible
            .iter()
            .map(|entry| {
                let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
                if entry.directory {
                    format!("{name}/")
                } else {
                    name.into_owned()
                }
            })
            .collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        drop(state);
        self.model.splice(0, self.model.n_items(), &names);
    }

    fn folder(&self, directory: PathBuf) {
        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.generation
        };
        if let Some(path) = self.path.upgrade() {
            path.set_text(&directory.to_string_lossy());
        }
        self.message("Loading folder…");
        let browser = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let mut entries = Vec::new();
                for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path();
                    entries.push(Entry {
                        directory: path.is_dir(),
                        path,
                    });
                }
                entries.sort_by(|a, b| {
                    b.directory
                        .cmp(&a.directory)
                        .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
                });
                Ok::<_, String>((directory, entries))
            })
            .await
            .unwrap_or_else(|_| Err("Could not list this folder".into()));
            if browser.window.upgrade().is_none() || browser.state.borrow().generation != generation
            {
                return;
            }
            match result {
                Ok((directory, entries)) => {
                    {
                        let mut state = browser.state.borrow_mut();
                        state.directory = directory;
                        state.entries = entries;
                    }
                    browser.refresh();
                    browser.message("");
                }
                Err(error) => browser.message(&format!("Could not open the folder: {error}")),
            }
        });
    }

    fn open(&self, selected: Rc<dyn Fn(gio::File)>) {
        let Some(entry) = self.path.upgrade() else {
            return;
        };
        let text = entry.text();
        if text.is_empty() {
            self.message("Enter a file or folder path.");
            return;
        }
        // This entry is a filesystem path, never a URI. No percent decoding:
        // native filenames may themselves contain literal percent characters.
        let path = self.state.borrow().directory.join(text.as_str());
        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.generation
        };
        let browser = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let metadata = std::fs::metadata(&path)
                    .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
                Ok::<_, String>((path, metadata.is_dir()))
            })
            .await
            .unwrap_or_else(|_| Err("Could not open this path".into()));
            let Some(window) = browser.window.upgrade() else {
                return;
            };
            if !window.is_visible() || browser.state.borrow().generation != generation {
                return;
            }
            match result {
                Ok((directory, true)) => browser.folder(directory),
                Ok((path, false)) => {
                    window.close();
                    selected(gio::File::for_path(path));
                }
                Err(error) => browser.message(&error),
            }
        });
    }
}

pub fn choose_markdown(
    parent: &gtk::ApplicationWindow,
    directory: &Path,
    on_selected: impl Fn(gio::File) + 'static,
) {
    let window = gtk::Window::builder()
        .title("Open a Markdown file")
        .transient_for(parent)
        .modal(true)
        .destroy_with_parent(true)
        .default_width(720)
        .default_height(480)
        .build();
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for set in [
        gtk::Widget::set_margin_start,
        gtk::Widget::set_margin_end,
        gtk::Widget::set_margin_top,
        gtk::Widget::set_margin_bottom,
    ] {
        set(layout.upcast_ref(), 12);
    }
    let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let up = gtk::Button::with_label("Up");
    up.set_tooltip_text(Some("Go to the parent folder"));
    navigation.append(&up);
    let path = gtk::Entry::builder().hexpand(true).build();
    path.set_widget_name("mdview-file-path");
    path.update_property(&[gtk::accessible::Property::Label("File or folder path")]);
    navigation.append(&path);
    layout.append(&navigation);
    let model = gtk::StringList::new(&[]);
    let selection = gtk::SingleSelection::new(Some(model.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .margin_start(8)
            .margin_end(8)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let value = item.item().and_downcast::<gtk::StringObject>().unwrap();
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&value.string());
        label.set_tooltip_text(Some(&value.string()));
    });
    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.update_property(&[gtk::accessible::Property::Label("Files and folders")]);
    let scroll = gtk::ScrolledWindow::builder()
        .has_frame(true)
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    layout.append(&scroll);
    let status = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build();
    status.add_css_class("error");
    layout.append(&status);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let all = gtk::CheckButton::with_label("All files");
    all.set_hexpand(true);
    controls.append(&all);
    let cancel = gtk::Button::with_label("Cancel");
    controls.append(&cancel);
    let open = gtk::Button::with_label("Open");
    open.add_css_class("suggested-action");
    controls.append(&open);
    layout.append(&controls);
    window.set_child(Some(&layout));
    window.set_default_widget(Some(&open));
    let browser = Browser {
        state: Rc::new(RefCell::new(State {
            directory: directory.to_owned(),
            entries: Vec::new(),
            visible: Vec::new(),
            generation: 0,
        })),
        model,
        selection,
        path: path.downgrade(),
        status: status.downgrade(),
        window: window.downgrade(),
        all: all.downgrade(),
    };
    let selected: Rc<dyn Fn(gio::File)> = Rc::new(on_selected);
    let action = browser.clone();
    let callback = selected.clone();
    open.connect_clicked(move |_| action.open(callback.clone()));
    let action = browser.clone();
    let callback = selected.clone();
    path.connect_activate(move |_| action.open(callback.clone()));
    let action = browser.clone();
    up.connect_clicked(move |_| {
        let directory = action.state.borrow().directory.parent().map(Path::to_owned);
        if let Some(directory) = directory {
            action.folder(directory);
        }
    });
    let action = browser.clone();
    all.connect_toggled(move |_| action.refresh());
    let state = browser.state.clone();
    let entry = browser.path.clone();
    browser.selection.connect_selected_notify(move |selection| {
        let target = state
            .borrow()
            .visible
            .get(selection.selected() as usize)
            .map(|entry| entry.path.clone());
        if let (Some(path), Some(target)) = (entry.upgrade(), target) {
            path.set_text(&target.to_string_lossy());
        }
    });
    let action = browser.clone();
    list.connect_activate(move |_, position| {
        let target = action
            .state
            .borrow()
            .visible
            .get(position as usize)
            .map(|entry| entry.path.clone());
        if let (Some(path), Some(target)) = (action.path.upgrade(), target) {
            path.set_text(&target.to_string_lossy());
            action.open(selected.clone());
        }
    });
    let weak = window.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    let keys = gtk::EventControllerKey::new();
    let weak = window.downgrade();
    let entry = path.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
            glib::Propagation::Stop
        } else if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && matches!(key, gtk::gdk::Key::l | gtk::gdk::Key::L)
        {
            if let Some(entry) = entry.upgrade() {
                entry.grab_focus();
                entry.select_region(0, -1);
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
    // A closing picker must discard any in-flight directory/path checks.
    let state = browser.state.clone();
    window.connect_close_request(move |_| {
        state.borrow_mut().generation += 1;
        crate::memory::reclaim_after_retirement();
        glib::Propagation::Proceed
    });
    browser.folder(directory.to_owned());
    window.present();
    path.grab_focus();
}

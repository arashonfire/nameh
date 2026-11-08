//! Application-wide reading preferences and native appearance selection.
use crate::window::DocumentWindow;
use gtk::{gio, glib, prelude::*};
use mdview::preferences::Preferences;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::{Rc, Weak},
};

pub struct ReadingSettings {
    pub values: RefCell<Preferences>,
    path: Option<PathBuf>,
    windows: RefCell<Vec<Weak<DocumentWindow>>>,
    appearance: RefCell<Option<gtk::CssProvider>>,
}

impl ReadingSettings {
    pub fn new(path: Option<PathBuf>) -> Rc<Self> {
        let values = path.as_deref().map(Preferences::load).unwrap_or_default();
        Rc::new(Self {
            values: RefCell::new(values),
            path,
            windows: RefCell::new(Vec::new()),
            appearance: RefCell::new(None),
        })
    }

    pub fn register(&self, window: &Rc<DocumentWindow>) {
        self.windows.borrow_mut().push(Rc::downgrade(window));
    }

    pub fn save(&self) {
        if let Some(path) = &self.path
            && let Err(error) = self.values.borrow().save(path)
        {
            eprintln!("Could not save reading preferences: {error}");
        }
    }

    fn refresh(&self) {
        self.windows.borrow_mut().retain(|w| w.strong_count() > 0);
        for window in self.windows.borrow().iter().filter_map(Weak::upgrade) {
            window.apply_reading();
        }
        self.save();
    }

    fn appearance(&self) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        if let Some(provider) = self.appearance.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &provider);
        }
        let theme = self.values.borrow().theme.clone();
        let high_contrast = gtk::Settings::default()
            .and_then(|settings| settings.gtk_theme_name())
            .is_some_and(|name| name.to_ascii_lowercase().contains("highcontrast"))
            || std::env::var("GTK_THEME")
                .is_ok_and(|name| name.to_ascii_lowercase().contains("highcontrast"));
        if theme != "system" && !high_contrast {
            // Keep the desktop's native widget rules. Import only GTK's named
            // palette so switching appearance does not install another complete
            // widget theme (and its renderer resources) over the desktop CSS.
            let native = gtk::CssProvider::new();
            native.load_named("Adwaita", if theme == "dark" { Some("dark") } else { None });
            let palette = native
                .to_str()
                .lines()
                .filter(|line| line.starts_with("@define-color "))
                .collect::<Vec<_>>()
                .join("\n");
            let provider = gtk::CssProvider::new();
            provider.load_from_data(&format!("{palette}\n{}", include_str!("appearance.css")));
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
            );
            *self.appearance.borrow_mut() = Some(provider);
        }
    }

    pub fn install(self: &Rc<Self>, application: &gtk::Application) {
        let this = self.clone();
        application.connect_startup(move |_| {
            this.appearance();
            if let Some(settings) = gtk::Settings::default() {
                for property in [
                    "gtk-font-name",
                    "gtk-theme-name",
                    "gtk-application-prefer-dark-theme",
                ] {
                    let weak = Rc::downgrade(&this);
                    settings.connect_notify_local(Some(property), move |_, _| {
                        if let Some(this) = weak.upgrade() {
                            // GTK updates inherited styles before remeasuring blocks.
                            glib::idle_add_local_once(move || {
                                this.appearance();
                                this.refresh();
                            });
                        }
                    });
                }
            }
        });
        let theme = gio::SimpleAction::new_stateful(
            "theme",
            Some(glib::VariantTy::STRING),
            &self.values.borrow().theme.to_variant(),
        );
        let weak = Rc::downgrade(self);
        theme.connect_activate(move |action, value| {
            let Some(value) = value.and_then(|v| v.str()) else {
                return;
            };
            if !matches!(value, "system" | "light" | "dark") {
                return;
            }
            if let Some(this) = weak.upgrade() {
                this.values.borrow_mut().theme = value.into();
                action.set_state(&value.to_variant());
                this.appearance();
                this.refresh();
            }
        });
        application.add_action(&theme);
        for (name, delta) in [("zoom-in", 10), ("zoom-out", -10), ("zoom-reset", 0)] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    let zoom = this.values.borrow().zoom;
                    this.values.borrow_mut().zoom = if delta == 0 {
                        100
                    } else {
                        (zoom + delta).clamp(50, 200)
                    };
                    this.refresh();
                }
            });
            application.add_action(&action);
        }
        let this = self.clone();
        application.connect_shutdown(move |_| {
            for window in this.windows.borrow().iter().filter_map(Weak::upgrade) {
                window.remember_size();
            }
            this.save();
        });
    }
}

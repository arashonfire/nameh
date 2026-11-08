use super::*;

impl DocumentWindow {
    pub(super) fn install_reading(self: &Rc<Self>) {
        for name in ["find", "find-next", "find-previous", "hide-search"] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                match name {
                    "hide-search" => this.hide_search(),
                    "find" => {
                        this.search_bar.set_reveal_child(true);
                        this.search_entry.grab_focus();
                        this.search_entry.select_region(0, -1);
                        this.update_search();
                    }
                    _ => this.next_match(name == "find-previous"),
                }
            });
            self.window.add_action(&action);
        }
        let weak = Rc::downgrade(self);
        self.search_entry.connect_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.update_search();
            }
        });
        let weak = Rc::downgrade(self);
        self.search_entry.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.next_match(false);
            }
        });
        // Escape closes search from either the document or entry. Other keys
        // continue through GTK's normal selection/copy/scrolling handlers.
        let keys = gtk::EventControllerKey::new();
        keys.set_name(Some("mdview-reading-keys"));
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                && !modifiers
                    .intersects(gdk::ModifierType::ALT_MASK | gdk::ModifierType::SUPER_MASK)
                && (key == gdk::Key::g || key == gdk::Key::G)
                && let Some(this) = weak.upgrade()
            {
                this.next_match(modifiers.contains(gdk::ModifierType::SHIFT_MASK));
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape
                && let Some(this) = weak.upgrade()
                && this.search_bar.reveals_child()
            {
                this.hide_search();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(keys);
        let weak = Rc::downgrade(self);
        self.window.connect_realize(move |window| {
            if let Some(surface) = window.surface() {
                let weak = weak.clone();
                let last = std::cell::Cell::new((0, 0));
                surface.connect_layout(move |_, width, height| {
                    if last.replace((width, height)) == (width, height) {
                        return;
                    }
                    let weak = weak.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(this) = weak.upgrade() {
                            this.remember_size();
                        }
                    });
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.stop_watching();
                if let Some(pending) = this.pending.borrow_mut().take() {
                    pending.abort();
                }
                this.state.borrow_mut().loading = false;
                this.remember_size();
                this.settings.save();
                this.cancel_search();
                this.search.borrow_mut().clear();
                gtk::style_context_remove_provider_for_display(
                    &gtk::prelude::WidgetExt::display(&this.window),
                    &this.zoom_style,
                );
            }
            glib::Propagation::Proceed
        });
    }

    pub(crate) fn remember_size(&self) {
        if self.window.is_fullscreen() {
            return;
        }
        let (width, height) = (self.window.width(), self.window.height());
        if (320..=7680).contains(&width) && (240..=4320).contains(&height) {
            let mut prefs = self.settings.values.borrow_mut();
            prefs.width = width;
            prefs.height = height;
        }
    }

    pub(crate) fn apply_reading(self: &Rc<Self>) {
        let zoom = self.settings.values.borrow().zoom;
        self.zoom_label.set_label(&format!("{zoom}%"));
        // Read the inherited system font each time, independently of text zoom.
        let base_font = self
            .body
            .pango_context()
            .font_description()
            .map(|font| font.size() as f64 / gtk::pango::SCALE as f64)
            .filter(|size| *size > 0.0)
            .unwrap_or(11.0);
        let size = base_font * f64::from(zoom) / 100.0;
        let id = self.body.widget_name();
        // Match the text and table-cell highlights exactly.
        let (background, foreground) = crate::search::highlight_colors(&self.body);
        let mut hit = background;
        hit.set_alpha(0.28);
        self.zoom_style.load_from_data(&format!("#{id} textview, #{id} button, #{id} label.mdview-cell {{ font-size: {size:.2}pt; }} #{id} .mdview-search-hit {{ background-color: {hit}; }} #{id} .mdview-search-active {{ background-color: {background}; color: {foreground}; }}"));
        if let Some(view) = self.state.borrow().view.clone() {
            // Let CSS/Pango update before remeasuring embedded block heights.
            glib::idle_add_local_once(move || view.resize_blocks());
        }
        self.update_search_with_reveal(false);
    }

    pub(super) fn cancel_search(&self) {
        if let Some(task) = self.search_pending.borrow_mut().take() {
            task.abort();
        }
    }

    fn hide_search(&self) {
        self.cancel_search();
        self.search_bar.set_reveal_child(false);
        self.search.borrow_mut().clear();
        self.search_count.set_label("0 matches");
        self.search_count.set_tooltip_text(None);
        if let Some(view) = &self.state.borrow().view {
            view.grab_focus();
        }
    }

    pub(super) fn update_search(self: &Rc<Self>) {
        self.update_search_with_reveal(true);
    }

    pub(super) fn update_search_with_reveal(self: &Rc<Self>, reveal: bool) {
        let active = self.search.borrow().position().0.saturating_sub(1);
        self.cancel_search();
        self.search_count.set_tooltip_text(None);
        self.search.borrow_mut().clear();
        let view = self.state.borrow().view.clone();
        let query = self.search_entry.text().to_string();
        if !self.search_bar.reveals_child() || view.is_none() || query.is_empty() {
            self.search_count.set_label("0 matches");
            return;
        }
        self.search_count.set_label("Searching…");
        let weak = Rc::downgrade(self);
        *self.search_pending.borrow_mut() = Some(glib::spawn_future_local(async move {
            // Typing and consecutive style/layout changes share one quiet period.
            glib::timeout_future(std::time::Duration::from_millis(150)).await;
            let mut search = crate::search::Search::default();
            search.find_async(&view.unwrap(), &query, reveal).await;
            if !reveal {
                search.restore_active(active);
            }
            if let Some(this) = weak.upgrade() {
                let (active, count) = search.position();
                this.search_count.set_tooltip_text((count > 1000).then_some("All matches are counted and navigable; the first 1,000 and the active match are highlighted."));
                this.search_count.set_label(&if count == 0 {
                    "0 matches".into()
                } else {
                    format!("{active} of {count}")
                });
                *this.search.borrow_mut() = search;
            }
        }));
    }

    fn next_match(self: &Rc<Self>, backwards: bool) {
        if !self.search_bar.reveals_child() {
            self.search_bar.set_reveal_child(true);
            self.update_search();
            self.search_entry.grab_focus();
            return;
        }
        self.search.borrow_mut().step(backwards);
        let (active, count) = self.search.borrow().position();
        self.search_count.set_label(&if count == 0 {
            "0 matches".into()
        } else {
            format!("{active} of {count}")
        });
    }
}

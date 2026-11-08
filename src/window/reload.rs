//! Directory notifications survive the rename-over save used by editors.
use super::*;
use std::time::Duration;

impl DocumentWindow {
    pub(super) fn stop_watching(&self) {
        if let Some(timer) = self.reload_timer.borrow_mut().take() {
            timer.remove();
        }
        if let Some(monitor) = self.monitor.borrow_mut().take() {
            monitor.cancel();
        }
        for monitor in self.target_monitors.borrow_mut().drain(..) {
            monitor.cancel();
        }
    }

    pub(super) fn queue_reload(self: &Rc<Self>) {
        if self.state.borrow().loading && !self.state.borrow().reloading {
            self.reload_dirty.set(true);
            return;
        }
        if let Some(timer) = self.reload_timer.borrow_mut().take() {
            timer.remove();
        }
        let loading = self.state.borrow().loading;
        if let Some(pending) = self.pending.borrow_mut().take()
            && loading
        {
            pending.abort();
        }
        self.state.borrow_mut().loading = false;
        self.state.borrow_mut().generation += 1;
        let weak = Rc::downgrade(self);
        *self.reload_timer.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(150),
            move || {
                if let Some(this) = weak.upgrade() {
                    this.reload_timer.borrow_mut().take();
                    let path = this.state.borrow().current_path.clone();
                    if let Some(path) = path {
                        this.perform(Request::Reload {
                            file: gio::File::for_path(path),
                        });
                    }
                }
            },
        ));
    }

    pub(super) fn watch(self: &Rc<Self>, path: &Path, watch_paths: &[std::path::PathBuf]) {
        self.stop_watching();
        for watched in watch_paths {
            let Some(parent) = watched.parent() else {
                continue;
            };
            let monitor = match gio::File::for_path(parent)
                .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            {
                Ok(monitor) => monitor,
                Err(error) => {
                    self.message(&format!("Automatic reload unavailable: {error}"), None);
                    continue;
                }
            };
            let watched = gio::File::for_path(watched);
            let path = path.to_path_buf();
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_, file, other, _event| {
                if !file.equal(&watched) && !other.is_some_and(|file| file.equal(&watched)) {
                    return;
                }
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if this.state.borrow().current_path.as_deref() != Some(&path) {
                    return;
                }
                this.queue_reload();
            });
            if self.monitor.borrow().is_none() {
                *self.monitor.borrow_mut() = Some(monitor);
            } else {
                self.target_monitors.borrow_mut().push(monitor);
            }
        }
    }
}

//! File selection and explicit desktop launch boundary.

pub use crate::picker::choose_markdown;
use gtk::gio;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    Uri(String),
    File(std::path::PathBuf),
}

pub type Completion = Box<dyn FnOnce(Result<(), String>)>;
pub type Launcher = std::rc::Rc<dyn Fn(&gtk::ApplicationWindow, LaunchTarget, Completion)>;

/// Called only by explicit classified link activation after local-file checks.
pub fn launch(parent: &gtk::ApplicationWindow, target: LaunchTarget, done: Completion) {
    match target {
        LaunchTarget::Uri(uri) => {
            if !matches!(
                nameh::document::classify_link(&uri),
                Some(nameh::document::LinkTarget::External { .. })
            ) {
                done(Err("Unsupported URI scheme".into()));
                return;
            }
            gtk::UriLauncher::new(&uri).launch(
                Some(parent),
                gio::Cancellable::NONE,
                move |result| done(result.map_err(|e| e.to_string())),
            );
        }
        LaunchTarget::File(path) => gtk::FileLauncher::new(Some(&gio::File::for_path(path)))
            .launch(Some(parent), gio::Cancellable::NONE, move |result| {
                done(result.map_err(|e| e.to_string()))
            }),
    }
}

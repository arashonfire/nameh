//! State owned by one document window.
use crate::document_view::DocumentView;
use gtk::gio;
use nameh::document::LinkTarget;
use std::path::PathBuf;

#[derive(Clone)]
pub enum Request {
    Reload {
        file: gio::File,
    },
    Open {
        file: gio::File,
        fragment: Option<String>,
    },
    Follow {
        document_path: PathBuf,
        target: LinkTarget,
    },
}

#[derive(Default)]
pub struct AppState {
    pub current_path: Option<PathBuf>,
    pub view: Option<DocumentView>,
    pub retry: Option<Request>,
    pub generation: u64,
    pub loading: bool,
    pub reloading: bool,
}

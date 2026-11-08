//! Filesystem-only document loading and resolution of already classified links.
use crate::document::{self, Document, LinkTarget};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Heading(String),
    Markdown {
        path: PathBuf,
        fragment: Option<String>,
    },
    OtherFile(PathBuf),
    ExternalUri(String),
}

#[derive(Debug)]
pub struct LoadedDocument {
    pub path: PathBuf,
    pub document: Document,
    pub stamp: FileStamp,
    pub watch_paths: Vec<PathBuf>,
}

/// Filesystem identity/version used to close the first-load monitoring gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: Option<std::time::SystemTime>,
}

impl FileStamp {
    fn from_metadata(value: std::fs::Metadata) -> Self {
        Self {
            device: value.dev(),
            inode: value.ino(),
            size: value.len(),
            modified: value.modified().ok(),
        }
    }
    pub fn read(path: &Path) -> Result<Self, std::io::Error> {
        std::fs::metadata(path).map(Self::from_metadata)
    }
}

/// Decode exactly once, after classification and splitting path from fragment.
/// The result must never be reclassified as a URI.
pub fn decode_component(value: &str) -> Option<String> {
    let mut decoded = Vec::new();
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' {
            let hi = (bytes.next()? as char).to_digit(16)?;
            let lo = (bytes.next()? as char).to_digit(16)?;
            (hi * 16 + lo) as u8
        } else {
            byte
        });
    }
    let text = String::from_utf8(decoded).ok()?;
    (!text.chars().any(|c| c.is_control() || c == '\\')).then_some(text)
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["md", "markdown", "mdown", "mkd"].contains(&s.to_ascii_lowercase().as_str())
    })
}

pub fn resolve(document_path: &Path, target: &LinkTarget) -> Result<Destination, String> {
    Ok(match target {
        LinkTarget::Heading(fragment) => {
            Destination::Heading(decode_component(fragment).ok_or("Invalid heading encoding")?)
        }
        LinkTarget::External { uri, .. } => {
            // Defense in depth for callers constructing model values directly.
            if document::classify_link(uri).as_ref() != Some(target) {
                return Err("Unsupported link target".into());
            }
            Destination::ExternalUri(uri.clone())
        }
        LinkTarget::Local(local) => {
            let decoded = decode_component(&local.path).ok_or("Invalid local path encoding")?;
            let fragment = local
                .fragment
                .as_deref()
                .map(|s| decode_component(s).ok_or("Invalid heading encoding"))
                .transpose()?;
            if decoded.is_empty() {
                return Ok(Destination::Heading(fragment.unwrap_or_default()));
            }
            let base = document_path.parent().ok_or("Document has no directory")?;
            let path = base.join(decoded);
            if is_markdown(&path) {
                Destination::Markdown { path, fragment }
            } else {
                Destination::OtherFile(path)
            }
        }
    })
}

fn regular_file(path: &Path) -> Result<File, String> {
    // Linux O_NONBLOCK avoids hanging on a FIFO, including one raced into place.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(0o4000)
        .open(path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("Not a regular file: {}", path.display()));
    }
    Ok(file)
}

pub fn load_document(path: &Path) -> Result<LoadedDocument, String> {
    let path = std::path::absolute(path).map_err(|e| format!("Invalid path: {e}"))?;
    let mut file = regular_file(&path)?;
    let stamp = FileStamp::from_metadata(file.metadata().map_err(|e| e.to_string())?);
    let mut source = String::new();
    file.read_to_string(&mut source)
        .map_err(|e| format!("Cannot read {} as UTF-8 text: {e}", path.display()))?;
    // Watch the logical directory entry (rename-over saves) and, for a
    // symlink, the resolved target as well. Resolve parents through the OS;
    // lexical removal of '..' would change symlink-directory semantics.
    let logical = path
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .zip(path.file_name())
        .map(|(parent, name)| parent.join(name))
        .unwrap_or_else(|| path.clone());
    let mut watch_paths = vec![logical];
    if let Ok(target) = path.canonicalize()
        && !watch_paths.contains(&target)
    {
        watch_paths.push(target);
    }
    Ok(LoadedDocument {
        path,
        document: document::parse(&source),
        stamp,
        watch_paths,
    })
}

/// Desktop launchers and executable/script files must not become launch targets.
/// This checks local files only and never invokes a URI API.
pub fn check_other_file(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {e}", path.display()))?;
    let mut file = regular_file(&path)?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let blocked = [
        "desktop", "service", "url", "webloc", "lnk", "sh", "bash", "zsh", "fish", "py", "pl",
        "rb", "js", "exe", "com", "bat", "cmd", "appimage", "so",
    ];
    let mut prefix = [0_u8; 4096];
    let count = file.read(&mut prefix).map_err(|e| e.to_string())?;
    let prefix = &prefix[..count];
    if metadata.permissions().mode() & 0o111 != 0
        || blocked.contains(&extension.as_str())
        || prefix.starts_with(b"#!")
        || prefix.starts_with(b"\x7fELF")
        || prefix.starts_with(b"MZ")
        || String::from_utf8_lossy(prefix)
            .lines()
            .any(|line| line.trim() == "[Desktop Entry]")
    {
        return Err(format!(
            "Executable files, scripts and desktop shortcuts cannot be opened: {}",
            path.display()
        ));
    }
    Ok(path)
}

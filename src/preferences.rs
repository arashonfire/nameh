//! Small, version-independent preferences; documents are never written here.
use gtk::glib;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub theme: String,
    pub zoom: i32,
    pub width: i32,
    pub height: i32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            zoom: 100,
            width: 850,
            height: 650,
        }
    }
}

impl Preferences {
    pub fn path() -> PathBuf {
        glib::user_config_dir().join("mdview/preferences.ini")
    }

    pub fn load(path: &Path) -> Self {
        let mut result = Self::default();
        let mut text = String::new();
        let Ok(file) = fs::File::open(path) else {
            return result;
        };
        if file.take(16_385).read_to_string(&mut text).is_err() || text.len() > 16_384 {
            return result;
        }
        let key = glib::KeyFile::new();
        if key.load_from_data(&text, glib::KeyFileFlags::NONE).is_err() {
            return result;
        }
        if let Ok(theme) = key.string("Reading", "theme")
            && matches!(theme.as_str(), "system" | "light" | "dark")
        {
            result.theme = theme.into();
        }
        for (name, value, min, max) in [
            ("zoom", &mut result.zoom, 50, 200),
            ("width", &mut result.width, 320, 7680),
            ("height", &mut result.height, 240, 4320),
        ] {
            if let Ok(number) = key.integer("Reading", name)
                && (min..=max).contains(&number)
            {
                *value = number;
            }
        }
        result
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let key = glib::KeyFile::new();
        key.set_string("Reading", "theme", &self.theme);
        key.set_integer("Reading", "zoom", self.zoom);
        key.set_integer("Reading", "width", self.width);
        key.set_integer("Reading", "height", self.height);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // GIO performs an atomic replacement, with a private creation mode.
        use gtk::gio::{self, prelude::*};
        gio::File::for_path(path)
            .replace_contents(
                key.to_data().as_bytes(),
                None,
                false,
                gio::FileCreateFlags::PRIVATE | gio::FileCreateFlags::REPLACE_DESTINATION,
                gio::Cancellable::NONE,
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_validation_and_restart() {
        let dir = std::env::temp_dir().join(format!("mdview-preferences-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.ini");
        assert_eq!(Preferences::load(&path), Preferences::default());
        for text in [
            "broken",
            "[Reading]\ntheme=invalid\nzoom=999\nwidth=-1\nheight=oops",
            &"a".repeat(16_385),
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(Preferences::load(&path), Preferences::default());
        }
        let saved = Preferences {
            theme: "dark".into(),
            zoom: 140,
            width: 960,
            height: 720,
        };
        saved.save(&path).unwrap();
        assert_eq!(Preferences::load(&path), saved);
        fs::remove_dir_all(dir).unwrap();
    }
}

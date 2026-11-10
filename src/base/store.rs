//! The saved views and library folders: JSON in the app's config folder through `rune::prefs`
//! (`~/.config/rune-music/prefs.json` on Linux, `%APPDATA%\rune-music` on Windows,
//! `~/Library/Application Support/rune-music` on macOS).

use rune::prefs::Prefs;
use serde::{Deserialize, Serialize};

use super::property::Property;
use super::view::View;

const APP_ID: &str = "rune-music";
const KEY: &str = "base";
const LIBRARY_KEY: &str = "library";
const ARTISTS_KEY: &str = "artists";
/// Bumped when the saved shape changes incompatibly.
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub version: u32,
    /// Never empty: the first view is "All songs" until the user renames or reorders it.
    pub views: Vec<View>,
    pub active: usize,
    /// The columns of album pages' and artist pages' song tables, which belong to no view.
    #[serde(default = "page_columns")]
    pub page_columns: Vec<Property>,
    /// Those tables' dragged column widths.
    #[serde(default)]
    pub page_widths: super::view::Widths,
}

/// What album and artist pages' song tables show after the title by default.
pub fn page_columns() -> Vec<Property> {
    vec![Property::Artist, Property::Format, Property::Duration]
}

impl Default for Base {
    fn default() -> Base {
        Base { version: VERSION, views: vec![View::new("All songs")], active: 0, page_columns: page_columns(), page_widths: Default::default() }
    }
}

impl Base {
    pub fn active(&self) -> &View {
        &self.views[self.active]
    }

    pub fn active_mut(&mut self) -> &mut View {
        &mut self.views[self.active]
    }

    /// Repairs what a hand-edited file could break: no views, or an active index past the end.
    fn normalized(mut self) -> Base {
        if self.views.is_empty() {
            self.views.push(View::new("All songs"));
        }
        self.active = self.active.min(self.views.len() - 1);
        self
    }
}

pub struct Store {
    prefs: Prefs,
}

impl Store {
    /// The user's saved views.
    pub fn open() -> Store {
        Store { prefs: Prefs::open(APP_ID) }
    }

    /// Views kept in this file.
    #[cfg(test)]
    pub fn at(path: impl Into<std::path::PathBuf>) -> Store {
        Store { prefs: Prefs::at(path) }
    }

    /// Nothing reaches disk: tests and headless runs.
    pub fn memory() -> Store {
        Store { prefs: Prefs::memory() }
    }

    /// The saved base, or the default when there is none or it's from an incompatible version.
    pub fn load(&self) -> Base {
        self.prefs.get::<Base>(KEY).filter(|b| b.version == VERSION).map_or_else(Base::default, Base::normalized)
    }

    pub fn save(&mut self, base: &Base) -> Result<(), String> {
        self.prefs.set(KEY, base).map_err(|e| format!("Couldn't save views: {e}"))
    }

    /// The saved library folders and files, scanned at launch; none when nothing is saved.
    pub fn load_library(&self) -> Vec<std::path::PathBuf> {
        self.prefs.get(LIBRARY_KEY).unwrap_or_default()
    }

    pub fn save_library(&mut self, roots: &[std::path::PathBuf]) -> Result<(), String> {
        self.prefs.set(LIBRARY_KEY, roots).map_err(|e| format!("Couldn't save the library folders: {e}"))
    }

    /// The merges and links between artists, hand-edited under `artists`; none when unset,
    /// an error when set but unreadable, so a typo isn't silently dropped.
    pub fn load_relations(&self) -> Result<crate::relations::Relations, String> {
        if !self.prefs.contains(ARTISTS_KEY) {
            return Ok(Default::default());
        }
        self.prefs.get(ARTISTS_KEY).ok_or_else(|| format!("Couldn't read \"{ARTISTS_KEY}\" in the preferences: check its merges and links"))
    }

    pub fn save_relations(&mut self, relations: &crate::relations::Relations) -> Result<(), String> {
        self.prefs.set(ARTISTS_KEY, relations).map_err(|e| format!("Couldn't save the artist links: {e}"))
    }
}

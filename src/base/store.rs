//! The saved views: JSON in the app's config folder through `rune::prefs`
//! (`~/.config/rune-music/prefs.json` on Linux, `%APPDATA%\rune-music` on Windows,
//! `~/Library/Application Support/rune-music` on macOS).

use rune::prefs::Prefs;
use serde::{Deserialize, Serialize};

use super::view::View;

const APP_ID: &str = "rune-music";
const KEY: &str = "base";
/// Bumped when the saved shape changes incompatibly.
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub version: u32,
    /// Never empty: the first view is "All songs" until the user renames or reorders it.
    pub views: Vec<View>,
    pub active: usize,
}

impl Default for Base {
    fn default() -> Base {
        Base { version: VERSION, views: vec![View::new("All songs")], active: 0 }
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
}

//! The saved views and library folders: JSON in the app's config folder through `rune::prefs`
//! (`~/.config/rune-music/prefs.json` on Linux, `%APPDATA%\rune-music` on Windows,
//! `~/Library/Application Support/rune-music` on macOS).

use rune::prefs::Prefs;
use serde::{Deserialize, Serialize};

use super::property::Property;
use super::view::View;

const APP_ID: &str = "rune-music";
const KEY: &str = "views";
const LIBRARY_KEY: &str = "library";
const ARTISTS_KEY: &str = "artists";
const LASTFM_KEY: &str = "lastfm";
const ZOOM_KEY: &str = "zoom";
const GRAPH_KEY: &str = "graph";
const TAB_KEY: &str = "tab";
/// Bumped when the saved shape changes incompatibly.
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Views {
    pub version: u32,
    /// Never empty: the first view is "All songs" until the user renames or reorders it.
    pub list: Vec<View>,
    pub active: usize,
    /// The columns of album pages' and artist pages' song tables, which belong to no view.
    #[serde(default = "page_columns")]
    pub page_columns: Vec<Property>,
    /// Those tables' dragged column widths.
    #[serde(default)]
    pub page_widths: super::view::Widths,
    /// The album grid card size of views that don't set their own.
    #[serde(default = "default_card_size")]
    pub card_size: f32,
}

fn default_card_size() -> f32 {
    super::view::CARD_SIZE_DEFAULT
}

/// What album and artist pages' song tables show after the title by default.
pub fn page_columns() -> Vec<Property> {
    vec![Property::Artist, Property::Format, Property::Duration]
}

impl Default for Views {
    fn default() -> Views {
        Views { version: VERSION, list: vec![View::new("All songs")], active: 0, page_columns: page_columns(), page_widths: Default::default(), card_size: default_card_size() }
    }
}

impl Views {
    pub fn active(&self) -> &View {
        &self.list[self.active]
    }

    pub fn active_mut(&mut self) -> &mut View {
        &mut self.list[self.active]
    }

    /// Repairs what a hand-edited file could break: no views, or an active index past the end.
    fn normalized(mut self) -> Views {
        if self.list.is_empty() {
            self.list.push(View::new("All songs"));
        }
        self.active = self.active.min(self.list.len() - 1);
        self.card_size = super::view::clamp_card_size(self.card_size);
        for view in &mut self.list {
            view.card_size = view.card_size.map(super::view::clamp_card_size);
        }
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

    /// The play log, beside the preferences file; in memory when they are. The error tells of
    /// lines it couldn't read.
    pub fn plays(&self) -> (crate::plays::Plays, Option<String>) {
        match self.prefs.path().and_then(|p| p.parent()) {
            Some(dir) => crate::plays::Plays::open(dir.join(crate::plays::FILE)),
            None => (crate::plays::Plays::memory(), None),
        }
    }

    /// The saved views, or the default when there is none or it's from an incompatible version.
    pub fn load(&self) -> Views {
        self.prefs.get::<Views>(KEY).filter(|b| b.version == VERSION).map_or_else(Views::default, Views::normalized)
    }

    pub fn save(&mut self, views: &Views) -> Result<(), String> {
        self.prefs.set(KEY, views).map_err(|e| format!("Couldn't save views: {e}"))
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

    /// The graph's camera and hidden link kinds; the default when unset, an error when unreadable.
    pub fn load_graph(&self) -> Result<crate::graph::state::Saved, String> {
        if !self.prefs.contains(GRAPH_KEY) {
            return Ok(Default::default());
        }
        self.prefs.get(GRAPH_KEY).ok_or_else(|| format!("Couldn't read \"{GRAPH_KEY}\" in the preferences"))
    }

    pub fn save_graph(&mut self, saved: &crate::graph::state::Saved) -> Result<(), String> {
        self.prefs.set(GRAPH_KEY, saved).map_err(|e| format!("Couldn't save the graph view: {e}"))
    }

    /// Where the graph's layout is cached, beside the preferences; none when they're in memory.
    pub fn graph_cache(&self) -> Option<std::path::PathBuf> {
        self.prefs.path().and_then(|p| p.parent()).map(|dir| dir.join(crate::graph::state::CACHE_FILE))
    }

    /// The tab last open, shown at launch; none when unset, an error when unreadable.
    pub fn load_tab(&self) -> Result<Option<crate::app::Tab>, String> {
        if !self.prefs.contains(TAB_KEY) {
            return Ok(None);
        }
        self.prefs.get(TAB_KEY).map(Some).ok_or_else(|| format!("Couldn't read \"{TAB_KEY}\" in the preferences"))
    }

    pub fn save_tab(&mut self, tab: &crate::app::Tab) -> Result<(), String> {
        self.prefs.set(TAB_KEY, tab).map_err(|e| format!("Couldn't save the open tab: {e}"))
    }

    /// The window's zoom, 1.0 when unset or unreadable.
    pub fn load_zoom(&self) -> f32 {
        self.prefs.get::<f32>(ZOOM_KEY).map_or(1.0, crate::app::clamp_zoom)
    }

    pub fn save_zoom(&mut self, zoom: f32) -> Result<(), String> {
        self.prefs.set(ZOOM_KEY, &zoom).map_err(|e| format!("Couldn't save the zoom: {e}"))
    }

    /// The Last.fm API credentials and session; none when unset, an error when unreadable.
    pub fn load_lastfm(&self) -> Result<Option<crate::lastfm::Saved>, String> {
        if !self.prefs.contains(LASTFM_KEY) {
            return Ok(None);
        }
        self.prefs.get(LASTFM_KEY).map(Some).ok_or_else(|| format!("Couldn't read \"{LASTFM_KEY}\" in the preferences"))
    }

    /// Saves the Last.fm settings, or removes them when there's nothing to keep.
    pub fn save_lastfm(&mut self, saved: &crate::lastfm::Saved) -> Result<(), String> {
        let result = match (&saved.credentials, &saved.session) {
            (None, None) => self.prefs.remove(LASTFM_KEY),
            _ => self.prefs.set(LASTFM_KEY, saved),
        };
        result.map_err(|e| format!("Couldn't save the Last.fm settings: {e}"))
    }
}

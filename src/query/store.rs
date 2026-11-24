//! The saved views and library folders: JSON in the app's config folder through `rune::prefs`
//! (`~/.config/peri/prefs.json` on Linux, `%APPDATA%\peri` on Windows,
//! `~/Library/Application Support/peri` on macOS).

use rune::prefs::Prefs;
use serde::{Deserialize, Serialize};

use super::property::Property;
use crate::queue::QueueReplace;
use super::view::View;
use crate::palette::{Appearance, Theme};

const APP_ID: &str = "peri";
const KEY: &str = "views";
const LIBRARY_KEY: &str = "library";
const ARTISTS_KEY: &str = "artists";
const LASTFM_KEY: &str = "lastfm";
const ZOOM_KEY: &str = "zoom";
const GRAPH_KEY: &str = "graph";
const TAB_KEY: &str = "tab";
const APPEARANCE_KEY: &str = "appearance";
const THEME_KEY: &str = "theme";
const UNLISTENED_ORDER_KEY: &str = "unlistened_order";
const TOP_WINDOW_KEY: &str = "top_window";
const DISCOVER_KEY: &str = "discover";
const QUEUE_REPLACE_KEY: &str = "queue_replace";
const QUEUE_PINNED_KEY: &str = "queue_pinned";
/// The play queue's file, beside the preferences.
const QUEUE_FILE: &str = "queue.json";
/// Bumped when the saved shape changes incompatibly.
const VERSION: u32 = 1;
/// Bumped when the saved queue's shape changes incompatibly.
pub const QUEUE_VERSION: u32 = 1;

/// The play queue as saved: tracks by path, so it survives a rescan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedQueue {
    pub version: u32,
    pub base: Vec<std::path::PathBuf>,
    pub order: Vec<SavedEntry>,
    pub pos: usize,
    pub edited: bool,
    pub removed: Vec<std::path::PathBuf>,
    pub shuffle: bool,
    /// Seconds into the current track.
    pub seconds: f64,
}

/// One place in the saved queue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedEntry {
    pub path: std::path::PathBuf,
    pub added: bool,
    pub pinned: bool,
}

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

    /// The play queue file, beside the preferences; none when they're in memory.
    fn queue_file(&self) -> Option<std::path::PathBuf> {
        self.prefs.path().and_then(|p| p.parent()).map(|dir| dir.join(QUEUE_FILE))
    }

    /// The saved play queue; none when nothing is saved. An error when the file is unreadable or
    /// from an incompatible version.
    pub fn load_queue(&self) -> Result<Option<SavedQueue>, String> {
        let Some(path) = self.queue_file() else { return Ok(None) };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Couldn't read the queue in {}: {e}", path.display())),
        };
        let saved: SavedQueue = serde_json::from_str(&text).map_err(|e| format!("Couldn't read the queue in {}: {e}", path.display()))?;
        if saved.version != QUEUE_VERSION {
            return Err(format!("Couldn't read the queue in {}: it's from another version of peri", path.display()));
        }
        Ok(Some(saved))
    }

    /// Writes the play queue, or removes the file when there is none. Nothing is written when
    /// the preferences are in memory.
    pub fn save_queue(&self, queue: Option<&SavedQueue>) -> Result<(), String> {
        let Some(path) = self.queue_file() else { return Ok(()) };
        let result = match queue {
            Some(saved) => serde_json::to_string(saved)
                .map_err(|e| e.to_string())
                .and_then(|text| {
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    }
                    std::fs::write(&path, text).map_err(|e| e.to_string())
                }),
            None => match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            },
        };
        result.map_err(|e| format!("Couldn't save the queue: {e}"))
    }

    /// What a new list does over an edited queue; Ask when unset, an error when unreadable.
    pub fn load_queue_replace(&self) -> Result<QueueReplace, String> {
        if !self.prefs.contains(QUEUE_REPLACE_KEY) {
            return Ok(QueueReplace::default());
        }
        self.prefs.get(QUEUE_REPLACE_KEY).ok_or_else(|| format!("Couldn't read \"{QUEUE_REPLACE_KEY}\" in the preferences"))
    }

    pub fn save_queue_replace(&mut self, policy: QueueReplace) -> Result<(), String> {
        self.prefs.set(QUEUE_REPLACE_KEY, &policy).map_err(|e| format!("Couldn't save the queue setting: {e}"))
    }

    /// Whether the queue panel is pinned open beside the page; false when unset.
    pub fn load_queue_pinned(&self) -> Result<bool, String> {
        if !self.prefs.contains(QUEUE_PINNED_KEY) {
            return Ok(false);
        }
        self.prefs.get(QUEUE_PINNED_KEY).ok_or_else(|| format!("Couldn't read \"{QUEUE_PINNED_KEY}\" in the preferences"))
    }

    pub fn save_queue_pinned(&mut self, pinned: bool) -> Result<(), String> {
        self.prefs.set(QUEUE_PINNED_KEY, &pinned).map_err(|e| format!("Couldn't save the queue panel: {e}"))
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

    /// The light, dark or system appearance; System when unset, an error when unreadable.
    pub fn load_appearance(&self) -> Result<Appearance, String> {
        if !self.prefs.contains(APPEARANCE_KEY) {
            return Ok(Appearance::default());
        }
        self.prefs.get(APPEARANCE_KEY).ok_or_else(|| format!("Couldn't read \"{APPEARANCE_KEY}\" in the preferences"))
    }

    pub fn save_appearance(&mut self, appearance: Appearance) -> Result<(), String> {
        self.prefs.set(APPEARANCE_KEY, &appearance).map_err(|e| format!("Couldn't save the appearance: {e}"))
    }

    /// The Unlistened Releases order; Newest when unset, an error when unreadable.
    pub fn load_unlistened_order(&self) -> Result<crate::app::UnlistenedOrder, String> {
        if !self.prefs.contains(UNLISTENED_ORDER_KEY) {
            return Ok(crate::app::UnlistenedOrder::default());
        }
        self.prefs.get(UNLISTENED_ORDER_KEY).ok_or_else(|| format!("Couldn't read \"{UNLISTENED_ORDER_KEY}\" in the preferences"))
    }

    pub fn save_unlistened_order(&mut self, order: crate::app::UnlistenedOrder) -> Result<(), String> {
        self.prefs.set(UNLISTENED_ORDER_KEY, &order).map_err(|e| format!("Couldn't save the Unlistened Releases order: {e}"))
    }

    /// The window the Home leaderboards show; all time when unset, an error when unreadable.
    pub fn load_top_window(&self) -> Result<crate::top::Window, String> {
        if !self.prefs.contains(TOP_WINDOW_KEY) {
            return Ok(crate::top::Window::default());
        }
        self.prefs.get(TOP_WINDOW_KEY).ok_or_else(|| format!("Couldn't read \"{TOP_WINDOW_KEY}\" in the preferences"))
    }

    pub fn save_top_window(&mut self, window: crate::top::Window) -> Result<(), String> {
        self.prefs.set(TOP_WINDOW_KEY, &window).map_err(|e| format!("Couldn't save the top window: {e}"))
    }

    /// The discovery tab; Unlistened when unset, an error when unreadable.
    pub fn load_discover(&self) -> Result<crate::discover::Tab, String> {
        if !self.prefs.contains(DISCOVER_KEY) {
            return Ok(crate::discover::Tab::default());
        }
        self.prefs.get(DISCOVER_KEY).ok_or_else(|| format!("Couldn't read \"{DISCOVER_KEY}\" in the preferences"))
    }

    pub fn save_discover(&mut self, tab: crate::discover::Tab) -> Result<(), String> {
        self.prefs.set(DISCOVER_KEY, &tab).map_err(|e| format!("Couldn't save the discovery tab: {e}"))
    }

    /// The theme; Bolis when unset, an error when unreadable.
    pub fn load_theme(&self) -> Result<Theme, String> {
        if !self.prefs.contains(THEME_KEY) {
            return Ok(Theme::default());
        }
        self.prefs.get(THEME_KEY).ok_or_else(|| format!("Couldn't read \"{THEME_KEY}\" in the preferences"))
    }

    pub fn save_theme(&mut self, theme: Theme) -> Result<(), String> {
        self.prefs.set(THEME_KEY, &theme).map_err(|e| format!("Couldn't save the theme: {e}"))
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

//! The player's state and everything that changes it: library scans, playback, the queue.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use rune::{FileDialogOptions, Image, ImageCache, Key, KeyEvent, TextEdit, Tx, Waker};

use crate::album::{self, Album, AlbumId};
use crate::audio::{self, Engine};
use crate::base::property::{Mode, Property};
use crate::covers::Covers;
use crate::base::store::{Base, Store};
use crate::base::view::{Evaluated, Layout, Section, Widths, evaluate};
use crate::library::{self, ScanMsg, Track};
use crate::queue::{Queue, Repeat, Rng};

/// How often the view refreshes the playback position while a track plays.
const TICK: Duration = Duration::from_millis(200);
/// Arrow-key and slider-key seek step, in seconds.
pub const SEEK_STEP: f64 = 5.0;
/// Previous restarts the track instead when it has played longer than this.
const RESTART_AFTER: f64 = 3.0;

/// Where sound goes.
pub enum Output {
    /// Not opened yet: it opens on first play, so launching never waits on the audio device.
    Closed,
    Open(Engine),
    /// Headless runs and tests: playback state changes, nothing sounds and time stands still.
    Silent,
}

pub struct Scan {
    rx: Receiver<ScanMsg>,
    /// Add to the library rather than replace it.
    append: bool,
    pub found: usize,
}

/// Which toolbar popover is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Views,
    Sort,
    Group,
    Filter,
    Columns,
}

/// What fills the space between the toolbar and the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    /// The active view.
    Library,
    /// One album's page, by identity so it survives a rescan.
    Album(AlbumId),
    /// One artist's page: their albums, the albums they appear on, their songs and credits.
    /// By name, matched ignoring case.
    Artist(String),
    /// One record label's page: the albums released under it. By name, matched ignoring case.
    Label(String),
    /// Artists, albums and songs matching the search field, which holds the query.
    Search,
    /// The app's settings: the library's folders.
    Settings,
}

/// The song details dialog: which song, and what was read from its file when it opened.
pub struct DetailsDialog {
    /// The song's file, so the dialog follows it across rescans.
    pub path: PathBuf,
    pub details: Result<crate::details::Details, String>,
    /// Whether "All tags" is expanded.
    pub raw_open: bool,
}

pub struct App {
    pub tracks: Vec<Track>,
    /// The library gathered into albums, in library order.
    pub albums: Vec<Album>,
    pub page: Page,
    /// The pages Back returns to, newest last; the library when empty.
    pub history: Vec<Page>,
    /// The artist page's Songs section, when toggled: `None` shows it collapsed if the artist
    /// has credits (so they come first), else open. Reset on every page change.
    pub artist_songs_open: Option<bool>,
    /// Album covers for cards and lists; `None` until setup has a waker.
    pub covers: Option<Covers>,
    /// Indices into `tracks` the active view shows, in play order. In an album view,
    /// every song of the shown albums, album by album.
    pub filtered: Vec<usize>,
    /// Album views: indices into `albums` the view shows, in order.
    pub shown_albums: Vec<usize>,
    /// The active view's groups, empty when it isn't grouped.
    pub sections: Vec<Section>,
    /// What the list shows: group headers and songs, skipping collapsed groups' songs.
    pub rows: Vec<Row>,
    /// Covers per grid row, from the list's width.
    pub columns: usize,
    /// Bumped whenever `filtered` or `sections` change, so the list knows its rows did.
    pub list_version: u64,
    /// The saved views; edits save at once.
    pub base: Base,
    /// The folders and files the library is made of, saved and scanned at launch.
    pub library: Vec<PathBuf>,
    store: Store,
    /// Collapsed group titles in the active view (this run only).
    pub collapsed: std::collections::HashSet<String>,
    pub panel: Option<Panel>,
    /// The popover select that's open, by key.
    pub open_select: Option<String>,
    /// The view being renamed in the views popover.
    pub renaming: Option<usize>,
    /// The rename field's editor. Always present: the runtime still reaches it to blur the
    /// field after a rename ends.
    pub rename: TextEdit,
    /// One editor per filter condition with a typed value, by path.
    pub filter_edits: std::collections::HashMap<Vec<usize>, TextEdit>,
    pub search: TextEdit,
    /// Each column's width as last laid out on screen, where a resize drag starts from.
    pub laid_out_widths: std::collections::HashMap<Property, f32>,
    /// Index into `tracks` of the highlighted row.
    pub selected: Option<usize>,
    /// Index into `tracks` of the loaded track.
    pub current: Option<usize>,
    pub queue: Option<Queue>,
    pub playing: bool,
    /// Seconds into the current track.
    pub position: f64,
    /// While the seek bar is dragged: where it would land, as a fraction of the track.
    pub seek_preview: Option<f32>,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub cover: Option<Image>,
    pub error: Option<String>,
    pub scan: Option<Scan>,
    pub drop_hover: bool,
    /// The open song details dialog.
    pub details: Option<DetailsDialog>,
    output: Output,
    rng: Rng,
    waker: Option<Waker>,
    images: Option<ImageCache>,
}

impl App {
    pub fn new(output: Output, store: Store) -> App {
        App {
            tracks: Vec::new(),
            albums: Vec::new(),
            page: Page::Library,
            history: Vec::new(),
            artist_songs_open: None,
            covers: None,
            filtered: Vec::new(),
            shown_albums: Vec::new(),
            sections: Vec::new(),
            rows: Vec::new(),
            columns: 4,
            list_version: 0,
            base: store.load(),
            library: store.load_library(),
            store,
            collapsed: Default::default(),
            panel: None,
            open_select: None,
            renaming: None,
            rename: TextEdit::new(""),
            filter_edits: Default::default(),
            search: TextEdit::new(""),
            laid_out_widths: Default::default(),
            selected: None,
            current: None,
            queue: None,
            playing: false,
            position: 0.0,
            seek_preview: None,
            volume: 0.8,
            muted: false,
            shuffle: false,
            repeat: Repeat::Off,
            cover: None,
            error: None,
            scan: None,
            drop_hover: false,
            details: None,
            output,
            rng: Rng::from_time(),
            waker: None,
            images: None,
        }
    }

    /// `AppSpec::setup`: keep the waker for the scan thread and cover decoding, then scan the
    /// folders or files named on the command line (for this run only), else the saved library.
    pub fn setup(&mut self, waker: Waker) {
        self.images = Some(ImageCache::new(waker.clone()));
        self.covers = Some(if matches!(self.output, Output::Silent) { Covers::synchronous(waker.clone()) } else { Covers::new(waker.clone()) });
        self.waker = Some(waker);
        let mut roots: Vec<PathBuf> = rune::extra_args().iter().map(PathBuf::from).collect();
        if roots.is_empty() {
            roots = self.library.clone();
        }
        if !roots.is_empty() {
            self.start_scan(roots, false);
            // Headless frames run on a virtual clock far faster than the scan, so a snapshot
            // waits for it and always shows the whole library.
            if matches!(self.output, Output::Silent) {
                while self.scan.is_some() {
                    self.drain_scan();
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    pub fn duration(&self) -> Option<f64> {
        self.current_track().and_then(|t| t.duration)
    }

    /// The position the view shows: the seek preview while dragging.
    pub fn shown_position(&self) -> f64 {
        match (self.seek_preview, self.duration()) {
            (Some(f), Some(d)) => f as f64 * d,
            _ => self.position,
        }
    }

    // Library

    pub fn start_scan(&mut self, roots: Vec<PathBuf>, append: bool) {
        let Some(waker) = self.waker.clone() else {
            self.error = Some("The library can't scan before the window is set up".into());
            return;
        };
        self.scan = Some(Scan { rx: library::scan(roots, move || waker()), append, found: 0 });
    }

    pub fn open_folder(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions { title: "Open a music folder".into(), ..Default::default() };
        tx.open_folder(options, |s: &mut App, result, _tx| match result {
            Ok(Some(path)) => s.replace_library(vec![path]),
            Ok(None) => {}
            Err(e) => s.error = Some(format!("Couldn't open the folder dialog: {e}")),
        });
    }

    /// Settings' "Add folder…": adds a folder to the saved library and scans it in.
    pub fn add_library_folder(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions { title: "Add a music folder".into(), ..Default::default() };
        tx.open_folder(options, |s: &mut App, result, _tx| match result {
            Ok(Some(path)) => s.add_to_library(vec![path]),
            Ok(None) => {}
            Err(e) => s.error = Some(format!("Couldn't open the folder dialog: {e}")),
        });
    }

    /// Makes `roots` the whole library, saved for next launch, and scans them.
    pub fn replace_library(&mut self, roots: Vec<PathBuf>) {
        self.library = roots.clone();
        self.save_library();
        self.start_scan(roots, false);
    }

    /// Adds `roots` (dropped or picked folders and files) to the saved library and scans them in.
    pub fn add_to_library(&mut self, roots: Vec<PathBuf>) {
        for root in &roots {
            if !self.library.contains(root) {
                self.library.push(root.clone());
            }
        }
        self.save_library();
        self.start_scan(roots, true);
    }

    /// Settings' remove button: drops saved root `i` and rescans what's left.
    pub fn remove_library_root(&mut self, i: usize) {
        if i >= self.library.len() {
            return;
        }
        self.library.remove(i);
        self.save_library();
        self.rescan_library();
    }

    /// Settings' "Forget library": no saved folders, and an empty library.
    pub fn forget_library(&mut self) {
        self.library.clear();
        self.save_library();
        self.rescan_library();
    }

    /// Scans the saved library afresh, replacing what's loaded; empties it when nothing is saved.
    pub fn rescan_library(&mut self) {
        if self.library.is_empty() {
            // Dropping the receiver stops a running scan.
            self.scan = None;
            self.set_library(Vec::new(), false);
            return;
        }
        self.start_scan(self.library.clone(), false);
    }

    fn save_library(&mut self) {
        if let Err(e) = self.store.save_library(&self.library) {
            self.error = Some(e);
        }
    }

    pub fn show_settings(&mut self) {
        self.go_to(Page::Settings);
    }

    /// Tests scan without a window: a waker that does nothing.
    #[cfg(test)]
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Takes the scan thread's messages; true when the view changes.
    fn drain_scan(&mut self) -> bool {
        let Some(scan) = &mut self.scan else { return false };
        let mut changed = false;
        let mut done = None;
        for msg in scan.rx.try_iter() {
            changed = true;
            match msg {
                ScanMsg::Progress(n) => scan.found = n,
                ScanMsg::Done { tracks, errors } => done = Some((tracks, errors)),
            }
        }
        if let Some((tracks, errors)) = done {
            let append = scan.append;
            self.scan = None;
            self.set_library(tracks, append);
            if let Some(first) = errors.first() {
                self.error = Some(match errors.len() {
                    1 => format!("Skipped {first}"),
                    n => format!("Skipped {first} and {} more", n - 1),
                });
            }
        }
        changed
    }

    /// Replaces (or adds to) the library, keeping the playing track and its place.
    pub fn set_library(&mut self, mut tracks: Vec<Track>, append: bool) {
        let playing_path = self.current_track().map(|t| t.path.clone());
        let selected_path = self.selected.and_then(|i| self.tracks.get(i)).map(|t| t.path.clone());
        if append {
            let mut all = std::mem::take(&mut self.tracks);
            all.retain(|t| !tracks.iter().any(|n| n.path == t.path));
            all.append(&mut tracks);
            library::sort(&mut all);
            tracks = all;
        }
        // Names one song's tags pair with IDs, or that stand alone, help split every other's.
        crate::credits::resolve(&mut tracks);
        library::sort(&mut tracks);
        self.tracks = tracks;
        self.albums = album::build(&self.tracks);
        let find = |path: &Option<PathBuf>, tracks: &[Track]| path.as_ref().and_then(|p| tracks.iter().position(|t| &t.path == p));
        self.selected = find(&selected_path, &self.tracks);
        let current = find(&playing_path, &self.tracks);
        self.refilter();
        match current {
            Some(i) => {
                self.current = Some(i);
                self.queue = Some(Queue::new(self.filtered_or(i), i, self.shuffle, &mut self.rng));
            }
            None if playing_path.is_some() => {
                // The playing file left the library: stop rather than play on unseen.
                self.stop();
            }
            None => {}
        }
    }

    /// Re-evaluates the active view over the library.
    pub fn refilter(&mut self) {
        let view = self.base.active();
        match view.mode {
            Mode::Songs => {
                let Evaluated { order, sections } = evaluate(view, &self.tracks, "");
                self.filtered = order;
                self.sections = sections;
                self.shown_albums.clear();
            }
            Mode::Albums => {
                let unsearched = evaluate(view, &self.albums, "");
                self.filtered = unsearched.order.iter().flat_map(|&a| self.albums[a].tracks.iter().copied()).collect();
                self.shown_albums = unsearched.order;
                self.sections = unsearched.sections;
            }
        }
        self.rebuild_rows();
    }

    pub fn mode(&self) -> Mode {
        self.base.active().mode
    }

    /// The album shown on the album page, if it's still in the library.
    pub fn open_album(&self) -> Option<usize> {
        match &self.page {
            Page::Album(id) => self.albums.iter().position(|a| &a.id() == id),
            Page::Library | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings => None,
        }
    }

    pub fn show_album(&mut self, a: usize, tx: &mut Tx) {
        if let Some(album) = self.albums.get(a) {
            self.go_to(Page::Album(album.id()));
            tx.scroll_to_instant("album-page", 0.0, 0.0);
        }
    }

    /// The song table columns on screen: the album or artist page's, else the view's.
    pub fn shown_columns(&self) -> &[Property] {
        match self.page {
            Page::Library => &self.base.active().columns,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings => &self.base.page_columns,
        }
    }

    /// The columns the Columns popover edits: the ones on screen.
    pub fn shown_columns_mut(&mut self) -> &mut Vec<Property> {
        match self.page {
            Page::Library => &mut self.base.active_mut().columns,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings => &mut self.base.page_columns,
        }
    }

    /// The dragged column widths of the tables on screen, which go with their columns.
    pub fn shown_widths(&self) -> &Widths {
        match self.page {
            Page::Library => &self.base.active().widths,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings => &self.base.page_widths,
        }
    }

    pub fn shown_widths_mut(&mut self) -> &mut Widths {
        match self.page {
            Page::Library => &mut self.base.active_mut().widths,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings => &mut self.base.page_widths,
        }
    }

    /// The artist shown on the artist page.
    pub fn open_artist(&self) -> Option<&str> {
        match &self.page {
            Page::Artist(name) => Some(name),
            Page::Library | Page::Album(_) | Page::Label(_) | Page::Search | Page::Settings => None,
        }
    }

    pub fn show_artist(&mut self, name: &str, tx: &mut Tx) {
        self.go_to(Page::Artist(name.to_string()));
        tx.scroll_to_instant("artist-page", 0.0, 0.0);
    }

    /// The label shown on the label page.
    pub fn open_label(&self) -> Option<&str> {
        match &self.page {
            Page::Label(name) => Some(name),
            Page::Library | Page::Album(_) | Page::Artist(_) | Page::Search | Page::Settings => None,
        }
    }

    pub fn show_label(&mut self, name: &str, tx: &mut Tx) {
        self.go_to(Page::Label(name.to_string()));
        tx.scroll_to_instant("label-page", 0.0, 0.0);
    }

    /// Opens `page`, remembering the one it leaves for Back.
    fn go_to(&mut self, page: Page) {
        self.close_panel();
        self.details = None;
        if page == self.page {
            return;
        }
        self.artist_songs_open = None;
        let left = std::mem::replace(&mut self.page, page);
        if left != Page::Library {
            self.history.push(left);
        }
    }

    /// Back: the page before this one, else the library.
    pub fn go_back(&mut self) {
        self.page = self.history.pop().unwrap_or(Page::Library);
        self.artist_songs_open = None;
    }

    pub fn show_library(&mut self) {
        self.history.clear();
        self.page = Page::Library;
    }

    /// Home: the active view, the search cleared.
    pub fn go_home(&mut self) {
        self.close_panel();
        self.details = None;
        self.search.clear();
        self.artist_songs_open = None;
        self.show_library();
    }

    /// Follows the search field: a query opens the results (returning to them when they're
    /// further back); clearing it leaves them for the page searched from.
    pub fn searched(&mut self, tx: &mut Tx) {
        let searching = !self.search.text.trim().is_empty();
        let back_at = self.history.iter().position(|p| *p == Page::Search);
        if searching {
            if self.page == Page::Search {
                return;
            }
            // A toolbar popover would be left open with its toolbar hidden.
            self.close_panel();
            match back_at {
                Some(at) => {
                    self.history.truncate(at);
                    self.page = Page::Search;
                    self.artist_songs_open = None;
                }
                None => self.go_to(Page::Search),
            }
            tx.scroll_to_instant("search-page", 0.0, 0.0);
        } else if self.page == Page::Search {
            self.go_back();
        } else if let Some(at) = back_at {
            // Pages opened from the results go back past where the results were.
            self.history.remove(at);
        }
    }

    /// Clears the search, leaving the results.
    pub fn clear_search(&mut self, tx: &mut Tx) {
        self.search.clear();
        self.searched(tx);
    }

    /// Plays song `i` of `list`, queueing the rest of `list` (an artist's songs).
    /// Opens song `i`'s details, reading its file now.
    pub fn show_details(&mut self, i: usize, _tx: &mut Tx) {
        let Some(path) = self.tracks.get(i).map(|t| t.path.clone()) else { return };
        self.close_panel();
        self.selected = Some(i);
        let details = crate::details::read(&path);
        self.details = Some(DetailsDialog { path, details, raw_open: false });
    }

    pub fn close_details(&mut self) {
        self.details = None;
    }

    /// The song the details dialog shows, if it's still in the library.
    pub fn details_track(&self) -> Option<usize> {
        let open = self.details.as_ref()?;
        self.tracks.iter().position(|t| t.path == open.path)
    }

    /// Shows the details dialog's song in the system's file manager.
    pub fn reveal_details_file(&mut self) {
        let Some(path) = self.details.as_ref().map(|d| d.path.clone()) else { return };
        if matches!(self.output, Output::Silent) {
            return;
        }
        if let Err(e) = reveal(&path) {
            self.error = Some(e);
        }
    }

    /// Plays song `i` of `list`, queueing the rest of `list` (an artist's songs).
    pub fn play_list(&mut self, list: Vec<usize>, i: usize, tx: &mut Tx) {
        self.selected = Some(i);
        self.queue = Some(Queue::new(list, i, self.shuffle, &mut self.rng));
        self.load(i, tx);
    }

    /// The album a song belongs to.
    pub fn album_of(&self, track: usize) -> Option<usize> {
        self.albums.iter().position(|a| a.tracks.contains(&track))
    }

    /// Plays album `a` from its song `from` (else its first), queueing the albums after it
    /// in the view's order (or just this album when it isn't in the view).
    pub fn play_album(&mut self, a: usize, from: Option<usize>, tx: &mut Tx) {
        let Some(album) = self.albums.get(a) else { return };
        let Some(&first) = from.or(album.tracks.first().copied()).as_ref() else { return };
        let list = if self.shown_albums.contains(&a) { self.filtered.clone() } else { album.tracks.clone() };
        self.selected = Some(first);
        self.queue = Some(Queue::new(list, first, false, &mut self.rng));
        if self.shuffle {
            self.shuffle = false;
        }
        self.load(first, tx);
    }

    /// Shuffles album `a` alone.
    pub fn shuffle_album(&mut self, a: usize, tx: &mut Tx) {
        let Some(album) = self.albums.get(a) else { return };
        let list = album.tracks.clone();
        let start = list[(self.rng.below(list.len() as u64)) as usize];
        self.shuffle = true;
        self.queue = Some(Queue::new(list, start, true, &mut self.rng));
        self.selected = Some(start);
        self.load(start, tx);
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.compute_rows();
        self.list_version += 1;
    }

    /// Saves the views and re-evaluates the active one.
    pub fn views_changed(&mut self) {
        if let Err(e) = self.store.save(&self.base) {
            self.error = Some(e);
        }
        self.refilter();
    }

    /// The visible list when it holds `i`, else the whole library.
    fn filtered_or(&self, i: usize) -> Vec<usize> {
        if self.filtered.contains(&i) { self.filtered.clone() } else { (0..self.tracks.len()).collect() }
    }

    fn compute_rows(&self) -> Vec<Row> {
        let grid = self.mode() == Mode::Albums && self.base.active().layout == Layout::Grid;
        let columns = self.columns.max(1);
        let items = |list: &[usize], section: Option<usize>, rows: &mut Vec<Row>| {
            if grid {
                rows.extend((0..list.len()).step_by(columns).map(|start| Row::Grid { start, section }));
                return;
            }
            rows.extend(list.iter().map(|&i| match self.mode() {
                Mode::Songs => Row::Track { track: i, section },
                Mode::Albums => Row::Album { album: i, section },
            }));
        };
        let mut rows = Vec::new();
        if self.sections.is_empty() {
            let list = match self.mode() {
                Mode::Songs => &self.filtered,
                Mode::Albums => &self.shown_albums,
            };
            items(list, None, &mut rows);
            return rows;
        }
        for (n, section) in self.sections.iter().enumerate() {
            rows.push(Row::Header(n));
            if !self.collapsed.contains(&section.title) {
                items(&section.items, Some(n), &mut rows);
            }
        }
        rows
    }

    /// The grid's column count changed with the window's width.
    pub fn set_columns(&mut self, columns: usize) {
        if columns != self.columns {
            self.columns = columns;
            self.rebuild_rows();
        }
    }

    /// The albums of a grid row.
    pub fn grid_albums(&self, start: usize, section: Option<usize>) -> &[usize] {
        let list = match section {
            Some(n) => &self.sections[n].items,
            None => &self.shown_albums,
        };
        let end = (start + self.columns.max(1)).min(list.len());
        &list[start.min(end)..end]
    }

    pub fn toggle_section(&mut self, title: &str) {
        if !self.collapsed.remove(title) {
            self.collapsed.insert(title.to_string());
        }
        self.rebuild_rows();
    }

    // Playback

    /// Plays library track `i`, queueing the visible list around it.
    pub fn play_index(&mut self, i: usize, tx: &mut Tx) {
        self.selected = Some(i);
        self.queue = Some(Queue::new(self.filtered_or(i), i, self.shuffle, &mut self.rng));
        self.load(i, tx);
    }

    fn load(&mut self, i: usize, tx: &mut Tx) {
        let Some(track) = self.tracks.get(i) else { return };
        let path = track.path.clone();
        let title = match track.artist() {
            Some(artist) => format!("{} — {artist}", track.title),
            None => track.title.clone(),
        };
        self.current = Some(i);
        self.position = 0.0;
        self.seek_preview = None;
        let gain = audio::gain(self.volume, self.muted);
        let loaded = match &mut self.output {
            Output::Silent => Ok(()),
            Output::Open(engine) => engine.load(&path, gain),
            Output::Closed => Engine::open().and_then(|mut engine| {
                let loaded = engine.load(&path, gain);
                self.output = Output::Open(engine);
                loaded
            }),
        };
        match loaded {
            Ok(()) => {
                self.playing = true;
                self.error = None;
                tx.set_title(title);
                tx.wake_after(TICK);
            }
            Err(e) => {
                self.playing = false;
                self.error = Some(e);
            }
        }
        self.cover = self.images.as_ref().and_then(|cache| library::read_cover(&path).map(|bytes| cache.load(bytes)));
    }

    pub fn stop(&mut self) {
        if let Output::Open(engine) = &mut self.output {
            engine.stop();
        }
        self.playing = false;
        self.current = None;
        self.queue = None;
        self.position = 0.0;
        self.cover = None;
    }

    /// Play/pause; with nothing loaded, plays the selected row, else the first visible one.
    pub fn toggle(&mut self, tx: &mut Tx) {
        if self.current.is_none() {
            if let Some(i) = self.selected.or(self.filtered.first().copied()) {
                self.play_index(i, tx);
            }
            return;
        }
        // A finished queue stays loaded but empty: play starts the track again.
        if !self.playing && self.engine().is_some_and(Engine::finished) {
            if let Some(i) = self.current {
                self.load(i, tx);
            }
            return;
        }
        self.playing = !self.playing;
        if let Some(engine) = self.engine() {
            if self.playing { engine.play() } else { engine.pause() }
        }
        if self.playing {
            tx.wake_after(TICK);
        }
    }

    pub fn next(&mut self, tx: &mut Tx) {
        // Next always moves on, even repeating one track; past the end it wraps.
        let repeat = if self.repeat == Repeat::One { Repeat::All } else { self.repeat };
        match self.queue.as_mut().and_then(|q| q.next(repeat)) {
            Some(i) => self.load(i, tx),
            None => self.finish(),
        }
    }

    pub fn prev(&mut self, tx: &mut Tx) {
        if self.position > RESTART_AFTER {
            self.seek(0.0);
            return;
        }
        let repeat = if self.repeat == Repeat::One { Repeat::All } else { self.repeat };
        match self.queue.as_mut().and_then(|q| q.prev(repeat)) {
            Some(i) => self.load(i, tx),
            None => self.seek(0.0),
        }
    }

    /// The queue ran out: keep the last track shown, stopped at its start.
    fn finish(&mut self) {
        if let Output::Open(engine) = &mut self.output {
            engine.stop();
        }
        self.playing = false;
        self.position = 0.0;
    }

    fn track_ended(&mut self, tx: &mut Tx) {
        match (self.repeat, self.current) {
            (Repeat::One, Some(i)) => self.load(i, tx),
            _ => match self.queue.as_mut().and_then(|q| q.next(self.repeat)) {
                Some(i) => self.load(i, tx),
                None => self.finish(),
            },
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        let end = self.duration().unwrap_or(f64::MAX);
        let to = seconds.clamp(0.0, end);
        match self.engine().map(|e| e.seek(to)) {
            Some(Err(e)) => self.error = Some(e),
            Some(Ok(())) | None => self.position = to,
        }
    }

    pub fn seek_by(&mut self, delta: f64) {
        if self.current.is_some() {
            self.seek(self.position + delta);
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = false;
        self.apply_gain();
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.apply_gain();
    }

    fn apply_gain(&self) {
        if let Some(engine) = self.engine() {
            engine.set_gain(audio::gain(self.volume, self.muted));
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if let Some(q) = &mut self.queue {
            if self.shuffle { q.shuffle(&mut self.rng) } else { q.unshuffle() }
        }
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.cycle();
    }

    fn engine(&self) -> Option<&Engine> {
        match &self.output {
            Output::Open(engine) => Some(engine),
            Output::Closed | Output::Silent => None,
        }
    }

    // Selection

    /// Moves the highlighted row by `delta` among the songs shown (skipping collapsed groups).
    pub fn move_selection(&mut self, delta: isize, tx: &mut Tx) {
        let shown: Vec<(usize, Option<usize>)> = self.rows.iter().filter_map(|r| match *r {
            Row::Track { track, section } => Some((track, section)),
            Row::Header(_) | Row::Album { .. } | Row::Grid { .. } => None,
        }).collect();
        if shown.is_empty() {
            return;
        }
        // In a grouped view a song can show twice; start from its first row.
        let at = self.selected.and_then(|i| shown.iter().position(|&(t, _)| t == i));
        let last = shown.len() as isize - 1;
        let to = match at {
            Some(at) => (at as isize + delta).clamp(0, last),
            None if delta < 0 => last,
            None => 0,
        } as usize;
        let (i, section) = shown[to];
        self.selected = Some(i);
        tx.scroll_into_view_in("tracks", row_key(i, section));
    }
}

/// A row of the track list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A group's header, by index into `App::sections`.
    Header(usize),
    Track { track: usize, section: Option<usize> },
    /// Album views: an album card in the list layout.
    Album { album: usize, section: Option<usize> },
    /// Album views in the grid layout: a row of up to `App::columns` covers, from `start`
    /// (an index into the section's albums, or into `shown_albums` when ungrouped).
    Grid { start: usize, section: Option<usize> },
}

/// A song row's key: unique per group, since a song can be in several.
impl App {
    pub fn close_panel(&mut self) {
        self.panel = None;
        self.open_select = None;
        self.renaming = None;
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        let open = self.panel != Some(panel);
        self.close_panel();
        if open {
            self.panel = Some(panel);
        }
    }
}

pub fn row_key(i: usize, section: Option<usize>) -> String {
    match section {
        Some(n) => format!("g{n}t{i}"),
        None => format!("t{i}"),
    }
}

/// An album row's key, unique per group as a song's is.
pub fn album_key(a: usize, section: Option<usize>) -> String {
    match section {
        Some(n) => format!("g{n}a{a}"),
        None => format!("a{a}"),
    }
}

pub fn grid_key(start: usize, section: Option<usize>) -> String {
    match section {
        Some(n) => format!("g{n}r{start}"),
        None => format!("r{start}"),
    }
}

pub fn header_key(n: usize) -> String {
    format!("g{n}")
}

/// `AppSpec::poll`: take scan results, follow the playing track, and move on when it ends.
/// Opens the folder holding `path` in the system's file manager, selecting the file where the
/// platform can.
fn reveal(path: &std::path::Path) -> Result<(), String> {
    let Some(dir) = path.parent() else { return Err(format!("{} has no folder", path.display())) };
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    } else {
        // The file manager D-Bus interface selects the file; xdg-open only opens its folder.
        let uri = format!("file://{}", path.display());
        let shown = std::process::Command::new("dbus-send")
            .args(["--session", "--dest=org.freedesktop.FileManager1", "--type=method_call", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems"])
            .arg(format!("array:string:{uri}"))
            .arg("string:")
            .status()
            .is_ok_and(|s| s.success());
        if shown {
            return Ok(());
        }
        let mut c = std::process::Command::new("xdg-open");
        c.arg(dir);
        c
    };
    cmd.spawn().map(drop).map_err(|e| format!("Couldn't open {}: {e}", dir.display()))
}

/// `AppSpec::poll`: take scan results, follow the playing track, and move on when it ends.
pub fn poll(s: &mut App, _: f64, tx: &mut Tx) -> bool {
    let mut changed = s.drain_scan();
    if let Some(covers) = &s.covers {
        changed |= covers.drain();
    }
    if s.playing {
        let (finished, position) = s.engine().map_or((false, None), |e| (e.finished(), e.position()));
        if finished {
            s.track_ended(tx);
            changed = true;
        } else if let Some(p) = position.filter(|p| (p - s.position).abs() > 0.05) {
            s.position = p;
            changed = true;
        }
        if s.playing {
            tx.wake_after(TICK);
        }
    }
    changed
}

/// `AppSpec::on_key`: keys nothing focused took.
pub fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let m = ev.mods;
    if s.panel.is_some() {
        // Escape closes an open popover; other keys stay with it.
        if ev.key == Key::Escape {
            s.close_panel();
            return true;
        }
        return false;
    }
    if ev.key == Key::Escape && s.page == Page::Search {
        s.clear_search(tx);
        return true;
    }
    if ev.key == Key::Escape && s.page != Page::Library {
        s.go_back();
        return true;
    }
    if ev.is_command('f') {
        s.search.focused = true;
        return true;
    }
    if ev.is_command('o') {
        s.open_folder(tx);
        return true;
    }
    match &ev.key {
        Key::Text(t) if t == " " && !m.cmd => s.toggle(tx),
        Key::Right if m.cmd || m.ctrl => s.next(tx),
        Key::Left if m.cmd || m.ctrl => s.prev(tx),
        Key::Right => s.seek_by(SEEK_STEP),
        Key::Left => s.seek_by(-SEEK_STEP),
        Key::Down => s.move_selection(1, tx),
        Key::Up => s.move_selection(-1, tx),
        Key::Enter => match s.selected {
            Some(i) => s.play_index(i, tx),
            None => return false,
        },
        Key::Text(t) if !m.cmd && !m.ctrl && !m.alt => match t.as_str() {
            "s" => s.toggle_shuffle(),
            "r" => s.cycle_repeat(),
            "m" => s.toggle_mute(),
            "i" => match s.selected.or(s.current) {
                Some(i) => s.show_details(i, tx),
                None => return false,
            },
            _ => return false,
        },
        _ => return false,
    }
    true
}

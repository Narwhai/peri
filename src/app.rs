//! The player's state and everything that changes it: library scans, playback, the queue.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use rune::{FileDialogOptions, Key, KeyEvent, Live, PointerPress, TextEdit, Tx, Waker};
use serde::{Deserialize, Serialize};

use crate::album::{self, Album, AlbumId};
use crate::audio::{self, Engine};
use crate::query::property::{Mode, Property};
use crate::covers::Covers;
use crate::pages::{ArtistModel, Pages, TopModel};
use crate::search::SearchResults;
use crate::collab::CollabPage;
use crate::label::LabelPage;
use crate::query::store::{SavedEntry, SavedQueue, Store, QUEUE_VERSION, Views};
use crate::query::view::{Evaluated, Layout, Section, Widths, evaluate, evaluate_matching, evaluate_search, matches_fields};
use crate::lastfm::{self, scrobbling::{Effect, Scrobbling}};
use crate::library::{self, ScanMsg, Track};
use crate::listen::{Listen, unix_now};
use crate::palette::{Appearance, Theme};
use crate::plays::Plays;
use crate::stats::Totals;
use crate::queue::{Entry, Queue, QueueReplace, Repeat, Rng};

/// How often the view refreshes the playback position while a track plays.
const TICK: Duration = Duration::from_millis(200);
/// The window's zoom range and keyboard step.
pub const ZOOM_MIN: f32 = 0.5;
pub const ZOOM_MAX: f32 = 2.0;
const ZOOM_STEP: f32 = 0.1;
/// How long the zoom level shows after a keyboard zoom.
const ZOOM_TOAST: Duration = Duration::from_millis(1200);
/// How long filter edits wait for a pause before the views are saved.
const SAVE_VIEWS_AFTER: Duration = Duration::from_millis(500);
/// Arrow-key and slider-key seek step, in seconds.
pub const SEEK_STEP: f64 = 5.0;
/// Previous restarts the track instead when it has played longer than this.
const RESTART_AFTER: f64 = 3.0;
/// How many albums Recently Added lists on the home page.
pub const RECENT_ALBUMS_COUNT: usize = 20;
/// How many songs Recently Played lists on the home page.
pub const RECENT_SONGS_COUNT: usize = 10;
/// How many albums Unlistened Releases lists on the home page.
pub const UNLISTENED_COUNT: usize = 20;
/// The queue panel's scroll view, and one row's height in it (its rows are a fixed height so the
/// scroll offset is computable).
pub const QUEUE_LIST_KEY: &str = "queue-list";
pub const QUEUE_ROW_H: f32 = 52.0;
/// How far a queue row may poke past the list's edge and still count as in view, in points.
const QUEUE_VISIBLE_SLACK: f32 = 0.5;

/// Where sound goes.
pub enum Output {
    /// Not opened yet: it opens on first play, so launching never waits on the audio device.
    Closed,
    Open(Engine),
    /// Headless runs and tests: playback state changes, nothing sounds and time stands still.
    Silent,
}

/// A list to start once the user has said what happens to an edited queue.
pub struct PendingPlay {
    queue: Queue,
    start: usize,
    /// The shuffle setting the start sets, if it sets one.
    shuffle: Option<bool>,
}

/// The user's answer to the queue-replacement question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueChoice {
    Replace,
    KeepAdded,
    Cancel,
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
    /// The artist page's Link artist popover.
    LinkArtist,
}

/// A popover list whose rows a grip drags into a new order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderList {
    Sorts,
    Columns,
    /// The queue panel's entries; only the upcoming ones move.
    Queue,
}

/// A row being dragged by its grip: `from` is its index in `list`, and `dy` how far the pointer
/// has moved since the press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reorder {
    pub list: ReorderList,
    pub from: usize,
    pub dy: f32,
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
    /// What two or more artists released or recorded together. By name, matched ignoring case.
    Collab(Vec<String>),
    /// Artists, albums and songs matching the search field, which holds the query.
    Search,
    /// The app's settings: the library's folders.
    Settings,
    /// Every artist, linked to those they've worked with.
    Graph,
    /// The home tab: the newest albums and the songs played last.
    Home,
}

/// The header's tabs, which the window remembers across launches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    Home,
    Library,
    Graph,
}

/// The order of the home page's Unlistened Releases, which the preferences keep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnlistenedOrder {
    /// Newest added first. The default when unset.
    #[default]
    Newest,
    Oldest,
    /// A random selection, re-rolled from the home page.
    Random,
}

impl UnlistenedOrder {
    pub const ALL: [UnlistenedOrder; 3] = [UnlistenedOrder::Newest, UnlistenedOrder::Oldest, UnlistenedOrder::Random];

    /// The value saved in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            UnlistenedOrder::Newest => "newest",
            UnlistenedOrder::Oldest => "oldest",
            UnlistenedOrder::Random => "random",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UnlistenedOrder::Newest => "Newest",
            UnlistenedOrder::Oldest => "Oldest",
            UnlistenedOrder::Random => "Random",
        }
    }
}

/// The song details dialog: which song, and what was read from its file when it opened.
pub struct DetailsDialog {
    /// The song's file, so the dialog follows it across rescans.
    pub path: PathBuf,
    /// What the file says; `None` while the details thread is still reading it.
    pub details: Option<Result<crate::details::Details, String>>,
    /// The details thread's result, until it arrives.
    reading: Option<Receiver<Result<crate::details::Details, String>>>,
    /// Whether "All tags" is expanded.
    pub raw_open: bool,
}

/// The artist page's tab: their releases, or every song on them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArtistTab {
    /// Their albums, EPs, singles and compilations as strips.
    #[default]
    Releases,
    /// Every song on their releases in one table.
    Songs,
}

/// What a song menu acts on: one song or one album.
#[derive(Clone, Debug, PartialEq)]
pub enum SongMenuTarget {
    Track(usize),
    Album(usize),
}

pub struct App {
    tracks: Vec<Track>,
    /// The library gathered into albums, in library order.
    albums: Vec<Album>,
    /// For each track, the index in `albums` of the album it is on; see `album_of`.
    album_index: Vec<Option<usize>>,
    /// Bumped whenever the tracks, albums or relations change; page models are keyed by it.
    data_version: u64,
    /// The page models last built, reused while their inputs are unchanged.
    pages: Pages,
    pub page: Page,
    /// The pages Back returns to, newest last; the library when empty.
    pub history: Vec<Page>,
    /// The pages Forward returns to, the next one last: those Back left. Opening a page clears it.
    pub forward: Vec<Page>,
    /// The artist page's tab, Releases or Songs. Reset on every page change.
    pub artist_tab: ArtistTab,
    /// Whether the artist page's Credits section is folded. Reset on every page change.
    pub artist_credits_closed: bool,
    /// The artist page's bands and members sections that are open, by the other artist's name.
    /// Reset on every page change.
    pub artist_related_open: Vec<String>,
    /// Whether artist pages take in what the artist released under their aliases. Kept from
    /// page to page for this run.
    pub artist_with_aliases: bool,
    /// Where the library list is scrolled to (x, y), kept current while it shows.
    pub library_scroll: Live<[f32; 2]>,
    /// `library_scroll` when a page last opened from the library, where Back returns to.
    library_scroll_saved: [f32; 2],
    /// The Home page's scroll, kept current while it shows.
    pub home_scroll: Live<[f32; 2]>,
    /// `home_scroll` when a page last opened from Home, where Back returns to.
    home_scroll_saved: [f32; 2],
    /// The Home strip's sideways scroll as `[offset, max]`, kept current while it shows.
    pub home_strip: Live<[f32; 2]>,
    /// The Home strip's viewport width, kept current while it shows.
    pub home_viewport: Live<f32>,
    /// The order of Unlistened Releases, kept in the preferences.
    pub unlistened_order: UnlistenedOrder,
    /// The seed of Unlistened Releases' Random order; a re-roll sets a new one.
    pub unlistened_seed: u64,
    /// The window the Home leaderboards show, kept in the preferences.
    pub top_window: crate::top::Window,
    /// The tab of the home page's discovery block, kept in the preferences.
    pub discover: crate::discover::Tab,
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
    /// The album grid's inner width as last laid out, which the column count follows.
    grid_width: Option<f32>,
    /// The colour theme, saved.
    pub theme: Theme,
    /// Light, dark or system appearance, saved.
    pub appearance: Appearance,
    /// The window's zoom: 1.0 is 100%, saved.
    pub zoom: f32,
    /// While the zoom slider is dragged: the zoom it would set. The zoom lands on release, so
    /// the slider doesn't move under the pointer.
    pub zoom_preview: Option<f32>,
    /// When a keyboard zoom last changed the zoom, which shows the level for a moment.
    pub zoom_toast: Option<std::time::Instant>,
    /// When a filter edit last changed the views without saving them; saved after a pause.
    views_unsaved: Option<std::time::Instant>,
    /// Bumped whenever `filtered` or `sections` change, so the list knows its rows did.
    pub list_version: u64,
    /// The saved views; edits save at once.
    pub views: Views,
    /// The folders and files the library is made of, saved and scanned at launch.
    pub library: Vec<PathBuf>,
    /// Merges and links between artists, from the preferences file (edited by hand for now).
    relations: crate::relations::Relations,
    /// Set when the saved relations couldn't be read: editing them would overwrite the file.
    relations_unreadable: bool,
    /// The Link artist popover: what the picked name becomes, and the name field.
    pub link_choice: crate::relations::LinkChoice,
    pub link_name: TextEdit,
    store: Store,
    /// Collapsed group titles in the active view (this run only).
    pub collapsed: std::collections::HashSet<String>,
    pub panel: Option<Panel>,
    /// The popover select that's open, by key.
    pub open_select: Option<String>,
    /// The drag of a sort or column grip in progress, if any.
    pub reorder: Option<Reorder>,
    /// The view being renamed in the views popover.
    pub renaming: Option<usize>,
    /// The rename field's editor. Always present: the runtime still reaches it to blur the
    /// field after a rename ends.
    pub rename: TextEdit,
    /// One editor per filter condition with a typed value, by path.
    pub filter_edits: std::collections::HashMap<Vec<usize>, TextEdit>,
    pub search: TextEdit,
    /// The view search box's text, shared by every view. Not saved; cleared when the view or mode changes.
    pub view_search: TextEdit,
    /// `view_search`'s words, lowercased, as last evaluated. The rows highlight them.
    pub search_words: Vec<String>,
    /// Album list views with a view search: the songs matching it, which the album cards show.
    /// `None` otherwise. Set by `refilter`.
    pub view_matches: Option<HashSet<usize>>,
    /// Each column's width as last laid out on screen, where a resize drag starts from.
    pub laid_out_widths: std::collections::HashMap<Property, f32>,
    /// Index into `tracks` of the highlighted row.
    pub selected: Option<usize>,
    /// Index into `tracks` of the loaded track.
    pub current: Option<usize>,
    pub queue: Option<Queue>,
    /// What a new list does over an edited queue, saved.
    pub queue_replace: QueueReplace,
    /// The queue panel is pinned: docked beside the page and shown at launch. Saved.
    pub queue_pinned: bool,
    /// The queue panel is showing. Pinned, it's docked; otherwise it floats over the page.
    /// Not saved: a pinned panel starts shown, an unpinned one starts hidden.
    pub queue_shown: bool,
    /// The replace prompt's "Don't ask again" checkbox, while the prompt is open.
    pub queue_dont_ask: bool,
    /// Where the queue list is scrolled, as `[offset y, viewport height]`, kept current while it shows.
    pub queue_scroll: Live<[f32; 2]>,
    /// A restored queue's panel scrolls to its place once it's on screen: the next `poll`.
    queue_scroll_due: bool,
    /// A list waiting on the user's answer, when the queue was edited and the policy is to ask.
    pub pending_play: Option<PendingPlay>,
    /// The queue saved at the last exit, restored once the library loads.
    queue_restore: Option<SavedQueue>,
    /// Until the first library load: nothing is saved, so the saved queue isn't lost first.
    restore_pending: bool,
    /// The track isn't loaded yet (restored, or the queue ran out): Play loads it at `position`.
    resume: bool,
    pub playing: bool,
    /// Seconds into the current track.
    pub position: f64,
    /// While the seek bar is dragged: where it would land, as a fraction of the track.
    pub seek_preview: Option<f32>,
    pub volume: f32,
    pub muted: bool,
    /// While the volume knob is held: the volume and mute it started from, which Escape restores.
    pub volume_before_drag: Option<(f32, bool)>,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub error: Option<String>,
    pub scan: Option<Scan>,
    pub drop_hover: bool,
    /// The open song details dialog.
    pub details: Option<DetailsDialog>,
    /// The open right-click menu on a song, album or list, and the point it opened at.
    pub song_menu: Option<(SongMenuTarget, PointerPress)>,
    output: Output,
    rng: Rng,
    waker: Option<Waker>,
    /// Last.fm: the connected account, and the playing song's scrobble.
    pub lastfm: Scrobbling,
    /// Every song's plays, saved as they count.
    plays: Plays,
    /// How long the loaded song has really played, until its play counts.
    listen: Option<Listen>,
    /// The artist graph, built when its tab first shows.
    pub graph: crate::graph::state::Graph,
}

impl App {
    pub fn new(output: Output, store: Store) -> App {
        App::with_lastfm(output, store, lastfm::built_in())
    }

    /// `new`, with `built_in` as the build's Last.fm API account: tests pick it, whatever
    /// this build was given.
    pub fn with_lastfm(output: Output, store: Store, built_in: Option<lastfm::Credentials>) -> App {
        let (relations, relations_error) = match store.load_relations() {
            Ok(r) => (r, None),
            Err(e) => (Default::default(), Some(e)),
        };
        let (lastfm, lastfm_error) = match store.load_lastfm() {
            Ok(saved) => (saved, None),
            Err(e) => (None, Some(e)),
        };
        let stale_session = lastfm.as_ref().is_some_and(|s| s.session.is_some());
        let (plays, plays_error) = store.plays();
        let (graph, graph_error) = match store.load_graph().and_then(crate::graph::state::Graph::new) {
            Ok(g) => (g, None),
            Err(e) => (crate::graph::state::Graph::with(None, 0), Some(e)),
        };
        // The tab last open is the page shown at launch; an unreadable one shows as an error.
        let (tab, tab_error) = match store.load_tab() {
            Ok(tab) => (tab, None),
            Err(e) => (None, Some(e)),
        };
        // An unreadable theme or appearance falls back to the default, and shows as an error.
        let (theme, theme_error) = match store.load_theme() {
            Ok(theme) => (theme, None),
            Err(e) => (Theme::default(), Some(e)),
        };
        let (appearance, appearance_error) = match store.load_appearance() {
            Ok(appearance) => (appearance, None),
            Err(e) => (Appearance::default(), Some(e)),
        };
        let (unlistened_order, unlistened_error) = match store.load_unlistened_order() {
            Ok(order) => (order, None),
            Err(e) => (UnlistenedOrder::default(), Some(e)),
        };
        let (discover, discover_error) = match store.load_discover() {
            Ok(tab) => (tab, None),
            Err(e) => (crate::discover::Tab::default(), Some(e)),
        };
        let (top_window, top_window_error) = match store.load_top_window() {
            Ok(window) => (window, None),
            Err(e) => (crate::top::Window::default(), Some(e)),
        };
        let (queue_restore, queue_error) = match store.load_queue() {
            Ok(saved) => (saved, None),
            Err(e) => (None, Some(e)),
        };
        let (queue_replace, queue_replace_error) = match store.load_queue_replace() {
            Ok(policy) => (policy, None),
            Err(e) => (QueueReplace::default(), Some(e)),
        };
        let (queue_pinned, queue_pinned_error) = match store.load_queue_pinned() {
            Ok(pinned) => (pinned, None),
            Err(e) => (false, Some(e)),
        };
        let page = match tab {
            Some(Tab::Home) => Page::Home,
            Some(Tab::Graph) => Page::Graph,
            Some(Tab::Library) | None => Page::Library,
        };
        let lastfm = Scrobbling::new(lastfm, built_in);
        let mut rng = Rng::from_time();
        let unlistened_seed = rng.seed();
        let mut app = App {
            tracks: Vec::new(),
            albums: Vec::new(),
            album_index: Vec::new(),
            data_version: 0,
            pages: Pages::default(),
            page,
            history: Vec::new(),
            forward: Vec::new(),
            artist_tab: ArtistTab::Releases,
            artist_credits_closed: false,
            artist_related_open: Vec::new(),
            artist_with_aliases: false,
            library_scroll: Live::new([0.0, 0.0]),
            library_scroll_saved: [0.0, 0.0],
            home_scroll: Live::new([0.0, 0.0]),
            home_scroll_saved: [0.0, 0.0],
            home_strip: Live::new([0.0; 2]),
            home_viewport: Live::new(0.0),
            unlistened_order,
            unlistened_seed,
            top_window,
            discover,
            covers: None,
            filtered: Vec::new(),
            shown_albums: Vec::new(),
            sections: Vec::new(),
            rows: Vec::new(),
            columns: 4,
            grid_width: None,
            theme,
            appearance,
            zoom: store.load_zoom(),
            zoom_preview: None,
            zoom_toast: None,
            views_unsaved: None,
            list_version: 0,
            views: store.load(),
            library: store.load_library(),
            relations_unreadable: relations_error.is_some(),
            relations,
            link_choice: crate::relations::LinkChoice::Merge,
            link_name: TextEdit::new(""),
            store,
            collapsed: Default::default(),
            panel: None,
            open_select: None,
            reorder: None,
            renaming: None,
            rename: TextEdit::new(""),
            filter_edits: Default::default(),
            search: TextEdit::new(""),
            view_search: TextEdit::new(""),
            search_words: Vec::new(),
            view_matches: None,
            laid_out_widths: Default::default(),
            selected: None,
            current: None,
            queue: None,
            queue_replace,
            queue_pinned,
            queue_shown: queue_pinned,
            queue_dont_ask: false,
            queue_scroll: Live::new([0.0, 0.0]),
            queue_scroll_due: false,
            pending_play: None,
            queue_restore,
            restore_pending: true,
            resume: false,
            playing: false,
            position: 0.0,
            seek_preview: None,
            volume: 0.8,
            muted: false,
            volume_before_drag: None,
            shuffle: false,
            repeat: Repeat::Off,
            error: relations_error.clone().or(lastfm_error).or(plays_error).or(graph_error).or(tab_error).or(theme_error).or(appearance_error).or(unlistened_error).or(top_window_error).or(discover_error).or(queue_error).or(queue_replace_error).or(queue_pinned_error),
            scan: None,
            drop_hover: false,
            details: None,
            song_menu: None,
            output,
            rng,
            waker: None,
            lastfm,
            plays,
            listen: None,
            graph,
        };
        // A session granted to another API key was dropped: forget it on disk too.
        if stale_session && app.lastfm.session().is_none() {
            app.save_lastfm();
        }
        app
    }

    /// `AppSpec::setup`: keep the waker for the scan thread and cover decoding, then scan the
    /// folders or files named on the command line (for this run only), else the saved library.
    pub fn setup(&mut self, waker: Waker) {
        let args: Vec<PathBuf> = rune::extra_args().iter().map(PathBuf::from).collect();
        self.setup_with(waker, args);
    }

    /// Sets up and scans `roots`, or the saved library when there are none.
    pub fn setup_with(&mut self, waker: Waker, roots: Vec<PathBuf>) {
        self.covers = Some(if matches!(self.output, Output::Silent) { Covers::synchronous(waker.clone()) } else { Covers::new(waker.clone()) });
        // Headless runs and tests never reach the network.
        if !matches!(self.output, Output::Silent) {
            let wake = waker.clone();
            self.lastfm.set_client(lastfm::Client::spawn(move || wake()));
        }
        self.waker = Some(waker);
        let mut roots = roots;
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
        // A running scan would be dropped with its results, so rescan every saved root instead.
        if self.scan.is_some() {
            self.start_scan(self.library.clone(), false);
        } else {
            self.start_scan(roots, true);
        }
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

    /// Takes the details thread's result when it has arrived; true when the view changes.
    fn drain_details(&mut self) -> bool {
        let Some(open) = &mut self.details else { return false };
        let Some(rx) = &open.reading else { return false };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err(format!("Couldn't read {}", open.path.display())),
        };
        open.details = Some(result);
        open.reading = None;
        true
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
            let paths: HashSet<&Path> = tracks.iter().map(|t| t.path.as_path()).collect();
            all.retain(|t| !paths.contains(t.path.as_path()));
            all.append(&mut tracks);
            library::sort(&mut all);
            tracks = all;
        }
        // Names one song's tags pair with IDs, or that stand alone, help split every other's.
        crate::credits::resolve(&mut tracks);
        library::sort(&mut tracks);
        self.plays.apply(&mut tracks);
        let old_tracks = std::mem::replace(&mut self.tracks, tracks);
        // Keep the queue playing: remap its indices onto the new tracks by path.
        let index: HashMap<&Path, usize> = self.tracks.iter().enumerate().map(|(i, t)| (t.path.as_path(), i)).collect();
        let remapped = self.queue.as_ref().and_then(|q| q.remap(|i| old_tracks.get(i).and_then(|t| index.get(t.path.as_path()).copied())));
        self.graph.invalidate();
        if self.page == Page::Graph {
            self.refresh_graph();
        }
        self.rebuild_albums();
        let find = |path: &Option<PathBuf>, tracks: &[Track]| path.as_ref().and_then(|p| tracks.iter().position(|t| &t.path == p));
        self.selected = find(&selected_path, &self.tracks);
        let current = find(&playing_path, &self.tracks);
        self.refilter();
        match current {
            Some(i) => {
                self.current = Some(i);
                self.queue = Some(match remapped {
                    Some(queue) => queue,
                    None => Queue::new(self.filtered_or(i), i, self.shuffle, &mut self.rng),
                });
            }
            None if playing_path.is_some() => {
                // The playing file left the library: stop rather than play on unseen.
                self.stop();
            }
            None => {}
        }
        // The first library load restores the queue saved at the last exit.
        if std::mem::take(&mut self.restore_pending) {
            self.restore_queue();
        }
    }

    /// Clears the view search box. The caller re-evaluates the view.
    pub fn clear_view_search(&mut self) {
        self.view_search.clear();
    }

    /// Re-evaluates the active view over the library.
    pub fn refilter(&mut self) {
        self.search_words = self.view_search.text.split_whitespace().map(str::to_lowercase).collect();
        let words = &self.search_words;
        let view = self.views.active();
        let mut matches = None;
        match view.mode {
            Mode::Songs => {
                let Evaluated { order, sections } = evaluate_search(view, &self.tracks, words);
                self.filtered = order;
                self.sections = sections;
                self.shown_albums.clear();
            }
            // An album list shows the albums with a matching song, and only those songs.
            Mode::Albums if view.layout == Layout::List && !words.is_empty() => {
                let songs: HashSet<usize> = (0..self.tracks.len()).filter(|&i| matches_fields(&self.tracks[i], &view.columns, words)).collect();
                let searched = evaluate_matching(view, &self.albums, |album| album.tracks.iter().any(|t| songs.contains(t)));
                self.filtered = searched.order.iter().flat_map(|&a| self.albums[a].tracks.iter().copied().filter(|t| songs.contains(t))).collect();
                self.shown_albums = searched.order;
                self.sections = searched.sections;
                matches = Some(songs);
            }
            Mode::Albums => {
                let searched = evaluate_search(view, &self.albums, words);
                self.filtered = searched.order.iter().flat_map(|&a| self.albums[a].tracks.iter().copied()).collect();
                self.shown_albums = searched.order;
                self.sections = searched.sections;
            }
        }
        self.view_matches = matches;
        // Views size their grid cards differently.
        if let Some(width) = self.grid_width {
            self.columns = crate::view::columns_for(width, self.card_size());
        }
        self.rebuild_rows();
    }

    pub fn mode(&self) -> Mode {
        self.views.active().mode
    }

    /// The songs view `v` lists, in its order: an album view's albums' songs, album by album.
    pub fn view_songs(&self, v: usize) -> Vec<usize> {
        let Some(view) = self.views.list.get(v) else { return Vec::new() };
        match view.mode {
            Mode::Songs => evaluate(view, &self.tracks, "").order,
            Mode::Albums => evaluate(view, &self.albums, "").order.iter().flat_map(|&a| self.albums[a].tracks.iter().copied()).collect(),
        }
    }

    /// The albums on the home page: the newest added, newest first; albums with no date are left out.
    pub fn recently_added(&self) -> Vec<usize> {
        let mut added: Vec<(i64, usize)> = self.albums.iter().enumerate().filter_map(|(a, album)| album.added.map(|at| (at, a))).collect();
        added.sort_by_key(|&(at, a)| (std::cmp::Reverse(at), a));
        added.into_iter().take(RECENT_ALBUMS_COUNT).map(|(_, a)| a).collect()
    }

    /// The songs on the home page: the ones played last, most recent first, ties by library order.
    pub fn recently_played(&self) -> Vec<usize> {
        let mut played: Vec<(i64, usize)> = self.tracks.iter().enumerate().filter_map(|(i, t)| t.last_played.map(|at| (at, i))).collect();
        played.sort_by_key(|&(at, i)| (std::cmp::Reverse(at), i));
        played.into_iter().take(RECENT_SONGS_COUNT).map(|(_, i)| i).collect()
    }

    /// The albums nobody has played: no plays and no song played, in `order`, at most
    /// `UNLISTENED_COUNT`. Random shuffles them with `seed`; the others sort by when they were
    /// added, undated albums last.
    pub fn unlistened(&self, order: UnlistenedOrder, seed: u64) -> Vec<usize> {
        let mut list: Vec<(Option<i64>, usize)> = self
            .albums
            .iter()
            .enumerate()
            .filter(|(_, album)| album.plays == 0 && !album.tracks.iter().any(|&t| self.tracks.get(t).is_some_and(|t| t.last_played.is_some())))
            .map(|(a, album)| (album.added, a))
            .collect();
        match order {
            UnlistenedOrder::Newest => list.sort_by_key(|&(added, a)| (added.is_none(), std::cmp::Reverse(added), a)),
            UnlistenedOrder::Oldest => list.sort_by_key(|&(added, a)| (added.is_none(), added, a)),
            UnlistenedOrder::Random => Rng::new(seed).shuffle(&mut list),
        }
        list.into_iter().take(UNLISTENED_COUNT).map(|(_, a)| a).collect()
    }

    /// `unlistened` for the current order and seed, kept until the library or either changes.
    pub fn unlistened_albums(&self) -> Rc<Vec<usize>> {
        self.pages.unlistened.get((self.data_version, self.unlistened_order, self.unlistened_seed), || self.unlistened(self.unlistened_order, self.unlistened_seed))
    }

    /// The search page's results for `query`, built once per library version and query.
    pub fn search_results(&self) -> Rc<SearchResults> {
        let query = self.search.text.trim().to_lowercase();
        self.pages.search.get((query.clone(), self.data_version), || crate::search::search(&query, &self.tracks, &self.albums, &self.relations))
    }

    /// The artist page for `name` and the artists involved with them, built once per library version.
    pub fn artist_model(&self, name: &str, with_aliases: bool) -> Rc<ArtistModel> {
        let key = (name.to_string(), with_aliases, self.data_version);
        self.pages.artist.get(key, || {
            let info = crate::artist::page(name, &self.tracks, &self.albums, &self.relations, with_aliases);
            let others = crate::artist::involved(name, &self.tracks, &self.albums, &self.relations, with_aliases);
            (info, others)
        })
    }

    /// The collaboration page for `names`, built once per library version.
    pub fn collab_model(&self, names: &[String]) -> Rc<CollabPage> {
        self.pages.collab.get((names.to_vec(), self.data_version), || crate::collab::page(names, &self.tracks, &self.albums, &self.relations))
    }

    /// The label page for `name`, built once per library version.
    pub fn label_model(&self, name: &str) -> Rc<LabelPage> {
        self.pages.label.get((name.to_string(), self.data_version), || crate::label::page(name, &self.albums))
    }

    /// `recently_added`, kept until the library changes.
    pub fn recent_albums(&self) -> Rc<Vec<usize>> {
        self.pages.recent_albums.get(self.data_version, || self.recently_added())
    }

    /// The Home leaderboards for the chosen window, kept until the library, the relations, the
    /// window or the day changes.
    pub fn top_model(&self) -> Rc<TopModel> {
        let now = unix_now().unwrap_or(0);
        let key = (self.data_version, self.top_window, now / 86_400);
        self.pages.top.get(key, || TopModel {
            artists: crate::top::artists(&self.tracks, &self.plays, &self.relations, self.top_window, now),
            albums: crate::top::albums(&self.tracks, &self.albums, &self.plays, self.top_window, now),
            songs: crate::top::songs(&self.tracks, &self.plays, self.top_window, now),
        })
    }

    /// A discovery tab's albums, kept like `top_model`. Unlistened is `unlistened_albums`.
    pub fn discover_albums(&self, tab: crate::discover::Tab) -> Rc<Vec<usize>> {
        let now = unix_now().unwrap_or(0);
        let key = (self.data_version, tab, now / 86_400);
        self.pages.discover.get(key, || match tab {
            crate::discover::Tab::Unlistened => self.unlistened(self.unlistened_order, self.unlistened_seed),
            crate::discover::Tab::Continue => crate::discover::continuing(&self.albums, &self.tracks),
            crate::discover::Tab::Rediscover => crate::discover::rediscover(&self.albums, now),
            crate::discover::Tab::New => crate::discover::new_this_month(&self.albums, now),
        })
    }

    /// The genre and decade shares, kept until the library changes.
    pub fn mix_model(&self) -> Rc<(crate::mix::Mix, crate::mix::Mix)> {
        self.pages.mix.get(self.data_version, || (crate::mix::genres(&self.tracks), crate::mix::decades(&self.tracks)))
    }

    /// The heatmap and streaks, from every play, in the local time zone. Kept by the library, the
    /// offset and the day.
    pub fn activity(&self) -> Rc<crate::activity::Activity> {
        let now = unix_now().unwrap_or(0);
        let offset = crate::activity::local_offset(now);
        let key = (self.data_version, offset, now / 86_400);
        self.pages.activity.get(key, || {
            let times = self.plays.plays_since(&self.tracks, i64::MIN).into_iter().map(|(_, at)| at);
            crate::activity::Activity::of(times, offset, now)
        })
    }

    /// The credits leaderboards for the chosen window, kept like `top_model`.
    pub fn people_model(&self) -> Rc<Vec<crate::people::RoleBoard>> {
        let now = unix_now().unwrap_or(0);
        let key = (self.data_version, self.top_window, now / 86_400);
        self.pages.people.get(key, || crate::people::boards(&self.tracks, &self.plays, self.top_window, now))
    }

    /// The library's totals for the stats strip, kept until the library changes.
    pub fn totals(&self) -> Rc<Totals> {
        self.pages.totals.get(self.data_version, || Totals::of(&self.tracks, &self.albums))
    }

    /// `recently_played`, kept until the library changes.
    pub fn recent_songs(&self) -> Rc<Vec<usize>> {
        self.pages.recent_songs.get(self.data_version, || self.recently_played())
    }

    /// The library's tracks, in library order.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// The library gathered into albums, in library order.
    pub fn albums(&self) -> &[Album] {
        &self.albums
    }

    /// The artist relations.
    pub fn relations(&self) -> &crate::relations::Relations {
        &self.relations
    }

    /// Applies `edit` to the track at `i` (if any), then rebuilds the albums, whose credits come from their tracks.
    pub fn edit_track(&mut self, i: usize, edit: impl FnOnce(&mut Track)) {
        let Some(track) = self.tracks.get_mut(i) else { return };
        edit(track);
        self.rebuild_albums();
        self.graph.invalidate();
        self.refilter();
    }

    /// Replaces the artist relations, as a change to them: page models built from them are rebuilt.
    pub fn set_relations(&mut self, relations: crate::relations::Relations) {
        self.relations = relations;
        self.data_version = self.data_version.wrapping_add(1);
        self.graph.invalidate();
    }

    /// The album shown on the album page, if it's still in the library.
    pub fn open_album(&self) -> Option<usize> {
        match &self.page {
            Page::Album(id) => self.albums.iter().position(|a| &a.id() == id),
            Page::Library | Page::Artist(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => None,
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
            Page::Library => &self.views.active().columns,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => &self.views.page_columns,
        }
    }

    /// The columns the Columns popover edits: the ones on screen.
    pub fn shown_columns_mut(&mut self) -> &mut Vec<Property> {
        match self.page {
            Page::Library => &mut self.views.active_mut().columns,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => &mut self.views.page_columns,
        }
    }

    /// The dragged column widths of the tables on screen, which go with their columns.
    pub fn shown_widths(&self) -> &Widths {
        match self.page {
            Page::Library => &self.views.active().widths,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => &self.views.page_widths,
        }
    }

    pub fn shown_widths_mut(&mut self) -> &mut Widths {
        match self.page {
            Page::Library => &mut self.views.active_mut().widths,
            Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => &mut self.views.page_widths,
        }
    }

    /// The artist shown on the artist page.
    pub fn open_artist(&self) -> Option<&str> {
        match &self.page {
            Page::Artist(name) => Some(name),
            Page::Library | Page::Album(_) | Page::Label(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => None,
        }
    }

    pub fn show_artist(&mut self, name: &str, tx: &mut Tx) {
        self.go_to(Page::Artist(name.to_string()));
        tx.scroll_to_instant("artist-page", 0.0, 0.0);
    }

    /// Changes the artist relations with `edit` and saves them. Refuses while the saved ones
    /// couldn't be read, so the file's hand edits aren't overwritten.
    fn edit_relations(&mut self, edit: impl FnOnce(&mut crate::relations::Relations) -> Result<(), String>) -> Result<(), String> {
        if self.relations_unreadable {
            return Err("The artist links in the preferences file couldn't be read: fix or remove its \"artists\" entry first".into());
        }
        let edited = edit(&mut self.relations);
        self.data_version = self.data_version.wrapping_add(1);
        edited?;
        self.graph.invalidate();
        self.store.save_relations(&self.relations)
    }

    /// Links the open artist to `other` as the popover's choice says, closing it on success.
    pub fn link_artist(&mut self, other: &str) {
        let Some(name) = self.open_artist().map(str::to_string) else { return };
        let choice = self.link_choice;
        match self.edit_relations(|r| r.add(&name, choice, other)) {
            Ok(()) => {
                self.error = None;
                self.close_panel();
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Removes `other` as related to the open artist in that way.
    pub fn unlink_artist(&mut self, relation: crate::relations::Relation, other: &str) {
        let Some(name) = self.open_artist() else { return };
        let names = self.relations.merged(name);
        if let Err(e) = self.edit_relations(|r| {
            r.unlink(&names, relation, other);
            Ok(())
        }) {
            self.error = Some(e);
        }
    }

    /// Takes `name` out of the open artist's merge. When it's the name the page is open under,
    /// the page moves to one of their other names.
    pub fn unmerge_artist(&mut self, name: &str) {
        let Some(open) = self.open_artist().map(str::to_string) else { return };
        let rest: Vec<String> = self.relations.merged(&open).into_iter().filter(|n| !n.eq_ignore_ascii_case(name)).collect();
        match self.edit_relations(|r| {
            r.unmerge(name);
            Ok(())
        }) {
            Ok(()) => {
                if open.eq_ignore_ascii_case(name)
                    && let Some(other) = rest.first()
                {
                    self.page = Page::Artist(other.clone());
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Opens the Link artist popover with an empty field, focused.
    pub fn open_link_artist(&mut self) {
        let open = self.panel != Some(Panel::LinkArtist);
        self.close_panel();
        if open {
            let mut edit = TextEdit::new("");
            edit.focused = true;
            self.link_name = edit;
            self.panel = Some(Panel::LinkArtist);
        }
    }

    /// The label shown on the label page.
    pub fn open_label(&self) -> Option<&str> {
        match &self.page {
            Page::Label(name) => Some(name),
            Page::Library | Page::Album(_) | Page::Artist(_) | Page::Collab(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => None,
        }
    }

    pub fn show_label(&mut self, name: &str, tx: &mut Tx) {
        self.go_to(Page::Label(name.to_string()));
        tx.scroll_to_instant("label-page", 0.0, 0.0);
    }

    /// The artists shown on the collaboration page.
    pub fn open_collab(&self) -> Option<&[String]> {
        match &self.page {
            Page::Collab(names) => Some(names),
            Page::Library | Page::Album(_) | Page::Artist(_) | Page::Label(_) | Page::Search | Page::Settings | Page::Graph | Page::Home => None,
        }
    }

    pub fn show_collab(&mut self, names: &[String], tx: &mut Tx) {
        self.go_to(Page::Collab(names.to_vec()));
        tx.scroll_to_instant("collab-page", 0.0, 0.0);
    }

    /// Opens `page`, remembering the one it leaves for Back.
    fn go_to(&mut self, page: Page) {
        self.close_panel();
        self.details = None;
        if page == self.page {
            return;
        }
        self.forward.clear();
        self.leave_for(page);
    }

    /// Shows `page`, remembering the one it leaves for Back.
    fn leave_for(&mut self, page: Page) {
        self.artist_tab = ArtistTab::Releases;
        self.artist_credits_closed = false;
        self.artist_related_open.clear();
        let left = std::mem::replace(&mut self.page, page);
        if left == Page::Library {
            self.library_scroll_saved = self.library_scroll.get();
        } else {
            if left == Page::Home {
                self.home_scroll_saved = self.home_scroll.get();
            }
            self.history.push(left);
        }
    }

    /// Back: the page before this one, else the library; the library and Home scroll to where they were left.
    pub fn go_back(&mut self, tx: &mut Tx) {
        self.close_panel();
        self.details = None;
        // The library, the graph and home are where Back ends.
        if matches!(self.page, Page::Library | Page::Graph | Page::Home) {
            return;
        }
        let back = self.history.pop().unwrap_or(Page::Library);
        let left = std::mem::replace(&mut self.page, back);
        self.forward.push(left);
        self.artist_tab = ArtistTab::Releases;
        self.artist_credits_closed = false;
        self.artist_related_open.clear();
        if self.page == Page::Library {
            let [x, y] = self.library_scroll_saved;
            tx.scroll_to_instant("tracks", x, y);
        }
        if self.page == Page::Home {
            let [x, y] = self.home_scroll_saved;
            tx.scroll_to_instant("home-page", x, y);
        }
        if self.page == Page::Graph {
            // An artist linked on the page just left changes the graph.
            self.refresh_graph();
        }
    }

    /// The Graph tab: every artist and their links, building the graph if the library changed.
    pub fn show_graph(&mut self) {
        self.show_tab(Page::Graph);
        self.save_tab(Tab::Graph);
        self.refresh_graph();
    }

    /// The Home tab: the newest albums and the songs played last.
    pub fn show_home(&mut self) {
        self.show_tab(Page::Home);
        self.save_tab(Tab::Home);
    }

    /// Shows a tab's page with no history, so Back ends there.
    fn show_tab(&mut self, page: Page) {
        self.close_panel();
        self.details = None;
        self.artist_tab = ArtistTab::Releases;
        self.artist_credits_closed = false;
        self.artist_related_open.clear();
        if self.page == Page::Library {
            self.library_scroll_saved = self.library_scroll.get();
        }
        self.history.clear();
        self.forward.clear();
        self.page = page;
    }

    /// Chooses the colour theme and saves it. A failed save shows as an error.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        if let Err(e) = self.store.save_theme(theme) {
            self.error = Some(e);
        }
    }

    /// Chooses light, dark or system appearance and saves it. A failed save shows as an error.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
        if let Err(e) = self.store.save_appearance(appearance) {
            self.error = Some(e);
        }
    }

    /// Chooses the order of Unlistened Releases and saves it. A failed save shows as an error.
    pub fn set_unlistened_order(&mut self, order: UnlistenedOrder) {
        self.unlistened_order = order;
        if let Err(e) = self.store.save_unlistened_order(order) {
            self.error = Some(e);
        }
    }

    /// Chooses the discovery tab and saves it. A failed save shows as an error.
    pub fn set_discover(&mut self, tab: crate::discover::Tab) {
        self.discover = tab;
        if let Err(e) = self.store.save_discover(tab) {
            self.error = Some(e);
        }
    }

    /// Chooses the window the Home leaderboards show and saves it. A failed save shows as an error.
    pub fn set_top_window(&mut self, window: crate::top::Window) {
        self.top_window = window;
        if let Err(e) = self.store.save_top_window(window) {
            self.error = Some(e);
        }
    }

    /// Picks a new random selection for Unlistened Releases.
    pub fn reroll_unlistened(&mut self) {
        self.unlistened_seed = self.rng.seed();
    }

    /// Saves the open tab, so the next launch opens it. A failed save shows as an error.
    fn save_tab(&mut self, tab: Tab) {
        if let Err(e) = self.store.save_tab(&tab) {
            self.error = Some(e);
        }
    }

    /// Whether the Graph tab is the one open: its page, or a page opened from it.
    pub fn in_graph(&self) -> bool {
        self.page == Page::Graph || self.history.first() == Some(&Page::Graph)
    }

    /// Whether the Home tab is the one open: its page, or a page opened from it.
    pub fn in_home(&self) -> bool {
        self.page == Page::Home || self.history.first() == Some(&Page::Home)
    }

    /// Rebuilds the graph when the library or relations changed since it was built.
    pub fn refresh_graph(&mut self) {
        if !self.graph.is_stale() {
            return;
        }
        let model = crate::graph::Model::build(&self.tracks, &self.relations);
        // Headless runs lay out on this thread, so a snapshot shows the settled graph.
        let waker = if matches!(self.output, Output::Silent) { None } else { self.waker.clone() };
        if let Err(e) = self.graph.refresh(model, self.store.graph_cache(), waker) {
            self.error = Some(e);
        }
    }

    /// Saves the graph's camera and hidden link kinds.
    pub fn save_graph(&mut self) {
        self.graph.moved_at = None;
        if let Err(e) = self.store.save_graph(&self.graph.saved()) {
            self.error = Some(e);
        }
    }

    /// Shows or hides one kind of link.
    pub fn toggle_graph_kind(&mut self, kind: crate::graph::EdgeKind) {
        self.graph.hidden ^= kind.bit();
        self.graph.version += 1;
        self.save_graph();
    }

    /// The graph's Reset layout: unpins every artist and arranges the graph from scratch.
    pub fn reset_graph_layout(&mut self) {
        let waker = if matches!(self.output, Output::Silent) { None } else { self.waker.clone() };
        if let Err(e) = self.graph.reset_layout(self.store.graph_cache(), waker) {
            self.error = Some(e);
        }
        self.save_graph();
    }

    /// Unpins one artist on the graph.
    pub fn unpin_graph_artist(&mut self, i: usize) {
        if let Err(e) = self.graph.unpin(i) {
            self.error = Some(e);
        }
    }

    /// The graph's find field changed: centre on the best match.
    pub fn graph_found(&mut self) {
        let found = self.graph.model.search(&self.graph.find.text);
        self.graph.selected = found;
        if let Some(i) = found {
            self.graph.centre_on(i, 0.8);
        }
    }

    pub fn show_library(&mut self) {
        self.history.clear();
        self.forward.clear();
        self.page = Page::Library;
    }

    /// Forward: the page Back last left, if nothing was opened since.
    pub fn go_forward(&mut self) {
        self.close_panel();
        self.details = None;
        if let Some(page) = self.forward.pop() {
            self.leave_for(page);
        }
    }

    /// Home: the active view, the search cleared.
    pub fn go_home(&mut self) {
        self.close_panel();
        self.details = None;
        self.search.clear();
        self.artist_tab = ArtistTab::Releases;
        self.artist_credits_closed = false;
        self.artist_related_open.clear();
        self.show_library();
        self.save_tab(Tab::Library);
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
                    self.forward.clear();
                    self.page = Page::Search;
                    self.artist_tab = ArtistTab::Releases;
                    self.artist_credits_closed = false;
                    self.artist_related_open.clear();
                }
                None => self.go_to(Page::Search),
            }
            tx.scroll_to_instant("search-page", 0.0, 0.0);
        } else if self.page == Page::Search {
            self.go_back(tx);
            // The results are gone: Forward skips them.
            self.forward.retain(|p| *p != Page::Search);
        } else {
            if let Some(at) = back_at {
                // Pages opened from the results go back past where the results were.
                self.history.remove(at);
            }
            self.forward.retain(|p| *p != Page::Search);
        }
    }

    /// Clears the search, leaving the results.
    pub fn clear_search(&mut self, tx: &mut Tx) {
        self.search.clear();
        self.searched(tx);
    }

    /// Opens song `i`'s details. A window reads the file on a thread; headless runs read it now.
    pub fn show_details(&mut self, i: usize, _tx: &mut Tx) {
        let Some(path) = self.tracks.get(i).map(|t| t.path.clone()) else { return };
        self.close_panel();
        self.selected = Some(i);
        let waker = if matches!(self.output, Output::Silent) { None } else { self.waker.clone() };
        if let Some(waker) = waker {
            let (tx, rx) = channel();
            let thread_path = path.clone();
            let spawned = std::thread::Builder::new().name("details".into()).spawn(move || {
                // The dialog may have closed meanwhile; then nobody receives and the result is dropped.
                let _ = tx.send(crate::details::read(&thread_path));
                waker();
            });
            if spawned.is_ok() {
                self.details = Some(DetailsDialog { path, details: None, reading: Some(rx), raw_open: false });
                return;
            }
        }
        let details = crate::details::read(&path);
        self.details = Some(DetailsDialog { path, details: Some(details), reading: None, raw_open: false });
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
        let shuffle = self.shuffle;
        self.start_list(list, i, shuffle, None, tx);
    }

    /// Rebuilds the albums from the tracks, and the index from each track to its album.
    fn rebuild_albums(&mut self) {
        self.albums = album::build(&self.tracks);
        self.data_version = self.data_version.wrapping_add(1);
        self.album_index = vec![None; self.tracks.len()];
        for (a, entry) in self.albums.iter().enumerate() {
            for &t in &entry.tracks {
                if let Some(slot) = self.album_index.get_mut(t).filter(|s| s.is_none()) {
                    *slot = Some(a);
                }
            }
        }
    }

    /// Refreshes the play counts after a play counts: updates the tracks, then only the albums
    /// whose songs' counts changed, so the albums keep their order and indices.
    fn refresh_play_stats(&mut self) {
        let before: Vec<(u32, Option<i64>)> = self.tracks.iter().map(|t| (t.plays, t.last_played)).collect();
        self.plays.apply(&mut self.tracks);
        let changed: HashSet<usize> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, t)| before.get(*i).is_some_and(|b| *b != (t.plays, t.last_played)))
            .filter_map(|(i, _)| self.album_of(i))
            .collect();
        for a in changed {
            let Some(album) = self.albums.get_mut(a) else { continue };
            album.plays = album.tracks.iter().filter_map(|&t| self.tracks.get(t)).map(|t| t.plays).sum();
            album.last_played = album.tracks.iter().filter_map(|&t| self.tracks.get(t)).fold(None, |acc, t| acc.max(t.last_played));
        }
        self.data_version = self.data_version.wrapping_add(1);
    }

    /// The album a song belongs to.
    pub fn album_of(&self, track: usize) -> Option<usize> {
        self.album_index.get(track).copied().flatten()
    }

    /// Plays album `a` from its song `from` (else its first), queueing the albums after it
    /// in the view's order (or just this album when it isn't in the view).
    pub fn play_album(&mut self, a: usize, from: Option<usize>, tx: &mut Tx) {
        let Some(album) = self.albums.get(a) else { return };
        let Some(&first) = from.or(album.tracks.first().copied()).as_ref() else { return };
        // The whole album, then the shown albums after it, even when a search hides some of their songs.
        let list = if self.shown_albums.contains(&a) { self.shown_albums.iter().flat_map(|&s| self.albums[s].tracks.iter().copied()).collect() } else { album.tracks.clone() };
        self.start_list(list, first, false, Some(false), tx);
    }

    /// Plays view `v`'s songs from the first, in order, as the album page's Play does.
    pub fn play_view(&mut self, v: usize, tx: &mut Tx) {
        let list = if v == self.views.active { self.filtered.clone() } else { self.view_songs(v) };
        let Some(&first) = list.first() else { return };
        self.start_list(list, first, false, Some(false), tx);
    }

    /// Shuffles album `a` alone.
    pub fn shuffle_album(&mut self, a: usize, tx: &mut Tx) {
        let Some(album) = self.albums.get(a) else { return };
        let list = album.tracks.clone();
        let start = list[(self.rng.below(list.len() as u64)) as usize];
        self.start_list(list, start, true, Some(true), tx);
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.compute_rows();
        self.list_version += 1;
    }

    /// Saves the views and re-evaluates the active one.
    pub fn views_changed(&mut self) {
        self.save_views();
        self.refilter();
    }

    /// The visible list when it holds `i`, else the whole library.
    fn filtered_or(&self, i: usize) -> Vec<usize> {
        if self.filtered.contains(&i) { self.filtered.clone() } else { (0..self.tracks.len()).collect() }
    }

    fn compute_rows(&self) -> Vec<Row> {
        let grid = self.mode() == Mode::Albums && self.views.active().layout == Layout::Grid;
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

    /// The grid's inner width changed with the window's: refit its columns.
    pub fn set_grid_width(&mut self, width: f32) {
        self.grid_width = Some(width);
        self.fit_columns();
    }

    /// Fits as many columns across the grid as covers of the active view's card size allow.
    fn fit_columns(&mut self) {
        let Some(width) = self.grid_width else { return };
        let columns = crate::view::columns_for(width, self.card_size());
        if columns != self.columns {
            self.columns = columns;
            self.rebuild_rows();
        }
    }

    /// The active view's grid card size: its own, else the default.
    pub fn card_size(&self) -> f32 {
        self.views.active().card_size.unwrap_or(self.views.card_size)
    }

    /// Sizes the active view's grid cards; `save` writes it out (a slider saves when let go).
    pub fn set_card_size(&mut self, size: f32, save: bool) {
        self.views.active_mut().card_size = Some(crate::query::view::clamp_card_size(size));
        self.fit_columns();
        if save {
            self.save_views();
        }
    }

    /// Makes the active view's card size the default, which views without their own follow.
    pub fn make_card_size_default(&mut self) {
        self.views.card_size = self.card_size();
        self.views.active_mut().card_size = None;
        self.fit_columns();
        self.save_views();
    }

    /// The active view follows the default card size again.
    pub fn reset_card_size(&mut self) {
        self.views.active_mut().card_size = None;
        self.fit_columns();
        self.save_views();
    }

    /// Re-evaluates the active view now, and saves the views after a pause.
    pub fn views_changed_later(&mut self) {
        self.refilter();
        self.views_unsaved = Some(std::time::Instant::now());
    }

    fn save_views(&mut self) {
        self.views_unsaved = None;
        if let Err(e) = self.store.save(&self.views) {
            self.error = Some(e);
        }
    }

    /// Zooms the window to `zoom` (within range, to the percent); `save` writes it out.
    pub fn set_zoom(&mut self, zoom: f32, save: bool) {
        self.zoom = clamp_zoom(zoom);
        if save {
            self.save_zoom();
        }
    }

    pub fn save_zoom(&mut self) {
        if let Err(e) = self.store.save_zoom(self.zoom) {
            self.error = Some(e);
        }
    }

    /// A keyboard zoom: `steps` of 10% (0 resets to 100%), showing the level for a moment.
    fn zoom_by(&mut self, steps: i32, tx: &mut Tx) {
        let zoom = if steps == 0 { 1.0 } else { ((self.zoom / ZOOM_STEP).round() + steps as f32) * ZOOM_STEP };
        self.set_zoom(zoom, true);
        self.zoom_toast = Some(std::time::Instant::now());
        tx.wake_after(ZOOM_TOAST);
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
        self.start_list(self.filtered_or(i), i, self.shuffle, None, tx);
    }

    /// Starts a new list at `start`, shuffled when `shuffle`, and sets the shuffle flag to
    /// `set_shuffle` when given. Over an edited queue the policy decides: `Ask` holds the list
    /// in `pending_play` until the user answers.
    fn start_list(&mut self, list: Vec<usize>, start: usize, shuffle: bool, set_shuffle: Option<bool>, tx: &mut Tx) {
        let fresh = Queue::new(list, start, shuffle, &mut self.rng);
        if !self.edited_with_upcoming() {
            return self.commit_list(fresh, start, set_shuffle, tx);
        }
        match self.queue_replace {
            QueueReplace::Replace => self.commit_list(fresh, start, set_shuffle, tx),
            QueueReplace::KeepAdded => {
                let kept = self.keep_added(fresh);
                self.commit_list(kept, start, set_shuffle, tx);
            }
            QueueReplace::Ask => self.pending_play = Some(PendingPlay { queue: fresh, start, shuffle: set_shuffle }),
        }
    }

    /// Makes `queue` the playing list and starts its track `start`.
    fn commit_list(&mut self, queue: Queue, start: usize, set_shuffle: Option<bool>, tx: &mut Tx) {
        self.selected = Some(start);
        self.queue = Some(queue);
        if let Some(shuffle) = set_shuffle {
            self.shuffle = shuffle;
        }
        self.load(start, tx);
    }

    /// Whether the queue has been edited and has tracks still to play: the case that asks first.
    fn edited_with_upcoming(&self) -> bool {
        self.queue.as_ref().is_some_and(|q| q.edited() && !q.upcoming().is_empty())
    }

    /// `fresh` with the current queue's upcoming added tracks kept after its current track.
    fn keep_added(&self, fresh: Queue) -> Queue {
        match &self.queue {
            Some(old) => old.replace_keeping_added(fresh),
            None => fresh,
        }
    }

    /// Whether the queue has upcoming tracks the user added (for the replace question).
    pub fn pending_has_added(&self) -> bool {
        self.queue.as_ref().is_some_and(|q| q.upcoming().iter().any(|e| e.added))
    }

    /// Sets what a new list does over an edited queue, and saves it.
    pub fn set_queue_replace(&mut self, policy: QueueReplace) {
        self.queue_replace = policy;
        if let Err(e) = self.store.save_queue_replace(policy) {
            self.error = Some(e);
        }
    }

    /// Answers the pending list's question: `remember` saves a Replace or KeepAdded answer as
    /// Drops the pending list: the question was cancelled, or closed without an answer.
    pub fn cancel_pending_play(&mut self) {
        self.pending_play = None;
        self.queue_dont_ask = false;
    }

    /// the policy. Cancel drops the list and changes nothing.
    pub fn resolve_pending_play(&mut self, choice: QueueChoice, remember: bool, tx: &mut Tx) {
        let Some(pending) = self.pending_play.take() else { return };
        let (policy, keep) = match choice {
            QueueChoice::Cancel => return,
            QueueChoice::Replace => (QueueReplace::Replace, false),
            QueueChoice::KeepAdded => (QueueReplace::KeepAdded, true),
        };
        if remember {
            self.set_queue_replace(policy);
        }
        let queue = if keep { self.keep_added(pending.queue) } else { pending.queue };
        self.commit_list(queue, pending.start, pending.shuffle, tx);
    }

    /// Play Next (`next`) or Play After: adds `tracks` to the queue. With no queue, they start one.
    fn queue_add(&mut self, tracks: Vec<usize>, next: bool, tx: &mut Tx) {
        let Some(&first) = tracks.first() else { return };
        if let Some(q) = self.queue.as_mut() {
            if next { q.play_next(&tracks) } else { q.play_after(&tracks) }
            self.save_queue();
            return;
        }
        let queue = Queue::new(tracks, first, false, &mut self.rng);
        self.commit_list(queue, first, None, tx);
    }

    /// Queues `tracks` to play right after the current track, in order.
    pub fn queue_play_next(&mut self, tracks: Vec<usize>, tx: &mut Tx) {
        self.queue_add(tracks, true, tx);
    }

    /// Queues `tracks` to play at the end of the queue.
    pub fn queue_play_after(&mut self, tracks: Vec<usize>, tx: &mut Tx) {
        self.queue_add(tracks, false, tx);
    }

    /// Removes queue entry `index`; the playing track can't be removed.
    pub fn queue_remove(&mut self, index: usize) {
        if self.queue.as_mut().is_some_and(|q| q.remove(index)) {
            self.save_queue();
        }
    }

    /// Moves upcoming queue entry `from` to `to`.
    pub fn queue_move(&mut self, from: usize, to: usize) {
        if self.queue.as_mut().is_some_and(|q| q.move_entry(from, to)) {
            self.save_queue();
        }
    }

    /// Plays queue entry `index` now.
    pub fn queue_jump(&mut self, index: usize, tx: &mut Tx) {
        let Some(track) = self.queue.as_mut().and_then(|q| q.jump(index)) else { return };
        self.load(track, tx);
    }

    /// Shows or hides the queue panel. Shown, it scrolls to the track before the current one.
    pub fn toggle_queue(&mut self, tx: &mut Tx) {
        self.queue_shown = !self.queue_shown;
        if self.queue_shown {
            self.scroll_queue(tx);
        }
    }

    /// Pins the panel beside the page, or lets it float over the page again. Pinning shows it.
    pub fn set_queue_pinned(&mut self, pinned: bool, tx: &mut Tx) {
        self.queue_pinned = pinned;
        self.queue_shown |= pinned;
        if let Err(e) = self.store.save_queue_pinned(pinned) {
            self.error = Some(e);
        }
        if self.queue_shown {
            self.scroll_queue(tx);
        }
    }

    /// Scrolls the queue panel so the entry before the current one is its top row: one played
    /// song shows above the current one, and the first entry when nothing has played.
    pub fn scroll_queue(&self, tx: &mut Tx) {
        let top = self.queue.as_ref().map_or(0, |q| q.pos().saturating_sub(1));
        tx.scroll_to_instant(QUEUE_LIST_KEY, 0.0, top as f32 * QUEUE_ROW_H);
    }

    /// Follows a track change while the panel shows. When the entry before the current one and
    /// the current one are both in view, the list stays put; otherwise it glides to `scroll_queue`'s place.
    pub fn follow_queue(&self, tx: &mut Tx) {
        let Some(queue) = self.queue.as_ref() else { return };
        let pos = queue.pos();
        let top = pos.saturating_sub(1);
        let [offset, viewport] = self.queue_scroll.get();
        let in_view = |n: usize| {
            let at = n as f32 * QUEUE_ROW_H;
            at >= offset - QUEUE_VISIBLE_SLACK && at + QUEUE_ROW_H <= offset + viewport + QUEUE_VISIBLE_SLACK
        };
        if in_view(top) && in_view(pos) {
            return;
        }
        tx.scroll_to(QUEUE_LIST_KEY, 0.0, top as f32 * QUEUE_ROW_H);
    }

    /// Saves the queue and where playback is in it. Nothing is saved before the first library
    /// load, so a queue saved at the last exit isn't lost before it's restored.
    fn save_queue(&mut self) {
        if self.restore_pending {
            return;
        }
        let saved = self.queue.as_ref().and_then(|q| self.saved_queue(q));
        if let Err(e) = self.store.save_queue(saved.as_ref()) {
            self.error = Some(e);
        }
    }

    /// The queue as saved: tracks by path. None when the current track isn't in the library.
    fn saved_queue(&self, q: &Queue) -> Option<SavedQueue> {
        let path = |i: usize| self.tracks.get(i).map(|t| t.path.clone());
        let base = q.base().iter().filter_map(|&i| path(i)).collect();
        let removed = q.removed().iter().filter_map(|&i| path(i)).collect();
        let mut order = Vec::with_capacity(q.entries().len());
        let mut pos = None;
        for (n, e) in q.entries().iter().enumerate() {
            let Some(path) = path(e.track) else { continue };
            if n == q.pos() {
                pos = Some(order.len());
            }
            order.push(SavedEntry { path, added: e.added, pinned: e.pinned });
        }
        Some(SavedQueue { version: QUEUE_VERSION, base, order, pos: pos?, edited: q.edited(), removed, shuffle: self.shuffle, seconds: self.position })
    }

    /// Restores the queue saved at the last exit, paused at its saved place. Tracks that left the
    /// library are dropped; nothing is restored when the current one did.
    fn restore_queue(&mut self) {
        let Some(saved) = self.queue_restore.take() else { return };
        if self.current.is_some() {
            return;
        }
        let restored = {
            let index: HashMap<&Path, usize> = self.tracks.iter().enumerate().map(|(i, t)| (t.path.as_path(), i)).collect();
            let find = |p: &Path| index.get(p).copied();
            let base = saved.base.iter().filter_map(|p| find(p.as_path())).collect();
            let removed = saved.removed.iter().filter_map(|p| find(p.as_path())).collect();
            let mut order = Vec::with_capacity(saved.order.len());
            let mut pos = None;
            for (n, e) in saved.order.iter().enumerate() {
                let Some(track) = find(e.path.as_path()) else { continue };
                if n == saved.pos {
                    pos = Some(order.len());
                }
                order.push(Entry { track, added: e.added, pinned: e.pinned });
            }
            pos.and_then(|pos| Queue::from_parts(base, order, pos, saved.edited, removed))
        };
        let Some(queue) = restored else { return };
        let track = queue.current();
        let duration = self.tracks.get(track).and_then(|t| t.duration);
        let seconds = duration.map_or(saved.seconds, |d| saved.seconds.min(d)).max(0.0);
        self.queue = Some(queue);
        self.queue_scroll_due = self.queue_shown;
        self.shuffle = saved.shuffle;
        self.current = Some(track);
        self.selected = Some(track);
        self.position = seconds;
        // Not loaded yet: Play loads the track and starts it here.
        self.resume = true;
    }

    /// The window title for track `i`: "Title — Artist", or just the title.
    fn title_line(&self, i: usize) -> Option<String> {
        let track = self.tracks.get(i)?;
        Some(match track.artist() {
            Some(artist) => format!("{} — {artist}", track.title),
            None => track.title.clone(),
        })
    }

    fn load(&mut self, i: usize, tx: &mut Tx) {
        self.resume = false;
        let Some(track) = self.tracks.get(i) else { return };
        let path = track.path.clone();
        let title = self.title_line(i);
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
                if let Some(title) = title {
                    tx.set_title(title);
                }
                tx.wake_after(TICK);
                self.listen = self.tracks.get(i).map(|t| Listen::new(t.duration));
                if self.queue_shown {
                    self.follow_queue(tx);
                }
                if let Some(track) = self.tracks.get(i)
                    && let Err(e) = self.lastfm.started(track)
                {
                    self.error = Some(e);
                }
            }
            Err(e) => {
                self.playing = false;
                self.error = Some(e);
                self.listen = None;
                self.lastfm.stopped();
            }
        }
        self.save_queue();
    }

    pub fn stop(&mut self) {
        if let Output::Open(engine) = &mut self.output {
            engine.stop();
        }
        self.playing = false;
        self.listen = None;
        self.lastfm.stopped();
        self.current = None;
        self.queue = None;
        self.resume = false;
        self.position = 0.0;
        self.save_queue();
    }

    /// Play/pause; with nothing loaded, plays the selected row, else the first visible one.
    pub fn toggle(&mut self, tx: &mut Tx) {
        if self.current.is_none() {
            if let Some(i) = self.selected.or(self.filtered.first().copied()) {
                self.play_index(i, tx);
            }
            return;
        }
        // A restored track isn't loaded until Play: it starts at the saved place.
        if self.resume {
            if let Some(i) = self.current {
                let seconds = self.position;
                self.load(i, tx);
                self.seek(seconds);
            }
            return;
        }
        self.playing = !self.playing;
        if let Some(engine) = self.engine() {
            if self.playing { engine.play() } else { engine.pause() }
        }
        if self.playing {
            tx.wake_after(TICK);
        } else {
            self.save_queue();
        }
    }

    pub fn next(&mut self, tx: &mut Tx) {
        // Next always moves on, even repeating one track; past the end it wraps.
        let repeat = if self.repeat == Repeat::One { Repeat::All } else { self.repeat };
        match self.queue.as_mut().and_then(|q| q.next(repeat)) {
            Some(i) => self.load(i, tx),
            None => self.finish(tx),
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

    /// The queue ran out: rewind to its first entry, paused at 0. Play starts it.
    fn finish(&mut self, tx: &mut Tx) {
        if let Output::Open(engine) = &mut self.output {
            engine.stop();
        }
        self.playing = false;
        self.listen = None;
        self.lastfm.stopped();
        if let Some(first) = self.queue.as_mut().and_then(|q| q.jump(0)) {
            self.current = Some(first);
            self.selected = Some(first);
        }
        self.position = 0.0;
        self.seek_preview = None;
        // Not loaded yet: Play loads the track and starts it from the top.
        self.resume = true;
        if let Some(title) = self.current.and_then(|i| self.title_line(i)) {
            tx.set_title(title);
        }
        if self.queue_shown {
            self.follow_queue(tx);
        }
        self.save_queue();
    }

    fn track_ended(&mut self, tx: &mut Tx) {
        match (self.repeat, self.current) {
            (Repeat::One, Some(i)) => self.load(i, tx),
            _ => match self.queue.as_mut().and_then(|q| q.next(self.repeat)) {
                Some(i) => self.load(i, tx),
                None => self.finish(tx),
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
        self.save_queue();
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

    /// Puts the volume and mute back as they were, as when a knob drag is cancelled.
    pub fn restore_volume(&mut self, volume: f32, muted: bool) {
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = muted;
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
        self.save_queue();
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

    // Last.fm

    /// Settings' "Connect": opens Last.fm's approval page once a token comes back.
    pub fn lastfm_connect(&mut self) {
        self.lastfm.connect();
        self.save_lastfm();
    }

    /// Settings' "Get an API account…".
    pub fn lastfm_create_account(&mut self, tx: &mut Tx) {
        if let Err(e) = tx.open_url(lastfm::CREATE_ACCOUNT) {
            self.lastfm.status = Some(format!("Couldn't open {}: {e}", lastfm::CREATE_ACCOUNT));
        }
    }

    /// Settings' "Finish connecting", after approving in the browser.
    pub fn lastfm_finish(&mut self) {
        self.lastfm.finish();
    }

    /// Settings' "Open the page again".
    pub fn lastfm_reopen(&mut self, tx: &mut Tx) {
        if let Some(url) = self.lastfm.approval_url()
            && let Err(e) = tx.open_url(&url)
        {
            self.lastfm.status = Some(format!("Couldn't open {url}: {e}"));
        }
    }

    pub fn lastfm_disconnect(&mut self) {
        self.lastfm.disconnect();
        self.lastfm.stopped();
        self.save_lastfm();
    }

    /// Settings' "Remove credentials": forgets the account and the API key and secret.
    pub fn lastfm_forget(&mut self) {
        self.lastfm.forget();
        self.save_lastfm();
    }

    fn save_lastfm(&mut self) {
        if let Err(e) = self.store.save_lastfm(&self.lastfm.saved) {
            self.error = Some(e);
        }
    }

    /// Counts a play of library song `i` at `at` (seconds since the epoch), as playing it does, and
    /// shows it in the counts. Headless scenes and tests use it to seed the play log.
    pub fn record_play(&mut self, i: usize, at: i64) -> Result<(), String> {
        let Some(track) = self.tracks.get(i) else { return Ok(()) };
        let recorded = self.plays.record(track, at);
        self.refresh_play_stats();
        self.refilter();
        recorded
    }

    /// Follows the loaded song, and counts its play once it has played long enough; true
    /// when that changed the counts shown.
    fn poll_plays(&mut self, now: f64) -> bool {
        let Some(listen) = &mut self.listen else { return false };
        if !listen.tick(now, self.position, self.playing) {
            return false;
        }
        let Some(track) = self.current.and_then(|i| self.tracks.get(i)) else { return false };
        let recorded = unix_now().and_then(|at| self.plays.record(track, at));
        if let Err(e) = recorded {
            self.error = Some(e);
        }
        // Copies of the song share its plays, and views may sort or filter on them.
        self.refresh_play_stats();
        self.refilter();
        true
    }

    /// Takes Last.fm's replies and follows the playing song; true when the view changes.
    fn poll_lastfm(&mut self, now: f64, tx: &mut Tx) -> bool {
        if let Err(e) = self.lastfm.tick(now, self.position, self.playing) {
            self.error = Some(e);
        }
        let saved = self.lastfm.saved.clone();
        let (changed, effects) = self.lastfm.drain();
        if self.lastfm.saved != saved {
            self.save_lastfm();
        }
        for effect in effects {
            match effect {
                Effect::OpenUrl(url) => {
                    if let Err(e) = tx.open_url(&url) {
                        self.lastfm.status = Some(format!("Couldn't open {url}: {e}"));
                    }
                }
                Effect::Error(e) => self.error = Some(e),
            }
        }
        changed
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

impl App {
    pub fn close_panel(&mut self) {
        if self.views_unsaved.is_some() {
            self.save_views();
        }
        self.panel = None;
        self.open_select = None;
        self.renaming = None;
        self.reorder = None;
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        let open = self.panel != Some(panel);
        self.close_panel();
        if open {
            self.panel = Some(panel);
        }
    }
}

/// A song row's key: unique per group, since a song can be in several.
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
        let uri = file_uri(path);
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

/// A `file://` URI for `path`, with every byte outside unreserved characters and `/` percent-encoded.
fn file_uri(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        std::borrow::Cow::Borrowed(path.as_os_str().as_bytes())
    };
    #[cfg(not(unix))]
    let bytes = std::borrow::Cow::<[u8]>::Owned(path.to_string_lossy().into_owned().into_bytes());
    let mut uri = String::from("file://");
    for &b in bytes.iter() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => uri.push(char::from(b)),
            _ => uri.push_str(&format!("%{b:02X}")),
        }
    }
    uri
}

/// `AppSpec::on_close`: saves the queue as the app closes.
pub fn on_close(s: &mut App, _tx: &mut Tx) {
    s.save_queue();
}

/// `AppSpec::poll`: take scan results, follow the playing track, and move on when it ends.
pub fn poll(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    let mut changed = s.drain_scan();
    if std::mem::take(&mut s.queue_scroll_due) {
        s.scroll_queue(tx);
        changed = true;
    }
    changed |= s.drain_details();
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
    changed |= s.poll_plays(now);
    match s.graph.tick(now) {
        Ok(moved) => changed |= moved,
        Err(e) => {
            s.error = Some(e);
            changed = true;
        }
    }
    if s.graph.animating() {
        tx.wake_after(crate::graph::state::SIM_FRAME);
    }
    let (graph_changed, graph_error) = s.graph.drain();
    changed |= graph_changed;
    if let Some(e) = graph_error {
        s.error = Some(e);
        changed = true;
    }
    if s.page == Page::Graph && s.graph.is_stale() {
        s.refresh_graph();
        changed = true;
    }
    match s.graph.moved_at {
        Some(at) if at.elapsed() >= crate::graph::state::SAVE_AFTER => s.save_graph(),
        Some(at) => tx.wake_after(crate::graph::state::SAVE_AFTER.saturating_sub(at.elapsed())),
        None => {}
    }
    match s.views_unsaved {
        Some(at) if at.elapsed() >= SAVE_VIEWS_AFTER => s.save_views(),
        Some(at) => tx.wake_after(SAVE_VIEWS_AFTER.saturating_sub(at.elapsed())),
        None => {}
    }
    changed |= s.poll_lastfm(now, tx);
    if s.zoom_toast.is_some_and(|at| at.elapsed() >= ZOOM_TOAST) {
        s.zoom_toast = None;
        changed = true;
    }
    changed
}

/// A zoom within range, rounded to the percent; anything not finite is 100%.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() { ((zoom * 100.0).round() / 100.0).clamp(ZOOM_MIN, ZOOM_MAX) } else { 1.0 }
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
    // Escape closes the floating queue panel first; a docked one stays until its button hides it.
    if ev.key == Key::Escape && s.queue_shown && !s.queue_pinned {
        s.queue_shown = false;
        return true;
    }
    if ev.key == Key::Escape && s.page == Page::Search {
        s.clear_search(tx);
        return true;
    }
    if ev.key == Key::Escape && s.page == Page::Graph {
        if s.graph.selected.is_none() && s.graph.find.text.is_empty() {
            return false;
        }
        s.graph.selected = None;
        s.graph.find.clear();
        return true;
    }
    if ev.key == Key::Escape && !matches!(s.page, Page::Library | Page::Home) {
        s.go_back(tx);
        return true;
    }
    if m.alt && !m.cmd && !m.ctrl && !m.shift {
        match ev.key {
            Key::Left => s.go_back(tx),
            Key::Right => s.go_forward(),
            _ => return false,
        }
        return true;
    }
    // Ctrl+F, or / as in Vim. A focused field takes its own /, so it still types.
    if ev.is_command('f') || (ev.key == Key::Text("/".into()) && !m.cmd && !m.ctrl && !m.alt) {
        s.search.focused = true;
        return true;
    }
    if ev.is_command('o') {
        s.open_folder(tx);
        return true;
    }
    // Ctrl+= or Ctrl++ zooms in, Ctrl+- out, Ctrl+0 back to 100%.
    if ev.is_command('=') || ev.is_command('+') {
        s.zoom_by(1, tx);
        return true;
    }
    if ev.is_command('-') {
        s.zoom_by(-1, tx);
        return true;
    }
    if ev.is_command('0') {
        s.zoom_by(0, tx);
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
            "q" => s.toggle_queue(tx),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_percent_encodes_reserved_and_non_ascii_bytes() {
        assert_eq!(file_uri(Path::new("/m/A #1/50% ?.flac")), "file:///m/A%20%231/50%25%20%3F.flac");
        assert_eq!(file_uri(Path::new("/m/Björk/a-b_c.~d")), "file:///m/Bj%C3%B6rk/a-b_c.~d");
    }
}

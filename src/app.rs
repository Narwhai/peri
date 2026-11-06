//! The player's state and everything that changes it: library scans, playback, the queue.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use rune::{FileDialogOptions, Image, ImageCache, Key, KeyEvent, TextEdit, Tx, Waker};

use crate::audio::{self, Engine};
use crate::base::store::{Base, Store};
use crate::base::view::{Evaluated, Section, evaluate};
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
}

pub struct App {
    pub tracks: Vec<Track>,
    /// Indices into `tracks` the active view and search show, in play order.
    pub filtered: Vec<usize>,
    /// The active view's groups, empty when it isn't grouped.
    pub sections: Vec<Section>,
    /// What the list shows: group headers and songs, skipping collapsed groups' songs.
    pub rows: Vec<Row>,
    /// Bumped whenever `filtered` or `sections` change, so the list knows its rows did.
    pub list_version: u64,
    /// The saved views; edits save at once.
    pub base: Base,
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
    output: Output,
    rng: Rng,
    waker: Option<Waker>,
    covers: Option<ImageCache>,
}

impl App {
    pub fn new(output: Output, store: Store) -> App {
        App {
            tracks: Vec::new(),
            filtered: Vec::new(),
            sections: Vec::new(),
            rows: Vec::new(),
            list_version: 0,
            base: store.load(),
            store,
            collapsed: Default::default(),
            panel: None,
            open_select: None,
            renaming: None,
            rename: TextEdit::new(""),
            filter_edits: Default::default(),
            search: TextEdit::new(""),
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
            output,
            rng: Rng::from_time(),
            waker: None,
            covers: None,
        }
    }

    /// `AppSpec::setup`: keep the waker for the scan thread and cover decoding, then scan any
    /// folders or files named on the command line.
    pub fn setup(&mut self, waker: Waker) {
        self.covers = Some(ImageCache::new(waker.clone()));
        self.waker = Some(waker);
        let roots: Vec<PathBuf> = rune::extra_args().iter().map(PathBuf::from).collect();
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
            Ok(Some(path)) => s.start_scan(vec![path], false),
            Ok(None) => {}
            Err(e) => s.error = Some(format!("Couldn't open the folder dialog: {e}")),
        });
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
        self.tracks = tracks;
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

    /// Re-evaluates the active view and search over the library.
    pub fn refilter(&mut self) {
        let query = self.search.text.trim().to_lowercase();
        let Evaluated { order, sections } = evaluate(self.base.active(), &self.tracks, &query);
        self.filtered = order;
        self.sections = sections;
        self.rebuild_rows();
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
        if self.sections.is_empty() {
            return self.filtered.iter().map(|&i| Row::Track { track: i, section: None }).collect();
        }
        let mut rows = Vec::new();
        for (n, section) in self.sections.iter().enumerate() {
            rows.push(Row::Header(n));
            if !self.collapsed.contains(&section.title) {
                rows.extend(section.tracks.iter().map(|&i| Row::Track { track: i, section: Some(n) }));
            }
        }
        rows
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
        self.cover = self.covers.as_ref().and_then(|cache| library::read_cover(&path).map(|bytes| cache.load(bytes)));
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
            Row::Header(_) => None,
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

pub fn header_key(n: usize) -> String {
    format!("g{n}")
}

/// `AppSpec::poll`: take scan results, follow the playing track, and move on when it ends.
pub fn poll(s: &mut App, _: f64, tx: &mut Tx) -> bool {
    let mut changed = s.drain_scan();
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
            _ => return false,
        },
        _ => return false,
    }
    true
}

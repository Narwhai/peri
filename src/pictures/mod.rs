//! Artist pictures the user gives: a file they choose or drop, an image or link they paste.
//! Each is shrunk, encoded as JPEG and copied into the data folder as `artists/<id>.jpg`, and
//! `artist-pictures.json` beside `prefs.json` says whose it is. Nothing is fetched on its own:
//! a link is downloaded only when the user pastes it, and the picture is theirs from then on.
//!
//! ```json
//! { "version": 1, "artists": [
//!   { "names": ["Armand Hammer"], "mbid": "…", "file": "artists/3f2a….jpg", "source": "local",
//!     "added": 1764000000, "updated": 1764000000 }
//! ] }
//! ```
//!
//! A picture belongs to a page's names (merged names share one), matched ignoring case like
//! the relations; aliases and band members keep their own.
//!
//! Without one of the user's, a page shows an `artist.jpg` from its music folders
//! ([`folder`]), then, when the user turned online lookups on, a picture from Wikimedia
//! Commons ([`online`]) with its attribution. That one is only shown until the user keeps it,
//! which copies it in as theirs; nothing replaces a picture the user has.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use rune::{Image, ImageCache, Waker};
use serde::{Deserialize, Serialize};

use crate::covers::{Inner, Slot, THUMB_PX, shrink};

pub mod folder;
pub mod online;

pub const INDEX_FILE: &str = "artist-pictures.json";
/// The folder under the data folder the pictures are copied into.
pub const FOLDER: &str = "artists";
const VERSION: u32 = 1;
/// Pictures are kept at most this many pixels on a side.
pub const STORED_PX: u32 = 1000;
const JPEG_QUALITY: u8 = 88;
/// The largest image file or download taken.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Images wider or taller than this aren't decoded.
const MAX_SIDE: u32 = 16_384;
/// Decoded pictures kept: more than the graph shows at once (`MAX_PICTURES` in its view), so
/// what's in view is never evicted and read again. At 480 px each is about 0.9 MB.
pub const CAPACITY: usize = 128;
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Extensions the file picker and drops take.
pub const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// Where a picture came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A file on this computer the user chose, dropped or copied.
    Local,
    /// An image or link the user pasted.
    User,
    /// A Wikimedia Commons picture a lookup found, which the user kept.
    Wikimedia,
    /// The cover of one of their releases, which the user picked.
    Cover,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The page's names, merged names included.
    pub names: Vec<String>,
    /// The artist's MusicBrainz ID, when their tags have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    /// The picture, relative to the data folder.
    pub file: String,
    pub source: Source,
    /// The link it was downloaded from, when pasted as one or kept from a lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Who made it and its license, for a picture kept from Wikimedia Commons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<online::Attribution>,
    /// The release whose cover it is, for a cover the user picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// When the artist first got a picture, and when it last changed (Unix seconds).
    pub added: u64,
    pub updated: u64,
}

impl Entry {
    fn matches(&self, names: &[String]) -> bool {
        same(&self.names, names)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    #[serde(default)]
    artists: Vec<Entry>,
    /// Pages whose looked-up picture the user removed: they aren't looked up again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    declined: Vec<Vec<String>>,
}

impl Index {
    fn empty() -> Index {
        Index { version: VERSION, artists: Vec::new(), declined: Vec::new() }
    }

    fn declined(&self, names: &[String]) -> bool {
        self.declined.iter().any(|d| same(d, names))
    }
}

/// Whether two lists of names share one, ignoring case.
fn same(a: &[String], b: &[String]) -> bool {
    a.iter().any(|n| b.iter().any(|m| n.trim().eq_ignore_ascii_case(m.trim())))
}

/// Whose picture: a page's names, and their MusicBrainz ID when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub names: Vec<String>,
    pub mbid: Option<String>,
}

/// What the user gave for a picture.
#[derive(Clone, Debug)]
pub enum Input {
    File(PathBuf),
    /// Image bytes, as pasted.
    Bytes(Vec<u8>),
    Url(String),
    /// A Wikimedia Commons file the user picked, kept with its credit.
    Commons(String),
    /// The cover of a release, read from its first song, and the release's title.
    Cover(PathBuf, String),
}

impl Input {
    fn source(&self) -> Source {
        match self {
            Input::File(_) => Source::Local,
            Input::Bytes(_) | Input::Url(_) => Source::User,
            Input::Commons(_) => Source::Wikimedia,
            Input::Cover(..) => Source::Cover,
        }
    }

    fn origin(&self) -> Origin {
        let url = match self {
            Input::Url(url) => Some(url.clone()),
            _ => None,
        };
        let release = match self {
            Input::Cover(_, title) => Some(title.clone()),
            _ => None,
        };
        Origin { source: self.source(), url, attribution: None, release }
    }
}

/// Where a stored picture came from, for its entry.
#[derive(Clone, Debug)]
struct Origin {
    source: Source,
    url: Option<String>,
    attribution: Option<online::Attribution>,
    release: Option<String>,
}

/// What an artist page's picture is.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// One the user gave: its entry says where from.
    User(Box<Entry>),
    /// An `artist.jpg` in their music folder.
    Folder(PathBuf),
    /// One a lookup found, shown until the user keeps or removes it.
    Online(online::Attribution),
    /// None: their first album's cover shows.
    Cover,
}

/// A lookup's state for a page, by its first name in lowercase.
#[derive(Clone)]
enum Lookup {
    Pending,
    /// Nothing found, or the lookup failed: tried again next launch or when asked.
    Nothing,
    Found(Arc<Looked>),
}

/// A picture a lookup found, ready to show and to keep.
struct Looked {
    jpeg: Arc<[u8]>,
    url: String,
    attribution: online::Attribution,
}

/// A lookup's answer: the page's key, and the prepared picture if one was found.
type LookupDone = (String, Option<Looked>);
type LookupJob = (String, Vec<String>, Option<String>);

/// What the reader shrinks: a file, or bytes already in memory.
enum ReadFrom {
    Path(PathBuf),
    Bytes(Arc<[u8]>),
}

impl ReadFrom {
    fn thumb(&self) -> Option<(u32, u32, Vec<u8>)> {
        match self {
            ReadFrom::Path(path) => read_limited(path).ok().and_then(|bytes| shrink(&bytes, THUMB_PX)),
            ReadFrom::Bytes(bytes) => shrink(bytes, THUMB_PX),
        }
    }
}

/// Downloads a link's bytes. Real runs use [`download`]; headless runs and tests never reach
/// the network, and tests may stand in their own.
pub type Fetch = Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;

/// The picture picker: the candidates a lookup found for a page, their thumbnails as they arrive.
pub struct Picker {
    /// The page's names.
    pub names: Vec<String>,
    /// `None` while the candidates are gathered.
    pub candidates: Option<Result<Vec<online::Candidate>, String>>,
    /// Each candidate's thumbnail, by file, once downloaded (`None` when it couldn't be).
    thumbs: HashMap<String, Option<Image>>,
}

impl Picker {
    /// A candidate's thumbnail: `None` while it downloads, `Some(None)` when it couldn't be.
    pub fn thumb(&self, file: &str) -> Option<Option<Image>> {
        self.thumbs.get(file).cloned()
    }
}

enum PickerMsg {
    Candidates(Result<Vec<online::Candidate>, String>),
    Thumb(String, Option<(u32, u32, Vec<u8>)>),
}

/// A finished import: whose, from what, and the JPEG to store or why there's none.
type Done = (Target, Origin, Result<Vec<u8>, String>);
/// The reader's jobs (cache key, what to read) and answers (key, shrunk RGBA if it decoded).
type Reads = (Sender<(String, ReadFrom)>, Receiver<(String, Option<(u32, u32, Vec<u8>)>)>);

pub struct ArtistPictures {
    /// The data folder; `None` keeps pictures in memory (tests, headless runs).
    dir: Option<PathBuf>,
    index: Index,
    /// The index file couldn't be read: changing it would overwrite the user's file.
    unreadable: bool,
    /// Pictures by file, when there's no data folder.
    memory: HashMap<String, Arc<[u8]>>,
    cache: RefCell<Inner<String>>,
    images: ImageCache,
    /// Reads and shrinks pictures off the UI thread; `None` reads them as asked.
    reads: Option<Reads>,
    /// Imports run on their own threads once there's a waker; else on the calling one.
    waker: Option<Waker>,
    imports: (Sender<Done>, Receiver<Done>),
    /// Pages whose picture is being imported, by their first name.
    pending: Vec<String>,
    fetch: Fetch,
    /// Lookups by the page's first name in lowercase, this session.
    lookups: RefCell<HashMap<String, Lookup>>,
    /// The lookup worker, once started: one at a time, so the sites are asked gently.
    lookup_worker: Option<(Sender<LookupJob>, Receiver<LookupDone>)>,
    /// Each page's music-folder picture, found once per library version.
    folders: RefCell<(u64, HashMap<String, Option<PathBuf>>)>,
    /// The open picker, and what its worker sends; a new picker drops the old one's messages.
    picker: Option<Picker>,
    picker_msgs: Option<Receiver<PickerMsg>>,
    /// Bumped when a picture is set, removed or declined, or a lookup finishes.
    changes: u64,
}

impl ArtistPictures {
    /// The pictures saved in `dir` (none in memory), read on the calling thread until
    /// [`ArtistPictures::start`]. The error tells of an index that couldn't be read.
    pub fn open(dir: Option<PathBuf>, fetch: Fetch) -> (ArtistPictures, Option<String>) {
        let (index, error) = match &dir {
            Some(dir) => match load_index(&dir.join(INDEX_FILE)) {
                Ok(index) => (index, None),
                Err(e) => (Index::empty(), Some(e)),
            },
            None => (Index::empty(), None),
        };
        let pictures = ArtistPictures {
            dir,
            index,
            unreadable: error.is_some(),
            memory: HashMap::new(),
            cache: RefCell::new(Inner::new(CAPACITY)),
            images: ImageCache::new(Arc::new(|| {})),
            reads: None,
            waker: None,
            imports: channel(),
            pending: Vec::new(),
            fetch,
            lookups: RefCell::new(HashMap::new()),
            lookup_worker: None,
            folders: RefCell::new((0, HashMap::new())),
            picker: None,
            picker_msgs: None,
            changes: 0,
        };
        (pictures, error)
    }

    /// Reads and imports pictures on worker threads from now on, waking the window with each.
    pub fn start(&mut self, waker: Waker) {
        let (jobs, inbox) = channel::<(String, ReadFrom)>();
        let (outbox, done) = channel();
        let wake = waker.clone();
        let spawned = std::thread::Builder::new().name("artist-pictures".into()).spawn(move || {
            for (key, from) in inbox {
                if outbox.send((key, from.thumb())).is_err() {
                    return;
                }
                wake();
            }
        });
        if spawned.is_ok() {
            self.reads = Some((jobs, done));
        }
        let (jobs, inbox) = channel::<LookupJob>();
        let (outbox, done) = channel();
        let (wake, fetch) = (waker.clone(), self.fetch.clone());
        let spawned = std::thread::Builder::new().name("artist-picture-lookups".into()).spawn(move || {
            for (key, names, mbid) in inbox {
                if outbox.send((key, look_up(&names, mbid.as_deref(), &fetch))).is_err() {
                    return;
                }
                wake();
            }
        });
        if spawned.is_ok() {
            self.lookup_worker = Some((jobs, done));
        }
        self.images = ImageCache::new(waker.clone());
        self.waker = Some(waker);
    }

    /// Stands in for downloads, in tests.
    #[cfg(test)]
    pub fn set_fetch(&mut self, fetch: Fetch) {
        self.fetch = fetch;
    }

    fn entry(&self, names: &[String]) -> Option<&Entry> {
        self.index.artists.iter().find(|e| e.matches(names))
    }

    /// Whether the page with these names has a picture.
    pub fn has(&self, names: &[String]) -> bool {
        self.entry(names).is_some()
    }

    /// The saved entry for these names, for tests and tools.
    #[cfg(test)]
    pub fn entry_for(&self, names: &[String]) -> Option<Entry> {
        self.entry(names).cloned()
    }

    /// Whether a picture for the page with these names is being imported.
    pub fn importing(&self, names: &[String]) -> bool {
        names.first().is_some_and(|n| self.pending.iter().any(|p| p.eq_ignore_ascii_case(n)))
    }

    /// The picture for the page with these names, asking for it when it isn't loaded yet.
    /// `None` while it loads or when there is none.
    pub fn get(&self, names: &[String]) -> Option<Image> {
        let file = self.entry(names)?.file.clone();
        let from = match (self.memory.get(&file), &self.dir) {
            (Some(bytes), _) => ReadFrom::Bytes(bytes.clone()),
            (None, Some(dir)) => ReadFrom::Path(dir.join(&file)),
            (None, None) => return None,
        };
        self.cached(file, from)
    }

    /// The image cached under `key`, read from `from` when it isn't yet: on the reader once
    /// started, else now.
    fn cached(&self, key: String, from: ReadFrom) -> Option<Image> {
        let known = self.cache.borrow().slots.get(&key).cloned();
        match known {
            Some((Slot::Ready(image), _)) => {
                self.cache.borrow_mut().touch(&key);
                Some(image).filter(|i| i.error().is_none())
            }
            Some((Slot::Pending | Slot::Missing, _)) => None,
            None => match &self.reads {
                Some((jobs, _)) => {
                    self.cache.borrow_mut().slots.insert(key.clone(), (Slot::Pending, 0));
                    let _ = jobs.send((key, from));
                    None
                }
                None => {
                    let image = from.thumb().map(|(w, h, rgba)| self.images.rgba(w, h, rgba));
                    if let Some(image) = &image {
                        // A snapshot shows the picture decoded.
                        let _ = image.wait();
                    }
                    self.cache.borrow_mut().arrive(key, image.clone().map_or(Slot::Missing, Slot::Ready));
                    image
                }
            },
        }
    }

    /// The page's `artist.jpg` from its music folders, looked for once per library `version`.
    pub fn folder(&self, info: &crate::artist::ArtistPage, tracks: &[crate::library::Track], albums: &[crate::album::Album], version: u64) -> Option<PathBuf> {
        let key = info.names.first()?.to_lowercase();
        let mut folders = self.folders.borrow_mut();
        if folders.0 != version {
            *folders = (version, HashMap::new());
        }
        folders.1.entry(key).or_insert_with(|| folder::find(info, tracks, albums)).clone()
    }

    /// The image of a music-folder picture.
    pub fn folder_image(&self, path: &Path) -> Option<Image> {
        self.cached(format!("folder:{}", path.display()), ReadFrom::Path(path.to_path_buf()))
    }

    /// The picture a lookup found for the page with these names, and its attribution; asks
    /// for a lookup when there's been none this session, with `mbid` naming them when their
    /// tags do. None while it runs, when it found nothing, or when the user removed the one it found.
    /// With `start` false (lookups off) it only shows what a lookup the user asked for found.
    pub fn online(&self, names: &[String], start: bool, mbid: impl FnOnce() -> Option<String>) -> Option<(Option<Image>, online::Attribution)> {
        let key = names.first()?.to_lowercase();
        if self.index.declined(names) {
            return None;
        }
        let known = self.lookups.borrow().get(&key).cloned();
        let looked = match known {
            Some(Lookup::Found(looked)) => looked,
            Some(Lookup::Pending | Lookup::Nothing) => return None,
            None if !start => return None,
            None => {
                self.start_lookup(key.clone(), names.to_vec(), mbid());
                let Some(Lookup::Found(looked)) = self.lookups.borrow().get(&key).cloned() else { return None };
                looked
            }
        };
        Some((self.cached(format!("online:{key}"), ReadFrom::Bytes(looked.jpeg.clone())), looked.attribution.clone()))
    }

    fn start_lookup(&self, key: String, names: Vec<String>, mbid: Option<String>) {
        match &self.lookup_worker {
            Some((jobs, _)) => {
                self.lookups.borrow_mut().insert(key.clone(), Lookup::Pending);
                let _ = jobs.send((key, names, mbid));
            }
            None => {
                let found = look_up(&names, mbid.as_deref(), &self.fetch);
                self.lookups.borrow_mut().insert(key, found.map_or(Lookup::Nothing, |l| Lookup::Found(Arc::new(l))));
            }
        }
    }

    /// Opens the picker for the page with these names: its candidates are gathered and their
    /// thumbnails downloaded off the UI thread once started, else now.
    pub fn open_picker(&mut self, names: &[String], mbid: Option<String>) {
        let (tx, rx) = channel();
        self.picker = Some(Picker { names: names.to_vec(), candidates: None, thumbs: HashMap::new() });
        self.picker_msgs = Some(rx);
        let (names, fetch) = (names.to_vec(), self.fetch.clone());
        let job = move |wake: &dyn Fn()| {
            let found = online::candidates(&names, mbid.as_deref(), &fetch);
            let thumbs: Vec<(String, String)> = found.iter().flatten().map(|c| (c.file.clone(), c.thumb.clone())).collect();
            if tx.send(PickerMsg::Candidates(found)).is_err() {
                return;
            }
            wake();
            for (file, url) in thumbs {
                let thumb = fetch(&url).ok().and_then(|bytes| shrink(&bytes, online::THUMB_WIDTH));
                if tx.send(PickerMsg::Thumb(file, thumb)).is_err() {
                    return;
                }
                wake();
            }
        };
        match self.waker.clone() {
            Some(waker) => {
                let spawned = std::thread::Builder::new().name("artist-picture-picker".into()).spawn(move || job(&*waker));
                if let Err(e) = spawned
                    && let Some(picker) = &mut self.picker
                {
                    picker.candidates = Some(Err(format!("Couldn't start the lookup: {e}")));
                }
            }
            None => {
                job(&|| {});
                self.drain_picker();
            }
        }
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
        self.picker_msgs = None;
    }

    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }

    /// Takes the picker worker's news; true when any arrived.
    fn drain_picker(&mut self) -> bool {
        let Some(rx) = &self.picker_msgs else { return false };
        let msgs: Vec<PickerMsg> = rx.try_iter().collect();
        let Some(picker) = &mut self.picker else { return false };
        for msg in &msgs {
            match msg {
                PickerMsg::Candidates(found) => picker.candidates = Some(found.clone()),
                PickerMsg::Thumb(file, thumb) => {
                    let image = thumb.as_ref().map(|(w, h, rgba)| self.images.rgba(*w, *h, rgba.clone()));
                    if let Some(image) = &image
                        && self.waker.is_none()
                    {
                        let _ = image.wait();
                    }
                    picker.thumbs.insert(file.clone(), image);
                }
            }
        }
        !msgs.is_empty()
    }

    /// Whether a lookup for the page with these names is running.
    pub fn looking_up(&self, names: &[String]) -> bool {
        names.first().is_some_and(|n| matches!(self.lookups.borrow().get(&n.to_lowercase()), Some(Lookup::Pending)))
    }

    /// Keeps the picture a lookup found for `target` as theirs: copied in with its attribution,
    /// and never replaced by a lookup again. An error when there's none to keep.
    pub fn keep_online(&mut self, target: &Target) -> Result<(), String> {
        let key = target.names.first().map(|n| n.to_lowercase()).unwrap_or_default();
        let Some(Lookup::Found(looked)) = self.lookups.borrow().get(&key).cloned() else {
            return Err("There's no looked-up picture to keep".into());
        };
        let origin = Origin { source: Source::Wikimedia, url: Some(looked.url.clone()), attribution: Some(looked.attribution.clone()), release: None };
        self.set(target, looked.jpeg.to_vec(), origin)
    }

    /// Stops showing the picture a lookup found for these names, now and in later sessions.
    pub fn decline_online(&mut self, names: &[String]) -> Result<(), String> {
        self.refuse_unreadable()?;
        if let Some(key) = names.first().map(|n| n.to_lowercase()) {
            self.lookups.borrow_mut().insert(key, Lookup::Nothing);
        }
        if !self.index.declined(names) {
            self.index.declined.push(names.to_vec());
        }
        self.changes += 1;
        self.save()
    }

    /// Changes whenever what any page would show could have: a picture arrived, was set or
    /// removed, or a lookup finished. Memoized drawing of many pictures (the graph) keys on it.
    pub fn revision(&self) -> u64 {
        self.cache.borrow().generation().wrapping_add(self.changes)
    }

    /// Takes pictures the reader finished; true when any arrived.
    pub fn drain(&mut self) -> bool {
        let mut looked = self.drain_picker();
        if let Some((_, done)) = &self.lookup_worker {
            for (key, found) in done.try_iter() {
                looked = true;
                self.changes += 1;
                self.lookups.borrow_mut().insert(key, found.map_or(Lookup::Nothing, |l| Lookup::Found(Arc::new(l))));
            }
        }
        let Some((_, done)) = &self.reads else { return looked };
        let arrived: Vec<(String, Slot)> = done
            .try_iter()
            .map(|(file, thumb)| (file, thumb.map_or(Slot::Missing, |(w, h, rgba)| Slot::Ready(self.images.rgba(w, h, rgba)))))
            .collect();
        let any = !arrived.is_empty();
        if any {
            self.cache.borrow_mut().arrive_all(arrived);
        }
        any || looked
    }

    /// Prepares `input` as the picture for `target`: on a worker once started, else now. Its
    /// result comes from [`ArtistPictures::finish`].
    pub fn import(&mut self, target: Target, input: Input) {
        if let Some(first) = target.names.first() {
            self.pending.push(first.clone());
        }
        let (outbox, fetch) = (self.imports.0.clone(), self.fetch.clone());
        let job = move || {
            let mut origin = input.origin();
            let bytes = match input {
                // A picked Commons file brings its credit, read with it.
                Input::Commons(file) => online::file_picture(&file, &fetch).map(|found| {
                    origin.url = Some(found.url);
                    origin.attribution = Some(found.attribution);
                    found.bytes
                }),
                input => input_bytes(input, &fetch),
            };
            let result = bytes.and_then(|bytes| prepare(&bytes));
            let _ = outbox.send((target, origin, result));
        };
        match self.waker.clone() {
            Some(waker) => {
                let spawned = std::thread::Builder::new().name("artist-picture-import".into()).spawn(move || {
                    job();
                    waker();
                });
                if let Err(e) = spawned {
                    let origin = Origin { source: Source::User, url: None, attribution: None, release: None };
                    let _ = self.imports.0.send((Target { names: Vec::new(), mbid: None }, origin, Err(format!("Couldn't start the import: {e}"))));
                }
            }
            None => job(),
        }
    }

    /// Stores the imports that finished. Errors say why one couldn't be used; true when any finished.
    pub fn finish(&mut self) -> (bool, Option<String>) {
        let done: Vec<Done> = self.imports.1.try_iter().collect();
        let mut error = None;
        for (target, origin, result) in &done {
            if let Some(i) = target.names.first().and_then(|first| self.pending.iter().position(|p| p.eq_ignore_ascii_case(first))) {
                self.pending.remove(i);
            }
            let stored = result.clone().and_then(|jpeg| self.set(target, jpeg, origin.clone()));
            if let Err(e) = stored {
                error = Some(e);
            }
        }
        (!done.is_empty(), error)
    }

    /// Saves `jpeg` as the picture of `target`'s names, replacing the one they had.
    fn set(&mut self, target: &Target, jpeg: Vec<u8>, origin: Origin) -> Result<(), String> {
        if target.names.is_empty() {
            return Err("Couldn't tell whose picture that is".into());
        }
        self.refuse_unreadable()?;
        let now = now();
        let file = format!("{FOLDER}/{}.jpg", new_id(&target.names));
        match &self.dir {
            Some(dir) => {
                let path = dir.join(&file);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't save the picture: {e}"))?;
                }
                std::fs::write(&path, &jpeg).map_err(|e| format!("Couldn't save the picture: {e}"))?;
            }
            None => {
                self.memory.insert(file.clone(), jpeg.into());
            }
        }
        let old: Vec<Entry> = self.index.artists.iter().filter(|e| e.matches(&target.names)).cloned().collect();
        let added = old.iter().map(|e| e.added).min().unwrap_or(now);
        let mbid = target.mbid.clone().or_else(|| old.iter().find_map(|e| e.mbid.clone()));
        self.index.artists.retain(|e| !e.matches(&target.names));
        let Origin { source, url, attribution, release } = origin;
        self.index.artists.push(Entry { names: target.names.clone(), mbid, file, source, url, attribution, release, added, updated: now });
        self.changes += 1;
        self.save()?;
        for e in old {
            self.forget_file(&e.file);
        }
        Ok(())
    }

    /// Removes the picture of the page with these names; the album cover shows again.
    pub fn remove(&mut self, names: &[String]) -> Result<(), String> {
        self.refuse_unreadable()?;
        let old: Vec<Entry> = self.index.artists.iter().filter(|e| e.matches(names)).cloned().collect();
        if old.is_empty() {
            return Ok(());
        }
        self.index.artists.retain(|e| !e.matches(names));
        // A kept Wikimedia picture they removed isn't looked up and shown again.
        if old.iter().any(|e| e.source == Source::Wikimedia) && !self.index.declined(names) {
            self.index.declined.push(names.to_vec());
            if let Some(key) = names.first().map(|n| n.to_lowercase()) {
                self.lookups.borrow_mut().insert(key, Lookup::Nothing);
            }
        }
        self.changes += 1;
        self.save()?;
        for e in old {
            self.forget_file(&e.file);
        }
        Ok(())
    }

    /// The saved entry of the page with these names: where its picture came from.
    pub fn entry_of(&self, names: &[String]) -> Option<Entry> {
        self.entry(names).cloned()
    }

    fn refuse_unreadable(&self) -> Result<(), String> {
        match self.unreadable {
            true => Err(format!("The saved artist pictures in {INDEX_FILE} couldn't be read, so they weren't changed")),
            false => Ok(()),
        }
    }

    fn forget_file(&mut self, file: &str) {
        self.memory.remove(file);
        self.cache.borrow_mut().slots.remove(file);
        // Only files Peri named, inside its own folder.
        if let Some(dir) = &self.dir
            && file.starts_with(&format!("{FOLDER}/"))
            && !file.contains("..")
        {
            let _ = std::fs::remove_file(dir.join(file));
        }
    }

    fn save(&self) -> Result<(), String> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let text = serde_json::to_string_pretty(&self.index).map_err(|e| e.to_string())?;
        let path = dir.join(INDEX_FILE);
        let tmp = dir.join(format!("{INDEX_FILE}.tmp"));
        std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(&tmp, text))
            .and_then(|_| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("Couldn't save the artist pictures: {e}"))
    }
}

fn load_index(path: &Path) -> Result<Index, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::empty()),
        Err(e) => return Err(format!("Couldn't read the artist pictures in {}: {e}", path.display())),
    };
    let index: Index = serde_json::from_str(&text).map_err(|e| format!("Couldn't read the artist pictures in {}: {e}", path.display()))?;
    if index.version != VERSION {
        return Err(format!("Couldn't read the artist pictures in {}: they're from another version of peri", path.display()));
    }
    Ok(index)
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A file name no other picture has: a hash of the names and the time.
fn new_id(names: &[String]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    names.iter().map(|n| n.to_lowercase()).collect::<Vec<_>>().hash(&mut hash);
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()).hash(&mut hash);
    std::process::id().hash(&mut hash);
    format!("{:016x}", hash.finish())
}

/// Runs a lookup and prepares what it found to show and keep. Failures find nothing: lookups
/// run unasked, so they don't interrupt with errors.
fn look_up(names: &[String], mbid: Option<&str>, fetch: &Fetch) -> Option<Looked> {
    let found = online::lookup(names, mbid, fetch).ok()??;
    let jpeg = prepare(&found.bytes).ok()?;
    Some(Looked { jpeg: jpeg.into(), url: found.url, attribution: found.attribution })
}

/// Whether `path` names an image the picker and drops take.
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Reads a file of at most [`MAX_BYTES`].
fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let meta = std::fs::metadata(path).map_err(|e| format!("Couldn't read {name}: {e}"))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("{name} is too large for a picture (over {} MB)", MAX_BYTES / (1024 * 1024)));
    }
    std::fs::read(path).map_err(|e| format!("Couldn't read {name}: {e}"))
}

fn input_bytes(input: Input, fetch: &Fetch) -> Result<Vec<u8>, String> {
    match input {
        Input::File(path) => read_limited(&path),
        Input::Bytes(bytes) if bytes.len() as u64 > MAX_BYTES => Err("That image is too large for a picture".into()),
        Input::Bytes(bytes) => Ok(bytes),
        Input::Url(url) | Input::Commons(url) => fetch(&url),
        Input::Cover(song, title) => crate::library::read_cover(&song).ok_or_else(|| format!("{title} has no cover")),
    }
}

/// Decodes an image, shrinks it to [`STORED_PX`] on a side, and encodes it as JPEG, any
/// transparency over white. An error when the bytes aren't an image peri reads.
pub fn prepare(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let not_image = || "That isn't an image Peri can read (PNG, JPEG, WebP or GIF)".to_string();
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|_| not_image())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    let img = reader.decode().map_err(|_| not_image())?;
    let img = if img.width() > STORED_PX || img.height() > STORED_PX { img.thumbnail(STORED_PX, STORED_PX) } else { img };
    let rgba = img.into_rgba8();
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let over = |c: u8| ((c as u16 * a as u16 + 255 * (255 - a as u16)) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb).map_err(|e| format!("Couldn't encode the picture: {e}"))?;
    Ok(out)
}

/// What a paste holds for a picture: an image, a copied image file, a link or a path. `None`
/// when it holds none of those.
pub fn from_clipboard(content: &rune::ClipboardContent) -> Option<Input> {
    if let Some(image) = content.image() {
        return Some(Input::Bytes(image.png().to_vec()));
    }
    if let Some(file) = content.files().iter().find(|f| is_image(f)) {
        return Some(Input::File(file.clone()));
    }
    from_text(content.text())
}

/// A pasted link to an image, or the path of an image file.
pub fn from_text(text: &str) -> Option<Input> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return (!text.chars().any(char::is_whitespace)).then(|| Input::Url(text.to_string()));
    }
    let path = PathBuf::from(text.strip_prefix("file://").unwrap_or(text));
    (path.is_absolute() && is_image(&path) && path.is_file()).then_some(Input::File(path))
}

/// Downloads a pasted link or a lookup's request: at most [`MAX_BYTES`], within [`DOWNLOAD_TIMEOUT`].
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        // MusicBrainz and Wikimedia ask for a user agent that says who's asking.
        .user_agent(concat!("peri/", env!("CARGO_PKG_VERSION"), " (https://github.com/Narwhai/peri)"))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| format!("Couldn't download the picture: {e}"))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => format!("That picture is too large (over {} MB)", MAX_BYTES / (1024 * 1024)),
            e => format!("Couldn't download the picture: {e}"),
        })
}

/// For headless runs and tests, which never reach the network.
pub fn no_downloads(_: &str) -> Result<Vec<u8>, String> {
    Err("Downloading pictures is off in headless runs".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn target(v: &[&str]) -> Target {
        Target { names: names(v), mbid: None }
    }

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let dir = std::env::temp_dir().join(format!("peri-pictures-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn open(dir: &Dir) -> ArtistPictures {
        let (pictures, error) = ArtistPictures::open(Some(dir.0.clone()), Arc::new(no_downloads));
        assert!(error.is_none(), "{error:?}");
        pictures
    }

    #[test]
    fn large_images_shrink_to_a_thousand_pixels_as_jpeg_and_transparency_goes_white() {
        let jpeg = prepare(&png(3000, 1500, [200, 30, 60, 255])).unwrap();
        assert_eq!(image::guess_format(&jpeg).unwrap(), image::ImageFormat::Jpeg);
        let img = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (1000, 500));
        let small = image::load_from_memory(&prepare(&png(40, 30, [0, 0, 0, 0])).unwrap()).unwrap().into_rgb8();
        assert_eq!((small.width(), small.height()), (40, 30), "small ones keep their size");
        assert!(small.get_pixel(5, 5).0.iter().all(|&c| c > 245), "clear pixels turn white: {:?}", small.get_pixel(5, 5));
        assert!(prepare(b"<html>not an image</html>").is_err());
    }

    #[test]
    fn a_picture_is_copied_in_and_found_by_any_of_its_names_ignoring_case() {
        let dir = Dir::new("set");
        let mut pictures = open(&dir);
        pictures.import(Target { names: names(&["The Alchemist", "Alan Maman"]), mbid: Some("alc".into()) }, Input::Bytes(png(1200, 1200, [10, 20, 30, 255])));
        assert_eq!(pictures.finish(), (true, None));
        let entry = pictures.entry_for(&names(&["alan maman"])).expect("found by a merged name");
        assert_eq!(entry.source, Source::User);
        assert_eq!(entry.mbid.as_deref(), Some("alc"));
        assert!(entry.file.starts_with("artists/") && entry.file.ends_with(".jpg"));
        let stored = image::load_from_memory(&std::fs::read(dir.0.join(&entry.file)).unwrap()).unwrap();
        assert_eq!(stored.width(), STORED_PX);
        assert!(!pictures.has(&names(&["Armand Hammer"])), "other artists keep their own");

        // Next launch reads it back.
        let reopened = open(&dir);
        assert_eq!(reopened.entry_for(&names(&["THE ALCHEMIST"])), Some(entry.clone()));
        let image = reopened.get(&names(&["The Alchemist"])).expect("loads");
        assert_eq!(image.wait().ok(), Some([THUMB_PX, THUMB_PX]), "shown at the cover size");
    }

    #[test]
    fn replacing_keeps_when_it_was_added_and_removes_the_old_file_and_remove_clears_it() {
        let dir = Dir::new("replace");
        let mut pictures = open(&dir);
        let file = dir.0.join("pic.png");
        std::fs::write(&file, png(50, 50, [1, 2, 3, 255])).unwrap();
        pictures.import(target(&["Somebody"]), Input::File(file));
        pictures.finish();
        let first = pictures.entry_for(&names(&["Somebody"])).unwrap();
        assert_eq!(first.source, Source::Local);
        pictures.import(target(&["somebody"]), Input::Bytes(png(60, 60, [4, 5, 6, 255])));
        pictures.finish();
        let second = pictures.entry_for(&names(&["Somebody"])).unwrap();
        assert_ne!(first.file, second.file);
        assert_eq!(second.added, first.added);
        assert!(!dir.0.join(&first.file).exists(), "the replaced picture is deleted");
        assert!(dir.0.join(&second.file).exists());
        pictures.remove(&names(&["Somebody"])).unwrap();
        assert!(!pictures.has(&names(&["Somebody"])));
        assert!(!dir.0.join(&second.file).exists());
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.0.join(INDEX_FILE)).unwrap()).unwrap();
        assert_eq!(saved["artists"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn bad_input_is_refused_and_nothing_is_saved() {
        let dir = Dir::new("bad");
        let mut pictures = open(&dir);
        pictures.import(target(&["A"]), Input::Bytes(b"plain text".to_vec()));
        let (finished, error) = pictures.finish();
        assert!(finished && error.is_some_and(|e| e.contains("isn't an image")));
        pictures.import(target(&["A"]), Input::Url("https://example.com/a.jpg".into()));
        assert!(pictures.finish().1.is_some_and(|e| e.contains("off in headless runs")), "headless runs never download");
        assert!(!pictures.has(&names(&["A"])));
        assert!(!dir.0.join(INDEX_FILE).exists());
    }

    #[test]
    fn an_unreadable_index_is_left_alone() {
        let dir = Dir::new("unreadable");
        std::fs::write(dir.0.join(INDEX_FILE), r#"{ "version": 1, "artistz": [] }"#).unwrap();
        let (mut pictures, error) = ArtistPictures::open(Some(dir.0.clone()), Arc::new(no_downloads));
        assert!(error.is_some());
        pictures.import(target(&["A"]), Input::Bytes(png(10, 10, [0, 0, 0, 255])));
        assert!(pictures.finish().1.is_some_and(|e| e.contains("couldn't be read")));
        assert!(std::fs::read_to_string(dir.0.join(INDEX_FILE)).unwrap().contains("artistz"));
    }

    #[test]
    fn pastes_offer_an_image_a_copied_file_a_link_or_a_path() {
        let image = rune::ClipboardImage::from_png(png(3, 2, [0, 0, 0, 255])).unwrap();
        assert!(matches!(from_clipboard(&rune::ClipboardContent::new("").with_image(image)), Some(Input::Bytes(_))));
        let files = rune::ClipboardContent::new("").with_files([PathBuf::from("/a/notes.txt"), PathBuf::from("/a/photo.JPG")]);
        assert!(matches!(from_clipboard(&files), Some(Input::File(p)) if p == Path::new("/a/photo.JPG")));
        let link = "https://lastfm.freetls.fastly.net/i/u/770x0/abc.jpg";
        assert!(matches!(from_text(&format!("  {link}\n")), Some(Input::Url(u)) if u == link));
        assert!(from_text("hello there").is_none());
        assert!(from_text("https://a b").is_none());
        assert!(from_text("/nowhere/at/all.png").is_none(), "a path to nothing");
    }
}

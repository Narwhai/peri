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

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use rune::{Image, ImageCache, Waker};
use serde::{Deserialize, Serialize};

use crate::covers::{Inner, Slot, THUMB_PX, shrink};

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
/// Decoded pictures kept; few artist pages are open at once.
const CAPACITY: usize = 32;
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
    /// The link it was downloaded from, when pasted as one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// When the artist first got a picture, and when it last changed (Unix seconds).
    pub added: u64,
    pub updated: u64,
}

impl Entry {
    fn matches(&self, names: &[String]) -> bool {
        self.names.iter().any(|n| names.iter().any(|m| n.trim().eq_ignore_ascii_case(m.trim())))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    #[serde(default)]
    artists: Vec<Entry>,
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
}

impl Input {
    fn source(&self) -> Source {
        match self {
            Input::File(_) => Source::Local,
            Input::Bytes(_) | Input::Url(_) => Source::User,
        }
    }

    fn url(&self) -> Option<String> {
        match self {
            Input::Url(url) => Some(url.clone()),
            _ => None,
        }
    }
}

/// Downloads a link's bytes. Real runs use [`download`]; headless runs and tests never reach
/// the network, and tests may stand in their own.
pub type Fetch = Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;

/// A finished import: whose, from what, and the JPEG to store or why there's none.
type Done = (Target, Source, Option<String>, Result<Vec<u8>, String>);
/// The reader's jobs (index file, path) and answers (file, shrunk RGBA if it decoded).
type Reads = (Sender<(String, PathBuf)>, Receiver<(String, Option<(u32, u32, Vec<u8>)>)>);

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
}

impl ArtistPictures {
    /// The pictures saved in `dir` (none in memory), read on the calling thread until
    /// [`ArtistPictures::start`]. The error tells of an index that couldn't be read.
    pub fn open(dir: Option<PathBuf>, fetch: Fetch) -> (ArtistPictures, Option<String>) {
        let (index, error) = match &dir {
            Some(dir) => match load_index(&dir.join(INDEX_FILE)) {
                Ok(index) => (index, None),
                Err(e) => (Index { version: VERSION, artists: Vec::new() }, Some(e)),
            },
            None => (Index { version: VERSION, artists: Vec::new() }, None),
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
        };
        (pictures, error)
    }

    /// Reads and imports pictures on worker threads from now on, waking the window with each.
    pub fn start(&mut self, waker: Waker) {
        let (jobs, inbox) = channel::<(String, PathBuf)>();
        let (outbox, done) = channel();
        let wake = waker.clone();
        let spawned = std::thread::Builder::new().name("artist-pictures".into()).spawn(move || {
            for (file, path) in inbox {
                let thumb = read_limited(&path).ok().and_then(|bytes| shrink(&bytes, THUMB_PX));
                if outbox.send((file, thumb)).is_err() {
                    return;
                }
                wake();
            }
        });
        if spawned.is_ok() {
            self.reads = Some((jobs, done));
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
        let known = self.cache.borrow().slots.get(&file).cloned();
        match known {
            Some((Slot::Ready(image), _)) => {
                self.cache.borrow_mut().touch(&file);
                Some(image).filter(|i| i.error().is_none())
            }
            Some((Slot::Pending | Slot::Missing, _)) => None,
            None => match (&self.reads, &self.dir) {
                (Some((jobs, _)), Some(dir)) if !self.memory.contains_key(&file) => {
                    self.cache.borrow_mut().slots.insert(file.clone(), (Slot::Pending, 0));
                    let _ = jobs.send((file.clone(), dir.join(&file)));
                    None
                }
                _ => {
                    let bytes = match self.memory.get(&file) {
                        Some(bytes) => Some(bytes.to_vec()),
                        None => self.dir.as_ref().and_then(|dir| read_limited(&dir.join(&file)).ok()),
                    };
                    let image = bytes.and_then(|b| shrink(&b, THUMB_PX)).map(|(w, h, rgba)| self.images.rgba(w, h, rgba));
                    if let Some(image) = &image {
                        // A snapshot shows the picture decoded.
                        let _ = image.wait();
                    }
                    self.cache.borrow_mut().arrive(file, image.clone().map_or(Slot::Missing, Slot::Ready));
                    image
                }
            },
        }
    }

    /// Takes pictures the reader finished; true when any arrived.
    pub fn drain(&self) -> bool {
        let Some((_, done)) = &self.reads else { return false };
        let arrived: Vec<(String, Slot)> = done
            .try_iter()
            .map(|(file, thumb)| (file, thumb.map_or(Slot::Missing, |(w, h, rgba)| Slot::Ready(self.images.rgba(w, h, rgba)))))
            .collect();
        let any = !arrived.is_empty();
        if any {
            self.cache.borrow_mut().arrive_all(arrived);
        }
        any
    }

    /// Prepares `input` as the picture for `target`: on a worker once started, else now. Its
    /// result comes from [`ArtistPictures::finish`].
    pub fn import(&mut self, target: Target, input: Input) {
        if let Some(first) = target.names.first() {
            self.pending.push(first.clone());
        }
        let (outbox, fetch) = (self.imports.0.clone(), self.fetch.clone());
        let job = move || {
            let (source, url) = (input.source(), input.url());
            let result = input_bytes(input, &fetch).and_then(|bytes| prepare(&bytes));
            let _ = outbox.send((target, source, url, result));
        };
        match self.waker.clone() {
            Some(waker) => {
                let spawned = std::thread::Builder::new().name("artist-picture-import".into()).spawn(move || {
                    job();
                    waker();
                });
                if let Err(e) = spawned {
                    let _ = self.imports.0.send((Target { names: Vec::new(), mbid: None }, Source::User, None, Err(format!("Couldn't start the import: {e}"))));
                }
            }
            None => job(),
        }
    }

    /// Stores the imports that finished. Errors say why one couldn't be used; true when any finished.
    pub fn finish(&mut self) -> (bool, Option<String>) {
        let done: Vec<Done> = self.imports.1.try_iter().collect();
        let mut error = None;
        for (target, source, url, result) in &done {
            if let Some(i) = target.names.first().and_then(|first| self.pending.iter().position(|p| p.eq_ignore_ascii_case(first))) {
                self.pending.remove(i);
            }
            let stored = result.clone().and_then(|jpeg| self.set(target, jpeg, *source, url.clone()));
            if let Err(e) = stored {
                error = Some(e);
            }
        }
        (!done.is_empty(), error)
    }

    /// Saves `jpeg` as the picture of `target`'s names, replacing the one they had.
    fn set(&mut self, target: &Target, jpeg: Vec<u8>, source: Source, url: Option<String>) -> Result<(), String> {
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
        self.index.artists.push(Entry { names: target.names.clone(), mbid, file, source, url, added, updated: now });
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
        self.save()?;
        for e in old {
            self.forget_file(&e.file);
        }
        Ok(())
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index { version: VERSION, artists: Vec::new() }),
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
        Input::Url(url) => fetch(&url),
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

/// Downloads a pasted link: at most [`MAX_BYTES`], within [`DOWNLOAD_TIMEOUT`].
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent(concat!("peri/", env!("CARGO_PKG_VERSION")))
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

//! Finding audio files under folders, reading their tags, and reading cover art.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};

/// Extensions rodio (through symphonia) can decode.
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "mp4", "aac", "aif", "aiff", "caf"];

/// File names checked next to a track when it embeds no cover.
const FOLDER_COVERS: &[&str] = &["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png"];

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub disc: Option<u32>,
    pub number: Option<u32>,
    /// Seconds, when the file's properties could be read.
    pub duration: Option<f64>,
}

impl Track {
    /// Whether every word of `query` (already lowercased) appears in the title, artist or album.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {}",
            self.title.to_lowercase(),
            self.artist.as_deref().map(str::to_lowercase).unwrap_or_default(),
            self.album.as_deref().map(str::to_lowercase).unwrap_or_default()
        );
        query.split_whitespace().all(|word| haystack.contains(word))
    }

    /// Library order: artist, album, disc, track number, then title, ignoring case.
    fn sort_key(&self) -> (String, String, u32, u32, String) {
        (
            self.artist.as_deref().map(str::to_lowercase).unwrap_or_default(),
            self.album.as_deref().map(str::to_lowercase).unwrap_or_default(),
            self.disc.unwrap_or(0),
            self.number.unwrap_or(0),
            self.title.to_lowercase(),
        )
    }
}

/// Sorts into library order.
pub fn sort(tracks: &mut [Track]) {
    tracks.sort_by_cached_key(Track::sort_key);
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub enum ScanMsg {
    /// Files found so far.
    Progress(usize),
    /// The sorted library and the files or folders that couldn't be read.
    Done { tracks: Vec<Track>, errors: Vec<String> },
}

/// Scans `roots` (folders, searched recursively, or audio files) on a worker thread. The worker
/// calls `wake` after each message and stops early once the receiver is dropped.
pub fn scan(roots: Vec<PathBuf>, wake: impl Fn() + Send + 'static) -> Receiver<ScanMsg> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("library-scan".into())
        .spawn(move || {
            let mut files = Vec::new();
            let mut errors = Vec::new();
            for root in &roots {
                collect(root, &mut files, &mut errors);
            }
            let mut tracks = Vec::with_capacity(files.len());
            for (i, path) in files.into_iter().enumerate() {
                tracks.push(read_track(path));
                if i % 50 == 49 && !send(&tx, ScanMsg::Progress(i + 1), &wake) {
                    return;
                }
            }
            sort(&mut tracks);
            send(&tx, ScanMsg::Done { tracks, errors }, &wake);
        })
        .expect("spawn the library scan thread");
    rx
}

fn send(tx: &Sender<ScanMsg>, msg: ScanMsg, wake: &impl Fn()) -> bool {
    let sent = tx.send(msg).is_ok();
    if sent {
        wake();
    }
    sent
}

fn collect(path: &Path, files: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    if path.is_file() {
        if is_audio(path) {
            files.push(path.to_path_buf());
        }
        return;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => return errors.push(format!("{}: {e}", path.display())),
    };
    let mut children: Vec<_> = entries.filter_map(Result::ok).collect();
    children.sort_by_key(|e| e.file_name());
    for entry in children {
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        // `file_type` doesn't follow symlinks, so a link back up the tree can't loop the scan.
        let Ok(kind) = entry.file_type() else { continue };
        if hidden {
            continue;
        }
        if kind.is_dir() {
            collect(&entry.path(), files, errors);
        } else if is_audio(&entry.path()) {
            files.push(entry.path());
        }
    }
}

fn read_track(path: PathBuf) -> Track {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let tagged = Probe::open(&path).and_then(|p| p.options(ParseOptions::new().read_cover_art(false)).read());
    let Ok(tagged) = tagged else {
        return Track { path, title: stem, artist: None, album: None, disc: None, number: None, duration: None };
    };
    let tag = best_tag(&tagged);
    let text = |f: fn(&Tag) -> Option<std::borrow::Cow<'_, str>>| {
        tag.and_then(f).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };
    let duration = tagged.properties().duration().as_secs_f64();
    Track {
        title: text(|t| t.title()).unwrap_or(stem),
        artist: text(|t| t.artist()),
        album: text(|t| t.album()),
        disc: tag.and_then(|t| t.disk()),
        number: tag.and_then(|t| t.track()),
        duration: (duration > 0.0).then_some(duration),
        path,
    }
}

fn best_tag(tagged: &TaggedFile) -> Option<&Tag> {
    tagged.primary_tag().or_else(|| tagged.first_tag())
}

/// The track's cover: its embedded front cover (else its first picture), else an image file
/// beside it. None when it has neither.
pub fn read_cover(path: &Path) -> Option<Vec<u8>> {
    let embedded = Probe::open(path).and_then(|p| p.read()).ok().and_then(|tagged| {
        let pictures: Vec<_> = tagged.tags().iter().flat_map(|t| t.pictures()).collect();
        let front = pictures.iter().find(|p| p.pic_type() == PictureType::CoverFront).or(pictures.first());
        front.map(|p| p.data().to_vec())
    });
    embedded.or_else(|| {
        let dir = path.parent()?;
        FOLDER_COVERS.iter().find_map(|name| std::fs::read(dir.join(name)).ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: Option<&str>, album: Option<&str>, number: Option<u32>) -> Track {
        Track {
            path: PathBuf::from(format!("/music/{title}.mp3")),
            title: title.into(),
            artist: artist.map(Into::into),
            album: album.map(Into::into),
            disc: None,
            number,
            duration: Some(1.0),
        }
    }

    #[test]
    fn search_matches_every_word_across_fields_ignoring_case() {
        let t = track("Sun Gong", Some("Laraaji"), Some("Celestial Vibration"), None);
        assert!(t.matches("laraaji sun"));
        assert!(t.matches("celestial"));
        assert!(!t.matches("laraaji moon"));
        assert!(t.matches(""));
    }

    #[test]
    fn audio_extensions_ignore_case() {
        assert!(is_audio(Path::new("a/B.FLAC")));
        assert!(is_audio(Path::new("x.m4a")));
        assert!(!is_audio(Path::new("cover.jpg")));
        assert!(!is_audio(Path::new("noext")));
    }

    #[test]
    fn library_sorts_by_artist_album_then_track_number() {
        let mut v = [
            track("B", Some("zed"), Some("one"), Some(1)),
            track("Second", Some("Abe"), Some("one"), Some(2)),
            track("First", Some("abe"), Some("one"), Some(1)),
        ];
        v.sort_by_cached_key(Track::sort_key);
        let titles: Vec<_> = v.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["First", "Second", "B"]);
    }

    #[test]
    fn scanning_finds_audio_recursively_skipping_hidden_and_other_files() {
        let root = std::env::temp_dir().join(format!("rune-music-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("album")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        for f in ["album/01 Intro.mp3", "album/cover.jpg", ".hidden/x.mp3", "loose.flac"] {
            std::fs::write(root.join(f), b"not really audio").unwrap();
        }
        let rx = scan(vec![root.clone()], || {});
        let tracks = loop {
            match rx.recv().unwrap() {
                ScanMsg::Progress(_) => {}
                ScanMsg::Done { tracks, .. } => break tracks,
            }
        };
        let mut titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["01 Intro", "loose"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}

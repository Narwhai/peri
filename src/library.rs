//! Finding audio files under folders, reading their tags, and reading cover art.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};

/// Extensions rodio (through symphonia) can decode.
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "mp4", "aac", "aif", "aiff", "caf"];

/// File names checked next to a track when it embeds no cover.
const FOLDER_COVERS: &[&str] = &["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    /// Track artists; a tag can hold several (separate items, or `;`-separated).
    pub artists: Vec<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    /// Genres; split on `;` and `,`, and across separate tag items.
    pub genres: Vec<String>,
    /// From the recording date, else the year, release date or original release date.
    pub year: Option<u32>,
    pub composers: Vec<String>,
    pub label: Option<String>,
    pub disc: Option<u32>,
    pub number: Option<u32>,
    /// Seconds, when the file's properties could be read.
    pub duration: Option<f64>,
    /// When the file was created (else last modified), in seconds since the Unix epoch.
    pub added: Option<i64>,
}

impl Track {
    /// The artists as one line, for display.
    pub fn artist(&self) -> Option<String> {
        (!self.artists.is_empty()).then(|| self.artists.join(", "))
    }

    /// Whether every word of `query` (already lowercased) appears in the title, artist or album.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {}",
            self.title.to_lowercase(),
            self.artists.join(" ").to_lowercase(),
            self.album.as_deref().map(str::to_lowercase).unwrap_or_default()
        );
        query.split_whitespace().all(|word| haystack.contains(word))
    }

    /// Library order: artist, album, disc, track number, then title, ignoring case.
    fn sort_key(&self) -> (String, String, u32, u32, String) {
        (
            self.artists.join(", ").to_lowercase(),
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
    let added = file_added(&path);
    let Ok(tagged) = tagged else {
        return Track { path, title: stem, added, ..Track::default() };
    };
    let tag = best_tag(&tagged);
    let text = |f: fn(&Tag) -> Option<std::borrow::Cow<'_, str>>| {
        tag.and_then(f).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };
    let item = |key: ItemKey| tag.and_then(|t| t.get_string(key)).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    let many = |keys: &[ItemKey], separators: &[char]| -> Vec<String> {
        let values = tag.into_iter().flat_map(|t| keys.iter().flat_map(move |&k| t.get_strings(k)));
        split_values(values, separators)
    };
    let duration = tagged.properties().duration().as_secs_f64();
    Track {
        title: text(|t| t.title()).unwrap_or(stem),
        artists: many(&[ItemKey::TrackArtist, ItemKey::TrackArtists], &[';', '\0']),
        album_artist: item(ItemKey::AlbumArtist),
        album: text(|t| t.album()),
        genres: many(&[ItemKey::Genre], &[';', ',', '\0']),
        year: tag.and_then(year),
        composers: many(&[ItemKey::Composer], &[';', '\0']),
        label: item(ItemKey::Label).or_else(|| item(ItemKey::Publisher)),
        disc: tag.and_then(|t| t.disk()),
        number: tag.and_then(|t| t.track()),
        duration: (duration > 0.0).then_some(duration),
        added,
        path,
    }
}

/// Splits tag values on `separators`, trimming, dropping empties and repeats (ignoring case).
pub fn split_values<'a>(values: impl Iterator<Item = &'a str>, separators: &[char]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in values.flat_map(|v| v.split(separators)) {
        let part = part.trim();
        if !part.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(part)) {
            out.push(part.to_string());
        }
    }
    out
}

fn year(tag: &Tag) -> Option<u32> {
    let parsed = |key: ItemKey| {
        let text = tag.get_string(key)?;
        // Dates start with the year: "2016", "2016-05-20", "2016/05".
        let digits: String = text.trim().chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok().filter(|y| (1000..=9999).contains(y))
    };
    tag.date()
        .map(|d| u32::from(d.year))
        .or_else(|| [ItemKey::Year, ItemKey::ReleaseDate, ItemKey::OriginalReleaseDate].into_iter().find_map(parsed))
}

fn file_added(path: &Path) -> Option<i64> {
    let meta = std::fs::metadata(path).ok()?;
    let time = meta.created().or_else(|_| meta.modified()).ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
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
            artists: artist.into_iter().map(Into::into).collect(),
            album: album.map(Into::into),
            number,
            duration: Some(1.0),
            ..Track::default()
        }
    }

    #[test]
    fn multi_value_tags_split_trim_and_drop_repeats() {
        let genres = split_values(["Rock;Indie Rock", " rock , Dream Pop", ""].into_iter(), &[';', ',']);
        assert_eq!(genres, ["Rock", "Indie Rock", "Dream Pop"]);
        let artists = split_values(["Armand Hammer & The Alchemist", "Big Thief;Laraaji"].into_iter(), &[';']);
        assert_eq!(artists, ["Armand Hammer & The Alchemist", "Big Thief", "Laraaji"], "only `;` separates artists");
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

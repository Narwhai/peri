//! Play counts: every play that counts (see [`crate::listen`]) is a line appended to
//! `plays.jsonl` next to `prefs.json`. The counts and last-played dates are tallied from it
//! at launch.
//!
//! Each line keeps what names the song (path, MusicBrainz recording ID, artist, title,
//! album), so how songs are matched can change later without losing plays. Copies of one
//! song share its plays: they match by recording ID, else artist, title and album ignoring
//! case, else by path.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::Track;

pub const FILE: &str = "plays.jsonl";

/// One play, as a line of the log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// When the play counted, in seconds since the Unix epoch.
    pub at: i64,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

impl Entry {
    pub fn of(track: &Track, at: i64) -> Entry {
        Entry { at, path: track.path.clone(), mbid: track.recording_id.clone(), artist: track.artist(), title: track.title.clone(), album: track.album.clone() }
    }

    fn key(&self) -> Key {
        key(self.mbid.as_deref(), self.artist.as_deref(), &self.title, self.album.as_deref(), &self.path)
    }
}

/// What makes two files one song.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Recording(String),
    Tags { artist: String, title: String, album: String },
    Path(PathBuf),
}

fn key(mbid: Option<&str>, artist: Option<&str>, title: &str, album: Option<&str>, path: &Path) -> Key {
    let norm = |s: &str| s.trim().to_lowercase();
    if let Some(id) = mbid.map(norm).filter(|id| !id.is_empty()) {
        return Key::Recording(id);
    }
    match artist.map(norm).filter(|a| !a.is_empty()) {
        Some(artist) => Key::Tags { artist, title: norm(title), album: album.map(norm).unwrap_or_default() },
        None => Key::Path(path.to_path_buf()),
    }
}

fn key_of(track: &Track) -> Key {
    key(track.recording_id.as_deref(), track.artist().as_deref(), &track.title, track.album.as_deref(), &track.path)
}

/// A song's plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub count: u32,
    pub last: Option<i64>,
}

impl Stats {
    fn add(&mut self, at: i64) {
        self.count += 1;
        self.last = self.last.max(Some(at));
    }
}

pub struct Plays {
    /// The log; `None` keeps plays in memory only (tests and headless runs).
    path: Option<PathBuf>,
    tally: HashMap<Key, Stats>,
    /// Every play in the order it was recorded, for time windows.
    events: Vec<(i64, Key)>,
}

impl Plays {
    /// Plays that live only as long as this value.
    pub fn memory() -> Plays {
        Plays { path: None, tally: HashMap::new(), events: Vec::new() }
    }

    /// Reads the log at `path`, which need not exist yet. Lines that can't be read are skipped
    /// and reported in the error, which comes with the plays that could be read.
    pub fn open(path: PathBuf) -> (Plays, Option<String>) {
        let mut plays = Plays { path: Some(path.clone()), tally: HashMap::new(), events: Vec::new() };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (plays, None),
            Err(e) => return (plays, Some(format!("Couldn't read the play counts in {}: {e}", path.display()))),
        };
        let mut bad = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Entry>(line) {
                Ok(entry) => plays.count(entry),
                Err(_) => bad += 1,
            }
        }
        let error = (bad > 0).then(|| format!("Skipped {bad} unreadable line{} in {}", if bad == 1 { "" } else { "s" }, path.display()));
        (plays, error)
    }

    /// Tallies one play and keeps its event.
    fn count(&mut self, entry: Entry) {
        let key = entry.key();
        self.tally.entry(key.clone()).or_default().add(entry.at);
        self.events.push((entry.at, key));
    }

    /// Every play that counts in `tracks` at or after `since`, as the index of the first track
    /// that is the played song and the play's time. Each play counts once, even when the song
    /// has several copies in the library.
    pub fn plays_since(&self, tracks: &[Track], since: i64) -> Vec<(usize, i64)> {
        let mut first: HashMap<Key, usize> = HashMap::new();
        for (i, t) in tracks.iter().enumerate() {
            first.entry(key_of(t)).or_insert(i);
        }
        self.events.iter().filter(|(at, _)| *at >= since).filter_map(|(at, key)| first.get(key).map(|&i| (i, *at))).collect()
    }

    pub fn stats(&self, track: &Track) -> Stats {
        self.tally.get(&key_of(track)).copied().unwrap_or_default()
    }

    /// Fills every track's play count and last-played date.
    pub fn apply(&self, tracks: &mut [Track]) {
        for t in tracks {
            let s = self.stats(t);
            t.plays = s.count;
            t.last_played = s.last;
        }
    }

    /// Counts a play of `track` and appends it to the log. The play counts in memory even when
    /// the log can't be written.
    pub fn record(&mut self, track: &Track, at: i64) -> Result<(), String> {
        let entry = Entry::of(track, at);
        self.count(entry.clone());
        let Some(path) = &self.path else { return Ok(()) };
        append(path, &entry).map_err(|e| format!("Couldn't save the play of \u{201c}{}\u{201d} to {}: {e}", track.title, path.display()))
    }
}

fn append(path: &Path, entry: &Entry) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    // One write per line, so a crash leaves at most a torn last line, which loading skips.
    file.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str, artist: Option<&str>, title: &str, album: Option<&str>, mbid: Option<&str>) -> Track {
        Track {
            path: path.into(),
            credit: artist.map(Into::into),
            title: title.into(),
            album: album.map(Into::into),
            recording_id: mbid.map(Into::into),
            ..Track::default()
        }
    }

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let dir = std::env::temp_dir().join(format!("peri-plays-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn plays_are_saved_and_tallied_at_the_next_launch() {
        let dir = Dir::new("saved");
        let path = dir.0.join(FILE);
        let joga = track("/m/joga.flac", Some("Björk"), "Jóga", Some("Homogenic"), None);
        let (mut plays, error) = Plays::open(path.clone());
        assert_eq!(error, None, "no log yet is no error");
        plays.record(&joga, 100).unwrap();
        plays.record(&joga, 300).unwrap();
        plays.record(&joga, 200).unwrap();
        let (again, error) = Plays::open(path);
        assert_eq!(error, None);
        assert_eq!(again.stats(&joga), Stats { count: 3, last: Some(300) });
    }

    #[test]
    fn copies_of_a_song_share_its_plays() {
        let mut plays = Plays::memory();
        let flac = track("/m/a/joga.flac", Some("Björk"), "Jóga", Some("Homogenic"), None);
        let mp3 = track("/old/joga.mp3", Some("björk "), "JÓGA", Some("homogenic"), None);
        plays.record(&flac, 1).unwrap();
        assert_eq!(plays.stats(&mp3).count, 1, "same artist, title and album ignoring case");
        let live = track("/m/b/joga.flac", Some("Björk"), "Jóga", Some("Live"), None);
        assert_eq!(plays.stats(&live).count, 0, "another album is another song");
        let tagged = track("/x.flac", Some("Bjork"), "Joga (remaster)", None, Some("MBID-1"));
        let other = track("/y.flac", Some("Björk"), "Jóga", None, Some("mbid-1"));
        plays.record(&tagged, 2).unwrap();
        assert_eq!(plays.stats(&other), Stats { count: 1, last: Some(2) }, "the recording ID decides first");
    }

    #[test]
    fn untagged_files_are_known_by_path() {
        let mut plays = Plays::memory();
        let a = track("/m/one.wav", None, "Untitled", None, None);
        let b = track("/m/two.wav", None, "Untitled", None, None);
        plays.record(&a, 1).unwrap();
        assert_eq!((plays.stats(&a).count, plays.stats(&b).count), (1, 0));
    }

    #[test]
    fn a_play_counts_once_in_a_window_and_goes_to_the_first_copy() {
        let mut plays = Plays::memory();
        let flac = track("/m/a/joga.flac", Some("Björk"), "Jóga", Some("Homogenic"), None);
        let mp3 = track("/old/joga.mp3", Some("björk "), "JÓGA", Some("homogenic"), None);
        let other = track("/m/other.flac", Some("Björk"), "Army", None, None);
        let library = [other.clone(), flac.clone(), mp3.clone()];
        plays.record(&mp3, 10).unwrap();
        plays.record(&flac, 20).unwrap();
        assert_eq!(plays.plays_since(&library, 0), vec![(1, 10), (1, 20)], "the first copy in library order takes both");
        assert_eq!(plays.plays_since(&library, 15), vec![(1, 20)], "only plays at or after the window's start");
    }

    #[test]
    fn events_follow_the_tally_and_unknown_songs_are_left_out() {
        let mut plays = Plays::memory();
        let joga = track("/m/joga.flac", Some("Björk"), "Jóga", None, None);
        let gone = track("/m/gone.flac", Some("Nobody"), "Missing", None, None);
        plays.record(&joga, 1).unwrap();
        plays.record(&joga, 2).unwrap();
        plays.record(&gone, 3).unwrap();
        assert_eq!(plays.plays_since(&[joga.clone(), gone.clone()], 0).len(), 3, "every play is kept");
        assert_eq!(plays.plays_since(std::slice::from_ref(&joga), 0), vec![(0, 1), (0, 2)], "the missing song isn't in the library");
        assert_eq!(plays.stats(&joga).count, 2, "the tally agrees");
    }

    #[test]
    fn the_log_gives_its_events_back_at_the_next_launch() {
        let dir = Dir::new("events");
        let path = dir.0.join(FILE);
        let joga = track("/m/joga.flac", Some("Björk"), "Jóga", None, None);
        let (mut plays, _) = Plays::open(path.clone());
        plays.record(&joga, 100).unwrap();
        plays.record(&joga, 300).unwrap();
        let (again, _) = Plays::open(path);
        assert_eq!(again.plays_since(std::slice::from_ref(&joga), 200), vec![(0, 300)]);
    }

    #[test]
    fn unreadable_lines_are_reported_and_the_rest_kept() {
        let dir = Dir::new("torn");
        std::fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join(FILE);
        let joga = track("/m/joga.flac", Some("Björk"), "Jóga", None, None);
        let good = serde_json::to_string(&Entry::of(&joga, 5)).unwrap();
        std::fs::write(&path, format!("{good}\n{{\"at\": 6, \"pa\n")).unwrap();
        let (plays, error) = Plays::open(path);
        assert_eq!(plays.stats(&joga).count, 1);
        assert!(error.is_some_and(|e| e.contains("1 unreadable line")));
    }
}

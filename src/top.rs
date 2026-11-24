//! What the library has been playing: the top artists, albums and songs over a window of time.
//! All time reads the tallies; a window counts the play events since its start.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::album::Album;
use crate::library::Track;
use crate::plays::Plays;
use crate::relations::Relations;

/// How many rows each leaderboard shows.
pub const TOP_COUNT: usize = 8;

/// The play windows the leaderboards offer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Window {
    Days7,
    Days30,
    #[default]
    All,
}

impl Serialize for Window {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for Window {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Window, D::Error> {
        let id = String::deserialize(d)?;
        Window::ALL
            .into_iter()
            .find(|w| w.id() == id)
            .ok_or_else(|| serde::de::Error::custom(format!("no window \"{id}\"")))
    }
}

impl Window {
    pub const ALL: [Window; 3] = [Window::Days7, Window::Days30, Window::All];

    /// The value saved in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Window::Days7 => "7",
            Window::Days30 => "30",
            Window::All => "all",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Window::Days7 => "7 days",
            Window::Days30 => "30 days",
            Window::All => "All time",
        }
    }

    /// The first second counted, given `now` in seconds since the epoch; `None` for all time.
    pub fn since(self, now: i64) -> Option<i64> {
        match self {
            Window::Days7 => Some(now - 7 * DAY),
            Window::Days30 => Some(now - 30 * DAY),
            Window::All => None,
        }
    }
}

const DAY: i64 = 86_400;

/// One ranked row: its name (or title) and its plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ranked<T> {
    pub item: T,
    pub plays: u32,
}

/// Sorts by plays, most first; ties keep the order they were first seen in.
fn rank<T: Clone + Eq + std::hash::Hash>(counts: impl IntoIterator<Item = T>) -> Vec<Ranked<T>> {
    let mut order: Vec<T> = Vec::new();
    let mut plays: HashMap<T, u32> = HashMap::new();
    for item in counts {
        let n = plays.entry(item.clone()).or_insert_with(|| {
            order.push(item.clone());
            0
        });
        *n += 1;
    }
    let mut out: Vec<Ranked<T>> = order
        .into_iter()
        .map(|item| Ranked {
            plays: plays[&item],
            item,
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.plays));
    out.truncate(TOP_COUNT);
    out
}

/// The play counts of each song in the window: all time reads the tallies, a window reads the
/// events since its start. Returns (library song, plays) for every song with plays.
fn song_plays(tracks: &[Track], plays: &Plays, window: Window, now: i64) -> Vec<(usize, u32)> {
    match window.since(now) {
        None => tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.plays > 0)
            .map(|(i, t)| (i, t.plays))
            .collect(),
        Some(since) => {
            let mut counts: HashMap<usize, u32> = HashMap::new();
            for (i, _) in plays.plays_since(tracks, since) {
                *counts.entry(i).or_default() += 1;
            }
            counts.into_iter().collect()
        }
    }
}

/// The songs played most in the window: library song indices with their plays.
pub fn songs(tracks: &[Track], plays: &Plays, window: Window, now: i64) -> Vec<Ranked<usize>> {
    let mut counts = song_plays(tracks, plays, window, now);
    counts.sort_by_key(|&(i, n)| (std::cmp::Reverse(n), i));
    counts
        .into_iter()
        .take(TOP_COUNT)
        .map(|(item, plays)| Ranked { item, plays })
        .collect()
}

/// The artists played most in the window. Each song's plays count for every main artist of the
/// song, with merged names counted as one artist.
pub fn artists(
    tracks: &[Track],
    plays: &Plays,
    relations: &Relations,
    window: Window,
    now: i64,
) -> Vec<Ranked<String>> {
    let mut names: Vec<String> = Vec::new();
    for (i, n) in song_plays(tracks, plays, window, now) {
        let track = &tracks[i];
        for artist in track.artists.iter().filter(|a| !track.featured.contains(a)) {
            // Merged names count as one artist, under the first of their names in sorted order.
            let name = relations
                .merged(artist)
                .into_iter()
                .min_by_key(|n| n.to_lowercase())
                .unwrap_or_else(|| artist.clone());
            for _ in 0..n {
                names.push(name.clone());
            }
        }
    }
    rank(names)
}

/// The albums played most in the window: album indices with their plays, from the songs.
pub fn albums(
    tracks: &[Track],
    albums: &[Album],
    plays: &Plays,
    window: Window,
    now: i64,
) -> Vec<Ranked<usize>> {
    let mut album_of: HashMap<usize, usize> = HashMap::new();
    for (a, album) in albums.iter().enumerate() {
        for &t in &album.tracks {
            album_of.insert(t, a);
        }
    }
    let mut counts: HashMap<usize, u32> = HashMap::new();
    for (i, n) in song_plays(tracks, plays, window, now) {
        if let Some(&a) = album_of.get(&i) {
            *counts.entry(a).or_default() += n;
        }
    }
    let mut out: Vec<Ranked<usize>> = counts
        .into_iter()
        .map(|(item, plays)| Ranked { item, plays })
        .collect();
    out.sort_by_key(|r| (std::cmp::Reverse(r.plays), r.item));
    out.truncate(TOP_COUNT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artists: &[&str], featured: &[&str], album: &str) -> Track {
        Track {
            path: format!("/m/{title}.flac").into(),
            title: title.into(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            featured: featured.iter().map(|a| a.to_string()).collect(),
            album: Some(album.into()),
            album_artist: artists.first().map(|a| a.to_string()),
            ..Track::default()
        }
    }

    const NOW: i64 = 1_800_000_000;

    /// Two songs by Björk on one album, one by Aphex on another, and some plays of each.
    fn library() -> (Vec<Track>, Plays) {
        let tracks = vec![
            track("Jóga", &["Björk"], &[], "Homogenic"),
            track("Army", &["Björk"], &[], "Homogenic"),
            track("Lichen", &["Aphex Twin"], &[], "Drukqs"),
        ];
        let mut plays = Plays::memory();
        // Jóga: 2 recent, 1 old. Lichen: 1 old. Army: none.
        plays.record(&tracks[0], NOW - DAY).unwrap();
        plays.record(&tracks[0], NOW - 2 * DAY).unwrap();
        plays.record(&tracks[0], NOW - 100 * DAY).unwrap();
        plays.record(&tracks[2], NOW - 100 * DAY).unwrap();
        let mut tracks = tracks;
        plays.apply(&mut tracks);
        (tracks, plays)
    }

    #[test]
    fn all_time_counts_every_play_and_a_window_only_the_recent_ones() {
        let (tracks, plays) = library();
        let all = songs(&tracks, &plays, Window::All, NOW);
        assert_eq!(
            all.iter()
                .map(|r| (tracks[r.item].title.as_str(), r.plays))
                .collect::<Vec<_>>(),
            [("Jóga", 3), ("Lichen", 1)]
        );
        let week = songs(&tracks, &plays, Window::Days7, NOW);
        assert_eq!(
            week.iter()
                .map(|r| (tracks[r.item].title.as_str(), r.plays))
                .collect::<Vec<_>>(),
            [("Jóga", 2)]
        );
    }

    #[test]
    fn an_empty_window_has_no_rows() {
        let (tracks, plays) = library();
        assert!(songs(&tracks, &plays, Window::Days7, NOW + 1000 * DAY).is_empty());
        assert!(
            artists(
                &tracks,
                &plays,
                &Relations::default(),
                Window::Days7,
                NOW + 1000 * DAY
            )
            .is_empty()
        );
        assert!(songs(&[], &Plays::memory(), Window::All, NOW).is_empty());
    }

    #[test]
    fn artists_count_each_main_artist_of_a_song_and_not_featured_ones() {
        let mut tracks = vec![track(
            "Duet",
            &["Armand Hammer", "Guest"],
            &["Guest"],
            "Haram",
        )];
        let mut plays = Plays::memory();
        plays.record(&tracks[0], NOW).unwrap();
        plays.record(&tracks[0], NOW).unwrap();
        plays.apply(&mut tracks);
        let ranked = artists(&tracks, &plays, &Relations::default(), Window::All, NOW);
        assert_eq!(
            ranked,
            vec![Ranked {
                item: "Armand Hammer".to_string(),
                plays: 2
            }]
        );
    }

    #[test]
    fn merged_artist_names_share_their_plays() {
        let mut tracks = vec![
            track("Ryan", &["Earl Sweatshirt"], &[], "I"),
            track("Thebe", &["Thebe Kgositsile"], &[], "II"),
        ];
        let mut plays = Plays::memory();
        plays.record(&tracks[0], NOW).unwrap();
        plays.record(&tracks[1], NOW).unwrap();
        plays.apply(&mut tracks);
        let relations: Relations =
            serde_json::from_str(r#"{"merges": [["Earl Sweatshirt", "Thebe Kgositsile"]]}"#)
                .unwrap();
        let ranked = artists(&tracks, &plays, &relations, Window::All, NOW);
        assert_eq!(ranked.len(), 1, "one artist");
        assert_eq!(ranked[0].plays, 2);
    }

    #[test]
    fn albums_add_up_their_songs_plays() {
        let (tracks, plays) = library();
        let gathered = crate::album::build(&tracks);
        let ranked = albums(&tracks, &gathered, &plays, Window::All, NOW);
        assert_eq!(
            ranked
                .iter()
                .map(|r| (gathered[r.item].title.as_str(), r.plays))
                .collect::<Vec<_>>(),
            [("Homogenic", 3), ("Drukqs", 1)]
        );
    }

    #[test]
    fn ties_keep_the_order_they_were_first_seen() {
        let tracks = vec![track("A", &["X"], &[], "1"), track("B", &["Y"], &[], "2")];
        let mut plays = Plays::memory();
        plays.record(&tracks[1], NOW).unwrap();
        plays.record(&tracks[0], NOW).unwrap();
        let mut tracks = tracks;
        plays.apply(&mut tracks);
        let ranked = songs(&tracks, &plays, Window::All, NOW);
        assert_eq!(
            ranked.iter().map(|r| r.item).collect::<Vec<_>>(),
            [0, 1],
            "same count: library order"
        );
    }

    #[test]
    fn windows_have_their_labels_and_ids() {
        assert_eq!(Window::default(), Window::All);
        assert_eq!(Window::ALL.map(Window::id), ["7", "30", "all"]);
        assert_eq!(Window::Days30.since(NOW), Some(NOW - 30 * DAY));
        assert_eq!(Window::All.since(NOW), None);
    }
}

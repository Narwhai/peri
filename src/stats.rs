//! The library's totals for the home page's stats strip: how many songs, albums and artists there
//! are, how long they run, and how much of the library has been played.

use std::collections::HashSet;

use crate::album::Album;
use crate::library::Track;

/// The library's totals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub songs: usize,
    /// Albums with a title; songs without an album tag don't count.
    pub albums: usize,
    /// Distinct main artists and album artists, ignoring case.
    pub artists: usize,
    /// Seconds of the songs whose length is known.
    pub seconds: f64,
    /// Titled albums with at least one play.
    pub played_albums: usize,
}

impl Totals {
    /// The totals of `tracks`, gathered into `albums` (see [`crate::album::build`]).
    pub fn of(tracks: &[Track], albums: &[Album]) -> Totals {
        let mut artists: HashSet<String> = HashSet::new();
        for t in tracks {
            artists.extend(
                t.artists
                    .iter()
                    .filter(|a| !t.featured.contains(a))
                    .map(|a| a.to_lowercase()),
            );
        }
        let titled: Vec<&Album> = albums.iter().filter(|a| !a.unknown).collect();
        for a in &titled {
            artists.extend(a.album_artists.iter().map(|a| a.to_lowercase()));
        }
        Totals {
            songs: tracks.len(),
            albums: titled.len(),
            artists: artists.len(),
            seconds: tracks.iter().filter_map(|t| t.duration).sum(),
            played_albums: titled.iter().filter(|a| a.plays > 0).count(),
        }
    }

    /// The share of titled albums with a play; `None` when there are no titled albums.
    pub fn played_share(&self) -> Option<f64> {
        (self.albums > 0).then(|| self.played_albums as f64 / self.albums as f64)
    }

    /// The total length in hours.
    pub fn hours(&self) -> f64 {
        self.seconds / 3600.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        title: &str,
        artists: &[&str],
        featured: &[&str],
        album: Option<&str>,
        duration: Option<f64>,
    ) -> Track {
        Track {
            path: format!("/m/{title}.flac").into(),
            title: title.into(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            featured: featured.iter().map(|a| a.to_string()).collect(),
            album: album.map(Into::into),
            album_artist: artists.first().map(|a| a.to_string()),
            duration,
            ..Track::default()
        }
    }

    fn totals(tracks: &[Track]) -> Totals {
        Totals::of(tracks, &crate::album::build(tracks))
    }

    #[test]
    fn counts_songs_albums_and_distinct_artists_ignoring_case() {
        let tracks = [
            track("One", &["Björk"], &[], Some("Homogenic"), Some(60.0)),
            track("Two", &["björk"], &[], Some("Homogenic"), Some(60.0)),
            track("Three", &["Aphex Twin"], &[], Some("Drukqs"), Some(60.0)),
        ];
        let t = totals(&tracks);
        assert_eq!((t.songs, t.albums, t.artists), (3, 2, 2));
    }

    #[test]
    fn featured_artists_are_not_counted_as_artists() {
        let tracks = [track(
            "Duet",
            &["Armand Hammer", "Guest"],
            &["Guest"],
            Some("Haram"),
            None,
        )];
        assert_eq!(
            totals(&tracks).artists,
            1,
            "the album artist is the main artist, not the guest"
        );
        let guest_only = [track("Guest spot", &["Guest"], &["Guest"], None, None)];
        assert_eq!(
            totals(&guest_only).artists,
            0,
            "a featured name alone isn't an artist"
        );
    }

    #[test]
    fn songs_without_an_album_tag_are_not_albums() {
        let tracks = [track("Loose", &["Solo"], &[], None, None)];
        let t = totals(&tracks);
        assert_eq!((t.songs, t.albums), (1, 0));
        assert_eq!(t.played_share(), None, "no titled albums, no share");
    }

    #[test]
    fn hours_add_up_the_known_lengths_only() {
        let tracks = [
            track("A", &["X"], &[], Some("Y"), Some(1800.0)),
            track("B", &["X"], &[], Some("Y"), Some(1800.0)),
            track("C", &["X"], &[], Some("Y"), None),
        ];
        assert_eq!(totals(&tracks).hours(), 1.0);
    }

    #[test]
    fn the_played_share_counts_albums_with_a_play() {
        let mut tracks = [
            track("A", &["X"], &[], Some("Played"), None),
            track("B", &["X"], &[], Some("Unplayed"), None),
        ];
        tracks[0].plays = 1;
        let t = totals(&tracks);
        assert_eq!((t.played_albums, t.albums), (1, 2));
        assert_eq!(t.played_share(), Some(0.5));
    }
}

//! What a collaboration page lists: the releases and songs two or more artists made together.
//! Names match ignoring case, and each artist's merged names (see [`crate::relations`]) count
//! as theirs.

use crate::album::Album;
use crate::library::Track;
use crate::relations::Relations;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollabPage {
    /// The artists, as the page was opened with them.
    pub names: Vec<String>,
    /// Albums whose album artists include every one of them, newest first.
    pub albums: Vec<usize>,
    /// Songs naming every one of them as an artist that aren't on `albums`, in library order.
    pub appears_on: Vec<usize>,
}

impl CollabPage {
    pub fn is_empty(&self) -> bool {
        self.albums.is_empty() && self.appears_on.is_empty()
    }

    /// "A & B", or "A, B & C".
    pub fn title(&self) -> String {
        title(&self.names)
    }

    /// What Play plays: their releases newest first (each in track order), then the songs
    /// they're on together elsewhere.
    pub fn play_list(&self, albums: &[Album]) -> Vec<usize> {
        self.albums.iter().flat_map(|&a| albums[a].tracks.iter().copied()).chain(self.appears_on.iter().copied()).collect()
    }

    /// What Play Discography plays: their releases oldest first, undated last.
    pub fn discography(&self, albums: &[Album]) -> Vec<usize> {
        let mut own = self.albums.clone();
        own.sort_by_key(|&a| (albums[a].year.is_none(), albums[a].year));
        own.iter().flat_map(|&a| albums[a].tracks.iter().copied()).collect()
    }
}

/// "A & B", or "A, B & C".
pub fn title(names: &[String]) -> String {
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} & {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// Whether every artist (each a list of their merged names) is among `credited`.
fn all_in(artists: &[Vec<String>], credited: &[String]) -> bool {
    artists.iter().all(|names| credited.iter().any(|c| names.iter().any(|n| c.trim().eq_ignore_ascii_case(n))))
}

/// Gathers what `names` released or recorded together.
pub fn page(names: &[String], tracks: &[Track], albums: &[Album], relations: &Relations) -> CollabPage {
    let artists: Vec<Vec<String>> = names.iter().map(|n| relations.merged(n)).collect();
    let mut own: Vec<usize> = (0..albums.len()).filter(|&a| all_in(&artists, &albums[a].album_artists)).collect();
    own.sort_by_key(|&a| (albums[a].year.is_none(), std::cmp::Reverse(albums[a].year)));
    let appears_on = (0..tracks.len()).filter(|&i| all_in(&artists, &tracks[i].artists) && !own.iter().any(|&a| albums[a].tracks.contains(&i))).collect();
    CollabPage { names: names.to_vec(), albums: own, appears_on }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credits::resolve;
    use crate::library::TaggedArtists;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn song(title: &str, album: &str, album_artist: &str, artists: &[&str], year: u32) -> Track {
        Track {
            path: format!("/m/{album}/{title}.flac").into(),
            title: title.into(),
            album: Some(album.into()),
            credit: Some(artists.join(" & ")),
            album_artist: Some(album_artist.into()),
            tagged: TaggedArtists { artists: strings(artists), album_artists: strings(&album_artist.split(" & ").collect::<Vec<_>>()), ..TaggedArtists::default() },
            year: Some(year),
            ..Track::default()
        }
    }

    fn library() -> (Vec<Track>, Vec<Album>) {
        let mut tracks = vec![
            song("Laraaji", "Mercy", "Armand Hammer & The Alchemist", &["Armand Hammer", "The Alchemist"], 2025),
            song("Old One", "Haram", "Armand Hammer & The Alchemist", &["Armand Hammer", "The Alchemist"], 2021),
            song("Solo", "Ruins", "Armand Hammer", &["Armand Hammer"], 2010),
            song("Guest Spot", "Ruins", "Armand Hammer", &["Armand Hammer", "The Alchemist"], 2010),
            song("Producer", "Rare Chandeliers", "The Alchemist", &["The Alchemist"], 2012),
        ];
        resolve(&mut tracks);
        let albums = crate::album::build(&tracks);
        (tracks, albums)
    }

    #[test]
    fn a_collab_lists_only_what_every_artist_is_on() {
        let (tracks, albums) = library();
        let p = page(&strings(&["armand hammer", "The Alchemist"]), &tracks, &albums, &Relations::default());
        let titles: Vec<&str> = p.albums.iter().map(|&a| albums[a].title.as_str()).collect();
        assert_eq!(titles, ["Mercy", "Haram"], "their joint albums, newest first; neither solo album");
        let songs: Vec<&str> = p.appears_on.iter().map(|&i| tracks[i].title.as_str()).collect();
        assert_eq!(songs, ["Guest Spot"], "a song with both on one artist's album");
        let played: Vec<&str> = p.play_list(&albums).iter().map(|&i| tracks[i].title.as_str()).collect();
        assert_eq!(played, ["Laraaji", "Old One", "Guest Spot"]);
        let disco: Vec<&str> = p.discography(&albums).iter().map(|&i| tracks[i].title.as_str()).collect();
        assert_eq!(disco, ["Old One", "Laraaji"]);
        assert_eq!(p.title(), "armand hammer & The Alchemist");
    }

    #[test]
    fn merged_names_count_and_an_artist_with_nothing_together_empties_the_page() {
        let (tracks, albums) = library();
        let relations = Relations { merges: vec![strings(&["Alan Maman", "The Alchemist"])], links: Vec::new() };
        let p = page(&strings(&["Armand Hammer", "Alan Maman"]), &tracks, &albums, &relations);
        assert_eq!(p.albums.len(), 2, "Alan Maman is The Alchemist");
        assert!(page(&strings(&["Armand Hammer", "Nobody"]), &tracks, &albums, &Relations::default()).is_empty());
        assert_eq!(page(&strings(&["A", "B", "C"]), &tracks, &albums, &Relations::default()).title(), "A, B & C");
    }
}

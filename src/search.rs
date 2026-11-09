//! What the search page lists for a query: the best matching artist's page (their albums,
//! songs, the songs they appear on and their credits), the other matching artists, then the
//! albums and songs that match but aren't that artist's.

use std::collections::HashMap;

use crate::album::Album;
use crate::artist::{self, ArtistPage};
use crate::library::Track;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchResults {
    /// The best matching artist: a name equal to the query, else the one with the most songs.
    pub artist: Option<ArtistPage>,
    /// The other artists whose names match, best first.
    pub artists: Vec<String>,
    /// Albums whose title or album artists match, not already among `artist`'s albums, newest
    /// first (undated last).
    pub albums: Vec<usize>,
    /// Songs whose title, artists or album match, not already listed for `artist` or on one of
    /// `albums`, in library order.
    pub songs: Vec<usize>,
}

impl SearchResults {
    pub fn is_empty(&self) -> bool {
        self.artist.is_none() && self.artists.is_empty() && self.albums.is_empty() && self.songs.is_empty()
    }
}

fn has_words(haystack: &str, query: &str) -> bool {
    let haystack = haystack.to_lowercase();
    query.split_whitespace().all(|word| haystack.contains(word))
}

/// Searches the library for `query` (any case); empty when the query is blank.
pub fn search(query: &str, tracks: &[Track], albums: &[Album]) -> SearchResults {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return SearchResults::default();
    }
    // Every name an artist, album artist or credit carries, by lowercase name: how many songs
    // name them, and the first spelling seen (the artist page settles the usual one).
    let mut names: HashMap<String, (String, usize)> = HashMap::new();
    for t in tracks {
        let mut seen: Vec<String> = Vec::new();
        for n in t.artists.iter().chain(&t.album_artists).chain(t.credits.iter().map(|c| &c.name)) {
            let key = n.trim().to_lowercase();
            if key.is_empty() || seen.contains(&key) || !has_words(&key, &query) {
                continue;
            }
            names.entry(key.clone()).or_insert_with(|| (n.trim().to_string(), 0)).1 += 1;
            seen.push(key);
        }
    }
    let mut ranked: Vec<(String, (String, usize))> = names.into_iter().collect();
    ranked.sort_by(|(ka, (_, ca)), (kb, (_, cb))| (*kb == query).cmp(&(*ka == query)).then(cb.cmp(ca)).then(ka.cmp(kb)));
    let mut ranked = ranked.into_iter().map(|(_, (name, _))| name);
    let artist = ranked.next().map(|name| artist::page(&name, tracks, albums));
    let artists: Vec<String> = ranked.collect();

    let empty = ArtistPage::default();
    let top = artist.as_ref().unwrap_or(&empty);
    let mut found: Vec<usize> = (0..albums.len()).filter(|&a| !top.albums.contains(&a) && albums[a].primary_match(&query)).collect();
    found.sort_by_key(|&a| std::cmp::Reverse(albums[a].year));
    let listed = |i: &usize| {
        top.songs.contains(i)
            || top.appears_on.contains(i)
            || top.credits.iter().any(|(c, _)| c == i)
            || top.albums.iter().chain(&found).any(|&a| albums[a].tracks.contains(i))
    };
    let songs = (0..tracks.len()).filter(|i| tracks[*i].matches(&query) && !listed(i)).collect();
    SearchResults { artist, artists, albums: found, songs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credits::{Credit, Role, resolve};
    use crate::library::TaggedArtists;

    fn song(title: &str, album: &str, album_artist: &str, artists: &[&str], credits: Vec<Credit>) -> Track {
        Track {
            path: format!("/m/{album}/{title}.flac").into(),
            title: title.into(),
            album: Some(album.into()),
            album_artist: Some(album_artist.into()),
            tagged: TaggedArtists { artists: artists.iter().map(|s| s.to_string()).collect(), ..TaggedArtists::default() },
            credits,
            ..Track::default()
        }
    }

    fn library() -> (Vec<Track>, Vec<Album>) {
        let mut tracks = vec![
            song("Lucky", "Essentials", "Erika de Casier", &["Erika de Casier"], vec![]),
            song("Drama", "Still", "Erika de Casier", &["Erika de Casier"], vec![]),
            song("Speak Daggers", "Speak Daggers", "Elias Rønnenfelt", &["Elias Rønnenfelt", "Erika de Casier"], vec![]),
            song("Alone", "Speak Daggers", "Elias Rønnenfelt", &["Elias Rønnenfelt"], vec![]),
            song("Written", "Elsewhere", "Somebody", &["Somebody"], vec![Credit::parse(Role::Writer, "Erika de Casier")]),
            song("Erika's Song", "Elsewhere", "Somebody", &["Somebody"], vec![]),
            song("Hi", "Hello", "Erika Two", &["Erika Two"], vec![]),
        ];
        resolve(&mut tracks);
        let albums = crate::album::build(&tracks);
        (tracks, albums)
    }

    fn titles(tracks: &[Track], list: &[usize]) -> Vec<String> {
        list.iter().map(|&i| tracks[i].title.clone()).collect()
    }

    #[test]
    fn the_matching_artist_leads_with_their_albums_songs_appearances_and_credits() {
        let (tracks, albums) = library();
        let r = search("erika de casier", &tracks, &albums);
        let top = r.artist.expect("an artist matches");
        assert_eq!(top.name, "Erika de Casier");
        let mut own: Vec<&str> = top.albums.iter().map(|&a| albums[a].title.as_str()).collect();
        own.sort_unstable();
        assert_eq!(own, ["Essentials", "Still"], "her albums, not Speak Daggers");
        assert_eq!(titles(&tracks, &top.appears_on), ["Speak Daggers"], "only the song she's on, not the whole album");
        assert_eq!(top.credits.len(), 1);
        assert!(r.artists.is_empty());
        assert!(r.albums.is_empty(), "Speak Daggers isn't her album");
        assert!(r.songs.is_empty(), "everything is already under her");
    }

    #[test]
    fn other_matching_artists_albums_and_songs_follow() {
        let (tracks, albums) = library();
        let r = search("Erika", &tracks, &albums);
        assert_eq!(r.artist.map(|a| a.name), Some("Erika de Casier".to_string()), "the artist with the most songs leads");
        assert_eq!(r.artists, ["Erika Two"]);
        assert_eq!(r.albums.iter().map(|&a| albums[a].title.as_str()).collect::<Vec<_>>(), ["Hello"]);
        assert_eq!(titles(&tracks, &r.songs), ["Erika's Song"]);
    }

    #[test]
    fn albums_list_newest_first() {
        let mut tracks = vec![
            song("a", "Old Sessions", "X", &["X"], vec![]),
            song("b", "Undated Sessions", "Y", &["Y"], vec![]),
            song("c", "New Sessions", "Z", &["Z"], vec![]),
        ];
        tracks[0].year = Some(1999);
        tracks[2].year = Some(2024);
        resolve(&mut tracks);
        let albums = crate::album::build(&tracks);
        let r = search("sessions", &tracks, &albums);
        assert_eq!(r.albums.iter().map(|&a| albums[a].title.as_str()).collect::<Vec<_>>(), ["New Sessions", "Old Sessions", "Undated Sessions"]);
    }

    #[test]
    fn an_exact_name_beats_a_busier_artist() {
        let (tracks, albums) = library();
        assert_eq!(search("erika two", &tracks, &albums).artist.map(|a| a.name), Some("Erika Two".to_string()));
    }

    #[test]
    fn titles_match_without_an_artist() {
        let (tracks, albums) = library();
        let r = search("daggers", &tracks, &albums);
        assert!(r.artist.is_none());
        assert_eq!(r.albums.iter().map(|&a| albums[a].title.as_str()).collect::<Vec<_>>(), ["Speak Daggers"]);
        assert!(r.songs.is_empty(), "its songs are on the album found");
        assert!(search("  ", &tracks, &albums).is_empty());
    }
}

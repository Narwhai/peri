//! What an artist page lists: the artist's own albums, the albums they appear on, their songs
//! (as main or featured artist), and the songs that credit them (composer, producer…).
//! Names match ignoring case.

use crate::album::Album;
use crate::credits::Role;
use crate::library::Track;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistPage {
    /// The name as the library spells it most often.
    pub name: String,
    /// Albums whose album artists include them, newest first.
    pub albums: Vec<usize>,
    /// Other albums holding songs they play on, newest first.
    pub appears_on: Vec<usize>,
    /// Songs they're an artist of, in library order.
    pub songs: Vec<usize>,
    /// Of `songs`, the ones that feature them.
    pub featured_on: Vec<usize>,
    /// Songs crediting them per role, in [`Role::ALL`] order, roles without songs left out.
    pub credits: Vec<(Role, Vec<usize>)>,
}

impl ArtistPage {
    pub fn is_empty(&self) -> bool {
        self.albums.is_empty() && self.appears_on.is_empty() && self.songs.is_empty() && self.credits.is_empty()
    }
}

fn has(names: &[String], name: &str) -> bool {
    names.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Gathers everything the library holds by or crediting `name`.
pub fn page(name: &str, tracks: &[Track], albums: &[Album]) -> ArtistPage {
    let name = name.trim();
    let songs: Vec<usize> = (0..tracks.len()).filter(|&i| has(&tracks[i].artists, name)).collect();
    let featured_on = songs.iter().copied().filter(|&i| has(&tracks[i].featured, name)).collect();
    let mut own: Vec<usize> = (0..albums.len()).filter(|&a| has(&albums[a].album_artists, name)).collect();
    let mut appears_on: Vec<usize> = (0..albums.len()).filter(|&a| !own.contains(&a) && albums[a].tracks.iter().any(|i| songs.contains(i))).collect();
    let newest_first = |list: &mut Vec<usize>| list.sort_by_key(|&a| std::cmp::Reverse(albums[a].year));
    newest_first(&mut own);
    newest_first(&mut appears_on);
    let credits = Role::ALL
        .into_iter()
        .map(|role| (role, (0..tracks.len()).filter(|&i| has(&tracks[i].credited(role), name)).collect::<Vec<usize>>()))
        .filter(|(_, songs)| !songs.is_empty())
        .collect();
    ArtistPage { name: spelling(name, tracks), albums: own, appears_on, songs, featured_on, credits }
}

/// The library's most common spelling of `name` (ignoring case), else `name`.
fn spelling(name: &str, tracks: &[Track]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    let all = tracks.iter().flat_map(|t| t.artists.iter().chain(&t.album_artists).map(String::as_str).chain(t.credits.iter().map(|c| c.name.as_str())));
    for n in all.filter(|n| n.eq_ignore_ascii_case(name)) {
        match counts.iter_mut().find(|(s, _)| *s == n) {
            Some((_, c)) => *c += 1,
            None => counts.push((n, 1)),
        }
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map_or_else(|| name.to_string(), |(s, _)| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credits::{Credit, resolve};
    use crate::library::TaggedArtists;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn song(title: &str, album: &str, credit: &str, album_artist: &str, artists: &[&str], credits: Vec<Credit>, year: u32) -> Track {
        Track {
            path: format!("/m/{album}/{title}.flac").into(),
            title: title.into(),
            album: Some(album.into()),
            credit: Some(credit.into()),
            album_artist: Some(album_artist.into()),
            tagged: TaggedArtists { artists: strings(artists), ..TaggedArtists::default() },
            credits,
            year: Some(year),
            ..Track::default()
        }
    }

    fn library() -> (Vec<Track>, Vec<Album>) {
        let mut tracks = vec![
            song("Calypso Gene", "Mercy", "Armand Hammer & The Alchemist feat. Silka & Cleo Reed", "Armand Hammer & The Alchemist", &["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"], vec![Credit::parse(Role::Producer, "The Alchemist")], 2025),
            song("Laraaji", "Mercy", "Armand Hammer & The Alchemist", "Armand Hammer & The Alchemist", &["Armand Hammer", "The Alchemist"], vec![Credit::parse(Role::Producer, "The Alchemist")], 2025),
            song("Solo", "Haram", "Armand Hammer", "Armand Hammer", &["Armand Hammer"], vec![], 2021),
            song("Wrote It", "Elsewhere", "Somebody", "Somebody", &["Somebody"], vec![Credit::parse(Role::Lyricist, "cleo reed")], 2020),
        ];
        resolve(&mut tracks);
        let albums = crate::album::build(&tracks);
        (tracks, albums)
    }

    fn titles(albums: &[Album], list: &[usize]) -> Vec<String> {
        list.iter().map(|&a| albums[a].title.clone()).collect()
    }

    #[test]
    fn a_collaborative_album_belongs_to_each_of_its_artists() {
        let (tracks, albums) = library();
        let p = page("armand hammer", &tracks, &albums);
        assert_eq!(p.name, "Armand Hammer");
        assert_eq!(titles(&albums, &p.albums), ["Mercy", "Haram"], "own albums, newest first");
        assert!(p.appears_on.is_empty());
        assert_eq!(titles(&albums, &page("The Alchemist", &tracks, &albums).albums), ["Mercy"]);
    }

    #[test]
    fn a_featured_artist_appears_on_the_album_and_has_their_credits() {
        let (tracks, albums) = library();
        let p = page("Cleo Reed", &tracks, &albums);
        assert!(p.albums.is_empty());
        assert_eq!(titles(&albums, &p.appears_on), ["Mercy"]);
        let song_titles: Vec<&str> = p.songs.iter().map(|&i| tracks[i].title.as_str()).collect();
        assert_eq!(song_titles, ["Calypso Gene"]);
        assert_eq!(p.featured_on, p.songs);
        assert_eq!(p.credits.len(), 1);
        assert_eq!((p.credits[0].0, tracks[p.credits[0].1[0]].title.as_str()), (Role::Lyricist, "Wrote It"), "credits match ignoring case");
    }

    #[test]
    fn producers_get_their_credited_songs() {
        let (tracks, albums) = library();
        let p = page("The Alchemist", &tracks, &albums);
        assert_eq!(p.credits.iter().map(|(r, s)| (*r, s.len())).collect::<Vec<_>>(), [(Role::Producer, 2)]);
        assert!(page("Nobody", &tracks, &albums).is_empty());
    }
}

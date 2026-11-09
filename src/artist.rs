//! What an artist page lists: the artist's own albums, the songs they appear on, their songs
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
    /// Their songs on other artists' albums, in library order.
    pub appears_on: Vec<usize>,
    /// Songs they're an artist of, in library order.
    pub songs: Vec<usize>,
    /// Of `songs`, the ones that feature them.
    pub featured_on: Vec<usize>,
    /// Songs crediting them, in library order, each with their roles on it in [`Role::ALL`] order.
    pub credits: Vec<(usize, Vec<Role>)>,
}

impl ArtistPage {
    pub fn is_empty(&self) -> bool {
        self.albums.is_empty() && self.appears_on.is_empty() && self.songs.is_empty() && self.credits.is_empty()
    }

    /// What Play plays: their own releases newest first (each in track order), then the songs
    /// they appear on; else, with neither, the songs crediting them, in library order.
    pub fn play_list(&self, albums: &[Album]) -> Vec<usize> {
        let list: Vec<usize> = self.albums.iter().flat_map(|&a| albums[a].tracks.iter().copied()).chain(self.appears_on.iter().copied()).collect();
        if !list.is_empty() {
            return list;
        }
        self.credited()
    }

    /// The songs crediting them, in library order.
    pub fn credited(&self) -> Vec<usize> {
        self.credits.iter().map(|&(i, _)| i).collect()
    }

    /// What Play Discography plays: their own releases oldest first, undated last, each in
    /// track order.
    pub fn discography(&self, albums: &[Album]) -> Vec<usize> {
        let mut own = self.albums.clone();
        own.sort_by_key(|&a| (albums[a].year.is_none(), albums[a].year));
        own.iter().flat_map(|&a| albums[a].tracks.iter().copied()).collect()
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
    let appears_on = songs.iter().copied().filter(|i| !own.iter().any(|&a| albums[a].tracks.contains(i))).collect();
    own.sort_by_key(|&a| std::cmp::Reverse(albums[a].year));
    let credits = (0..tracks.len())
        .map(|i| (i, Role::ALL.into_iter().filter(|&role| has(&tracks[i].credited(role), name)).collect::<Vec<Role>>()))
        .filter(|(_, roles)| !roles.is_empty())
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
        let song_titles = |list: &[usize]| list.iter().map(|&i| tracks[i].title.clone()).collect::<Vec<String>>();
        assert_eq!(song_titles(&p.appears_on), ["Calypso Gene"], "the song they're on, not all of Mercy");
        assert_eq!(song_titles(&p.songs), ["Calypso Gene"]);
        assert_eq!(p.featured_on, p.songs);
        assert_eq!(p.credits.len(), 1);
        assert_eq!((tracks[p.credits[0].0].title.as_str(), p.credits[0].1.as_slice()), ("Wrote It", [Role::Lyricist].as_slice()), "credits match ignoring case");
    }

    #[test]
    fn play_takes_own_releases_newest_first_then_appearances_and_discography_oldest_first() {
        let (tracks, albums) = library();
        let names = |list: Vec<usize>| list.into_iter().map(|i| tracks[i].title.clone()).collect::<Vec<String>>();
        let p = page("Armand Hammer", &tracks, &albums);
        assert_eq!(names(p.play_list(&albums)), ["Calypso Gene", "Laraaji", "Solo"], "Mercy (2025) in album order, then Haram (2021)");
        assert_eq!(names(p.discography(&albums)), ["Solo", "Calypso Gene", "Laraaji"], "Haram (2021), then Mercy (2025)");
        let guest = page("Cleo Reed", &tracks, &albums);
        assert_eq!(names(guest.play_list(&albums)), ["Calypso Gene"], "with no releases of their own, their appearances");
        assert!(guest.discography(&albums).is_empty());
        let writer = page("Somebody", &tracks, &albums);
        assert_eq!(names(writer.play_list(&albums)), ["Wrote It"]);
    }

    #[test]
    fn producers_get_their_credited_songs() {
        let (tracks, albums) = library();
        let p = page("The Alchemist", &tracks, &albums);
        assert_eq!(p.credits.iter().map(|(i, r)| (tracks[*i].title.as_str(), r.as_slice())).collect::<Vec<_>>(), [("Calypso Gene", [Role::Producer].as_slice()), ("Laraaji", [Role::Producer].as_slice())]);
        assert!(page("Nobody", &tracks, &albums).is_empty());
    }

    #[test]
    fn a_song_with_several_of_their_credits_lists_once_with_each_role() {
        let (mut tracks, _) = library();
        tracks[3].credits.extend([Credit::parse(Role::Producer, "Cleo Reed"), Credit::parse(Role::Composer, "Cleo Reed")]);
        let albums = crate::album::build(&tracks);
        let p = page("Cleo Reed", &tracks, &albums);
        assert_eq!(p.credits.len(), 1, "one entry per song");
        assert_eq!(p.credits[0].1, [Role::Composer, Role::Lyricist, Role::Producer], "roles in credit order");
        assert_eq!(p.credited(), [3]);
    }
}

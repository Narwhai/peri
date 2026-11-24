//! What an artist page lists: the artist's own albums, the songs they appear on, their songs
//! (as main or featured artist), and the songs that credit them (composer, producer…).
//! Names match ignoring case, and an artist merged with other names (see
//! [`crate::relations`]) gathers what each of them is credited with.

use std::collections::HashSet;

use crate::album::Album;
use crate::credits::Role;
use crate::library::Track;
use crate::relations::{Relation, Relations};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtistPage {
    /// Of their names, the one the library credits most prominently, spelled as it does
    /// most often.
    pub name: String,
    /// Every name merged into this page, `name` first; just `name` when there's no merge.
    pub names: Vec<String>,
    /// Artists linked to them, in the order the links list them.
    pub related: Vec<(Relation, String)>,
    /// Whether the lists below also hold what their aliases released and are credited with.
    pub with_aliases: bool,
    /// Every name the lists below match: `names`, and with aliases theirs too.
    pub matched: Vec<String>,
    /// Albums whose album artists include them, newest first.
    pub albums: Vec<usize>,
    /// Their songs on other artists' albums, in library order.
    pub appears_on: Vec<usize>,
    /// Songs they're an artist of, in library order.
    pub songs: Vec<usize>,
    /// Of `songs`, the ones that feature them.
    pub featured_on: Vec<usize>,
    /// Songs on other artists' albums crediting them, in library order, each with their roles
    /// on it in [`Role::ALL`] order.
    pub credits: Vec<(usize, Vec<Role>)>,
    /// Their credits on their own albums: per album of `albums` with any, each role with how
    /// many of its songs credit them with it, in [`Role::ALL`] order.
    pub album_credits: Vec<(usize, Vec<(Role, usize)>)>,
}

impl ArtistPage {
    /// Whether the links name any alias, so the page can include what they released.
    pub fn has_aliases(&self) -> bool {
        self.related.iter().any(|(r, _)| *r == Relation::Alias)
    }

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

    /// Every song in their play list or crediting them, each once.
    pub fn all_songs(&self, albums: &[Album]) -> Vec<usize> {
        distinct(self.play_list(albums).into_iter().chain(self.credited()))
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

fn has(names: &[String], of: &[String]) -> bool {
    names.iter().any(|n| of.iter().any(|o| n.eq_ignore_ascii_case(o)))
}

/// Gathers everything the library holds by or crediting `name`, or any name merged with it;
/// `with_aliases`, also any name they release as.
pub fn page(name: &str, tracks: &[Track], albums: &[Album], relations: &Relations, with_aliases: bool) -> ArtistPage {
    let merged = relations.merged(name);
    let all = if with_aliases { relations.with_aliases(name) } else { merged.clone() };
    let of = all.as_slice();
    let songs: Vec<usize> = (0..tracks.len()).filter(|&i| has(&tracks[i].artists, of)).collect();
    let featured_on = songs.iter().copied().filter(|&i| has(&tracks[i].featured, of)).collect();
    let mut own: Vec<usize> = (0..albums.len()).filter(|&a| has(&albums[a].album_artists, of)).collect();
    let own_tracks: HashSet<usize> = own.iter().flat_map(|&a| albums[a].tracks.iter().copied()).collect();
    let appears_on = songs.iter().copied().filter(|i| !own_tracks.contains(i)).collect();
    own.sort_by_key(|&a| std::cmp::Reverse(albums[a].year));
    let roles = |i: usize| Role::ALL.into_iter().filter(|&role| has(&tracks[i].credited(role), of)).collect::<Vec<Role>>();
    let on_own = |i: usize| own_tracks.contains(&i);
    let credits = (0..tracks.len()).filter(|&i| !on_own(i)).map(|i| (i, roles(i))).filter(|(_, roles)| !roles.is_empty()).collect();
    let album_credits = own
        .iter()
        .map(|&a| {
            let on: Vec<Vec<Role>> = albums[a].tracks.iter().map(|&i| roles(i)).collect();
            let counts = Role::ALL.into_iter().map(|r| (r, on.iter().filter(|rs| rs.contains(&r)).count())).filter(|&(_, n)| n > 0).collect::<Vec<_>>();
            (a, counts)
        })
        .filter(|(_, counts)| !counts.is_empty())
        .collect();
    let names = spellings(&merged, tracks);
    let related = relations.related(&merged);
    // The page's own names as it spells them, then their aliases'.
    let matched = names.iter().cloned().chain(all.iter().filter(|n| !has(&names, std::slice::from_ref(n))).cloned()).collect();
    ArtistPage { name: names[0].clone(), names, related, with_aliases, matched, albums: own, appears_on, songs, featured_on, credits, album_credits }
}

/// The artists involved with `name` (see [`Relations::involved`]), each with their page; those
/// with nothing in the library left out.
pub fn involved(name: &str, tracks: &[Track], albums: &[Album], relations: &Relations, with_aliases: bool) -> Vec<(Relation, ArtistPage)> {
    relations
        .involved(name, with_aliases)
        .into_iter()
        .map(|(relation, other)| (relation, page(&other, tracks, albums, relations, with_aliases)))
        .filter(|(_, other)| !other.is_empty())
        .collect()
}

/// What Play Everything plays: `name`'s play list, then each involved artist's, each song once
/// where it first comes.
pub fn everything(name: &str, tracks: &[Track], albums: &[Album], relations: &Relations, with_aliases: bool) -> Vec<usize> {
    let own = page(name, tracks, albums, relations, with_aliases);
    let others = involved(name, tracks, albums, relations, with_aliases);
    distinct(own.play_list(albums).into_iter().chain(others.iter().flat_map(|(_, other)| other.play_list(albums))))
}

/// `songs` without repeats, each kept where it first comes.
fn distinct(songs: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let mut seen = HashSet::new();
    songs.into_iter().filter(|&i| seen.insert(i)).collect()
}

/// Whether any song names `name` as an artist, album artist or credit, ignoring case.
pub fn known(name: &str, tracks: &[Track]) -> bool {
    let name = name.trim();
    tracks.iter().any(|t| t.artists.iter().chain(&t.album_artists).chain(t.credits.iter().map(|c| &c.name)).any(|n| n.eq_ignore_ascii_case(name)))
}

/// Names the library knows (artists, album artists, credited people) holding every word of
/// `query`, ignoring case: one equal to it first, then by how many songs name them, at most
/// `limit`, none of `exclude`. Each is spelled as first seen.
pub fn suggest(query: &str, tracks: &[Track], exclude: &[String], limit: usize) -> Vec<String> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(String, String, usize)> = Vec::new();
    for t in tracks {
        let mut seen: Vec<String> = Vec::new();
        for n in t.artists.iter().chain(&t.album_artists).chain(t.credits.iter().map(|c| &c.name)) {
            let key = n.trim().to_lowercase();
            if key.is_empty() || seen.contains(&key) || !query.split_whitespace().all(|w| key.contains(w)) || exclude.iter().any(|e| e.eq_ignore_ascii_case(&key)) {
                continue;
            }
            match found.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, c)) => *c += 1,
                None => found.push((key.clone(), n.trim().to_string(), 1)),
            }
            seen.push(key);
        }
    }
    found.sort_by(|(ka, _, ca), (kb, _, cb)| (*kb == query).cmp(&(*ka == query)).then(cb.cmp(ca)).then(ka.cmp(kb)));
    found.into_iter().take(limit).map(|(_, n, _)| n).collect()
}

/// How much one mention of a name counts towards it leading the page: album artist, then
/// artist, featured artist, and any other credit.
const ALBUM_ARTIST: usize = 4;
const MAIN_ARTIST: usize = 3;
const FEATURED: usize = 2;
const CREDIT: usize = 1;

/// `names` as the library spells each most often (as given when it doesn't), the one with the
/// most prominent credits first, ties going to the earlier name.
fn spellings(names: &[String], tracks: &[Track]) -> Vec<String> {
    // Per spelling, its weight; spellings of one name (ignoring case) add up per name.
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for t in tracks {
        let mentions = t
            .album_artists
            .iter()
            .map(|n| (n, ALBUM_ARTIST))
            .chain(t.artists.iter().map(|n| (n, if t.featured.contains(n) { FEATURED } else { MAIN_ARTIST })))
            .chain(t.credits.iter().map(|c| (&c.name, CREDIT)));
        for (n, weight) in mentions.filter(|(n, _)| names.iter().any(|m| n.eq_ignore_ascii_case(m))) {
            match counts.iter_mut().find(|(s, _)| *s == n.as_str()) {
                Some((_, c)) => *c += weight,
                None => counts.push((n.as_str(), weight)),
            }
        }
    }
    let mut out: Vec<(String, usize)> = names
        .iter()
        .map(|name| {
            let of_name = counts.iter().filter(|(s, _)| s.eq_ignore_ascii_case(name));
            let total = of_name.clone().map(|&(_, c)| c).sum();
            let spelling = of_name.max_by_key(|&&(_, c)| c).map_or_else(|| name.clone(), |&(s, _)| s.to_string());
            (spelling, total)
        })
        .collect();
    // Stable, so ties keep the merge's order.
    out.sort_by_key(|&(_, total)| std::cmp::Reverse(total));
    out.into_iter().map(|(s, _)| s).collect()
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
        let p = page("armand hammer", &tracks, &albums, &Relations::default(), false);
        assert_eq!(p.name, "Armand Hammer");
        assert_eq!(titles(&albums, &p.albums), ["Mercy", "Haram"], "own albums, newest first");
        assert!(p.appears_on.is_empty());
        assert_eq!(titles(&albums, &page("The Alchemist", &tracks, &albums, &Relations::default(), false).albums), ["Mercy"]);
    }

    #[test]
    fn a_featured_artist_appears_on_the_album_and_has_their_credits() {
        let (tracks, albums) = library();
        let p = page("Cleo Reed", &tracks, &albums, &Relations::default(), false);
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
        let p = page("Armand Hammer", &tracks, &albums, &Relations::default(), false);
        assert_eq!(names(p.play_list(&albums)), ["Calypso Gene", "Laraaji", "Solo"], "Mercy (2025) in album order, then Haram (2021)");
        assert_eq!(names(p.discography(&albums)), ["Solo", "Calypso Gene", "Laraaji"], "Haram (2021), then Mercy (2025)");
        let guest = page("Cleo Reed", &tracks, &albums, &Relations::default(), false);
        assert_eq!(names(guest.play_list(&albums)), ["Calypso Gene"], "with no releases of their own, their appearances");
        assert!(guest.discography(&albums).is_empty());
        let writer = page("Somebody", &tracks, &albums, &Relations::default(), false);
        assert_eq!(names(writer.play_list(&albums)), ["Wrote It"]);
    }

    #[test]
    fn credits_on_their_own_albums_go_with_the_album_not_the_credits_list() {
        let (mut tracks, _) = library();
        // He also produced Somebody's "Wrote It".
        tracks[3].credits.push(Credit::parse(Role::Producer, "The Alchemist"));
        let albums = crate::album::build(&tracks);
        let p = page("The Alchemist", &tracks, &albums, &Relations::default(), false);
        assert_eq!(p.credits.iter().map(|(i, r)| (tracks[*i].title.as_str(), r.as_slice())).collect::<Vec<_>>(), [("Wrote It", [Role::Producer].as_slice())]);
        let mercy = albums.iter().position(|a| a.title == "Mercy").expect("Mercy");
        assert_eq!(p.album_credits, [(mercy, vec![(Role::Producer, 2)])], "both Mercy songs");
        assert!(page("Nobody", &tracks, &albums, &Relations::default(), false).is_empty());
    }

    #[test]
    fn an_album_credit_counts_the_songs_it_covers() {
        let (mut tracks, _) = library();
        tracks[2].credits.push(Credit::parse(Role::Mixer, "Armand Hammer"));
        tracks[1].credits.push(Credit::parse(Role::Mixer, "Armand Hammer"));
        tracks[1].credits.push(Credit::parse(Role::Composer, "Armand Hammer"));
        let albums = crate::album::build(&tracks);
        let p = page("Armand Hammer", &tracks, &albums, &Relations::default(), false);
        let at = |t: &str| albums.iter().position(|a| a.title == t).expect("album");
        let mut got = p.album_credits.clone();
        got.sort_by_key(|(a, _)| *a);
        let mut want = vec![(at("Mercy"), vec![(Role::Composer, 1), (Role::Mixer, 1)]), (at("Haram"), vec![(Role::Mixer, 1)])];
        want.sort_by_key(|(a, _)| *a);
        assert_eq!(got, want);
        assert!(p.credits.is_empty());
    }

    #[test]
    fn a_song_with_several_of_their_credits_lists_once_with_each_role() {
        let (mut tracks, _) = library();
        tracks[3].credits.extend([Credit::parse(Role::Producer, "Cleo Reed"), Credit::parse(Role::Composer, "Cleo Reed")]);
        let albums = crate::album::build(&tracks);
        let p = page("Cleo Reed", &tracks, &albums, &Relations::default(), false);
        assert_eq!(p.credits.len(), 1, "one entry per song");
        assert_eq!(p.credits[0].1, [Role::Composer, Role::Lyricist, Role::Producer], "roles in credit order");
        assert_eq!(p.credited(), [3]);
    }

    #[test]
    fn merged_names_share_one_page_led_by_the_most_prominent_name() {
        let (mut tracks, _) = library();
        // Under his legal name he produced and wrote Somebody's song; as Armand Hammer he's
        // the album artist of two albums.
        tracks[3].credits.extend([Credit::parse(Role::Producer, "Billy Woods"), Credit::parse(Role::Lyricist, "Billy Woods")]);
        let albums = crate::album::build(&tracks);
        let relations = Relations { merges: vec![strings(&["billy woods", "Armand Hammer"])], links: Vec::new() };
        let p = page("billy woods", &tracks, &albums, &relations, false);
        assert_eq!(p.name, "Armand Hammer", "album artist outweighs producer, whichever name opened the page");
        assert_eq!(p.names, ["Armand Hammer", "Billy Woods"], "each spelled as the library does");
        assert_eq!(titles(&albums, &p.albums), ["Mercy", "Haram"]);
        assert_eq!(p.credits.iter().map(|(i, r)| (tracks[*i].title.as_str(), r.as_slice())).collect::<Vec<_>>(), [("Wrote It", [Role::Lyricist, Role::Producer].as_slice())]);
        assert_eq!(page("Armand Hammer", &tracks, &albums, &relations, false), p, "either name opens the same page");
    }

    #[test]
    fn a_page_lists_the_artists_linked_to_any_of_its_names() {
        use crate::relations::{Link, LinkKind};
        let (tracks, albums) = library();
        let relations = Relations {
            merges: vec![strings(&["Armand Hammer", "AH"])],
            links: vec![Link { from: "ah".into(), kind: LinkKind::Alias, to: "Somebody".into() }, Link { from: "Cleo Reed".into(), kind: LinkKind::MemberOf, to: "Armand Hammer".into() }],
        };
        let p = page("Armand Hammer", &tracks, &albums, &relations, false);
        assert_eq!(p.related, [(Relation::Alias, "Somebody".to_string()), (Relation::Member, "Cleo Reed".to_string())]);
        assert_eq!(page("Cleo Reed", &tracks, &albums, &relations, false).related, [(Relation::MemberOf, "Armand Hammer".to_string())]);
        assert!(page("Cleo Reed", &tracks, &albums, &relations, false).albums.is_empty(), "a link doesn't merge pages");
    }

    #[test]
    fn with_aliases_the_page_holds_what_each_name_released_under_its_own_name() {
        use crate::relations::{Link, LinkKind};
        let (tracks, albums) = library();
        let relations = Relations { merges: Vec::new(), links: vec![Link { from: "Somebody".into(), kind: LinkKind::Alias, to: "Armand Hammer".into() }] };
        let alone = page("Somebody", &tracks, &albums, &relations, false);
        assert!(alone.has_aliases());
        assert_eq!(titles(&albums, &alone.albums), ["Elsewhere"]);
        let all = page("Somebody", &tracks, &albums, &relations, true);
        assert_eq!(all.matched, ["Somebody", "Armand Hammer"]);
        assert_eq!(all.name, "Somebody", "still their page, named as before");
        assert_eq!(all.names, ["Somebody"]);
        assert_eq!(all.related, alone.related);
        assert_eq!(titles(&albums, &all.albums), ["Mercy", "Haram", "Elsewhere"], "newest first across both names");
        assert!(!page("Cleo Reed", &tracks, &albums, &relations, false).has_aliases());
    }

    #[test]
    fn suggestions_match_every_word_rank_exact_then_most_named_and_skip_excluded() {
        let (tracks, _) = library();
        assert_eq!(suggest("a", &tracks, &[], 3), ["Armand Hammer", "The Alchemist", "Silka"], "most named first");
        assert_eq!(suggest("ham arm", &tracks, &[], 5), ["Armand Hammer"]);
        assert_eq!(suggest("cleo", &tracks, &strings(&["CLEO REED"]), 5), Vec::<String>::new());
        assert_eq!(suggest("somebody", &tracks, &[], 5), ["Somebody"]);
        assert!(suggest("  ", &tracks, &[], 5).is_empty());
    }

    #[test]
    fn play_everything_is_their_list_then_each_involved_artists_once_each_song() {
        use crate::relations::{Link, LinkKind};
        let (tracks, albums) = library();
        let link = |from: &str, to: &str| Link { from: from.into(), kind: LinkKind::MemberOf, to: to.into() };
        let relations = Relations { merges: Vec::new(), links: vec![link("Cleo Reed", "Armand Hammer"), link("Somebody", "Armand Hammer")] };
        let members = involved("Armand Hammer", &tracks, &albums, &relations, false);
        assert_eq!(members.iter().map(|(r, p)| (*r, p.name.as_str())).collect::<Vec<_>>(), [(Relation::Member, "Cleo Reed"), (Relation::Member, "Somebody")], "link order");
        let names = |list: Vec<usize>| list.into_iter().map(|i| tracks[i].title.clone()).collect::<Vec<String>>();
        assert_eq!(names(everything("Armand Hammer", &tracks, &albums, &relations, false)), ["Calypso Gene", "Laraaji", "Solo", "Wrote It"], "their own first, Calypso Gene once");
        assert!(involved("Cleo Reed", &tracks, &albums, &relations, false).iter().all(|(r, _)| *r == Relation::MemberOf), "Cleo Reed's band is Armand Hammer");
        assert!(involved("Somebody", &tracks, &albums, &Relations::default(), false).is_empty());
        let former = Relations { merges: Vec::new(), links: vec![Link { from: "Cleo Reed".into(), kind: LinkKind::FormerMemberOf, to: "Somebody".into() }] };
        assert_eq!(involved("Somebody", &tracks, &albums, &former, false).iter().map(|(r, p)| (*r, p.name.as_str())).collect::<Vec<_>>(), [(Relation::FormerMember, "Cleo Reed")], "a former member gets a section");
        assert_eq!(names(everything("Somebody", &tracks, &albums, &former, false)).len(), everything("Somebody", &tracks, &albums, &Relations::default(), false).len() + 1, "and plays with Play Everything");
    }
}

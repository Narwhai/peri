//! Albums: songs gathered by album artist (else artist), album title and year. Songs without
//! an album tag gather per artist under "Unknown album".

use std::collections::HashMap;

use crate::base::property::{Property, Record, Value, list, number, text};
use crate::credits::{self, Credit};
use crate::library::Track;

pub const UNKNOWN_ALBUM: &str = "Unknown album";
pub const UNKNOWN_ARTIST: &str = "Unknown artist";

#[derive(Clone, Debug, PartialEq)]
pub struct Album {
    pub title: String,
    /// The album artist, else the first track's artists; `None` when no song names any.
    pub artist: Option<String>,
    /// The album's artists one per act ("Armand Hammer", "The Alchemist"), from its first song.
    pub album_artists: Vec<String>,
    pub year: Option<u32>,
    /// Indices into the library, in disc then track order.
    pub tracks: Vec<usize>,
    /// Whether these are the artist's songs that have no album tag.
    pub unknown: bool,
    /// Every song's genres, artists and credits, without repeats.
    pub genres: Vec<String>,
    pub artists: Vec<String>,
    pub featured: Vec<String>,
    pub credits: Vec<Credit>,
    pub label: Option<String>,
    pub formats: Vec<String>,
    pub folder: Option<String>,
    /// Total seconds of the songs whose length is known.
    pub duration: f64,
    /// When the newest song was added.
    pub added: Option<i64>,
}

impl Album {
    pub fn artist_name(&self) -> &str {
        self.artist.as_deref().unwrap_or(UNKNOWN_ARTIST)
    }

    /// A key that names the album across rescans, for the album page.
    pub fn id(&self) -> AlbumId {
        AlbumId {
            artist: self.artist.as_deref().unwrap_or_default().to_lowercase(),
            title: if self.unknown { String::new() } else { self.title.to_lowercase() },
            year: self.year,
        }
    }

    /// Discs the album spans, when more than one.
    pub fn discs(&self, tracks: &[Track]) -> Vec<Option<u32>> {
        let mut discs: Vec<Option<u32>> = Vec::new();
        for &i in &self.tracks {
            let d = tracks[i].disc;
            if !discs.contains(&d) {
                discs.push(d);
            }
        }
        discs
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AlbumId {
    artist: String,
    title: String,
    year: Option<u32>,
}

impl Record for Album {
    fn value(&self, p: Property) -> Value {
        match p {
            Property::Title | Property::Album => Value::Text(self.title.clone()),
            Property::AlbumArtist => list(&self.album_artists),
            Property::Artist => list(&self.artists),
            Property::Featured => list(&self.featured),
            Property::Genre => list(&self.genres),
            Property::Year => number(self.year.map(f64::from)),
            Property::Composer
            | Property::Lyricist
            | Property::Writer
            | Property::Producer
            | Property::Arranger
            | Property::Conductor
            | Property::Performer
            | Property::Engineer
            | Property::Mixer
            | Property::Remixer
            | Property::DjMixer => p.role().map_or(Value::None, |role| list(&credits::names(&self.credits, role))),
            Property::Label => text(self.label.as_deref()),
            Property::Tracks => Value::Number(self.tracks.len() as f64),
            Property::Duration => number((self.duration > 0.0).then_some(self.duration)),
            Property::Format => list(&self.formats),
            Property::Folder => text(self.folder.as_deref()),
            Property::Added => number(self.added.map(|s| s as f64)),
            Property::Track | Property::Disc => Value::None,
        }
    }

    fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {}", self.title, self.artist.as_deref().unwrap_or_default(), self.artists.join(" ")).to_lowercase();
        query.split_whitespace().all(|word| haystack.contains(word))
    }
}

/// Gathers the library into albums, in library order (artist, album, disc, track).
pub fn build(tracks: &[Track]) -> Vec<Album> {
    let mut albums: Vec<Album> = Vec::new();
    let mut index: HashMap<(String, String, Option<u32>), usize> = HashMap::new();
    for (i, t) in tracks.iter().enumerate() {
        let title = t.album.as_deref().map(str::trim).filter(|a| !a.is_empty());
        let artist = t.album_artist.clone().or_else(|| t.main_artist());
        // Songs without an album gather per artist whatever their year.
        let year = if title.is_some() { t.year } else { None };
        let key = (artist.as_deref().unwrap_or_default().to_lowercase(), title.map(str::to_lowercase).unwrap_or_default(), year);
        let at = *index.entry(key).or_insert_with(|| {
            albums.push(Album {
                title: title.unwrap_or(UNKNOWN_ALBUM).to_string(),
                artist: artist.clone(),
                album_artists: if t.album_artists.is_empty() { t.artists.iter().filter(|a| !t.featured.contains(a)).cloned().collect() } else { t.album_artists.clone() },
                year,
                tracks: Vec::new(),
                unknown: title.is_none(),
                genres: Vec::new(),
                artists: Vec::new(),
                featured: Vec::new(),
                credits: Vec::new(),
                label: None,
                formats: Vec::new(),
                folder: None,
                duration: 0.0,
                added: None,
            });
            albums.len() - 1
        });
        let a = &mut albums[at];
        a.tracks.push(i);
        merge(&mut a.genres, &t.genres);
        merge(&mut a.artists, &t.artists);
        merge(&mut a.featured, &t.featured);
        for c in &t.credits {
            if !a.credits.contains(c) {
                a.credits.push(c.clone());
            }
        }
        if a.label.is_none() {
            a.label = t.label.clone();
        }
        if let Some(ext) = t.path.extension().and_then(|e| e.to_str()) {
            merge(&mut a.formats, &[ext.to_lowercase()]);
        }
        if a.folder.is_none() {
            a.folder = t.path.parent().and_then(|p| p.to_str()).map(String::from);
        }
        a.duration += t.duration.unwrap_or(0.0);
        a.added = a.added.max(t.added);
    }
    for a in &mut albums {
        a.tracks.sort_by_key(|&i| (tracks[i].disc.unwrap_or(0), tracks[i].number.unwrap_or(u32::MAX), i));
    }
    albums
}

fn merge(into: &mut Vec<String>, values: &[String]) {
    for v in values {
        if !into.iter().any(|x| x.eq_ignore_ascii_case(v)) {
            into.push(v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, album: Option<&str>, album_artist: Option<&str>, artist: &str, year: Option<u32>, disc: Option<u32>, number: u32) -> Track {
        Track {
            path: format!("/m/{title}.flac").into(),
            title: title.into(),
            artists: vec![artist.into()],
            album_artist: album_artist.map(Into::into),
            album: album.map(Into::into),
            year,
            disc,
            number: Some(number),
            duration: Some(100.0),
            genres: vec!["Rock".into()],
            ..Track::default()
        }
    }

    #[test]
    fn albums_gather_by_artist_title_and_year() {
        let tracks = vec![
            song("a2", Some("Your Day Will Come"), Some("Chanel Beads"), "Chanel Beads", Some(2024), None, 2),
            song("a1", Some("your day will come"), None, "Chanel Beads", Some(2024), None, 1),
            song("b1", Some("Your Day Will Come"), Some("Chanel Beads"), "Chanel Beads", Some(2026), None, 1),
            song("c1", Some("Mercy"), Some("Armand Hammer & The Alchemist"), "Armand Hammer & The Alchemist feat. Silka", Some(2025), None, 1),
            song("c2", Some("Mercy"), Some("Armand Hammer & The Alchemist"), "Armand Hammer & The Alchemist", Some(2025), None, 2),
        ];
        let albums = build(&tracks);
        let shown: Vec<(&str, Option<u32>, Vec<&str>)> =
            albums.iter().map(|a| (a.title.as_str(), a.year, a.tracks.iter().map(|&i| tracks[i].title.as_str()).collect())).collect();
        assert_eq!(
            shown,
            [
                ("Your Day Will Come", Some(2024), vec!["a1", "a2"]),
                ("Your Day Will Come", Some(2026), vec!["b1"]),
                ("Mercy", Some(2025), vec!["c1", "c2"]),
            ],
            "same name in another year is another album; tracks in track order; featured artists stay on the album artist's album"
        );
        assert_ne!(albums[0].id(), albums[1].id());
        assert_eq!(albums[2].artists.len(), 2);
        assert_eq!(albums[2].duration, 200.0);
    }

    #[test]
    fn a_collaborative_album_groups_and_filters_under_each_album_artist() {
        use crate::base::filter::{Condition, Group, Node, Operator};
        use crate::base::view::{Direction, GroupBy, View, evaluate};
        use crate::library::TaggedArtists;
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        let mut mercy = song("c1", Some("Mercy"), Some("Armand Hammer & The Alchemist"), "Armand Hammer & The Alchemist", Some(2025), None, 1);
        mercy.artists.clear();
        mercy.tagged = TaggedArtists {
            artists: ids(&["Armand Hammer", "The Alchemist"]),
            artist_ids: ids(&["ah", "alc"]),
            album_artists: Vec::new(),
            album_artist_ids: ids(&["ah", "alc"]),
        };
        let mut tracks = vec![mercy, song("h1", Some("Haram"), Some("Armand Hammer"), "Armand Hammer", Some(2021), None, 1)];
        crate::credits::resolve(&mut tracks);
        let albums = build(&tracks);
        assert_eq!(albums[0].value(Property::AlbumArtist), Value::List(ids(&["Armand Hammer", "The Alchemist"])));

        let mut view = View::new("v");
        view.group_by = Some(GroupBy { property: Property::AlbumArtist, direction: Direction::Asc });
        let groups: Vec<(String, usize)> = evaluate(&view, &albums, "").sections.iter().map(|s| (s.title.clone(), s.items.len())).collect();
        assert_eq!(groups, [("Armand Hammer".to_string(), 2), ("The Alchemist".to_string(), 1)]);

        view.group_by = None;
        view.filter = Group { children: vec![Node::Condition(Condition { property: Property::AlbumArtist, op: Operator::Has, value: "the alchemist".into() })], ..Group::default() };
        let shown: Vec<&str> = evaluate(&view, &albums, "").order.iter().map(|&a| albums[a].title.as_str()).collect();
        assert_eq!(shown, ["Mercy"]);
        // Songs read the same list.
        assert_eq!(Property::AlbumArtist.value(&tracks[0]), Value::List(ids(&["Armand Hammer", "The Alchemist"])));
    }

    #[test]
    fn songs_without_an_album_gather_per_artist() {
        let tracks = vec![
            song("x", None, None, "dazegxd", Some(2020), None, 1),
            song("y", None, None, "dazegxd", Some(2021), None, 2),
            song("z", None, None, "Desired", None, None, 1),
        ];
        let albums = build(&tracks);
        let shown: Vec<(&str, &str, usize, bool)> = albums.iter().map(|a| (a.title.as_str(), a.artist_name(), a.tracks.len(), a.unknown)).collect();
        assert_eq!(shown, [(UNKNOWN_ALBUM, "dazegxd", 2, true), (UNKNOWN_ALBUM, "Desired", 1, true)]);
    }

    #[test]
    fn multi_disc_albums_order_by_disc_then_track() {
        let tracks = vec![
            song("d2t1", Some("Revenge"), Some("Jung An Tagen"), "Jung An Tagen", Some(2025), Some(2), 1),
            song("d1t2", Some("Revenge"), Some("Jung An Tagen"), "Jung An Tagen", Some(2025), Some(1), 2),
            song("d1t1", Some("Revenge"), Some("Jung An Tagen"), "Jung An Tagen", Some(2025), Some(1), 1),
        ];
        let albums = build(&tracks);
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].tracks.iter().map(|&i| tracks[i].title.as_str()).collect::<Vec<_>>(), ["d1t1", "d1t2", "d2t1"]);
        assert_eq!(albums[0].discs(&tracks), [Some(1), Some(2)]);
    }

    #[test]
    fn search_matches_album_title_and_artists() {
        let tracks = vec![song("c1", Some("Mercy"), Some("Armand Hammer & The Alchemist"), "Silka", Some(2025), None, 1)];
        let album = &build(&tracks)[0];
        assert!(album.matches("mercy"));
        assert!(album.matches("alchemist"));
        assert!(album.matches("silka"), "featured artists are searchable");
        assert!(!album.matches("c1"), "song titles are matched separately");
    }
}

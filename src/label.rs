//! What a label page lists: the albums released under a record label. Names match ignoring
//! case and surrounding spaces.

use crate::album::Album;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LabelPage {
    /// The name as the library spells it most often.
    pub name: String,
    /// Albums released under the label, newest first (undated last).
    pub albums: Vec<usize>,
}

impl LabelPage {
    /// What Play plays: the albums newest first, each in track order.
    pub fn play_list(&self, albums: &[Album]) -> Vec<usize> {
        self.albums.iter().flat_map(|&a| albums[a].tracks.iter().copied()).collect()
    }
}

fn is(label: Option<&str>, name: &str) -> bool {
    label.is_some_and(|l| l.trim().eq_ignore_ascii_case(name))
}

/// Gathers the albums released under `name`.
pub fn page(name: &str, albums: &[Album]) -> LabelPage {
    let name = name.trim();
    let mut list: Vec<usize> = (0..albums.len()).filter(|&a| is(albums[a].label.as_deref(), name)).collect();
    list.sort_by_key(|&a| (albums[a].year.is_none(), std::cmp::Reverse(albums[a].year)));
    LabelPage { name: spelling(name, albums), albums: list }
}

/// The library's most common spelling of `name` (ignoring case), else `name`.
fn spelling(name: &str, albums: &[Album]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for l in albums.iter().filter_map(|a| a.label.as_deref()).map(str::trim).filter(|l| l.eq_ignore_ascii_case(name)) {
        match counts.iter_mut().find(|(s, _)| *s == l) {
            Some((_, c)) => *c += 1,
            None => counts.push((l, 1)),
        }
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map_or_else(|| name.to_string(), |(s, _)| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Track;

    fn song(title: &str, album: &str, artist: &str, label: Option<&str>, year: Option<u32>, number: u32) -> Track {
        Track {
            path: format!("/m/{album}/{title}.flac").into(),
            title: title.into(),
            album: Some(album.into()),
            credit: Some(artist.into()),
            album_artist: Some(artist.into()),
            label: label.map(String::from),
            year,
            number: Some(number),
            ..Track::default()
        }
    }

    fn library() -> (Vec<Track>, Vec<Album>) {
        let mut tracks = vec![
            song("Old 1", "Old", "A", Some("Warp"), Some(1999), 1),
            song("Old 2", "Old", "A", Some("Warp"), Some(1999), 2),
            song("New", "New", "B", Some("warp "), Some(2024), 1),
            song("Undated", "Undated", "C", Some("Warp"), None, 1),
            song("Other", "Other", "D", Some("Ninja Tune"), Some(2010), 1),
            song("None", "Untagged", "E", None, Some(2011), 1),
        ];
        crate::credits::resolve(&mut tracks);
        crate::library::sort(&mut tracks);
        let albums = crate::album::build(&tracks);
        (tracks, albums)
    }

    #[test]
    fn lists_the_labels_albums_newest_first_ignoring_case() {
        let (_, albums) = library();
        let p = page("WARP", &albums);
        assert_eq!(p.name, "Warp", "the most common spelling");
        let titles: Vec<&str> = p.albums.iter().map(|&a| albums[a].title.as_str()).collect();
        assert_eq!(titles, ["New", "Old", "Undated"]);
        assert!(page("Nobody", &albums).albums.is_empty());
    }

    #[test]
    fn play_takes_each_album_in_track_order_newest_first() {
        let (tracks, albums) = library();
        let names: Vec<String> = page("Warp", &albums).play_list(&albums).into_iter().map(|i| tracks[i].title.clone()).collect();
        assert_eq!(names, ["New", "Old 1", "Old 2", "Undated"]);
    }
}

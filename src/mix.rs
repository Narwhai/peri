//! What the library sounds like: the share of plays per genre and per decade of year. A song's
//! plays count for each of its genres, split evenly among them. With no plays yet, the shares are
//! the library's instead, so the lists still show something, and they say which they show.

use std::collections::HashMap;

use crate::library::Track;

/// How many bars each list shows.
pub const MIX_COUNT: usize = 6;

/// Where a list's shares come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Share of the plays.
    Plays,
    /// Share of the library's songs: nothing has been played yet.
    Library,
}

/// One bar: a name and its share of the whole (0 to 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub name: String,
    pub share: f64,
}

/// A list of bars, most share first.
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    pub source: Source,
    pub bars: Vec<Bar>,
}

/// The weight each of `tracks` brings to a list, keyed by the labels the track falls under: its
/// plays, or 1 per song when none have been played. Returns the weights and where they come from.
fn weights(
    tracks: &[Track],
    labels: impl Fn(&Track) -> Vec<String>,
) -> (HashMap<String, f64>, Source) {
    let played: u32 = tracks.iter().map(|t| t.plays).sum();
    let mut weights: HashMap<String, f64> = HashMap::new();
    for t in tracks {
        let names = labels(t);
        if names.is_empty() {
            continue;
        }
        let weight = if played > 0 { f64::from(t.plays) } else { 1.0 };
        if weight == 0.0 {
            continue;
        }
        let each = weight / names.len() as f64;
        for name in names {
            *weights.entry(name).or_default() += each;
        }
    }
    (
        weights,
        if played > 0 {
            Source::Plays
        } else {
            Source::Library
        },
    )
}

/// The bars from `weights`, most first and capped at `MIX_COUNT`; the shares are of the whole
/// (including the names left off the list).
fn bars(weights: HashMap<String, f64>) -> Vec<Bar> {
    let total: f64 = weights.values().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut list: Vec<(String, f64)> = weights.into_iter().collect();
    list.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    list.into_iter()
        .take(MIX_COUNT)
        .map(|(name, w)| Bar {
            name,
            share: w / total,
        })
        .collect()
}

/// The genres' share: a song's genres from the tags, split evenly among them.
pub fn genres(tracks: &[Track]) -> Mix {
    let (weights, source) = weights(tracks, |t| t.genres.clone());
    Mix {
        source,
        bars: bars(weights),
    }
}

/// The decade of a year, as "1990s".
pub fn decade(year: u32) -> String {
    format!("{}s", year / 10 * 10)
}

/// The decades' share, from each song's year. Songs with no year are left out.
pub fn decades(tracks: &[Track]) -> Mix {
    let (weights, source) = weights(tracks, |t| {
        t.year.map(|y| vec![decade(y)]).unwrap_or_default()
    });
    Mix {
        source,
        bars: bars(weights),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(genres: &[&str], year: Option<u32>, plays: u32) -> Track {
        Track {
            path: format!("/m/{plays}-{year:?}.flac").into(),
            genres: genres.iter().map(|g| g.to_string()).collect(),
            year,
            plays,
            ..Track::default()
        }
    }

    fn share(mix: &Mix, name: &str) -> Option<f64> {
        mix.bars.iter().find(|b| b.name == name).map(|b| b.share)
    }

    #[test]
    fn a_song_s_plays_split_across_its_genres() {
        let tracks = [song(&["Jazz", "Soul"], None, 4), song(&["Jazz"], None, 2)];
        let mix = genres(&tracks);
        assert_eq!(mix.source, Source::Plays);
        assert!(
            (share(&mix, "Jazz").unwrap() - 4.0 / 6.0).abs() < 1e-9,
            "2 of the first song's 4, plus the second's 2, of 6"
        );
        assert!((share(&mix, "Soul").unwrap() - 2.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn shares_of_the_bars_add_to_one_when_every_name_is_listed() {
        let tracks = [song(&["A"], None, 1), song(&["B"], None, 3)];
        let mix = genres(&tracks);
        assert!((mix.bars.iter().map(|b| b.share).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(mix.bars[0].name, "B", "the largest first");
    }

    #[test]
    fn with_no_plays_the_library_share_is_shown_and_labelled() {
        let tracks = [
            song(&["Jazz"], None, 0),
            song(&["Jazz"], None, 0),
            song(&["Ambient"], None, 0),
        ];
        let mix = genres(&tracks);
        assert_eq!(mix.source, Source::Library);
        assert!(
            (share(&mix, "Jazz").unwrap() - 2.0 / 3.0).abs() < 1e-9,
            "two of three songs"
        );
    }

    #[test]
    fn decades_group_years_and_leave_out_undated_songs() {
        let tracks = [
            song(&[], Some(1987), 2),
            song(&[], Some(1992), 2),
            song(&[], Some(2014), 4),
            song(&[], None, 9),
        ];
        let mix = decades(&tracks);
        assert_eq!(mix.source, Source::Plays);
        // 2 plays in the 1980s, 2 in the 1990s, 4 in the 2010s: of 8, the undated 9 left out.
        assert_eq!(share(&mix, "1980s"), Some(0.25));
        assert_eq!(share(&mix, "1990s"), Some(0.25));
        assert_eq!(share(&mix, "2010s"), Some(0.5));
        assert_eq!(decade(1999), "1990s");
    }

    #[test]
    fn unplayed_songs_are_left_out_once_something_has_played() {
        let tracks = [song(&["Played"], None, 3), song(&["Unplayed"], None, 0)];
        let mix = genres(&tracks);
        assert_eq!(mix.source, Source::Plays);
        assert_eq!(share(&mix, "Played"), Some(1.0));
        assert_eq!(share(&mix, "Unplayed"), None, "no plays, no share");
    }

    #[test]
    fn songs_without_genres_or_years_give_no_bars() {
        let tracks = [song(&[], None, 3)];
        assert!(genres(&tracks).bars.is_empty());
        assert!(decades(&tracks).bars.is_empty());
    }

    #[test]
    fn the_lists_are_capped_at_mix_count() {
        let names: Vec<String> = (0..MIX_COUNT + 4).map(|n| format!("G{n:02}")).collect();
        let tracks: Vec<Track> = names.iter().map(|n| song(&[n.as_str()], None, 1)).collect();
        assert_eq!(genres(&tracks).bars.len(), MIX_COUNT);
    }
}

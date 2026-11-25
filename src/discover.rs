//! Albums to pick up next: the ones started and not finished, the ones played a lot and then left
//! alone, and the ones added this month. Each is a pure function of the library and the time, so
//! the home page's discovery block can be tested with fixed times.

use crate::album::Album;
use crate::library::Track;

/// How many albums each tab keeps: a pool the home page's grid takes as many whole rows from as
/// fit the window.
pub const DISCOVER_COUNT: usize = 60;
/// An album counts as rediscoverable once it has this many plays.
pub const REDISCOVER_PLAYS: u32 = 5;
/// ...and was last played this long ago, in days.
pub const REDISCOVER_DAYS: i64 = 60;
/// How far back "New this month" looks, in days.
pub const NEW_DAYS: i64 = 30;

const DAY: i64 = 86_400;

/// The discovery tabs, in the order the switch shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    Unlistened,
    Continue,
    Rediscover,
    New,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Unlistened, Tab::Continue, Tab::Rediscover, Tab::New];

    /// The value saved in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Tab::Unlistened => "unlistened",
            Tab::Continue => "continue",
            Tab::Rediscover => "rediscover",
            Tab::New => "new",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Unlistened => "Unlistened",
            Tab::Continue => "Continue",
            Tab::Rediscover => "Rediscover",
            Tab::New => "New this month",
        }
    }
}

/// How much of an album has been played: its songs with a play, of its songs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub played: usize,
    pub total: usize,
}

/// The played and total songs of `album`.
pub fn progress(album: &Album, tracks: &[Track]) -> Progress {
    let played = album
        .tracks
        .iter()
        .filter(|&&t| tracks.get(t).is_some_and(|t| t.plays > 0))
        .count();
    Progress {
        played,
        total: album.tracks.len(),
    }
}

/// Albums started but not finished: some songs played, not all. Most recently played first, the
/// most recent last (so the one the listener was last in is at the top of the list).
pub fn continuing(albums: &[Album], tracks: &[Track]) -> Vec<usize> {
    let mut list: Vec<(i64, usize)> = albums
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.unknown)
        .filter(|(_, a)| {
            let p = progress(a, tracks);
            p.played > 0 && p.played < p.total
        })
        .map(|(i, a)| (a.last_played.unwrap_or(0), i))
        .collect();
    list.sort_by_key(|&(at, i)| (std::cmp::Reverse(at), i));
    list.into_iter()
        .take(DISCOVER_COUNT)
        .map(|(_, i)| i)
        .collect()
}

/// Albums with at least `REDISCOVER_PLAYS` plays whose last play is older than
/// `REDISCOVER_DAYS`, by plays, most first.
pub fn rediscover(albums: &[Album], now: i64) -> Vec<usize> {
    let cutoff = now - REDISCOVER_DAYS * DAY;
    let mut list: Vec<(u32, usize)> = albums
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            !a.unknown && a.plays >= REDISCOVER_PLAYS && a.last_played.is_some_and(|at| at < cutoff)
        })
        .map(|(i, a)| (a.plays, i))
        .collect();
    list.sort_by_key(|&(plays, i)| (std::cmp::Reverse(plays), i));
    list.into_iter()
        .take(DISCOVER_COUNT)
        .map(|(_, i)| i)
        .collect()
}

/// Albums added in the last `NEW_DAYS`, newest first.
pub fn new_this_month(albums: &[Album], now: i64) -> Vec<usize> {
    let since = now - NEW_DAYS * DAY;
    let mut list: Vec<(i64, usize)> = albums
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            a.added
                .filter(|&at| at >= since && at <= now)
                .map(|at| (at, i))
        })
        .collect();
    list.sort_by_key(|&(at, i)| (std::cmp::Reverse(at), i));
    list.into_iter()
        .take(DISCOVER_COUNT)
        .map(|(_, i)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    /// One album `title` of `songs` songs, with the first `played` of them played at `at`.
    fn album(
        title: &str,
        songs: usize,
        played: usize,
        at: Option<i64>,
        added: Option<i64>,
    ) -> (Album, Vec<Track>) {
        let tracks: Vec<Track> = (0..songs)
            .map(|n| Track {
                path: format!("/m/{title}/{n}.flac").into(),
                title: format!("{title} {n}"),
                album: Some(title.into()),
                plays: u32::from(n < played),
                last_played: if n < played { at } else { None },
                ..Track::default()
            })
            .collect();
        let mut a = crate::album::build(&tracks).remove(0);
        a.added = added;
        a.last_played = at;
        a.plays = played as u32;
        (a, tracks)
    }

    /// The library of several albums, with each one's songs in a single list.
    fn library(parts: Vec<(Album, Vec<Track>)>) -> (Vec<Album>, Vec<Track>) {
        let mut albums = Vec::new();
        let mut tracks: Vec<Track> = Vec::new();
        for (mut a, t) in parts {
            let offset = tracks.len();
            a.tracks = (offset..offset + t.len()).collect();
            tracks.extend(t);
            albums.push(a);
        }
        (albums, tracks)
    }

    #[test]
    fn continue_lists_albums_part_way_through_most_recent_first() {
        let (albums, tracks) = library(vec![
            album("Old", 4, 2, Some(NOW - 100 * DAY), None),
            album("Fresh", 4, 1, Some(NOW - DAY), None),
            album("Finished", 3, 3, Some(NOW - DAY), None),
            album("Untouched", 3, 0, None, None),
        ]);
        let list = continuing(&albums, &tracks);
        assert_eq!(
            list.iter()
                .map(|&a| albums[a].title.as_str())
                .collect::<Vec<_>>(),
            ["Fresh", "Old"]
        );
    }

    #[test]
    fn rediscover_needs_plays_and_an_old_last_play() {
        let (albums, _) = library(vec![
            album("Loved long ago", 6, 6, Some(NOW - 200 * DAY), None),
            album("Loved recently", 6, 6, Some(NOW - 2 * DAY), None),
            album("Barely loved", 6, 2, Some(NOW - 200 * DAY), None),
        ]);
        let list = rediscover(&albums, NOW);
        assert_eq!(
            list.iter()
                .map(|&a| albums[a].title.as_str())
                .collect::<Vec<_>>(),
            ["Loved long ago"]
        );
    }

    #[test]
    fn rediscover_sorts_by_plays() {
        let (mut albums, _) = library(vec![
            album("Five", 5, 5, Some(NOW - 200 * DAY), None),
            album("Nine", 9, 9, Some(NOW - 200 * DAY), None),
        ]);
        albums[0].plays = 5;
        albums[1].plays = 9;
        let list = rediscover(&albums, NOW);
        assert_eq!(
            list.iter()
                .map(|&a| albums[a].title.as_str())
                .collect::<Vec<_>>(),
            ["Nine", "Five"]
        );
    }

    #[test]
    fn new_this_month_is_the_last_thirty_days_newest_first() {
        let (albums, _) = library(vec![
            album("Old", 2, 0, None, Some(NOW - 90 * DAY)),
            album("Week", 2, 0, None, Some(NOW - 7 * DAY)),
            album("Day", 2, 0, None, Some(NOW - DAY)),
            album("Future", 2, 0, None, Some(NOW + DAY)),
        ]);
        let list = new_this_month(&albums, NOW);
        assert_eq!(
            list.iter()
                .map(|&a| albums[a].title.as_str())
                .collect::<Vec<_>>(),
            ["Day", "Week"]
        );
    }

    #[test]
    fn progress_counts_songs_with_a_play() {
        let (albums, tracks) = library(vec![album("Half", 4, 2, Some(NOW), None)]);
        assert_eq!(
            progress(&albums[0], &tracks),
            Progress {
                played: 2,
                total: 4
            }
        );
    }

    #[test]
    fn tabs_have_ids_and_labels() {
        assert_eq!(Tab::default(), Tab::Unlistened);
        assert_eq!(
            Tab::ALL.map(Tab::id),
            ["unlistened", "continue", "rediscover", "new"]
        );
        assert_eq!(Tab::New.label(), "New this month");
    }
}

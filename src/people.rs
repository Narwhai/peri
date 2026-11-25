//! The people behind what's been played: for each credit role, the names credited on the songs
//! played, weighted by those songs' plays in a window.

use std::collections::HashMap;

use crate::credits::Role;
use crate::library::Track;
use crate::plays::Plays;
use crate::top::{Ranked, Window, song_plays};

/// How many names each role lists.
pub const PEOPLE_COUNT: usize = 5;

/// One role's names, most plays first.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleBoard {
    pub role: Role,
    pub names: Vec<Ranked<String>>,
}

/// The roles with at least one name in the window, in the order credits list them. A name
/// counts once per song, however many times it's credited on that song.
pub fn boards(tracks: &[Track], plays: &Plays, window: Window, now: i64) -> Vec<RoleBoard> {
    let mut weights: HashMap<Role, HashMap<String, u32>> = HashMap::new();
    for (i, n) in song_plays(tracks, plays, window, now) {
        let t = &tracks[i];
        for role in Role::ALL {
            for name in t.credited(role) {
                *weights.entry(role).or_default().entry(name).or_default() += n;
            }
        }
    }
    Role::ALL
        .into_iter()
        .filter_map(|role| {
            let counts = weights.remove(&role)?;
            let mut names: Vec<Ranked<String>> = counts
                .into_iter()
                .map(|(item, plays)| Ranked { item, plays })
                .collect();
            names.sort_by(|a, b| {
                b.plays
                    .cmp(&a.plays)
                    .then_with(|| a.item.to_lowercase().cmp(&b.item.to_lowercase()))
            });
            names.truncate(PEOPLE_COUNT);
            Some(RoleBoard { role, names })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credits::Credit;

    const NOW: i64 = 1_800_000_000;

    fn song(title: &str, credits: &[(Role, &str)]) -> Track {
        Track {
            path: format!("/m/{title}.flac").into(),
            title: title.into(),
            credits: credits
                .iter()
                .map(|&(role, name)| Credit {
                    role,
                    name: name.into(),
                    detail: None,
                })
                .collect(),
            ..Track::default()
        }
    }

    fn played(tracks: &mut [Track], plays: &[(usize, u32)]) -> Plays {
        let mut log = Plays::memory();
        for &(i, n) in plays {
            for _ in 0..n {
                log.record(&tracks[i], NOW).unwrap();
            }
        }
        log.apply(tracks);
        log
    }

    #[test]
    fn names_are_weighted_by_their_songs_plays_per_role() {
        let mut tracks = vec![
            song("A", &[(Role::Producer, "Kenny"), (Role::Engineer, "Ann")]),
            song("B", &[(Role::Producer, "Kenny")]),
            song("C", &[(Role::Producer, "Other")]),
        ];
        let plays = played(&mut tracks, &[(0, 2), (1, 1), (2, 1)]);
        let boards = boards(&tracks, &plays, Window::All, NOW);
        let producers = boards
            .iter()
            .find(|b| b.role == Role::Producer)
            .expect("producers");
        assert_eq!(
            producers.names,
            vec![
                Ranked {
                    item: "Kenny".into(),
                    plays: 3
                },
                Ranked {
                    item: "Other".into(),
                    plays: 1
                }
            ]
        );
        let engineers = boards
            .iter()
            .find(|b| b.role == Role::Engineer)
            .expect("engineers");
        assert_eq!(
            engineers.names,
            vec![Ranked {
                item: "Ann".into(),
                plays: 2
            }]
        );
    }

    #[test]
    fn roles_without_played_songs_are_left_out() {
        let mut tracks = vec![
            song("A", &[(Role::Mixer, "Mix")]),
            song("B", &[(Role::Composer, "Unplayed")]),
        ];
        let plays = played(&mut tracks, &[(0, 1)]);
        let boards = boards(&tracks, &plays, Window::All, NOW);
        assert_eq!(
            boards.iter().map(|b| b.role).collect::<Vec<_>>(),
            [Role::Mixer]
        );
    }

    #[test]
    fn a_window_counts_only_recent_plays() {
        let mut tracks = vec![
            song("A", &[(Role::Composer, "Old")]),
            song("B", &[(Role::Composer, "New")]),
        ];
        let mut log = Plays::memory();
        log.record(&tracks[0], NOW - 100 * 86_400).unwrap();
        log.record(&tracks[1], NOW - 86_400).unwrap();
        log.apply(&mut tracks);
        let week = boards(&tracks, &log, Window::Days7, NOW);
        assert_eq!(
            week[0].names,
            vec![Ranked {
                item: "New".into(),
                plays: 1
            }]
        );
        assert_eq!(boards(&tracks, &log, Window::All, NOW)[0].names.len(), 2);
    }

    #[test]
    fn a_name_counts_once_per_song_and_the_list_is_capped() {
        let mut tracks = vec![song(
            "A",
            &[(Role::Performer, "Twice"), (Role::Performer, "twice")],
        )];
        let plays = played(&mut tracks, &[(0, 1)]);
        assert_eq!(
            boards(&tracks, &plays, Window::All, NOW)[0].names,
            vec![Ranked {
                item: "Twice".into(),
                plays: 1
            }]
        );
        let many: Vec<Track> = (0..PEOPLE_COUNT + 3)
            .map(|n| song(&format!("S{n}"), &[(Role::Remixer, &format!("R{n:02}"))]))
            .collect();
        let mut many = many;
        let each_once: Vec<(usize, u32)> = (0..many.len()).map(|i| (i, 1)).collect();
        let plays = played(&mut many, &each_once);
        assert_eq!(
            boards(&many, &plays, Window::All, NOW)[0].names.len(),
            PEOPLE_COUNT
        );
    }

    #[test]
    fn no_plays_means_no_boards() {
        let tracks = vec![song("A", &[(Role::Composer, "X")])];
        assert!(boards(&tracks, &Plays::memory(), Window::All, NOW).is_empty());
    }
}

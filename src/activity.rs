//! When the library gets played: plays per local day, the days a streak runs over, and the grid the
//! home page draws. Days are counted from the Unix epoch in local time: a play's day is its time
//! plus the local UTC offset, divided into days. Everything here takes the offset and `now`, so it
//! can be tested with fixed values.

use std::collections::BTreeMap;

const DAY: i64 = 86_400;

/// Days shown in the heatmap, the last this many including today.
pub const WEEKS: i64 = 26;
/// Fewer active days than this, and the heatmap is replaced by a note saying so.
pub const MIN_ACTIVE_DAYS: usize = 3;

/// The local day (days since the epoch) a play at `at` seconds falls on, at `offset` seconds east
/// of UTC.
pub fn day_of(at: i64, offset: i64) -> i64 {
    (at + offset).div_euclid(DAY)
}

/// Plays per local day, from the times of every play.
pub fn per_day(times: impl IntoIterator<Item = i64>, offset: i64) -> BTreeMap<i64, u32> {
    let mut days = BTreeMap::new();
    for at in times {
        *days.entry(day_of(at, offset)).or_default() += 1;
    }
    days
}

/// The current and longest runs of consecutive active days. A run counts as current when it
/// reaches today, or yesterday (so a streak isn't lost before today's first play).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Streak {
    pub current: u32,
    pub longest: u32,
}

/// The streaks in `days` as of `today`.
pub fn streak(days: &BTreeMap<i64, u32>, today: i64) -> Streak {
    let mut longest = 0u32;
    let mut run = 0u32;
    let mut prev: Option<i64> = None;
    for &day in days.keys() {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        longest = longest.max(run);
        prev = Some(day);
    }
    // The current run ends at the last active day; it counts only if that day is today or
    // yesterday.
    let current = match days.keys().next_back() {
        Some(&last) if last == today || last == today - 1 => {
            let mut n = 0u32;
            let mut day = last;
            while days.contains_key(&day) {
                n += 1;
                day -= 1;
            }
            n
        }
        _ => 0,
    };
    Streak { current, longest }
}

/// One day of the heatmap: its day number and its plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub day: i64,
    pub plays: u32,
}

/// The heatmap's days: `WEEKS` weeks ending with today's week, oldest first. Weeks run Monday to
/// Sunday, so each column is one week; the grid's first column may start before the window.
/// Returns one column per week, each seven cells (Monday first), with `None` for days after
/// today or before the window's start.
pub fn grid(days: &BTreeMap<i64, u32>, today: i64) -> Vec<[Option<Cell>; 7]> {
    // Day 0 (1970-01-01) was a Thursday, so a Monday-first weekday is (day + 3) mod 7.
    let weekday = (today + 3).rem_euclid(7);
    let this_monday = today - weekday;
    let first_monday = this_monday - (WEEKS - 1) * 7;
    (0..WEEKS)
        .map(|w| {
            let monday = first_monday + w * 7;
            let mut column = [None; 7];
            for (d, slot) in column.iter_mut().enumerate() {
                let day = monday + d as i64;
                if day <= today && day >= first_monday {
                    *slot = Some(Cell {
                        day,
                        plays: days.get(&day).copied().unwrap_or(0),
                    });
                }
            }
            column
        })
        .collect()
}

/// The heatmap and the streaks, as of today.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub grid: Vec<[Option<Cell>; 7]>,
    pub streak: Streak,
    /// Days with at least one play, over the whole history.
    pub active_days: usize,
    /// The most plays on one day in the grid.
    pub busiest: u32,
}

impl Activity {
    /// Everything the home page's activity block shows, from the play times and the offset.
    pub fn of(times: impl IntoIterator<Item = i64>, offset: i64, now: i64) -> Activity {
        let days = per_day(times, offset);
        let today = day_of(now, offset);
        let grid = grid(&days, today);
        let busiest = grid
            .iter()
            .flatten()
            .flatten()
            .map(|c| c.plays)
            .max()
            .unwrap_or(0);
        Activity {
            streak: streak(&days, today),
            active_days: days.len(),
            busiest,
            grid,
        }
    }

    /// Whether there are enough active days for the heatmap to say anything.
    pub fn enough(&self) -> bool {
        self.active_days >= MIN_ACTIVE_DAYS
    }
}

/// How strong a day's cell is, 0 to 4: none, then quartiles of the busiest day.
pub fn intensity(plays: u32, busiest: u32) -> u8 {
    if plays == 0 || busiest == 0 {
        return 0;
    }
    let share = f64::from(plays) / f64::from(busiest);
    if share > 0.75 {
        4
    } else if share > 0.5 {
        3
    } else if share > 0.25 {
        2
    } else {
        1
    }
}

/// The UTC offset (seconds east) at `at`, read from the system's local time. On unix this asks
/// `localtime_r`; elsewhere it is UTC.
pub fn local_offset(at: i64) -> i64 {
    local_offset_impl(at)
}

#[cfg(unix)]
fn local_offset_impl(at: i64) -> i64 {
    let t = at as libc::time_t;
    // SAFETY: `localtime_r` writes only into `tm`, which lives for the call; both pointers are valid.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        // `tm_gmtoff` is an `i32` on some unixes and an `i64` on others.
        #[allow(clippy::useless_conversion)]
        let offset: i64 = tm.tm_gmtoff.into();
        offset
    }
}

#[cfg(not(unix))]
fn local_offset_impl(_at: i64) -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-24, a Tuesday, as days since the epoch.
    const TODAY: i64 = 20_781;
    const NOON: i64 = TODAY * DAY + 12 * 3600;

    fn days(list: &[i64]) -> BTreeMap<i64, u32> {
        list.iter().map(|&d| (d, 1)).collect()
    }

    #[test]
    fn a_play_falls_on_its_local_day() {
        // 23:30 UTC is already the next day at +01:00.
        let late = TODAY * DAY + 23 * 3600 + 1800;
        assert_eq!(day_of(late, 0), TODAY);
        assert_eq!(day_of(late, 3600), TODAY + 1);
        assert_eq!(day_of(late, -3600), TODAY);
        assert_eq!(day_of(-1, 0), -1, "before the epoch counts back a day");
    }

    #[test]
    fn plays_are_counted_per_local_day() {
        let times = [NOON, NOON + 60, NOON + DAY];
        let counts = per_day(times, 0);
        assert_eq!(counts.get(&TODAY), Some(&2));
        assert_eq!(counts.get(&(TODAY + 1)), Some(&1));
    }

    #[test]
    fn a_streak_runs_over_consecutive_days_and_ends_at_today_or_yesterday() {
        let running = days(&[TODAY - 4, TODAY - 3, TODAY - 2, TODAY - 1, TODAY]);
        assert_eq!(
            streak(&running, TODAY),
            Streak {
                current: 5,
                longest: 5
            }
        );
        let yesterday = days(&[TODAY - 3, TODAY - 2, TODAY - 1]);
        assert_eq!(
            streak(&yesterday, TODAY),
            Streak {
                current: 3,
                longest: 3
            },
            "still going until today's over"
        );
        let lapsed = days(&[TODAY - 5, TODAY - 4, TODAY - 3]);
        assert_eq!(
            streak(&lapsed, TODAY),
            Streak {
                current: 0,
                longest: 3
            },
            "lapsed: no current run"
        );
    }

    #[test]
    fn the_longest_run_can_be_an_old_one() {
        let gaps = days(&[10, 11, 12, 13, 20, 21, TODAY - 1, TODAY]);
        assert_eq!(
            streak(&gaps, TODAY),
            Streak {
                current: 2,
                longest: 4
            }
        );
        assert_eq!(streak(&BTreeMap::new(), TODAY), Streak::default());
    }

    #[test]
    fn the_grid_is_weeks_of_monday_first_columns_ending_today() {
        let counts = days(&[TODAY, TODAY - 1]);
        let weeks = grid(&counts, TODAY);
        assert_eq!(weeks.len(), WEEKS as usize);
        let last = weeks.last().expect("the last week");
        // 2026-11-24 is a Tuesday: Monday, then today, then the days after are empty.
        assert_eq!(last[0].map(|c| c.day), Some(TODAY - 1));
        assert_eq!(
            last[1],
            Some(Cell {
                day: TODAY,
                plays: 1
            })
        );
        assert_eq!(last[2], None, "tomorrow isn't shown");
        assert_eq!(last[1].map(|c| c.plays), Some(1));
        // Monday-first: a Monday is 0 under (day + 3) mod 7, as day 0 was a Thursday.
        let monday = |d: i64| (d + 3).rem_euclid(7) == 0;
        assert!(
            weeks.iter().all(|w| w[0].is_none_or(|c| monday(c.day))),
            "every column starts on a Monday"
        );
    }

    #[test]
    fn intensity_is_zero_for_no_plays_and_steps_up_to_four() {
        assert_eq!(intensity(0, 8), 0);
        assert_eq!(intensity(1, 8), 1);
        assert_eq!(intensity(3, 8), 2);
        assert_eq!(intensity(5, 8), 3);
        assert_eq!(intensity(8, 8), 4);
        assert_eq!(intensity(4, 0), 0, "no busiest day, nothing to shade");
    }

    #[test]
    fn the_local_offset_is_a_real_offset() {
        let offset = local_offset(NOON);
        assert!(offset.abs() <= 14 * 3600, "a plausible offset: {offset}");
    }
}

//! How long the playing song has really been heard: paused time and seeks don't count. A
//! play counts by Last.fm's rule, which both scrobbles and play counts follow.

/// Whether a play counts, by Last.fm's rule: the track is longer than 30 seconds and has played
/// for half its length or 4 minutes, whichever comes first. With no known length, only the
/// 4 minutes count (a track that played that long is longer than 30 seconds).
pub fn counts(duration: Option<f64>, played: f64) -> bool {
    const FOUR_MINUTES: f64 = 240.0;
    match duration {
        Some(d) => d > 30.0 && played >= (d / 2.0).min(FOUR_MINUTES),
        None => played >= FOUR_MINUTES,
    }
}

/// One play of a song, followed tick by tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Listen {
    duration: Option<f64>,
    played: f64,
    /// The last position seen while playing, and the clock then.
    last: Option<(f64, f64)>,
    counted: bool,
}

impl Listen {
    pub fn new(duration: Option<f64>) -> Listen {
        Listen { duration, played: 0.0, last: None, counted: false }
    }

    /// Every frame: counts the time played since the last tick (when the position moved no
    /// further than the clock did, so seeks don't count). True on the one tick the play
    /// starts to count.
    pub fn tick(&mut self, now: f64, position: f64, playing: bool) -> bool {
        if !playing {
            self.last = None;
            return false;
        }
        if let Some((p0, t0)) = self.last {
            let moved = position - p0;
            if moved > 0.0 && moved <= (now - t0) + 0.5 {
                self.played += moved;
            }
        }
        self.last = Some((position, now));
        if self.counted || !counts(self.duration, self.played) {
            return false;
        }
        self.counted = true;
        true
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> Result<i64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("The clock is before 1970 ({e})"))
        .and_then(|d| i64::try_from(d.as_secs()).map_err(|e| format!("The clock is out of range ({e})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays from `from` to `to` seconds, a second per tick, starting at clock `now`; how many
    /// ticks said the play counts.
    fn play(l: &mut Listen, now: &mut f64, from: f64, to: f64) -> usize {
        let mut p = from;
        let mut counted = 0;
        while p <= to {
            counted += usize::from(l.tick(*now, p, true));
            p += 1.0;
            *now += 1.0;
        }
        counted
    }

    #[test]
    fn the_rule() {
        assert!(!counts(Some(30.0), 30.0), "30 seconds is too short");
        assert!(!counts(Some(31.0), 15.0));
        assert!(counts(Some(31.0), 15.5));
        assert!(!counts(Some(600.0), 239.0));
        assert!(counts(Some(600.0), 240.0), "4 minutes count before half of 10");
        assert!(!counts(None, 200.0));
        assert!(counts(None, 240.0));
    }

    #[test]
    fn counts_once_at_half_the_song() {
        let mut l = Listen::new(Some(100.0));
        let mut now = 0.0;
        assert_eq!(play(&mut l, &mut now, 0.0, 49.0), 0, "49 seconds of 100 isn't half");
        assert_eq!(play(&mut l, &mut now, 50.0, 99.0), 1, "counted once");
    }

    #[test]
    fn seeks_and_pauses_dont_count() {
        let mut l = Listen::new(Some(100.0));
        let mut now = 0.0;
        play(&mut l, &mut now, 0.0, 10.0);
        assert_eq!(play(&mut l, &mut now, 80.0, 99.0), 0, "30 seconds heard of 100");
        let mut l = Listen::new(Some(100.0));
        for t in 0..200 {
            assert!(!l.tick(f64::from(t), 5.0, false));
        }
    }
}

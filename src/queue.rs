//! Play order: the list a track was started from, optionally shuffled, and repeat modes.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn cycle(self) -> Repeat {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

/// xorshift64*: enough randomness for a shuffle, with no dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn from_time() -> Rng {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64);
        Rng::new(nanos)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Fisher–Yates.
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Library indices in play order, and where playback is in them.
#[derive(Clone, Debug, PartialEq)]
pub struct Queue {
    /// The list as it was when playback started, unshuffled.
    base: Vec<usize>,
    order: Vec<usize>,
    pos: usize,
}

impl Queue {
    /// A queue over `list` starting at `start`, which must be in it. Shuffled, `start` plays
    /// first and the rest follow in random order.
    pub fn new(list: Vec<usize>, start: usize, shuffle: bool, rng: &mut Rng) -> Queue {
        let pos = list.iter().position(|&i| i == start).expect("the started track is in its list");
        let mut q = Queue { order: list.clone(), base: list, pos };
        if shuffle {
            q.shuffle(rng);
        }
        q
    }

    pub fn current(&self) -> usize {
        self.order[self.pos]
    }


    /// Move to the next track; at the end, wrap with `Repeat::All` and stop otherwise.
    /// `Repeat::One` is the caller's: a track ending replays it, a press of Next still moves on.
    pub fn next(&mut self, repeat: Repeat) -> Option<usize> {
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
        } else if repeat == Repeat::All {
            self.pos = 0;
        } else {
            return None;
        }
        Some(self.current())
    }

    pub fn prev(&mut self, repeat: Repeat) -> Option<usize> {
        if self.pos > 0 {
            self.pos -= 1;
        } else if repeat == Repeat::All {
            self.pos = self.order.len() - 1;
        } else {
            return None;
        }
        Some(self.current())
    }

    /// Shuffle what's left around the current track, which moves to the front.
    pub fn shuffle(&mut self, rng: &mut Rng) {
        let current = self.current();
        let mut rest: Vec<usize> = self.base.iter().copied().filter(|&i| i != current).collect();
        rng.shuffle(&mut rest);
        self.order = std::iter::once(current).chain(rest).collect();
        self.pos = 0;
    }

    /// Back to the list's own order, at the current track.
    pub fn unshuffle(&mut self) {
        let current = self.current();
        self.order = self.base.clone();
        self.pos = self.order.iter().position(|&i| i == current).expect("the current track is in the base list");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_order_it_stops_at_the_end_unless_repeating_all() {
        let mut rng = Rng::new(1);
        let mut q = Queue::new(vec![10, 11, 12], 11, false, &mut rng);
        assert_eq!(q.current(), 11);
        assert_eq!(q.next(Repeat::Off), Some(12));
        assert_eq!(q.next(Repeat::Off), None);
        assert_eq!(q.current(), 12, "stopping at the end keeps the last track current");
        assert_eq!(q.next(Repeat::All), Some(10));
        assert_eq!(q.prev(Repeat::Off), None);
        assert_eq!(q.prev(Repeat::All), Some(12));
    }

    #[test]
    fn shuffle_plays_the_started_track_first_and_every_track_once() {
        let mut rng = Rng::new(42);
        let list: Vec<usize> = (0..50).collect();
        let mut q = Queue::new(list.clone(), 17, true, &mut rng);
        assert_eq!(q.current(), 17);
        let mut seen = vec![q.current()];
        while let Some(i) = q.next(Repeat::Off) {
            seen.push(i);
        }
        assert_ne!(&seen[1..], (0..50).filter(|&i| i != 17).collect::<Vec<_>>(), "the rest is shuffled");
        seen.sort();
        assert_eq!(seen, list);
    }

    #[test]
    fn unshuffling_returns_to_list_order_at_the_current_track() {
        let mut rng = Rng::new(7);
        let mut q = Queue::new((0..20).collect(), 3, true, &mut rng);
        q.next(Repeat::Off);
        let now = q.current();
        q.unshuffle();
        assert_eq!(q.current(), now);
        assert_eq!(q.next(Repeat::Off), (now + 1 < 20).then_some(now + 1));
    }

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(Repeat::Off.cycle(), Repeat::All);
        assert_eq!(Repeat::All.cycle(), Repeat::One);
        assert_eq!(Repeat::One.cycle(), Repeat::Off);
    }
}

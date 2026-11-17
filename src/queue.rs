//! Play order: the list a track was started from, optionally shuffled, edited by the user, and
//! repeat modes.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

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

/// What a new list does when it starts over a queue the user has edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueReplace {
    /// Ask first. The default when unset.
    #[default]
    Ask,
    /// Drop the upcoming tracks, added ones included.
    Replace,
    /// Keep the added tracks, right after the new list's current track.
    KeepAdded,
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

    /// A number in `0..n` (`n` > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// Fisher–Yates.
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// One place in the queue, and why it's there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Library index of the track.
    pub track: usize,
    /// Put there by Play Next or Play After rather than by the list.
    pub added: bool,
    /// Kept in its place when shuffling: added, or moved by the user.
    pub pinned: bool,
}

impl Entry {
    fn from_list(track: usize) -> Entry {
        Entry { track, added: false, pinned: false }
    }

    fn added(track: usize) -> Entry {
        Entry { track, added: true, pinned: true }
    }
}

/// Library indices in play order, and where playback is in them.
#[derive(Clone, Debug, PartialEq)]
pub struct Queue {
    /// The list as it was when playback started, unshuffled. Added tracks aren't in it.
    base: Vec<usize>,
    /// The played entries, then the current one at `pos`, then the upcoming ones.
    order: Vec<Entry>,
    pos: usize,
    /// Tracks the user removed. Shuffling and unshuffling don't bring them back.
    removed: HashSet<usize>,
    /// Whether anything was added, removed or moved since the list started.
    edited: bool,
}

impl Queue {
    /// A queue over `list` starting at `start`. If `start` isn't in `list`, it is inserted at the
    /// front so a stale list can't panic playback. Shuffled, `start` plays first and the rest
    /// follow in random order.
    pub fn new(mut list: Vec<usize>, start: usize, shuffle: bool, rng: &mut Rng) -> Queue {
        let pos = match list.iter().position(|&i| i == start) {
            Some(pos) => pos,
            None => {
                list.insert(0, start);
                0
            }
        };
        if shuffle {
            // Nothing has played yet, so the started track is first and the rest are shuffled.
            let mut rest: Vec<usize> = list.iter().copied().filter(|&i| i != start).collect();
            rng.shuffle(&mut rest);
            let order = std::iter::once(start).chain(rest).map(Entry::from_list).collect();
            return Queue::fresh(list, order, 0);
        }
        let order = list.iter().map(|&t| Entry::from_list(t)).collect();
        Queue::fresh(list, order, pos)
    }

    /// A queue with nothing edited or removed yet.
    fn fresh(base: Vec<usize>, order: Vec<Entry>, pos: usize) -> Queue {
        Queue { base, order, pos, removed: HashSet::new(), edited: false }
    }

    /// A queue from saved parts. None when `pos` isn't an entry.
    pub fn from_parts(base: Vec<usize>, order: Vec<Entry>, pos: usize, edited: bool, removed: HashSet<usize>) -> Option<Queue> {
        (pos < order.len()).then_some(Queue { base, order, pos, removed, edited })
    }

    pub fn current(&self) -> usize {
        self.order[self.pos].track
    }

    /// Every entry, played ones first.
    pub fn entries(&self) -> &[Entry] {
        &self.order
    }

    /// Where the current entry is among `entries`.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The entries after the current one.
    pub fn upcoming(&self) -> &[Entry] {
        &self.order[self.pos + 1..]
    }

    /// The list as it was when playback started.
    pub fn base(&self) -> &[usize] {
        &self.base
    }

    pub fn removed(&self) -> &HashSet<usize> {
        &self.removed
    }

    pub fn edited(&self) -> bool {
        self.edited
    }

    /// The queue with every index mapped by `map` (e.g. onto a rebuilt library). Entries that
    /// map to nothing are dropped; order, flags, shuffle and position are kept. `None` when the
    /// current track doesn't map.
    pub fn remap(&self, map: impl Fn(usize) -> Option<usize>) -> Option<Queue> {
        let base: Vec<usize> = self.base.iter().filter_map(|&i| map(i)).collect();
        let removed: HashSet<usize> = self.removed.iter().filter_map(|&i| map(i)).collect();
        let mut order = Vec::with_capacity(self.order.len());
        let mut pos = None;
        for (n, e) in self.order.iter().enumerate() {
            let Some(track) = map(e.track) else { continue };
            if n == self.pos {
                pos = Some(order.len());
            }
            order.push(Entry { track, ..*e });
        }
        Some(Queue { base, order, pos: pos?, removed, edited: self.edited })
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

    /// Inserts `tracks` right after the current track, in this order.
    pub fn play_next(&mut self, tracks: &[usize]) {
        if tracks.is_empty() {
            return;
        }
        let tail = self.order.split_off(self.pos + 1);
        self.order.extend(tracks.iter().map(|&t| Entry::added(t)));
        self.order.extend(tail);
        self.edited = true;
    }

    /// Appends `tracks` at the very end of the order: the end of the shuffled order when shuffled.
    pub fn play_after(&mut self, tracks: &[usize]) {
        if tracks.is_empty() {
            return;
        }
        self.order.extend(tracks.iter().map(|&t| Entry::added(t)));
        self.edited = true;
    }

    /// Removes entry `index`, played or upcoming. The current entry can't be removed: false
    /// when `index` is that or isn't an entry.
    pub fn remove(&mut self, index: usize) -> bool {
        if index == self.pos || index >= self.order.len() {
            return false;
        }
        let entry = self.order.remove(index);
        if index < self.pos {
            self.pos -= 1;
        }
        self.removed.insert(entry.track);
        self.edited = true;
        true
    }

    /// Moves upcoming entry `from` to upcoming slot `to` (both indices into `entries`), and pins
    /// it. Only upcoming entries move: false when either isn't upcoming or nothing changes.
    pub fn move_entry(&mut self, from: usize, to: usize) -> bool {
        let upcoming = self.pos + 1..self.order.len();
        if from == to || !upcoming.contains(&from) || !upcoming.contains(&to) {
            return false;
        }
        let mut entry = self.order.remove(from);
        entry.pinned = true;
        self.order.insert(to, entry);
        self.edited = true;
        true
    }

    /// Makes entry `index` the current one; everything before it becomes history. Its track,
    /// or None when there is no such entry.
    pub fn jump(&mut self, index: usize) -> Option<usize> {
        let track = self.order.get(index)?.track;
        self.pos = index;
        Some(track)
    }

    /// Shuffles the upcoming tracks. Pinned entries keep their slots; the other slots take the
    /// tracks that were in them, in random order. History, the current track and removed tracks
    /// are untouched.
    pub fn shuffle(&mut self, rng: &mut Rng) {
        let mut pool: Vec<usize> = self.upcoming().iter().filter(|e| !e.pinned).map(|e| e.track).collect();
        rng.shuffle(&mut pool);
        let upcoming = self.order.split_off(self.pos + 1);
        self.order.extend(refill(&upcoming, pool));
    }

    /// Back to the list's own order. From a list track, the history is the list before it and
    /// the upcoming tracks are the list after it; pinned entries keep their slots. From an added
    /// track, the history stays and the list's unplayed tracks follow in list order.
    pub fn unshuffle(&mut self) {
        let current = self.current();
        let Some(k) = self.base.iter().position(|&t| t == current) else {
            let taken = self.taken();
            let pool: Vec<usize> = self.base.iter().copied().filter(|t| !taken.contains(t)).collect();
            let upcoming = self.order.split_off(self.pos + 1);
            self.order.extend(refill(&upcoming, pool));
            return;
        };
        let pinned: HashSet<usize> = self.upcoming().iter().filter(|e| e.pinned).map(|e| e.track).collect();
        let skip = |t: usize| pinned.contains(&t) || self.removed.contains(&t);
        let history: Vec<Entry> = self.base[..k].iter().copied().filter(|&t| !skip(t)).map(Entry::from_list).collect();
        let pool: Vec<usize> = self.base[k + 1..].iter().copied().filter(|&t| !skip(t)).collect();
        let upcoming = self.order.split_off(self.pos + 1);
        let pos = history.len();
        let mut order = history;
        order.push(self.order[self.pos]);
        order.extend(refill(&upcoming, pool));
        self.order = order;
        self.pos = pos;
    }

    /// The new queue `new`, with this queue's upcoming added tracks put right after its current
    /// track, in order. `new` itself when there are none.
    pub fn replace_keeping_added(&self, mut new: Queue) -> Queue {
        let added: Vec<Entry> = self.upcoming().iter().filter(|e| e.added).copied().collect();
        if added.is_empty() {
            return new;
        }
        let tail = new.order.split_off(new.pos + 1);
        new.order.extend(added);
        new.order.extend(tail);
        new.edited = true;
        new
    }

    /// Tracks that are played, current, pinned or removed: none of them is shuffled back in.
    fn taken(&self) -> HashSet<usize> {
        self.order[..=self.pos]
            .iter()
            .map(|e| e.track)
            .chain(self.upcoming().iter().filter(|e| e.pinned).map(|e| e.track))
            .chain(self.removed.iter().copied())
            .collect()
    }
}

/// `slots` with their unpinned entries replaced by `pool` in order; pinned entries keep their
/// place. Tracks left in `pool` are appended, and a slot is dropped when `pool` runs out.
fn refill(slots: &[Entry], pool: Vec<usize>) -> Vec<Entry> {
    let mut pool = pool.into_iter();
    let mut out = Vec::with_capacity(slots.len());
    for e in slots {
        if e.pinned {
            out.push(*e);
        } else if let Some(track) = pool.next() {
            out.push(Entry::from_list(track));
        }
    }
    out.extend(pool.map(Entry::from_list));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(q: &Queue) -> Vec<usize> {
        q.entries().iter().map(|e| e.track).collect()
    }

    fn queue(list: Vec<usize>, start: usize) -> Queue {
        Queue::new(list, start, false, &mut Rng::new(1))
    }

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
    fn a_start_missing_from_the_list_is_put_at_the_front() {
        let mut rng = Rng::new(3);
        let mut q = Queue::new(vec![1, 2], 9, false, &mut rng);
        assert_eq!(q.current(), 9);
        assert_eq!(q.next(Repeat::Off), Some(1));
        assert_eq!(q.prev(Repeat::Off), Some(9));
    }

    #[test]
    fn remap_keeps_order_shuffle_and_position_and_drops_missing_tracks() {
        let mut rng = Rng::new(3);
        let q = Queue::new(vec![5, 6, 7, 8, 9], 7, true, &mut rng);
        let before = q.current();
        // Track 8 left the library and the rest shifted down by five.
        let r = q.remap(|i| (i != 8).then(|| i - 5)).expect("the current track maps");
        assert_eq!(r.current(), before - 5);
        let expected: Vec<usize> = tracks(&q).into_iter().filter(|&i| i != 8).map(|i| i - 5).collect();
        assert_eq!(tracks(&r), expected);
        assert_eq!(r.base, vec![0, 1, 2, 4]);
    }

    #[test]
    fn remap_is_none_when_the_current_track_is_gone() {
        let mut rng = Rng::new(3);
        let q = Queue::new(vec![1, 2, 3], 2, false, &mut rng);
        assert_eq!(q.remap(|i| (i != 2).then_some(i)), None);
    }

    #[test]
    fn remap_carries_the_flags_and_the_edit() {
        let mut q = queue((0..5).collect(), 2);
        q.play_next(&[9]);
        let r = q.remap(|i| (i != 1).then_some(i)).expect("the current track maps");
        assert_eq!(tracks(&r), vec![0, 2, 9, 3, 4]);
        assert_eq!(r.pos(), 1);
        assert!(r.entries()[2].added && r.entries()[2].pinned);
        assert!(r.edited());
        assert_eq!(r.base(), &[0, 2, 3, 4]);
    }

    #[test]
    fn play_next_puts_the_tracks_after_the_current_one_in_order() {
        let mut q = queue((0..4).collect(), 1);
        q.play_next(&[10, 11]);
        assert_eq!(tracks(&q), vec![0, 1, 10, 11, 2, 3]);
        assert_eq!(q.current(), 1);
        assert!(q.entries()[2].added && q.entries()[2].pinned);
        assert!(q.entries()[3].added && q.entries()[3].pinned);
        assert!(!q.entries()[4].added && !q.entries()[4].pinned);
        assert!(q.edited());
    }

    #[test]
    fn play_next_of_nothing_is_no_edit() {
        let mut q = queue((0..4).collect(), 1);
        q.play_next(&[]);
        assert!(!q.edited());
        assert_eq!(tracks(&q), vec![0, 1, 2, 3]);
    }

    #[test]
    fn play_after_appends_at_the_very_end_of_the_shuffled_order() {
        let mut q = Queue::new((0..6).collect(), 0, true, &mut Rng::new(5));
        q.play_after(&[20, 21]);
        assert_eq!(tracks(&q)[6..], [20, 21]);
        assert!(q.entries()[6].added && q.entries()[6].pinned);
        assert!(q.edited());
    }

    #[test]
    fn remove_takes_out_history_or_upcoming_but_not_the_current_track() {
        let mut q = queue((0..4).collect(), 0);
        q.next(Repeat::Off);
        q.next(Repeat::Off);
        assert!(!q.remove(2), "the current track stays");
        assert!(q.remove(0), "history can go");
        assert_eq!(q.pos(), 1);
        assert_eq!(q.current(), 2);
        assert!(q.remove(2), "so can upcoming tracks");
        assert_eq!(tracks(&q), vec![1, 2]);
        assert!(!q.remove(9), "not an entry");
        assert!(q.edited());
    }

    #[test]
    fn move_reorders_upcoming_entries_only_and_pins_the_moved_one() {
        let mut q = queue((0..5).collect(), 0);
        q.next(Repeat::Off);
        assert!(!q.move_entry(0, 3), "history doesn't move");
        assert!(!q.move_entry(2, 9), "out of range");
        assert!(!q.move_entry(2, 2), "nothing to move");
        assert!(q.move_entry(4, 2));
        assert_eq!(tracks(&q), vec![0, 1, 4, 2, 3]);
        assert!(q.entries()[2].pinned);
        assert!(!q.entries()[3].pinned);
        assert!(q.edited());
    }

    #[test]
    fn jump_makes_an_entry_current_with_everything_before_it_as_history() {
        let mut q = queue((0..5).collect(), 0);
        assert_eq!(q.jump(3), Some(3));
        assert_eq!((q.pos(), q.current()), (3, 3));
        assert_eq!(q.prev(Repeat::Off), Some(2));
        assert_eq!(q.jump(9), None);
        assert!(!q.edited(), "jumping isn't an edit");
    }

    #[test]
    fn shuffle_keeps_history_the_current_track_and_pinned_slots() {
        let mut q = queue((0..10).collect(), 0);
        for _ in 0..3 {
            q.next(Repeat::Off);
        }
        q.play_next(&[50]);
        assert!(q.move_entry(10, 6));
        assert_eq!(tracks(&q), vec![0, 1, 2, 3, 50, 4, 9, 5, 6, 7, 8]);
        let before = tracks(&q);
        q.shuffle(&mut Rng::new(9));
        let after = tracks(&q);
        assert_eq!(&after[..4], &[0, 1, 2, 3], "history and the current track stay");
        assert_eq!(q.pos(), 3);
        assert_eq!((after[4], after[6]), (50, 9), "pinned entries keep their slots");
        assert!(q.entries()[4].pinned && q.entries()[6].pinned);
        let mut all = after.clone();
        all.sort();
        assert_eq!(all, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 50]);
        assert_ne!(after, before, "the other upcoming tracks are shuffled");
    }

    #[test]
    fn shuffling_and_unshuffling_do_not_bring_back_removed_tracks() {
        let mut q = queue((0..6).collect(), 0);
        assert!(q.remove(4));
        q.shuffle(&mut Rng::new(2));
        assert!(!tracks(&q).contains(&4));
        q.unshuffle();
        assert_eq!(tracks(&q), vec![0, 1, 2, 3, 5]);
    }

    #[test]
    fn unshuffling_from_a_list_track_puts_the_list_after_it() {
        let mut q = queue((0..6).collect(), 0);
        q.shuffle(&mut Rng::new(4));
        q.next(Repeat::Off);
        q.next(Repeat::Off);
        let now = q.current();
        q.unshuffle();
        assert_eq!(q.current(), now);
        assert_eq!(tracks(&q), (0..6).collect::<Vec<_>>());
        assert_eq!(q.pos(), now);
    }

    #[test]
    fn unshuffling_keeps_pinned_entries_at_their_offsets_after_the_current_track() {
        let mut q = queue((0..5).collect(), 0);
        q.next(Repeat::Off);
        q.play_next(&[50]);
        q.unshuffle();
        assert_eq!(tracks(&q), vec![0, 1, 50, 2, 3, 4]);
        assert_eq!(q.pos(), 1);
    }

    #[test]
    fn unshuffling_from_an_added_track_keeps_history_and_fills_in_list_order() {
        let mut q = Queue::new((0..4).collect(), 0, true, &mut Rng::new(3));
        let first = q.current();
        q.play_next(&[9]);
        q.next(Repeat::Off);
        assert_eq!(q.current(), 9);
        q.unshuffle();
        let rest: Vec<usize> = (0..4).filter(|&t| t != first).collect();
        assert_eq!(tracks(&q), [vec![first, 9], rest].concat());
        assert_eq!(q.current(), 9);
        assert!(q.entries()[1].added);
    }

    #[test]
    fn replace_keeping_added_puts_the_added_tracks_after_the_new_current() {
        let mut old = queue((0..3).collect(), 0);
        old.play_next(&[7, 8]);
        let kept = old.replace_keeping_added(queue(vec![20, 21, 22], 21));
        assert_eq!(tracks(&kept), vec![20, 21, 7, 8, 22]);
        assert_eq!(kept.current(), 21);
        assert!(kept.entries()[2].added && kept.entries()[3].added);
        assert!(kept.edited());
    }

    #[test]
    fn replace_keeping_added_without_added_upcoming_is_the_new_queue() {
        let old = queue((0..3).collect(), 0);
        let new = queue(vec![20, 21], 20);
        assert_eq!(old.replace_keeping_added(new.clone()), new);
    }

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(Repeat::Off.cycle(), Repeat::All);
        assert_eq!(Repeat::All.cycle(), Repeat::One);
        assert_eq!(Repeat::One.cycle(), Repeat::Off);
    }
}

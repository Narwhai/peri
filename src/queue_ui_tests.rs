//! The play queue through the app: the replace question for an edited queue, the remembered
//! answer, and the queue saved across launches.

use std::path::PathBuf;

use rune::{Env, Runtime};

use crate::app::{App, Output, QueueChoice};
use crate::query::store::Store;
use crate::queue::QueueReplace;
use crate::view::view;

/// A folder for the saved files, removed after the test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("peri-queue-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An app over `store` with `tracks` as its library, as a first launch scans them.
fn runtime(store: Store, tracks: Vec<crate::library::Track>) -> Runtime<App> {
    let app = App::new(Output::Silent, store);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.update(move |s, _| s.set_library(tracks, false));
    rt.frame(0.0);
    rt
}

/// The library index of the demo song titled `title`.
fn idx(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.tracks().iter().position(|t| t.title == title).unwrap_or_else(|| panic!("no song {title}"))
}

fn all(rt: &Runtime<App>) -> Vec<usize> {
    (0..rt.state.tracks().len()).collect()
}

/// The queue's entries as (title, added, pinned).
fn entries(rt: &Runtime<App>) -> Vec<(String, bool, bool)> {
    let queue = rt.state.queue.as_ref().expect("a queue");
    queue.entries().iter().map(|e| (rt.state.tracks()[e.track].title.clone(), e.added, e.pinned)).collect()
}

/// The entries from the current one on: what plays next, the current track first.
fn from_current(rt: &Runtime<App>) -> Vec<(String, bool, bool)> {
    let pos = rt.state.queue.as_ref().map_or(0, |q| q.pos());
    entries(rt).into_iter().skip(pos).collect()
}

fn current_title(rt: &Runtime<App>) -> String {
    rt.state.current_track().expect("a track is loaded").title.clone()
}

/// A started list over the whole library, with ETA queued right after its first track.
fn edited(rt: &mut Runtime<App>) {
    let list = all(rt);
    let eta = idx(rt, "ETA");
    rt.update(move |s, tx| s.play_list(list, 0, tx));
    rt.update(move |s, tx| s.queue_play_next(vec![eta], tx));
}

#[test]
fn a_new_list_over_an_edited_queue_waits_for_the_answer() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    edited(&mut rt);
    let before = entries(&rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    assert!(rt.state.pending_play.is_some(), "the question is pending");
    assert_eq!(entries(&rt), before, "nothing changed yet");
    assert_ne!(current_title(&rt), "Says", "playback didn't start the new list");
    assert!(rt.state.pending_has_added());
}

#[test]
fn a_new_list_over_an_unedited_queue_replaces_it_at_once() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    let list = all(&rt);
    rt.update(move |s, tx| s.play_list(list, 0, tx));
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    assert!(rt.state.pending_play.is_none());
    assert_eq!(current_title(&rt), "Says");
}

#[test]
fn an_edited_queue_with_nothing_upcoming_is_replaced_at_once() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    let (first, second) = (idx(&rt, "New Jeans"), idx(&rt, "Super Shy"));
    let eta = idx(&rt, "ETA");
    rt.update(move |s, tx| s.play_list(vec![first, second], first, tx));
    rt.update(move |s, tx| s.queue_play_next(vec![eta], tx));
    // Remove the upcoming ETA and Super Shy, leaving nothing after the current track.
    rt.update(|s, _| s.queue_remove(1));
    rt.update(|s, _| s.queue_remove(1));
    assert!(rt.state.queue.as_ref().is_some_and(|q| q.edited() && q.upcoming().is_empty()));
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    assert!(rt.state.pending_play.is_none());
    assert_eq!(current_title(&rt), "Says");
}

#[test]
fn replace_drops_the_added_tracks_and_asks_again_next_time() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    edited(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    rt.update(|s, tx| s.resolve_pending_play(QueueChoice::Replace, false, tx));
    assert!(rt.state.pending_play.is_none());
    assert_eq!(current_title(&rt), "Says");
    assert!(!rt.state.pending_has_added());
    assert_eq!(rt.state.queue_replace, QueueReplace::Ask, "not remembered");
}

#[test]
fn keep_added_puts_the_added_tracks_after_the_new_current_track() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    edited(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    rt.update(|s, tx| s.resolve_pending_play(QueueChoice::KeepAdded, false, tx));
    let got = from_current(&rt);
    assert_eq!(got[0].0, "Says");
    assert_eq!(got[1], ("ETA".to_string(), true, true), "the added track follows the new current one");
    assert_eq!(rt.state.queue_replace, QueueReplace::Ask);
}

#[test]
fn cancel_changes_nothing() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    edited(&mut rt);
    let before = entries(&rt);
    let current = rt.state.current;
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    rt.update(|s, tx| s.resolve_pending_play(QueueChoice::Cancel, true, tx));
    assert!(rt.state.pending_play.is_none());
    assert_eq!(entries(&rt), before);
    assert_eq!(rt.state.current, current);
    assert_eq!(rt.state.queue_replace, QueueReplace::Ask, "cancel doesn't remember");
}

#[test]
fn a_remembered_answer_is_saved_and_applied_at_the_next_launch() {
    let dir = Dir::new("policy");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()), crate::demo_tracks());
    edited(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    rt.update(|s, tx| s.resolve_pending_play(QueueChoice::KeepAdded, true, tx));
    assert_eq!(rt.state.queue_replace, QueueReplace::KeepAdded);

    let mut rt = runtime(Store::at(prefs), crate::demo_tracks());
    assert_eq!(rt.state.queue_replace, QueueReplace::KeepAdded, "saved and reloaded");
    edited(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    assert!(rt.state.pending_play.is_none(), "the remembered answer applies without asking");
    assert_eq!(from_current(&rt)[1].0, "ETA");
}

#[test]
fn an_unreadable_policy_is_an_error_and_asks() {
    let dir = Dir::new("bad-policy");
    std::fs::create_dir_all(&dir.0).expect("a folder");
    let prefs = dir.0.join("prefs.json");
    std::fs::write(&prefs, r#"{"queue_replace": "Sometimes"}"#).expect("a file");
    let rt = runtime(Store::at(prefs), crate::demo_tracks());
    assert_eq!(rt.state.queue_replace, QueueReplace::Ask);
    assert!(rt.state.error.is_some(), "the bad setting is reported");
}

#[test]
fn the_queue_is_saved_and_restored_paused_at_its_place() {
    let dir = Dir::new("round-trip");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()), crate::demo_tracks());
    let list = all(&rt);
    let eta = idx(&rt, "ETA");
    rt.update(move |s, tx| {
        s.shuffle = true;
        s.play_list(list, 0, tx);
    });
    rt.update(|s, tx| s.next(tx));
    rt.update(|s, tx| s.next(tx));
    rt.update(move |s, tx| s.queue_play_next(vec![eta], tx));
    rt.update(|s, _| s.seek(37.0));
    rt.update(|s, tx| s.toggle(tx));
    assert!(!rt.state.playing);

    let expected = entries(&rt);
    let (pos, current) = (rt.state.queue.as_ref().map(|q| q.pos()), current_title(&rt));
    assert!(dir.0.join("queue.json").exists(), "the queue is saved beside the preferences");

    let mut rt = runtime(Store::at(prefs), crate::demo_tracks());
    assert_eq!(entries(&rt), expected, "the tracks, their flags and the order");
    assert_eq!(rt.state.queue.as_ref().map(|q| q.pos()), pos);
    assert_eq!(current_title(&rt), current);
    assert!(rt.state.shuffle, "the shuffle setting");
    assert!(!rt.state.playing, "restored paused");
    assert!((rt.state.position - 37.0).abs() < 1e-9, "at the saved second: {}", rt.state.position);

    rt.update(|s, tx| s.toggle(tx));
    assert!(rt.state.playing, "Play starts the restored track");
    assert!((rt.state.position - 37.0).abs() < 1e-9, "from the saved second");
}

#[test]
fn a_restored_queue_drops_tracks_that_left_the_library() {
    let dir = Dir::new("left-library");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()), crate::demo_tracks());
    edited(&mut rt);
    rt.update(|s, tx| s.toggle(tx));
    let current = current_title(&rt);
    let expected: Vec<(String, bool, bool)> = entries(&rt).into_iter().filter(|(t, _, _)| t != "ETA").collect();

    let mut library = crate::demo_tracks();
    library.retain(|t| t.title != "ETA");
    let rt = runtime(Store::at(prefs), library);
    assert_eq!(entries(&rt), expected);
    assert_eq!(current_title(&rt), current);
}

#[test]
fn a_saved_queue_whose_current_track_left_the_library_is_not_restored() {
    let dir = Dir::new("left-current");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()), crate::demo_tracks());
    edited(&mut rt);
    rt.update(|s, tx| s.toggle(tx));
    let current = current_title(&rt);

    let mut library = crate::demo_tracks();
    library.retain(|t| t.title != current);
    let rt = runtime(Store::at(prefs), library);
    assert!(rt.state.queue.is_none());
    assert!(rt.state.current.is_none());
}

#[test]
fn stop_clears_the_saved_queue() {
    let dir = Dir::new("stop");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()), crate::demo_tracks());
    edited(&mut rt);
    assert!(dir.0.join("queue.json").exists());
    rt.update(|s, _| s.stop());
    assert!(!dir.0.join("queue.json").exists());
    let rt = runtime(Store::at(prefs), crate::demo_tracks());
    assert!(rt.state.queue.is_none());
}

#[test]
fn an_unreadable_saved_queue_is_an_error_and_restores_nothing() {
    let dir = Dir::new("bad-queue");
    std::fs::create_dir_all(&dir.0).expect("a folder");
    let prefs = dir.0.join("prefs.json");
    std::fs::write(dir.0.join("queue.json"), "not a queue").expect("a file");
    let rt = runtime(Store::at(prefs), crate::demo_tracks());
    assert!(rt.state.error.is_some(), "the unreadable queue is reported");
    assert!(rt.state.queue.is_none());
}

#[test]
fn a_memory_store_saves_no_queue_file() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    edited(&mut rt);
    rt.update(|s, _| s.stop());
    assert!(rt.state.error.is_none());
}


#[test]
fn the_queue_edits_play_next_after_remove_move_and_jump() {
    let mut rt = runtime(Store::memory(), crate::demo_tracks());
    let (a, b, c, d) = (idx(&rt, "New Jeans"), idx(&rt, "Super Shy"), idx(&rt, "ETA"), idx(&rt, "Says"));
    rt.update(move |s, tx| s.play_list(vec![a, b], a, tx));
    rt.update(move |s, tx| s.queue_play_next(vec![c], tx));
    rt.update(move |s, tx| s.queue_play_after(vec![d], tx));
    assert_eq!(from_current(&rt).iter().map(|e| e.0.as_str()).collect::<Vec<_>>(), ["New Jeans", "ETA", "Super Shy", "Says"]);
    rt.update(|s, _| s.queue_move(3, 2));
    assert_eq!(from_current(&rt)[2].0, "Says", "the upcoming entries reorder");
    rt.update(|s, _| s.queue_remove(2));
    assert_eq!(from_current(&rt).len(), 3);
    rt.update(|s, _| s.queue_remove(0));
    assert_eq!(current_title(&rt), "New Jeans", "the playing track can't be removed");
    rt.update(|s, tx| s.queue_jump(1, tx));
    assert_eq!(current_title(&rt), "ETA", "jump plays the entry");
    assert!(rt.state.playing);
}

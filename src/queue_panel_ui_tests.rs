//! The queue panel driven headlessly: opening and closing it, pinning and the saved pin, the rows
//! and their marks, jumping, removing, reordering, the initial scroll, the replace prompt and the
//! Settings choice.

use std::path::PathBuf;

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output};
use crate::query::store::Store;
use crate::queue::{QueueReplace, Repeat};
use crate::view::view;

/// A folder for the saved files, removed after the test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("peri-panel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn runtime(store: Store) -> Runtime<App> {
    let mut app = App::new(Output::Silent, store);
    app.set_library(crate::demo_tracks(), false);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn double_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
    }
    settle(rt);
}

/// Drags the element keyed `key` by `dy` points, in steps as a hand moves.
fn drag_down(rt: &mut Runtime<App>, key: &str, dy: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    for step in 1..=4 {
        rt.pointer_move(x, y + dy * step as f32 / 4.0);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x, y + dy);
    settle(rt);
}

/// Scrolls `key` into view inside the scroll view `scroller`.
fn reveal(rt: &mut Runtime<App>, scroller: &str, key: &str) {
    rt.update(|_, tx| tx.scroll_into_view_in(scroller.to_string(), key.to_string()));
    settle(rt);
}

fn press_key(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}")).rect
}

/// The text drawn under the element keyed `key`: its own, or the first text beneath it.
fn text_of(rt: &Runtime<App>, key: &str) -> String {
    let elements = rt.elements();
    let Some(at) = elements.iter().position(|e| e.key.as_deref() == Some(key)) else { return String::new() };
    let depth = elements[at].path.len();
    elements[at..]
        .iter()
        .take_while(|e| e.path.len() >= depth && e.path.starts_with(&elements[at].path))
        .find_map(|e| e.text.clone())
        .unwrap_or_default()
}

/// The library index of the demo song titled `title`.
fn idx(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.tracks().iter().position(|t| t.title == title).unwrap_or_else(|| panic!("no song {title}"))
}

/// The titles of the queue's entries, in order.
fn titles(rt: &Runtime<App>) -> Vec<String> {
    let queue = rt.state.queue.as_ref().expect("a queue");
    queue.entries().iter().map(|e| rt.state.tracks()[e.track].title.clone()).collect()
}

fn current_title(rt: &Runtime<App>) -> String {
    rt.state.current_track().expect("a track is loaded").title.clone()
}

/// Plays the library without ETA from its first song, then Play Next puts ETA after the current
/// one. ETA is queued once, so it's the only added entry.
fn playing_with_added(rt: &mut Runtime<App>) {
    let list: Vec<usize> = (0..rt.state.tracks().len()).filter(|&i| rt.state.tracks()[i].title != "ETA").collect();
    let eta = idx(rt, "ETA");
    rt.update(move |s, tx| s.play_list(list, 0, tx));
    rt.update(move |s, tx| s.queue_play_next(vec![eta], tx));
    settle(rt);
}

#[test]
fn the_queue_button_and_q_open_and_close_the_panel() {
    let mut rt = runtime(Store::memory());
    assert!(!has(&rt, "queue-panel"), "hidden at first");
    click(&mut rt, "queue");
    assert!(has(&rt, "queue-panel"), "the button opens it");
    click(&mut rt, "queue");
    assert!(!has(&rt, "queue-panel"), "the button closes it");
    press_key(&mut rt, Key::Text("q".into()));
    assert!(has(&rt, "queue-panel"), "Q opens it");
    press_key(&mut rt, Key::Text("q".into()));
    assert!(!has(&rt, "queue-panel"), "Q closes it");
}

#[test]
fn escape_closes_a_floating_panel() {
    let mut rt = runtime(Store::memory());
    press_key(&mut rt, Key::Text("q".into()));
    press_key(&mut rt, Key::Escape);
    assert!(!has(&rt, "queue-panel"), "Escape closes it");
}

#[test]
fn the_panel_floats_over_the_page_and_pinning_docks_it_beside_the_page() {
    let dir = Dir::new("dock");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    click(&mut rt, "queue");
    assert!(has(&rt, "queue-float"), "floating: it's in the overlay over the page");
    let floating_w = rect(&rt, "queue-panel")[2];
    click(&mut rt, "queue-pin");
    assert!(!has(&rt, "queue-float"), "pinned: no overlay");
    assert!(rt.state.queue_pinned && rt.state.queue_shown);
    assert_eq!(rect(&rt, "queue-panel")[2], floating_w, "the same width, docked");
    let page = rect(&rt, "page-row");
    let panel = rect(&rt, "queue-panel");
    assert!(panel[0] >= page[0] + page[2] - panel[2] - 1.0, "the panel sits at the page's right edge");
    assert!(panel[1] >= page[1] && panel[1] + panel[3] <= page[1] + page[3] + 1.0, "beside the page, not over the header or bar");
    assert!(rect(&rt, "queue-panel")[0] + panel[2] <= 980.0 + 1.0, "inside the window");
    assert!(dir.0.join("prefs.json").exists());
}

#[test]
fn unpinning_turns_the_panel_back_into_an_overlay_and_keeps_it_open() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "queue");
    click(&mut rt, "queue-pin");
    click(&mut rt, "queue-pin");
    assert!(!rt.state.queue_pinned);
    assert!(rt.state.queue_shown, "unpinning keeps it open");
    assert!(has(&rt, "queue-float"), "back to the overlay");
}

#[test]
fn a_pinned_panel_is_saved_and_shown_at_the_next_launch() {
    let dir = Dir::new("pin-saved");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    click(&mut rt, "queue");
    click(&mut rt, "queue-pin");
    assert_eq!(Store::at(prefs.clone()).load_queue_pinned(), Ok(true), "saved");

    let rt = runtime(Store::at(prefs));
    assert!(rt.state.queue_pinned, "reloaded pinned");
    assert!(rt.state.queue_shown, "a pinned panel is shown at launch");
    assert!(has(&rt, "queue-panel"));
}

#[test]
fn an_unpinned_panel_stays_hidden_at_the_next_launch() {
    let dir = Dir::new("unpinned");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    click(&mut rt, "queue");
    let rt = runtime(Store::at(prefs));
    assert!(!rt.state.queue_pinned);
    assert!(!has(&rt, "queue-panel"), "shown is not saved: a floating panel starts hidden");
}

#[test]
fn an_unreadable_pin_pref_is_an_error_and_starts_unpinned() {
    let dir = Dir::new("bad-pin");
    std::fs::create_dir_all(&dir.0).expect("a folder");
    let prefs = dir.0.join("prefs.json");
    std::fs::write(&prefs, r#"{"queue_pinned": "yes please"}"#).expect("a file");
    let rt = runtime(Store::at(prefs));
    assert!(!rt.state.queue_pinned);
    assert!(rt.state.error.is_some(), "the bad pref is reported");
}

#[test]
fn played_rows_are_greyed_the_current_row_is_marked_and_added_rows_say_so() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    let pos = rt.state.queue.as_ref().map_or(0, |q| q.pos());
    rt.update(|s, tx| s.next(tx));
    rt.update(|s, tx| s.next(tx));
    settle(&mut rt);
    click(&mut rt, "queue");
    let pos_now = rt.state.queue.as_ref().map_or(0, |q| q.pos());
    assert_eq!(pos_now, pos + 2);
    let played = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("queue-row0")).expect("a played row");
    assert!(played.label.as_deref().is_some_and(|l| l.contains("by")), "rows are labelled with title and artist");
    let current = format!("queue-row{pos_now}");
    assert!(has(&rt, &current), "the current row is built");
    assert!(!has(&rt, &format!("{current}-remove")), "the current row has no remove button");
    assert!(!has(&rt, &format!("{current}-handle")), "the current row has no grip");
    let added = titles(&rt).iter().position(|t| t == "ETA").expect("ETA is queued");
    assert!(has(&rt, &format!("queue-row{added}-added")), "the added row has its marker");
    assert!(!has(&rt, "queue-row0-added") || added == 0, "only added rows are marked");
}

#[test]
fn a_played_row_has_no_grip_but_an_upcoming_row_does() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    click(&mut rt, "queue");
    let pos = rt.state.queue.as_ref().map_or(0, |q| q.pos());
    assert_eq!(pos, 0);
    assert!(has(&rt, "queue-row1-handle"), "an upcoming row has a grip");
    rt.update(|s, tx| s.next(tx));
    settle(&mut rt);
    assert!(has(&rt, "queue-row0-remove"), "a played row can still be removed");
    assert!(!has(&rt, "queue-row0-handle"), "a played row has no grip");
}

#[test]
fn double_clicking_a_row_jumps_to_it() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    click(&mut rt, "queue");
    let target = idx(&rt, "Says");
    let row = titles(&rt).iter().position(|t| *t == rt.state.tracks()[target].title).expect("Says is queued");
    reveal(&mut rt, crate::app::QUEUE_LIST_KEY, &format!("queue-row{row}"));
    double_click(&mut rt, &format!("queue-row{row}"));
    assert_eq!(current_title(&rt), "Says", "the double-click plays it");
    assert!(rt.state.playing);
}

#[test]
fn the_remove_button_takes_a_row_out_of_the_queue() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    click(&mut rt, "queue");
    let before = titles(&rt);
    let eta = before.iter().position(|t| t == "ETA").expect("ETA is queued");
    click(&mut rt, &format!("queue-row{eta}-remove"));
    let after = titles(&rt);
    assert_eq!(after.len(), before.len() - 1);
    assert!(!after.contains(&"ETA".to_string()), "ETA is gone");
    assert_eq!(current_title(&rt), before[0], "the playing track stays");
}

#[test]
fn dragging_a_grip_reorders_the_upcoming_rows() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    click(&mut rt, "queue");
    let before = titles(&rt);
    let row_h = crate::app::QUEUE_ROW_H;
    drag_down(&mut rt, "queue-row1-handle", row_h * 2.0);
    let after = titles(&rt);
    assert_eq!(after[3], before[1], "the dragged row lands two rows down");
    assert_eq!(after[1], before[2], "the rows it passed move up");
    assert_eq!(current_title(&rt), before[0], "the playing track doesn't move");
}

#[test]
fn the_initial_scroll_puts_the_entry_before_the_current_one_at_the_top() {
    let mut rt = runtime(Store::memory());
    // The whole library: long enough to scroll, and the third song plays.
    let all: Vec<usize> = (0..rt.state.tracks().len()).collect();
    rt.update(move |s, tx| s.play_list(all, 2, tx));
    click(&mut rt, "queue");
    assert_eq!(rt.state.queue.as_ref().map(|q| q.pos()), Some(2));
    let list = rect(&rt, crate::app::QUEUE_LIST_KEY);
    let top = rect(&rt, "queue-row1");
    assert!((top[1] - list[1]).abs() < 1.0, "pos-1 is the top row: {top:?} in {list:?}");
}

#[test]
fn the_initial_scroll_is_zero_when_nothing_has_played() {
    let mut rt = runtime(Store::memory());
    let (a, b) = (idx(&rt, "New Jeans"), idx(&rt, "Super Shy"));
    rt.update(move |s, tx| s.play_list(vec![a, b], a, tx));
    click(&mut rt, "queue");
    let list = rect(&rt, crate::app::QUEUE_LIST_KEY);
    let top = rect(&rt, "queue-row0");
    assert!((top[1] - list[1]).abs() < 1.0, "the first row is at the top");
}

#[test]
fn the_top_row_follows_the_current_track_while_the_panel_is_open() {
    let mut rt = runtime(Store::memory());
    let all: Vec<usize> = (0..rt.state.tracks().len()).collect();
    rt.update(move |s, tx| s.play_list(all, 0, tx));
    click(&mut rt, "queue");
    // Scroll away from the top, then move on: the panel follows the new current track.
    rt.wheel(700.0, 300.0, 200.0);
    settle(&mut rt);
    rt.update(|s, tx| s.next(tx));
    rt.update(|s, tx| s.next(tx));
    settle(&mut rt);
    let list = rect(&rt, crate::app::QUEUE_LIST_KEY);
    let top = rect(&rt, "queue-row1");
    assert!((top[1] - list[1]).abs() < 1.0, "scrolled back to the new pos-1");
}

#[test]
fn repeat_all_shows_a_note_after_the_last_row() {
    let mut rt = runtime(Store::memory());
    let (a, b) = (idx(&rt, "New Jeans"), idx(&rt, "Super Shy"));
    rt.update(move |s, tx| s.play_list(vec![a, b], a, tx));
    click(&mut rt, "queue");
    assert!(!has(&rt, "queue-repeat"), "no note while repeat is off");
    rt.update(|s, _| s.repeat = Repeat::All);
    settle(&mut rt);
    reveal(&mut rt, crate::app::QUEUE_LIST_KEY, "queue-repeat");
    let note = text_of(&rt, "queue-repeat");
    assert_eq!(note, "Then repeats from the start");
}

#[test]
fn an_empty_queue_says_how_to_start() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "queue");
    assert_eq!(text_of(&rt, "queue-empty"), "Nothing queued.");
}

#[test]
fn an_edited_queue_asks_which_way_to_go_and_the_buttons_answer() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    settle(&mut rt);
    assert!(rt.state.pending_play.is_some(), "the prompt is up");
    assert!(has(&rt, "queue-replace-dialog"));
    assert!(has(&rt, "queue-keep-button"), "Keep added songs is offered with added songs upcoming");
    click(&mut rt, "queue-cancel");
    assert!(rt.state.pending_play.is_none(), "Cancel drops the list");
    assert_ne!(current_title(&rt), "Says");

    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    settle(&mut rt);
    click(&mut rt, "queue-replace-button");
    assert!(rt.state.pending_play.is_none());
    assert_eq!(current_title(&rt), "Says", "Replace starts the new list");
    assert_eq!(rt.state.queue_replace, QueueReplace::Ask, "not remembered without the box");
}

#[test]
fn keep_added_keeps_the_added_songs_and_dont_ask_again_saves_the_answer() {
    let dir = Dir::new("keep");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    playing_with_added(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    settle(&mut rt);
    // The checkbox is the dialog's only checkbox; click its own box, not the row around it.
    let (x, y, w, h) = rt.elements().into_iter().find(|e| e.role == "checkbox").map(|e| (e.rect[0], e.rect[1], e.rect[2], e.rect[3])).expect("the checkbox");
    rt.pointer_down(x + w / 2.0, y + h / 2.0);
    rt.pointer_up(x + w / 2.0, y + h / 2.0);
    settle(&mut rt);
    assert!(rt.state.queue_dont_ask, "the box is ticked");
    click(&mut rt, "queue-keep-button");
    assert_eq!(current_title(&rt), "Says");
    assert!(titles(&rt).contains(&"ETA".to_string()), "the added song is kept");
    assert_eq!(rt.state.queue_replace, QueueReplace::KeepAdded);
    assert!(!rt.state.queue_dont_ask, "the box resets once the prompt closes");
    assert_eq!(Store::at(prefs).load_queue_replace(), Ok(QueueReplace::KeepAdded), "saved");
}

#[test]
fn escape_on_the_prompt_is_cancel() {
    let mut rt = runtime(Store::memory());
    playing_with_added(&mut rt);
    let says = idx(&rt, "Says");
    rt.update(move |s, tx| s.play_index(says, tx));
    settle(&mut rt);
    press_key(&mut rt, Key::Escape);
    assert!(rt.state.pending_play.is_none(), "Escape cancels the list");
    assert_ne!(current_title(&rt), "Says");
}

#[test]
fn the_settings_choice_sets_and_saves_the_policy() {
    let dir = Dir::new("settings");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    rt.update(|s, _| s.show_settings());
    settle(&mut rt);
    click(&mut rt, "keep");
    assert_eq!(rt.state.queue_replace, QueueReplace::KeepAdded);
    assert_eq!(Store::at(prefs).load_queue_replace(), Ok(QueueReplace::KeepAdded));
}






#[test]
fn a_pinned_queue_restored_at_launch_scrolls_to_the_entry_before_the_current_one() {
    let dir = Dir::new("launch-scroll");
    let prefs = dir.0.join("prefs.json");
    let mut rt = runtime(Store::at(prefs.clone()));
    let all: Vec<usize> = (0..rt.state.tracks().len()).collect();
    rt.update(move |s, tx| s.play_list(all, 4, tx));
    click(&mut rt, "queue");
    click(&mut rt, "queue-pin");
    rt.update(|s, tx| s.toggle(tx));
    rt.update(|s, tx| s.toggle(tx));
    drop(rt);

    let mut rt = runtime(Store::at(prefs));
    settle(&mut rt);
    assert_eq!(rt.state.queue.as_ref().map(|q| q.pos()), Some(4), "restored at the saved place");
    let list = rect(&rt, crate::app::QUEUE_LIST_KEY);
    let top = rect(&rt, "queue-row3");
    assert!((top[1] - list[1]).abs() < 1.0, "pos-1 is the top row after a launch: {top:?} in {list:?}");
}

#[test]
fn escape_closes_an_open_popover_before_the_floating_queue() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "queue");
    rt.update(|s, _| s.toggle_panel(crate::app::Panel::Sort));
    settle(&mut rt);
    press_key(&mut rt, Key::Escape);
    assert!(rt.state.panel.is_none(), "the popover closes first");
    assert!(rt.state.queue_shown, "the queue stays open");
    press_key(&mut rt, Key::Escape);
    assert!(!rt.state.queue_shown, "a second Escape closes the queue");
}

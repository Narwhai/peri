//! The right-click menu on songs and albums: Play Next, Play After, a song's details, and how it
//! closes.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output};
use crate::query::store::Store;
use crate::view::view;

fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
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

/// Right-clicks the element keyed `key`.
fn right_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(rt);
}

/// Presses the menu item labelled `label`, found by its text.
fn choose(rt: &mut Runtime<App>, label: &str) {
    let key = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("song-menu-item")) && e.label.as_deref() == Some(label))
        .and_then(|e| e.key.clone())
        .unwrap_or_else(|| panic!("no menu item {label}"));
    click(rt, &key);
}

fn press_key(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.find(key).is_some()
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

/// Plays the library from its first song, which is "The Dance #1".
fn playing(rt: &mut Runtime<App>) {
    let list: Vec<usize> = (0..rt.state.tracks().len()).collect();
    rt.update(move |s, tx| s.play_list(list, 0, tx));
    settle(rt);
}

#[test]
fn play_next_from_a_song_row_queues_it_after_the_current_song_marked_added() {
    let mut rt = runtime();
    playing(&mut rt);
    let eta = idx(&rt, "ETA");
    click(&mut rt, "mode/songs");
    right_click(&mut rt, &format!("t{eta}"));
    assert!(has(&rt, "song-menu-menu"), "the menu opened");
    choose(&mut rt, "Play Next");
    assert!(!has(&rt, "song-menu-menu"), "the menu closed");
    let queue = titles(&rt);
    assert_eq!(queue[1], "ETA", "right after the current song");
    click(&mut rt, "queue");
    assert!(has(&rt, "queue-row1-added"), "the added row is marked");
}

#[test]
fn play_after_from_a_song_row_queues_it_at_the_end() {
    let mut rt = runtime();
    playing(&mut rt);
    let eta = idx(&rt, "ETA");
    click(&mut rt, "mode/songs");
    right_click(&mut rt, &format!("t{eta}"));
    choose(&mut rt, "Play After");
    let queue = titles(&rt);
    assert_eq!(queue.last().map(String::as_str), Some("ETA"), "at the end of the queue");
}

#[test]
fn play_next_from_an_album_card_queues_its_tracks_in_album_order() {
    let mut rt = runtime();
    playing(&mut rt);
    let album = rt.state.albums().iter().position(|a| a.title == "Get Up").expect("an album");
    let tracks: Vec<usize> = rt.state.albums()[album].tracks.clone();
    let titles_in_order: Vec<String> = tracks.iter().map(|&t| rt.state.tracks()[t].title.clone()).collect();
    click(&mut rt, "mode/albums");
    right_click(&mut rt, &format!("a{album}"));
    choose(&mut rt, "Play Next");
    let queue = titles(&rt);
    assert_eq!(queue[1..1 + titles_in_order.len()], titles_in_order[..], "the album's tracks follow the current song in album order");
}

#[test]
fn play_next_with_nothing_playing_starts_that_song() {
    let mut rt = runtime();
    let eta = idx(&rt, "ETA");
    click(&mut rt, "mode/songs");
    right_click(&mut rt, &format!("t{eta}"));
    choose(&mut rt, "Play Next");
    assert_eq!(rt.state.current_track().map(|t| t.title.as_str()), Some("ETA"), "playback starts on it");
}

#[test]
fn escape_closes_the_menu_without_changing_the_queue() {
    let mut rt = runtime();
    playing(&mut rt);
    let before = titles(&rt);
    let eta = idx(&rt, "ETA");
    click(&mut rt, "mode/songs");
    right_click(&mut rt, &format!("t{eta}"));
    assert!(has(&rt, "song-menu-menu"), "the menu opened");
    press_key(&mut rt, Key::Escape);
    assert!(!has(&rt, "song-menu-menu"), "Escape closes it");
    assert_eq!(titles(&rt), before, "the queue is unchanged");
}

#[test]
fn a_home_song_row_has_the_menu_too() {
    let mut rt = runtime();
    playing(&mut rt);
    let eta = idx(&rt, "ETA");
    // Recently Played lists only songs that have played.
    rt.update(move |s, _| s.record_play(eta, 1_700_000_000).expect("the play is recorded"));
    settle(&mut rt);
    let before = titles(&rt).len();
    click(&mut rt, "home-tab");
    right_click(&mut rt, &format!("recent-songs-t{eta}"));
    assert!(has(&rt, "song-menu-menu"), "the menu opens from Home");
    choose(&mut rt, "Play After");
    let queue = titles(&rt);
    assert_eq!(queue.len(), before + 1, "one song was queued from Home");
    assert_eq!(queue.last().map(String::as_str), Some("ETA"), "at the end of the queue");
}


#[test]
fn the_menu_opens_at_the_pointer_when_the_app_is_zoomed() {
    let mut rt = runtime();
    rt.update(|s, _| s.zoom = 1.25);
    settle(&mut rt);
    click(&mut rt, "mode/songs");
    let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("t5")).expect("row t5").rect;
    let (x, y) = (r[0] + 100.0, r[1] + r[3] / 2.0);
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    let menu = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("song-menu-menu")).expect("the menu opened").rect;
    assert!((menu[0] - x).abs() < 1.0, "menu left {} at the pointer's x {x}", menu[0]);
    // Near the window's bottom the menu may flip above the pointer; either edge is at it.
    let at_y = (menu[1] - y).abs() < 1.0 || (menu[1] + menu[3] - y).abs() < 1.0;
    assert!(at_y, "menu {menu:?} touches the pointer's y {y}");
}

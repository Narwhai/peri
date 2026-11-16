//! Song table columns and the song details dialog, driven headlessly.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, Panel, album_key, row_key};
use crate::query::property::{Mode, Property};
use crate::query::store::Store;
use crate::credits::{Credit, Role};
use crate::library::Track;
use crate::view::view;

/// The demo library, with credits on Laraaji's songs.
fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    let mut tracks: Vec<Track> = crate::demo_tracks();
    for t in tracks.iter_mut().filter(|t| t.album.as_deref() == Some("Day of Radiance")) {
        t.credits = vec![Credit::parse(Role::Producer, "Brian Eno"), Credit::parse(Role::Composer, "Laraaji")];
    }
    app.set_library(tracks, false);
    let mut rt = Runtime::new(app, view, (1200.0, 760.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
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
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element or link {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn key(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn idx(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.tracks.iter().position(|t| t.title == title).unwrap_or_else(|| panic!("no song {title}"))
}

/// The text of the element keyed `key` inside row `row`.
fn cell(rt: &Runtime<App>, row: &str, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && e.path.contains(&format!("/{row}/"))).and_then(|e| e.text)
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn columns_show_and_hide_from_the_popover_and_save_with_the_view() {
    let mut rt = runtime();
    let dance = row_key(idx(&rt, "The Dance #1"), None);
    assert_eq!(cell(&rt, &dance, "col-artist").as_deref(), Some("Laraaji"));
    assert!(cell(&rt, &dance, "col-producer").is_none(), "Producer is hidden by default");
    click(&mut rt, "columns");
    assert_eq!(rt.state.panel, Some(Panel::Columns));
    click(&mut rt, "column-producer");
    click(&mut rt, "column-album");
    assert_eq!(rt.state.views.active().columns, [Property::Artist, Property::Duration, Property::Producer]);
    assert_eq!(cell(&rt, &dance, "col-producer").as_deref(), Some("Brian Eno"));
    assert!(cell(&rt, &dance, "col-album").is_none());
    // Dragging Producer's grip up one row moves it before Duration.
    drag_vertical(&mut rt, "column-producer-handle", -32.0);
    assert_eq!(rt.state.views.active().columns, [Property::Artist, Property::Producer, Property::Duration]);
    click(&mut rt, "columns-reset");
    assert_eq!(rt.state.views.active().columns, crate::query::view::default_columns());
}

#[test]
fn dragging_a_column_grip_across_rows_reorders_and_escape_cancels() {
    let mut rt = runtime();
    click(&mut rt, "columns");
    click(&mut rt, "column-producer");
    click(&mut rt, "column-album");
    assert_eq!(rt.state.views.active().columns, [Property::Artist, Property::Duration, Property::Producer]);
    // Producer, the last shown column, dragged up past both others.
    drag_vertical(&mut rt, "column-producer-handle", -64.0);
    assert_eq!(rt.state.views.active().columns, [Property::Producer, Property::Artist, Property::Duration]);
    // A drag that Escape cancels keeps the order.
    let (x, y) = drag_held(&mut rt, "column-producer-handle", 64.0);
    rt.physical_input(rune::PhysicalInput::Key { key: rune::PhysicalKeyCode::Escape, mods: Default::default(), pressed: true, repeat: false });
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.views.active().columns, [Property::Producer, Property::Artist, Property::Duration]);
    assert!(rt.state.reorder.is_none(), "the drag ends with the cancel");
}

#[test]
fn a_credit_columns_names_open_the_artist_page() {
    let mut rt = runtime();
    rt.update(|s, _| {
        s.views.active_mut().columns = vec![Property::Producer];
        s.views_changed();
    });
    settle(&mut rt);
    click(&mut rt, "Brian Eno");
    assert_eq!(rt.state.page, Page::Artist("Brian Eno".into()));
    assert!(rt.find("credits").is_some());
}

#[test]
fn the_album_column_opens_the_songs_album() {
    let mut rt = runtime();
    let dance = idx(&rt, "The Dance #1");
    assert_eq!(cell(&rt, &row_key(dance, None), "col-album").as_deref(), Some("Day of Radiance"));
    click(&mut rt, &format!("album:{dance}"));
    let a = rt.state.albums.iter().position(|a| a.title == "Day of Radiance").expect("album");
    assert_eq!(rt.state.page, Page::Album(rt.state.albums[a].id()));
}

#[test]
fn song_views_show_covers_until_switched_off_and_save_it() {
    let mut rt = runtime();
    let dance = idx(&rt, "The Dance #1");
    let row = row_key(dance, None);
    let cover = format!("cover{dance}");
    let in_row = |rt: &Runtime<App>| rt.elements().into_iter().any(|e| e.key.as_deref() == Some(cover.as_str()) && e.path.contains(&format!("/{row}/")));
    assert!(rt.state.views.active().covers, "new views show covers");
    assert!(in_row(&rt));
    click(&mut rt, "columns");
    click(&mut rt, "columns-covers");
    assert!(!rt.state.views.active().covers);
    assert!(!in_row(&rt));
    assert!(rt.state.views.list.iter().filter(|v| v.covers).count() < rt.state.views.list.len(), "only the active view changes");
}

#[test]
fn the_playing_song_shows_its_icon_on_the_cover_and_keeps_its_number() {
    let mut rt = runtime();
    let dance = idx(&rt, "The Dance #1");
    rt.update(|s, tx| s.play_index(dance, tx));
    settle(&mut rt);
    let n = rt.state.filtered.iter().position(|&i| i == dance).expect("listed") + 1;
    let row = row_key(dance, None);
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| e.path.contains(&format!("/{row}/"))).filter_map(|e| e.text).collect();
    assert!(texts.contains(&n.to_string()), "the number stays: {texts:?}");
    assert!(rt.find(&format!("cover{dance}")).is_some());
}

#[test]
fn the_covers_switch_shows_only_for_song_views() {
    let mut rt = runtime();
    click(&mut rt, "columns");
    assert!(has(&rt, "columns-covers"));
    rt.update(|s, _| {
        s.views.active_mut().set_mode(Mode::Albums);
        s.views_changed();
    });
    settle(&mut rt);
    assert!(!has(&rt, "columns-covers"));
}

#[test]
fn album_pages_use_their_own_columns() {
    let mut rt = runtime();
    rt.update(|s, _| {
        s.views.active_mut().set_mode(Mode::Albums);
        s.views_changed();
    });
    settle(&mut rt);
    let a = rt.state.albums.iter().position(|a| a.title == "Day of Radiance").expect("album");
    click(&mut rt, &album_key(a, None));
    let row = format!("t{}", idx(&rt, "The Dance #1"));
    assert_eq!(cell(&rt, &row, "col-format").as_deref(), Some("FLAC"), "pages default to artist, format, duration");
    click(&mut rt, "columns");
    click(&mut rt, "column-composer");
    assert_eq!(cell(&rt, &row, "col-composer").as_deref(), Some("Laraaji"));
    assert!(rt.state.views.page_columns.contains(&Property::Composer));
    assert!(!rt.state.views.active().columns.contains(&Property::Composer), "the view's columns don't change");
}

#[test]
fn the_details_dialog_opens_from_a_row_and_the_i_key_and_closes_with_escape() {
    let mut rt = runtime();
    let dance = idx(&rt, "The Dance #1");
    // The details button shows on the row; pressing it opens the dialog without playing.
    let button = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("details") && e.path.contains(&format!("/{}/", row_key(dance, None)))).expect("a details button");
    let (x, y) = (button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.details_track(), Some(dance));
    assert!(rt.state.current.is_none());
    assert!(has(&rt, "details-file"));
    assert!(has(&rt, "details-credits"), "credits list by role");
    assert!(has(&rt, "details-error"), "the demo file can't be read, and the dialog says so");
    key(&mut rt, Key::Escape);
    assert!(rt.state.details.is_none(), "Escape closes the dialog, not the page");
    // I opens the selected song's details.
    rt.update(|s, _| s.selected = Some(dance));
    key(&mut rt, Key::Text("i".into()));
    assert_eq!(rt.state.details_track(), Some(dance));
}

#[test]
fn a_name_in_the_dialog_opens_the_artist_and_closes_the_dialog() {
    let mut rt = runtime();
    let dance = idx(&rt, "The Dance #1");
    rt.update(|s, tx| s.show_details(dance, tx));
    settle(&mut rt);
    click(&mut rt, "Brian Eno");
    assert!(rt.state.details.is_none());
    assert_eq!(rt.state.page, Page::Artist("Brian Eno".into()));
}

/// The dialog on a real file: its audio properties, IDs and every raw tag.
/// `PERI_MUSIC_FILE=song.flac cargo test real_details_dialog -- --ignored`
#[test]
#[ignore = "needs PERI_MUSIC_FILE"]
fn real_details_dialog() {
    let path = std::env::var("PERI_MUSIC_FILE").expect("set PERI_MUSIC_FILE to an audio file");
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(vec![Track { path: path.into(), title: "Real".into(), ..Track::default() }], false);
    let mut rt = Runtime::new(app, view, (1200.0, 1400.0), Env::new(true, 1.0));
    rt.update(|s, tx| s.show_details(0, tx));
    settle(&mut rt);
    assert!(!has(&rt, "details-error"));
    assert!(!has(&rt, "raw0"), "raw tags start collapsed");
    // The toggle sits below the fold of the dialog's scroll.
    rt.update(|_, tx| tx.scroll_into_view_in("details-scroll", "details-raw-toggle"));
    settle(&mut rt);
    click(&mut rt, "details-raw-toggle");
    assert!(has(&rt, "raw0"), "raw tags list once opened");
}



/// The laid-out width of the first element keyed `key` (a row's cell or a header).
fn width_of(rt: &Runtime<App>, key: &str) -> f32 {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect[2]
}

/// Drags the element keyed `key` by `dy` points, in steps as a hand moves.
fn drag_vertical(rt: &mut Runtime<App>, key: &str, dy: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    for step in 1..=4 {
        rt.pointer_move(x, y + dy * step as f32 / 4.0);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x, y + dy);
    settle(rt);
}

/// Presses the grip `key`, drags it by `dy`, and leaves the pointer held: the caller ends the gesture.
fn drag_held(rt: &mut Runtime<App>, key: &str, dy: f32) -> (f32, f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    for step in 1..=4 {
        rt.pointer_move(x, y + dy * step as f32 / 4.0);
        rt.frame(1.0 / 60.0);
    }
    (x, y + dy)
}

fn drag(rt: &mut Runtime<App>, key: &str, dx: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    for step in 1..=4 {
        rt.pointer_move(x + dx * step as f32 / 4.0, y);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x + dx, y);
    settle(rt);
}

#[test]
fn dragging_a_header_edge_resizes_the_column_and_double_click_or_reset_restores_it() {
    let mut rt = runtime();
    let album = "table-header-col-album";
    let grown = width_of(&rt, album);
    assert!(grown > crate::view::table_width(Property::Album), "album names grow with spare room, got {grown}");
    drag(&mut rt, &format!("{album}-resize"), 60.0);
    let set = rt.state.views.active().widths.get(&Property::Album).copied().expect("dragging sets a width");
    assert!((set - (grown + 60.0)).abs() <= 1.0, "the drag starts from the width on screen: {grown} + 60 ≠ {set}");
    assert!((width_of(&rt, album) - set).abs() <= 1.0);
    assert!((width_of(&rt, "col-album") - set).abs() <= 1.0, "rows follow the header");
    // Never narrower than the minimum.
    drag(&mut rt, &format!("{album}-resize"), -1000.0);
    assert_eq!(rt.state.views.active().widths.get(&Property::Album).copied(), Some(40.0));
    // Double-click lets it size itself again.
    let (x, y) = rt.find(&format!("{album}-resize")).expect("handle");
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
    }
    settle(&mut rt);
    assert!(rt.state.views.active().widths.is_empty());
    // Reset widths in the Columns popover clears every dragged width.
    drag(&mut rt, "table-header-col-duration-resize", 30.0);
    assert!(!rt.state.views.active().widths.is_empty());
    click(&mut rt, "columns");
    click(&mut rt, "columns-reset-widths");
    assert!(rt.state.views.active().widths.is_empty());
}

#[test]
fn page_tables_keep_their_own_widths() {
    let mut rt = runtime();
    // Brian Eno produced Laraaji's album: his credits are a page table.
    rt.update(|s, tx| s.show_artist("Brian Eno", tx));
    settle(&mut rt);
    drag(&mut rt, "credits-header-col-artist-resize", 50.0);
    assert!(rt.state.views.page_widths.contains_key(&Property::Artist));
    assert!(rt.state.views.active().widths.is_empty(), "the view's widths are its own");
}

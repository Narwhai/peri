//! Back from a page opened from the library returns the list to where it was scrolled.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, row_key};
use crate::query::property::Mode;
use crate::query::store::Store;
use crate::library::Track;
use crate::view::view;

/// Forty albums of five songs: far taller than the window, in songs or albums.
fn tracks() -> Vec<Track> {
    let mut tracks = Vec::new();
    for a in 0..40u32 {
        for n in 1..=5u32 {
            tracks.push(Track {
                path: format!("/m/Artist {a:02}/Album {a:02}/{n:02}.flac").into(),
                title: format!("Song {a:02}-{n}"),
                credit: Some(format!("Artist {a:02}")),
                album_artist: Some(format!("Artist {a:02}")),
                album: Some(format!("Album {a:02}")),
                year: Some(1980 + a),
                number: Some(n),
                duration: Some(180.0),
                ..Track::default()
            });
        }
    }
    tracks
}

fn runtime(mode: Mode) -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(tracks(), false);
    if mode == Mode::Albums {
        app.views.active_mut().set_mode(Mode::Albums);
        app.views_changed();
    }
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

/// Scrolls the list down by `dy` points.
fn scroll_down(rt: &mut Runtime<App>, dy: f32) {
    let (x, y) = rt.find("tracks").expect("the list shows");
    rt.wheel(x, y, -dy);
    settle(rt);
}

fn scrolled(rt: &Runtime<App>) -> f32 {
    rt.state.library_scroll.get()[1]
}

#[test]
fn back_from_an_artist_page_returns_the_song_list_to_where_it_was() {
    let mut rt = runtime(Mode::Songs);
    scroll_down(&mut rt, 1500.0);
    let before = scrolled(&rt);
    assert!(before > 1000.0, "the list scrolled, to {before}");
    let song = rt.state.tracks().iter().position(|t| t.title == "Song 08-1").expect("Song 08-1");
    assert!(rt.find(&row_key(song, None)).is_some(), "a row 1500 points down shows");
    rt.update(|s, tx| s.show_artist("Artist 08", tx));
    settle(&mut rt);
    assert!(rt.find("tracks").is_none(), "the artist page replaces the list");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Library);
    assert!((scrolled(&rt) - before).abs() < 1.0, "back at {before}, not {}", scrolled(&rt));
    assert!(rt.find(&row_key(song, None)).is_some(), "the same rows show again");
}

#[test]
fn escape_from_an_album_page_returns_the_album_grid_to_where_it_was() {
    let mut rt = runtime(Mode::Albums);
    scroll_down(&mut rt, 1200.0);
    let before = scrolled(&rt);
    assert!(before > 1000.0, "the grid scrolled, to {before}");
    rt.update(|s, tx| s.show_album(30, tx));
    settle(&mut rt);
    // A page opened from that page, then back twice, still lands where the list was.
    rt.update(|s, tx| s.show_artist("Artist 30", tx));
    settle(&mut rt);
    for _ in 0..2 {
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
    }
    assert_eq!(rt.state.page, Page::Library);
    assert!((scrolled(&rt) - before).abs() < 1.0, "back at {before}, not {}", scrolled(&rt));
}

#[test]
fn home_still_opens_the_list_where_it_was_left() {
    let mut rt = runtime(Mode::Songs);
    scroll_down(&mut rt, 800.0);
    rt.update(|s, tx| s.show_artist("Artist 01", tx));
    settle(&mut rt);
    click(&mut rt, "library-tab");
    assert_eq!(rt.state.page, Page::Library);
    assert!(scrolled(&rt) < 1.0, "Home opens the list at the top, at {}", scrolled(&rt));
}

/// The grid opens at four columns until its width is known, then refits to the window:
/// the rows that rebuilds land in place rather than sliding up from below.
#[test]
fn the_album_grid_refits_its_columns_without_sliding_the_rows() {
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(tracks(), false);
    app.views.active_mut().set_mode(Mode::Albums);
    app.views_changed();
    let mut rt = Runtime::new(app, view, (1280.0, 900.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    let columns = rt.state.columns;
    assert!(columns > 4, "1280 points refit past the first four columns, got {columns}");
    let second = crate::app::album_key(columns, None);
    let y = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(second.as_str())).map(|e| e.rect[1]);
    let first = y(&rt).unwrap_or_else(|| panic!("no {second}"));
    settle(&mut rt);
    let rest = y(&rt).unwrap_or_else(|| panic!("no {second}"));
    assert!((first - rest).abs() < 0.5, "the second row starts where it rests: {first} vs {rest}");
}

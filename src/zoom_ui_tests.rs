//! The album grid's card size and the window's zoom, driven headlessly.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, album_key};
use crate::query::store::Store;
use crate::query::view::{CARD_SIZE_DEFAULT, CARD_SIZE_MAX, CARD_SIZE_MIN, Layout};
use crate::view::view;

struct Prefs(std::path::PathBuf);

impl Prefs {
    fn new(name: &str) -> Prefs {
        let dir = std::env::temp_dir().join(format!("peri-zoom-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Prefs(dir)
    }

    fn store(&self) -> Store {
        Store::at(self.0.join("prefs.json"))
    }
}

impl Drop for Prefs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn runtime(store: Store) -> Runtime<App> {
    let mut app = App::new(Output::Silent, store);
    crate::scene_library(&mut app);
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

fn rect_of(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
}

/// Presses the slider keyed `key` at `fraction` of its width and lets go there.
fn slide_to(rt: &mut Runtime<App>, key: &str, fraction: f32) {
    let r = rect_of(rt, key);
    // Just inside the ends, which are the hit box's edges.
    let (x, y) = (r[0] + 1.0 + (r[2] - 2.0) * fraction, r[1] + r[3] / 2.0);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    settle(rt);
}

fn command(rt: &mut Runtime<App>, c: char) {
    rt.key(KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, ..Mods::default() } });
    settle(rt);
}

fn album_grid(rt: &mut Runtime<App>) {
    click(rt, "mode/albums");
    assert_eq!(rt.state.views.active().layout, Layout::Grid);
}

#[test]
fn the_card_size_slider_resizes_cards_and_refits_the_columns() {
    let prefs = Prefs::new("cards");
    let mut rt = runtime(prefs.store());
    album_grid(&mut rt);
    assert_eq!(rt.state.card_size(), CARD_SIZE_DEFAULT);
    let small_columns = rt.state.columns;
    let small = rect_of(&rt, &album_key(0, None))[2];
    assert!((small - CARD_SIZE_DEFAULT).abs() < 1.0, "cards are the default size, got {small}");

    slide_to(&mut rt, "card-size", 1.0);
    let large = rt.state.card_size();
    assert!(large > CARD_SIZE_MAX - 4.0, "the slider's end is near the largest size, got {large}");
    assert_eq!(rt.state.views.active().card_size, Some(large), "the view keeps its own size");
    assert!(rt.state.columns < small_columns, "bigger cards fit fewer columns: {} vs {small_columns}", rt.state.columns);
    let wide = rect_of(&rt, &album_key(0, None))[2];
    assert!((wide - large.floor()).abs() < 1.0, "cards follow the size: {wide} vs {large}");

    slide_to(&mut rt, "card-size", 0.0);
    assert!(rt.state.card_size() < CARD_SIZE_MIN + 4.0);
    assert!(rt.state.columns > small_columns, "smaller cards fit more columns");

    // Saved with the view.
    let reopened = runtime(prefs.store());
    assert_eq!(reopened.state.views.active().card_size, rt.state.views.active().card_size);
}

#[test]
fn views_without_their_own_card_size_follow_the_default() {
    let prefs = Prefs::new("default");
    let mut rt = runtime(prefs.store());
    album_grid(&mut rt);
    slide_to(&mut rt, "card-size", 1.0);
    let size = rt.state.card_size();
    click(&mut rt, "card-size-default");
    assert_eq!(rt.state.views.card_size, size, "the view's size becomes the default");
    assert_eq!(rt.state.views.active().card_size, None, "and the view follows it");
    assert!(rt.find("card-size-default").is_none(), "nothing left to make the default");

    // A new view starts at the default.
    rt.update(|s, _| {
        let mut other = crate::query::view::View::new("Other");
        other.set_mode(crate::query::property::Mode::Albums);
        s.views.list.push(other);
        s.views.active = s.views.list.len() - 1;
        s.views_changed();
    });
    settle(&mut rt);
    assert_eq!(rt.state.card_size(), size);

    // Its own size, then double-click back to the default.
    slide_to(&mut rt, "card-size", 0.0);
    assert_ne!(rt.state.card_size(), size);
    let (x, y) = rt.find("card-size").unwrap();
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
    }
    settle(&mut rt);
    assert_eq!(rt.state.views.active().card_size, None, "double-click follows the default again");
    assert_eq!(rt.state.card_size(), size);

    let reopened = runtime(prefs.store());
    assert_eq!(reopened.state.views.card_size, size, "the default is saved");
}

#[test]
fn the_card_size_slider_shows_only_on_album_grids() {
    let mut rt = runtime(Store::memory());
    assert!(rt.find("card-size").is_none(), "song views have no cards");
    album_grid(&mut rt);
    assert!(rt.find("card-size").is_some());
    click(&mut rt, "layout/list");
    assert!(rt.find("card-size").is_none(), "list cards don't resize");
}

#[test]
fn keyboard_zoom_steps_resets_and_is_saved() {
    let prefs = Prefs::new("keys");
    let mut rt = runtime(prefs.store());
    assert_eq!(rt.state.zoom, 1.0);
    let header = rect_of(&rt, "header");

    command(&mut rt, '=');
    assert_eq!(rt.state.zoom, 1.1);
    assert!(rt.find("zoom-toast").is_some(), "the level shows after a keyboard zoom");
    command(&mut rt, '+');
    assert_eq!(rt.state.zoom, 1.2);
    let zoomed = rect_of(&rt, "header");
    assert!((zoomed[3] - header[3] * 1.2).abs() < 0.5, "the header draws 1.2 times as tall: {} vs {}", zoomed[3], header[3]);

    let reopened = runtime(prefs.store());
    assert_eq!(reopened.state.zoom, 1.2, "the zoom is saved");

    command(&mut rt, '-');
    command(&mut rt, '-');
    command(&mut rt, '-');
    assert_eq!(rt.state.zoom, 0.9);
    command(&mut rt, '0');
    assert_eq!(rt.state.zoom, 1.0);

    for _ in 0..30 {
        command(&mut rt, '-');
    }
    assert_eq!(rt.state.zoom, crate::app::ZOOM_MIN, "zoom stops at its smallest");
}

#[test]
fn the_zoom_toast_goes_away() {
    let mut rt = runtime(Store::memory());
    command(&mut rt, '=');
    assert!(rt.find("zoom-toast").is_some());
    rt.state.zoom_toast = Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
    settle(&mut rt);
    assert!(rt.find("zoom-toast").is_none());
}

#[test]
fn dragging_the_zoom_slider_previews_and_zooms_on_release() {
    let mut rt = runtime(Store::memory());
    let r = rect_of(&rt, "zoom");
    let y = r[1] + r[3] / 2.0;
    rt.pointer_down(r[0] + 1.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(r[0] + r[2] / 2.0, y);
    settle(&mut rt);
    assert_eq!(rt.state.zoom, 1.0, "nothing zooms mid-drag");
    let preview = rt.state.zoom_preview.expect("a preview while dragging");
    assert_eq!(text_of(&rt, "zoom-level"), format!("{}%", (preview * 100.0).round()), "the level follows the drag");
    rt.pointer_up(r[0] + r[2] / 2.0, y);
    settle(&mut rt);
    assert_eq!(rt.state.zoom, preview, "letting go zooms");
    assert_eq!(rt.state.zoom_preview, None);
}

#[test]
fn the_header_zoom_slider_and_level_reset() {
    let mut rt = runtime(Store::memory());
    slide_to(&mut rt, "zoom", 1.0);
    assert!(rt.state.zoom > crate::app::ZOOM_MAX - 0.05, "the slider's end is near the largest zoom, got {}", rt.state.zoom);
    assert!(rt.find("zoom-toast").is_none(), "the slider shows its own level");
    click(&mut rt, "zoom-level");
    assert_eq!(rt.state.zoom, 1.0, "clicking the level resets it");
}

#[test]
fn zooming_out_fits_more_album_columns() {
    let mut rt = runtime(Store::memory());
    album_grid(&mut rt);
    let columns = rt.state.columns;
    command(&mut rt, '-');
    command(&mut rt, '-');
    command(&mut rt, '-');
    assert!(rt.state.columns > columns, "the window holds more of the grid at 70%: {} vs {columns}", rt.state.columns);
}

#[test]
fn zoom_never_lays_the_app_out_smaller_than_the_smallest_window() {
    let mut rt = runtime(Store::memory());
    rt.update(|s, _| s.set_zoom(2.0, false));
    settle(&mut rt);
    // 980 x 640 holds 720 x 420 at most 1.36 times.
    let window = rect_of(&rt, "zoom-frame");
    for key in ["settings", "filter", "now-playing"] {
        let r = rect_of(&rt, key);
        assert!(r[0] + r[2] <= window[2] + 0.5, "{key} stays in the window: {r:?}");
    }
    assert_eq!(text_of(&rt, "zoom-level"), "max 136%");
    assert_eq!(rt.state.zoom, 2.0, "the zoom asked for is kept for a bigger window");
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

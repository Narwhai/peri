//! Album views driven headlessly: switching modes and layouts, the album page, playing albums.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, album_key};
use crate::base::filter::{Condition, Node, Operator};
use crate::base::property::{Mode, Property};
use crate::base::store::Store;
use crate::base::view::Layout;
use crate::view::view;

fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// A point on row `key` clear of its links: just inside its left edge, past the number.
fn row_point(rt: &Runtime<App>, key: &str) -> (f32, f32) {
    let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no row {key}")).rect;
    (r[0] + 6.0, r[1] + r[3] / 2.0)
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

fn album(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.albums.iter().position(|a| a.title == title).unwrap_or_else(|| panic!("no album {title}"))
}

fn shown_titles(rt: &Runtime<App>) -> Vec<String> {
    rt.state.shown_albums.iter().map(|&a| rt.state.albums[a].title.clone()).collect()
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

#[test]
fn switching_to_albums_shows_a_grid_of_every_album() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    assert_eq!(rt.state.base.active().mode, Mode::Albums);
    assert_eq!(rt.state.base.active().layout, Layout::Grid);
    assert_eq!(text_of(&rt, "count"), "4 albums");
    assert_eq!(shown_titles(&rt), ["Live Sessions", "Day of Radiance", "Get Up", "Spaces"]);
    for a in 0..4 {
        assert!(rt.find(&album_key(a, None)).is_some(), "card {a} shows");
    }
    assert!(rt.state.columns >= 4, "980 points fit at least four columns, got {}", rt.state.columns);
    click(&mut rt, "layout/list");
    assert_eq!(rt.state.base.active().layout, Layout::List);
    assert!(rt.find("play-album").is_some(), "list cards have a play button");
}

#[test]
fn sorting_albums_by_artist_clusters_each_artists_albums() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    rt.update(|s, _| {
        s.base.active_mut().sorts = vec![crate::base::view::Sort { property: Property::Year, direction: crate::base::view::Direction::Asc }];
        s.views_changed();
    });
    settle(&mut rt);
    assert_eq!(shown_titles(&rt), ["Day of Radiance", "Spaces", "Get Up", "Live Sessions"], "1980, 2013, 2023, 2024");
    rt.update(|s, _| {
        s.base.active_mut().group_by = Some(crate::base::view::GroupBy { property: Property::Genre, direction: crate::base::view::Direction::Asc });
        s.views_changed();
    });
    settle(&mut rt);
    let groups: Vec<(String, usize)> = rt.state.sections.iter().map(|s| (s.title.clone(), s.items.len())).collect();
    assert_eq!(groups[0], ("Ambient".to_string(), 2), "Day of Radiance and Spaces are both ambient");
}

#[test]
fn clicking_a_card_opens_the_album_page_and_escape_goes_back() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    let a = album(&rt, "Day of Radiance");
    click(&mut rt, &album_key(a, None));
    assert_eq!(rt.state.open_album(), Some(a));
    assert_eq!(text_of(&rt, "page-title"), "Day of Radiance");
    assert_eq!(text_of(&rt, "page-artist"), "Laraaji");
    // Laraaji also plays on Live Sessions, but that album's artist is "Big Thief & Laraaji".
    assert!(rt.find("more-by").is_none());
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Library);
    click(&mut rt, &album_key(a, None));
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Library);
}

#[test]
fn playing_an_album_continues_into_the_next_album_of_the_view() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    let a = album(&rt, "Get Up");
    click(&mut rt, &album_key(a, None));
    click(&mut rt, "page-play");
    let titles = |rt: &Runtime<App>| rt.state.tracks[rt.state.current.expect("playing")].title.clone();
    assert_eq!(titles(&rt), "New Jeans");
    for _ in 0..4 {
        click(&mut rt, "next");
    }
    assert_eq!(titles(&rt), "An Aborted Beginning", "after Get Up's four songs comes Spaces");
}

#[test]
fn double_clicking_a_track_plays_its_album_from_there() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    let a = album(&rt, "Get Up");
    click(&mut rt, &album_key(a, None));
    let shy = rt.state.tracks.iter().position(|t| t.title == "Super Shy").unwrap();
    let (x, y) = row_point(&rt, &format!("t{shy}"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
    }
    settle(&mut rt);
    assert_eq!(rt.state.current, Some(shy));
    click(&mut rt, "next");
    assert_eq!(rt.state.tracks[rt.state.current.unwrap()].title, "ETA");
}

#[test]
fn the_now_playing_cover_opens_the_playing_album() {
    let mut rt = runtime();
    let eta = rt.state.tracks.iter().position(|t| t.title == "ETA").unwrap();
    rt.update(|s, tx| s.play_index(eta, tx));
    settle(&mut rt);
    click(&mut rt, "cover");
    assert_eq!(rt.state.open_album(), Some(album(&rt, "Get Up")));
}

#[test]
fn search_finds_albums_by_title_artist_or_one_of_their_songs() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    let search = |rt: &mut Runtime<App>, q: &str| {
        rt.update(|s, _| {
            s.search = rune::TextEdit::new(q);
            s.refilter();
        });
        settle(rt);
        shown_titles(rt)
    };
    assert_eq!(search(&mut rt, "radiance"), ["Day of Radiance"]);
    assert_eq!(search(&mut rt, "laraaji"), ["Live Sessions", "Day of Radiance"], "artist, and a guest on Live Sessions");
    assert_eq!(search(&mut rt, "hammers"), ["Spaces"], "a song title finds its album");
    assert_eq!(text_of(&rt, "count"), "1 of 4 albums");
}

#[test]
fn album_filters_use_album_properties_and_switching_modes_drops_song_only_ones() {
    let mut rt = runtime();
    rt.update(|s, _| {
        let cond = |property, op, value: &str| Node::Condition(Condition { property, op, value: value.into() });
        s.base.active_mut().filter.children = vec![cond(Property::Year, Operator::Ge, "2013"), cond(Property::Track, Operator::Is, "1")];
        s.views_changed();
    });
    click(&mut rt, "mode/albums");
    let filter = &rt.state.base.active().filter.children;
    assert_eq!(filter.len(), 1, "albums have no track number, so that condition goes");
    assert_eq!(shown_titles(&rt), ["Live Sessions", "Get Up", "Spaces"]);
    rt.update(|s, _| {
        s.base.active_mut().filter.children = vec![Node::Condition(Condition { property: Property::Tracks, op: Operator::Gt, value: "3".into() })];
        s.views_changed();
    });
    settle(&mut rt);
    assert_eq!(shown_titles(&rt), ["Get Up", "Spaces"], "four tracks each");
}

#[test]
fn an_album_view_saves_its_mode_and_layout() {
    let dir = std::env::temp_dir().join(format!("rune-music-albums-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.frame(0.0);
    click(&mut rt, "mode/albums");
    click(&mut rt, "layout/list");
    let loaded = Store::at(&file).load();
    assert_eq!((loaded.active().mode, loaded.active().layout), (Mode::Albums, Layout::List));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_grid_fits_more_columns_in_a_wider_window() {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (720.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    click(&mut rt, "mode/albums");
    let narrow = rt.state.columns;
    rt.set_viewport((1600.0, 600.0), 1.0);
    settle(&mut rt);
    assert!(rt.state.columns > narrow, "{} columns at 1600 vs {narrow} at 720", rt.state.columns);
    assert_eq!(rt.state.rows.len(), 4usize.div_ceil(rt.state.columns));
}

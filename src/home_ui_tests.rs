//! The Home tab driven headlessly: its place in the header, the recently added albums and
//! played songs, playing from the list, opening pages from it, and the tab remembered.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, RECENT_COUNT};
use crate::query::store::Store;
use crate::library::Track;
use crate::view::view;

/// The demo library with `last_played` set on some songs, on the Home tab.
fn runtime() -> Runtime<App> {
    runtime_sized((1200.0, 900.0))
}

/// `runtime` in a window `size` points big.
fn runtime_sized(size: (f32, f32)) -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_home(&mut app);
    let mut rt = Runtime::new(app, view, size, Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Frames for a scroll glide to land.
fn settle_scroll(rt: &mut Runtime<App>) {
    for _ in 0..240 {
        rt.frame(1.0 / 60.0);
    }
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

/// Clicks the element keyed `key`.
fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Double-clicks the element keyed `key`.
fn double_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
    }
    settle(rt);
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}")).rect
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn album(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.albums().iter().position(|a| a.title == title).unwrap_or_else(|| panic!("no album {title}"))
}

fn track(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.tracks().iter().position(|t| t.title == title).unwrap_or_else(|| panic!("no track {title}"))
}

fn titles(rt: &Runtime<App>, list: &[usize]) -> Vec<String> {
    list.iter().map(|&i| rt.state.tracks()[i].title.clone()).collect()
}

fn escape(rt: &mut Runtime<App>) {
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(rt);
}

#[test]
fn the_home_tab_sits_left_of_the_library_and_shows_home() {
    let mut rt = runtime();
    assert_eq!(rt.state.page, Page::Home);
    let home = rect(&rt, "home-tab");
    let library = rect(&rt, "library-tab");
    let graph = rect(&rt, "graph-tab");
    assert!(home[0] < library[0] && library[0] < graph[0], "Home, Library, Graph from the left");
    assert!(rt.find("toolbar").is_none(), "the views' toolbar is hidden on Home");
    assert!(has(&rt, "home-page"));
    click(&mut rt, "library-tab");
    assert_eq!(rt.state.page, Page::Library);
    click(&mut rt, "home-tab");
    assert_eq!(rt.state.page, Page::Home);
    assert_eq!(rt.state.history, Vec::<Page>::new());
}

/// The search is centred in the window, without covering either group of header controls.
fn assert_header_layout(rt: &Runtime<App>) {
    let header = rect(rt, "header");
    let tabs = rect(rt, "tabs");
    let search = rect(rt, "search");
    let zoom = rect(rt, "zoom-control");
    let settings = rect(rt, "settings");
    let centre = |r: [f32; 4]| r[0] + r[2] / 2.0;
    assert!((centre(search) - centre(header)).abs() < 1.0, "search {search:?} is centred in {header:?}");
    assert!(search[0] >= tabs[0] + tabs[2], "search {search:?} clears tabs {tabs:?}");
    assert!(search[0] + search[2] <= zoom[0], "search {search:?} clears zoom {zoom:?}");
    assert!(zoom[0] + zoom[2] <= settings[0] && settings[0] + settings[2] <= header[0] + header[2], "zoom and settings fit in the header");
    assert!((tabs[1] + tabs[3] - header[1] - header[3]).abs() < 1.0, "tabs still meet the header's bottom edge");
}

#[test]
fn the_search_input_has_two_point_corners() {
    let mut rt = runtime();
    let radius = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("search")).expect("search").radius;
    assert_eq!(radius(&rt), 2.0);
    click(&mut rt, "search");
    assert_eq!(radius(&rt), 2.0, "the corners stay the same when focused");
}

#[test]
fn search_is_centred_in_the_header_at_different_sizes_and_zoom_levels() {
    for width in [720.0, 800.0, 980.0, 1200.0] {
        let rt = runtime_sized((width, 900.0));
        assert_header_layout(&rt);
        assert!(rect(&rt, "search")[2] >= 80.0, "search remains usable at width {width}");
    }
    let mut rt = runtime_sized((980.0, 640.0));
    rt.update(|s, _| s.set_zoom(2.0, false));
    settle(&mut rt);
    assert_header_layout(&rt);
}

#[test]
fn the_centred_search_and_header_controls_work_at_minimum_width_while_scanning() {
    let mut rt = runtime_sized((720.0, 900.0));
    rt.poll = None;
    rt.update(|s, _| {
        s.set_waker(std::sync::Arc::new(|| {}));
        s.start_scan(Vec::new(), true);
        s.scan.as_mut().expect("scanning").found = 1234;
    });
    settle(&mut rt);
    assert_eq!(text_of(&rt, "scan-status"), "Scanning… 1234 files");
    assert_header_layout(&rt);
    let status = rect(&rt, "scan-status");
    let search = rect(&rt, "search");
    assert!(status[2] > 20.0 && status[0] >= search[0] + search[2] && status[0] + status[2] <= rect(&rt, "zoom-control")[0], "scan progress stays visible beside zoom: {status:?}");

    click(&mut rt, "search");
    rt.key(KeyEvent { key: Key::Text("laraaji".into()), mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.search.text, "laraaji");
    assert_eq!(rt.state.page, Page::Search);
    assert_header_layout(&rt);
    click(&mut rt, "search/action");
    assert_eq!(rt.state.search.text, "");
    assert_eq!(rt.state.page, Page::Home);
    click(&mut rt, "graph-tab");
    assert_eq!(rt.state.page, Page::Graph);
    assert_header_layout(&rt);
    click(&mut rt, "settings");
    assert_eq!(rt.state.page, Page::Settings);
}

#[test]
fn the_header_recentres_when_the_window_is_resized() {
    let mut rt = runtime_sized((720.0, 900.0));
    for width in [1200.0, 800.0, 720.0] {
        rt.set_viewport((width, 900.0), 1.0);
        settle(&mut rt);
        assert_header_layout(&rt);
    }
}

#[test]
fn recently_added_cards_are_200_wide_and_the_strip_spans_the_page_body() {
    let rt = runtime();
    for &a in &rt.state.recently_added() {
        assert_eq!(rect(&rt, &format!("recent-albums{a}"))[2], 200.0, "card {a} is 200 wide");
    }
    // The body less its 28 side padding, and the two arrow buttons (28 wide) with 6 gaps.
    let body = rect(&rt, "home-page-body")[2];
    let strip = rect(&rt, "recent-albums-strip")[2];
    assert!((strip - (body - 2.0 * 28.0 - 2.0 * 28.0 - 2.0 * 6.0)).abs() < 1.0, "strip {strip} spans body {body}");
}

#[test]
fn the_arrows_scroll_the_recently_added_strip_and_stop_at_its_ends() {
    let mut rt = runtime_sized((640.0, 900.0));
    let first = rt.state.recently_added()[0];
    let card = format!("recent-albums{first}");
    let start = rect(&rt, &card)[0];
    assert!(rt.state.home_strip.get()[1] > 0.0, "the strip overflows in a narrow window");
    click(&mut rt, "recent-albums-prev");
    assert_eq!(rect(&rt, &card)[0], start, "at the start, Back leaves it unchanged");
    click(&mut rt, "recent-albums-next");
    assert!(rect(&rt, &card)[0] < start, "Forward moves the first card left");
    click(&mut rt, "recent-albums-prev");
    settle_scroll(&mut rt);
    assert_eq!(rect(&rt, &card)[0], start, "Back brings it back");
    for _ in 0..10 {
        click(&mut rt, "recent-albums-next");
    }
    let [at, max] = rt.state.home_strip.get();
    assert_eq!(at, max, "clamped at the far end");
    click(&mut rt, "recent-albums-next");
    assert_eq!(rt.state.home_strip.get()[0], max, "Forward at the end does nothing");
}

#[test]
fn recently_added_shows_the_newest_albums_first() {
    let rt = runtime();
    let newest = rt.state.recently_added();
    assert_eq!(newest.len(), 4, "the demo has four albums, all dated");
    assert_eq!(rt.state.albums()[newest[0]].title, "Live Sessions", "2024 is the newest");
    assert_eq!(rt.state.albums()[newest[3]].title, "Day of Radiance", "1980 is the oldest");
    for &a in &newest {
        assert!(has(&rt, &format!("recent-albums{a}")), "card for album {a} shows");
    }
}

#[test]
fn recently_added_takes_at_most_recent_count_and_leaves_out_undated_albums() {
    let mut rt = runtime();
    let mut tracks: Vec<Track> = rt.state.tracks().to_vec();
    // More dated albums than the page lists, plus one song with no date at all.
    for n in 0..RECENT_COUNT + 2 {
        tracks.push(Track {
            path: format!("/extra/{n:02}.flac").into(),
            title: format!("Extra {n}"),
            album: Some(format!("Extra album {n}")),
            album_artist: Some("Extra".into()),
            added: Some(1_900_000_000 + n as i64),
            ..Track::default()
        });
    }
    tracks.push(Track { path: "/extra/undated.flac".into(), title: "Undated".into(), album: Some("Undated album".into()), ..Track::default() });
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let newest = rt.state.recently_added();
    assert_eq!(newest.len(), RECENT_COUNT);
    let added: Vec<Option<i64>> = newest.iter().map(|&a| rt.state.albums()[a].added).collect();
    let mut sorted = added.clone();
    sorted.sort_by_key(|&d| std::cmp::Reverse(d));
    assert_eq!(added, sorted, "newest first");
    assert!(newest.iter().all(|&a| rt.state.albums()[a].title != "Undated album"), "an album with no date is left out");
    assert!(!has(&rt, "recent-albums-empty"));
}

#[test]
fn recently_played_lists_the_last_songs_most_recent_first() {
    let rt = runtime();
    let played = rt.state.recently_played();
    assert_eq!(titles(&rt, &played), ["Mary", "ETA", "Says", "New Jeans", "Hammers"], "played songs only, most recent first");
    for &i in &played {
        assert!(has(&rt, &format!("recent-songs-t{i}")), "row for song {i} shows");
    }
}

#[test]
fn recently_played_ties_break_by_library_order_and_never_played_songs_are_left_out() {
    // A fresh library with no earlier plays, so only the songs played below are listed.
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    app.show_home();
    // More songs than the page lists, to be played at one moment.
    let mut tracks = app.tracks().to_vec();
    let demo = tracks.len();
    for n in 0..RECENT_COUNT + 1 {
        tracks.push(Track { path: format!("/extra/{n:02}.flac").into(), title: format!("Extra {n}"), ..Track::default() });
    }
    app.set_library(tracks, false);
    let mut rt = Runtime::new(app, view, (1200.0, 900.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    // Every extra song played at one moment: the list keeps the first, in library order.
    let at = 1_800_000_000;
    for i in demo..demo + RECENT_COUNT + 1 {
        rt.update(|s, _| {
            let _ = s.record_play(i, at);
        });
    }
    let played = rt.state.recently_played();
    assert_eq!(played.len(), RECENT_COUNT, "RECENT_COUNT at most");
    assert!(played.iter().all(|&i| rt.state.tracks()[i].last_played == Some(at)), "all tied");
    assert!(played.windows(2).all(|w| w[0] < w[1]), "ties by library order, ascending");
    assert_eq!(played, (demo..demo + RECENT_COUNT).collect::<Vec<_>>(), "never-played songs are left out; ties keep library order");
}

#[test]
fn double_clicking_a_recently_played_song_plays_the_list_from_it() {
    let mut rt = runtime();
    let eta = track(&rt, "ETA");
    double_click(&mut rt, &format!("recent-songs-t{eta}"));
    assert_eq!(rt.state.current, Some(eta));
    let list = rt.state.recently_played();
    let next = list[list.iter().position(|&i| i == eta).unwrap_or_else(|| panic!("ETA is listed")) + 1];
    rt.update(|s, tx| s.next(tx));
    settle(&mut rt);
    assert_eq!(rt.state.current, Some(next), "the next row is what plays next");
}

#[test]
fn a_single_click_selects_a_recently_played_song_without_playing_it() {
    let mut rt = runtime();
    let says = track(&rt, "Says");
    click(&mut rt, &format!("recent-songs-t{says}"));
    assert_eq!(rt.state.selected, Some(says));
    assert_eq!(rt.state.current, None);
}

#[test]
fn an_album_opened_from_home_keeps_home_highlighted_and_back_returns() {
    let mut rt = runtime();
    let spaces = album(&rt, "Spaces");
    click(&mut rt, &format!("recent-albums{spaces}"));
    assert_eq!(rt.state.page, Page::Album(rt.state.albums()[spaces].id()));
    assert!(rt.state.in_home(), "Home stays highlighted on a page opened from it");
    assert!(!rt.state.in_graph());
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Home);
    // Home has no Back button; Back from it does nothing (the mouse's Back button and Escape both reach go_back).
    rt.update(|s, tx| s.go_back(tx));
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Home, "Back stops at Home");
    assert!(rt.state.history.is_empty());
}

#[test]
fn a_card_on_home_opens_its_album() {
    let mut rt = runtime();
    let live = album(&rt, "Live Sessions");
    click(&mut rt, &format!("recent-albums{live}"));
    assert_eq!(rt.state.open_album(), Some(live));
    assert!(rt.state.in_home());
}

#[test]
fn escape_on_home_does_nothing() {
    let mut rt = runtime();
    escape(&mut rt);
    assert_eq!(rt.state.page, Page::Home);
    assert!(rt.state.history.is_empty());
}

#[test]
fn an_empty_library_says_so_on_both_sections() {
    let mut rt = runtime();
    rt.update(|s, _| s.set_library(Vec::new(), false));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "recent-albums-empty"), "No albums in the library yet.");
    assert_eq!(text_of(&rt, "recent-songs-empty"), "Nothing played yet.");
    assert!(!has(&rt, "recent-songs-table"));
}

#[test]
fn the_chosen_tab_is_remembered_across_launches() {
    let dir = std::env::temp_dir().join(format!("peri-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut rt = {
        let mut app = App::new(Output::Silent, Store::at(&file));
        crate::scene_home(&mut app);
        let mut rt = Runtime::new(app, view, (1200.0, 900.0), Env::new(true, 1.0));
        rt.frame(0.0);
        settle(&mut rt);
        rt
    };
    click(&mut rt, "library-tab");
    let next = App::new(Output::Silent, Store::at(&file));
    assert_eq!(next.page, Page::Library, "the Library tab is what was open");
    let mut rt = Runtime::new(next, view, (1200.0, 900.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    click(&mut rt, "graph-tab");
    assert_eq!(App::new(Output::Silent, Store::at(&file)).page, Page::Graph);
    click(&mut rt, "home-tab");
    assert_eq!(App::new(Output::Silent, Store::at(&file)).page, Page::Home);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unreadable_saved_tab_opens_the_library_and_says_so() {
    let dir = std::env::temp_dir().join(format!("peri-home-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    let file = dir.join("prefs.json");
    std::fs::write(&file, r#"{"tab": "sideways"}"#).unwrap_or_else(|e| panic!("{e}"));
    let app = App::new(Output::Silent, Store::at(&file));
    assert_eq!(app.page, Page::Library);
    assert!(app.error.as_deref().is_some_and(|e| e.contains("\"tab\"")), "{:?}", app.error);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unset_tab_loads_as_none() {
    assert_eq!(Store::memory().load_tab(), Ok(None));
}

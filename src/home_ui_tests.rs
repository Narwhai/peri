//! The Home tab driven headlessly: its place in the header, the recently added albums and
//! played songs, playing from the list, opening pages from it, and the tab remembered.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, RECENT_ALBUMS_COUNT, RECENT_SONGS_COUNT, UNLISTENED_COUNT, UnlistenedOrder};
use crate::query::store::Store;
use crate::library::Track;
use crate::view::view;

/// The demo library with `last_played` set on some songs, on the Home tab.
fn runtime() -> Runtime<App> {
    // Tall enough for the Home page's blocks, so the controls and cards a test clicks are on screen.
    runtime_sized((1200.0, 1800.0))
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

/// Double-clicks the point `(x, y)`.
fn double_click(rt: &mut Runtime<App>, (x, y): (f32, f32)) {
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
    let mut rt = runtime_sized((640.0, 1800.0));
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
    for n in 0..RECENT_ALBUMS_COUNT + 2 {
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
    assert_eq!(newest.len(), RECENT_ALBUMS_COUNT);
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
    for n in 0..RECENT_SONGS_COUNT + 1 {
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
    for i in demo..demo + RECENT_SONGS_COUNT + 1 {
        rt.update(|s, _| {
            let _ = s.record_play(i, at);
        });
    }
    let played = rt.state.recently_played();
    assert_eq!(played.len(), RECENT_SONGS_COUNT, "RECENT_SONGS_COUNT at most");
    assert!(played.iter().all(|&i| rt.state.tracks()[i].last_played == Some(at)), "all tied");
    assert!(played.windows(2).all(|w| w[0] < w[1]), "ties by library order, ascending");
    assert_eq!(played, (demo..demo + RECENT_SONGS_COUNT).collect::<Vec<_>>(), "never-played songs are left out; ties keep library order");
}

#[test]
fn double_clicking_a_recently_played_song_plays_the_list_from_it() {
    // Taller than the stats, top and people blocks, so the Recently Played rows are on screen.
    let mut rt = runtime_sized((1200.0, 1800.0));
    let eta = track(&rt, "ETA");
    let at = title_of(&rt, &format!("recent-songs-t{eta}"));
    double_click(&mut rt, at);
    assert_eq!(rt.state.current, Some(eta));
    let list = rt.state.recently_played();
    let next = list[list.iter().position(|&i| i == eta).unwrap_or_else(|| panic!("ETA is listed")) + 1];
    rt.update(|s, tx| s.next(tx));
    settle(&mut rt);
    assert_eq!(rt.state.current, Some(next), "the next row is what plays next");
}

#[test]
fn a_single_click_selects_a_recently_played_song_without_playing_it() {
    let mut rt = runtime_sized((1200.0, 1600.0));
    let says = track(&rt, "Says");
    let (x, y) = title_of(&rt, &format!("recent-songs-t{says}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
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

/// One unplayed song on its own album `title`, added at `added`.
fn album_track(title: &str, added: Option<i64>) -> Track {
    Track { path: format!("/unlistened/{title}.flac").into(), title: title.into(), album: Some(title.into()), album_artist: Some("Tester".into()), added, ..Track::default() }
}

/// Titles of the albums `list` names.
fn album_titles(rt: &Runtime<App>, list: &[usize]) -> Vec<String> {
    list.iter().map(|&a| rt.state.albums()[a].title.clone()).collect()
}

#[test]
fn recently_played_shows_at_most_recent_songs_rows_each_with_a_cover() {
    let mut rt = runtime();
    let count = rt.state.tracks().len();
    rt.update(|s, _| {
        for i in 0..count {
            let _ = s.record_play(i, 1_800_000_100 + i as i64);
        }
    });
    settle(&mut rt);
    let rows = rt.state.recently_played();
    assert_eq!(rows.len(), RECENT_SONGS_COUNT);
    for i in 0..count {
        assert_eq!(has(&rt, &format!("recent-songs-t{i}")), rows.contains(&i), "song {i} has a row only when listed");
    }
    for &i in &rows {
        assert!(has(&rt, &format!("recent-cover{i}")), "row {i} shows its album cover");
    }
}

#[test]
fn unlistened_lists_only_albums_whose_songs_all_have_no_plays() {
    let rt = runtime();
    // Spaces, Get Up and Live Sessions each have a played song, so only Day of Radiance is left.
    let unlistened = rt.state.unlistened_albums();
    assert_eq!(album_titles(&rt, &unlistened), ["Day of Radiance"]);
    assert!(has(&rt, &format!("unlistened-albums{}", unlistened[0])), "its card shows");
}

#[test]
fn unlistened_newest_and_oldest_put_undated_albums_last() {
    let mut rt = runtime();
    let tracks = vec![album_track("Mid", Some(2005)), album_track("Undated", None), album_track("Late", Some(2010)), album_track("Early", Some(1990))];
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let newest = rt.state.unlistened(UnlistenedOrder::Newest, 0);
    assert_eq!(album_titles(&rt, &newest), ["Late", "Mid", "Early", "Undated"]);
    let oldest = rt.state.unlistened(UnlistenedOrder::Oldest, 0);
    assert_eq!(album_titles(&rt, &oldest), ["Early", "Mid", "Late", "Undated"]);
}

#[test]
fn unlistened_keeps_at_most_unlistened_count_albums() {
    let mut rt = runtime();
    let tracks: Vec<Track> = (0..UNLISTENED_COUNT + 5).map(|n| album_track(&format!("Album {n:02}"), Some(1_000 + n as i64))).collect();
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let newest = rt.state.unlistened_albums();
    assert_eq!(newest.len(), UNLISTENED_COUNT);
    assert_eq!(rt.state.albums()[newest[0]].title, format!("Album {:02}", UNLISTENED_COUNT + 4), "newest first");
    assert!(has(&rt, &format!("unlistened-albums{}", newest[0])), "the first card shows");
}

/// The cards of the Discover grid keyed `wall`, row by row, as their rects.
fn grid_rows(rt: &Runtime<App>, wall: &str) -> Vec<Vec<[f32; 4]>> {
    let mut rows = Vec::new();
    while let Some(r) = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("{wall}-row{}", rows.len()))) {
        let [x, y, w, h] = r.rect;
        let cards: Vec<[f32; 4]> = rt
            .elements()
            .into_iter()
            .filter(|e| e.key.as_deref().is_some_and(|k| k.strip_prefix("unlistened-albums").is_some_and(|n| n.parse::<usize>().is_ok())))
            .map(|e| e.rect)
            .filter(|c| c[0] >= x - 0.5 && c[0] + c[2] <= x + w + 0.5 && c[1] >= y - 0.5 && c[1] + c[3] <= y + h + 0.5)
            .collect();
        rows.push(cards);
    }
    rows
}

#[test]
fn the_discover_grid_fills_whole_rows_flush_with_both_edges_at_any_width() {
    let mut rt = runtime();
    let tracks: Vec<Track> = (0..UNLISTENED_COUNT + 5).map(|n| album_track(&format!("Album {n:02}"), Some(1_000 + n as i64))).collect();
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let list = rt.state.unlistened_albums();
    for width in [720.0, 900.0, 1013.0, 1200.0, 1600.0, 2400.0] {
        rt.set_viewport((width, 1800.0), 1.0);
        settle(&mut rt);
        let wall = rect(&rt, "unlistened-albums-wall");
        let columns = crate::view::columns_for(wall[2], 200.0);
        let shown: Vec<usize> = list.iter().copied().filter(|a| has(&rt, &format!("unlistened-albums{a}"))).collect();
        assert_eq!(shown.len(), columns * 3, "{width}: three full rows of {columns}");
        assert_eq!(shown, list[..shown.len()], "{width}: the list's first albums, in order");
        let rows = grid_rows(&rt, "unlistened-albums-wall");
        assert_eq!(rows.len(), 3, "{width}: three rows");
        for (n, cards) in rows.iter().enumerate() {
            assert_eq!(cards.len(), columns, "{width}: row {n} is full");
            let left = cards.iter().map(|c| c[0]).fold(f32::MAX, f32::min);
            let right = cards.iter().map(|c| c[0] + c[2]).fold(f32::MIN, f32::max);
            assert!((left - wall[0]).abs() < 1.0, "{width}: row {n} starts at the left edge");
            assert!((right - (wall[0] + wall[2])).abs() < 1.0, "{width}: row {n} reaches the right edge");
            assert!(cards.iter().all(|c| c[2] == 200.0), "{width}: cards keep their width");
        }
    }
}

#[test]
fn unlistened_order_is_chosen_from_the_segmented_control_and_saved() {
    let dir = std::env::temp_dir().join(format!("peri-unlistened-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    app.set_library(vec![album_track("Mid", Some(2005)), album_track("Late", Some(2010)), album_track("Early", Some(1990))], false);
    app.show_home();
    let mut rt = Runtime::new(app, view, (1200.0, 1800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    assert_eq!(rt.state.unlistened_order, UnlistenedOrder::Newest, "Newest unless chosen");
    click(&mut rt, "oldest");
    assert_eq!(rt.state.unlistened_order, UnlistenedOrder::Oldest);
    assert_eq!(album_titles(&rt, &rt.state.unlistened_albums()), ["Early", "Mid", "Late"]);
    let next = App::new(Output::Silent, Store::at(&file));
    assert_eq!(next.unlistened_order, UnlistenedOrder::Oldest, "the order is saved");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_shuffle_button_shows_only_in_random_and_rerolls_the_seed() {
    let mut rt = runtime_sized((1200.0, 1600.0));
    assert!(!has(&rt, "unlistened-reroll"), "no shuffle in Newest");
    click(&mut rt, "random");
    assert_eq!(rt.state.unlistened_order, UnlistenedOrder::Random);
    assert!(has(&rt, "unlistened-reroll"), "shuffle shows in Random");
    let seed = rt.state.unlistened_seed;
    click(&mut rt, "unlistened-reroll");
    assert_ne!(rt.state.unlistened_seed, seed, "a re-roll sets a new seed");
    click(&mut rt, "newest");
    assert!(!has(&rt, "unlistened-reroll"), "no shuffle once Newest is back");
}

#[test]
fn random_order_is_deterministic_for_a_seed_and_capped() {
    let mut rt = runtime();
    let tracks: Vec<Track> = (0..UNLISTENED_COUNT + 5).map(|n| album_track(&format!("Album {n:02}"), Some(1_000 + n as i64))).collect();
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let first = rt.state.unlistened(UnlistenedOrder::Random, 7);
    assert_eq!(first, rt.state.unlistened(UnlistenedOrder::Random, 7), "the same seed gives the same order");
    assert_eq!(first.len(), UNLISTENED_COUNT);
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), UNLISTENED_COUNT, "no album twice");
    assert_ne!(first, rt.state.unlistened(UnlistenedOrder::Random, 8), "another seed shuffles differently");
}

#[test]
fn unlistened_says_so_once_every_album_has_been_played() {
    let mut rt = runtime();
    let count = rt.state.tracks().len();
    rt.update(|s, _| {
        for i in 0..count {
            let _ = s.record_play(i, 1_800_000_100 + i as i64);
        }
    });
    settle(&mut rt);
    assert_eq!(text_of(&rt, "unlistened-albums-empty"), "Every album has been played.");
    assert!(!has(&rt, "unlistened-albums-wall"));
}

#[test]
fn the_stats_strip_sits_below_the_discover_block_and_shows_the_totals() {
    let rt = runtime();
    let strip = rect(&rt, "stats");
    let discover = rect(&rt, "discover");
    assert!(strip[1] > discover[1], "the strip is below the Discover block");
    let totals = rt.state.totals();
    assert_eq!(text_of(&rt, "stat-songs"), totals.songs.to_string(), "the Songs tile figure");
    assert_eq!(text_of(&rt, "stat-albums"), totals.albums.to_string());
    assert_eq!(text_of(&rt, "stat-artists"), totals.artists.to_string());
    assert_eq!(text_of(&rt, "stat-hours"), format!("{:.1}", totals.hours()));
    let share = totals.played_share().expect("the demo has albums");
    assert_eq!(text_of(&rt, "stat-albums-played"), format!("{:.0}%", share * 100.0));
}

#[test]
fn an_empty_library_says_so_in_the_stats_strip() {
    let mut rt = runtime();
    rt.update(|s, _| s.set_library(Vec::new(), false));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "stats-empty"), "No songs in the library yet.");
    assert!(!has(&rt, "stats-tiles"));
}

/// Each demo song's play, now-ish, so the 7 and 30 day windows have plays to show.
fn play_demo_songs_now(rt: &mut Runtime<App>) {
    let now = crate::listen::unix_now().expect("the clock");
    let count = rt.state.tracks().len();
    rt.update(move |s, _| {
        for i in 0..count {
            let _ = s.record_play(i, now - 60);
        }
    });
    settle(rt);
}

#[test]
fn the_top_block_shows_three_columns_of_the_leaders() {
    let rt = runtime();
    assert!(has(&rt, "top-artists") && has(&rt, "top-albums") && has(&rt, "top-songs"));
    assert_eq!(rt.state.top_window, crate::top::Window::All, "all time unless chosen");
    let first = rt.state.top_model().songs[0].item;
    assert!(has(&rt, "top-song0"), "the most played song is listed first");
    assert!(rt.find("top-song0").is_some(), "its row shows");
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(rt.state.tracks()[first].title.as_str())), "the row names the song");
}

#[test]
fn an_empty_window_says_no_plays_in_it() {
    // A library with songs but no plays: the demo's plays are stamped in 2027, after today.
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    app.show_home();
    let mut rt = Runtime::new(app, view, (1200.0, 1600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    click(&mut rt, "7");
    assert_eq!(rt.state.top_window, crate::top::Window::Days7);
    assert_eq!(text_of(&rt, "top-empty"), "No plays in the 7 days.");
    assert!(!has(&rt, "top-columns"));
}

#[test]
fn switching_the_window_follows_recent_plays_and_is_saved() {
    let dir = std::env::temp_dir().join(format!("peri-top-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_home(&mut app);
    let mut rt = Runtime::new(app, view, (1200.0, 1800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    play_demo_songs_now(&mut rt);
    click(&mut rt, "7");
    assert!(has(&rt, "top-columns"), "recent plays show in the 7 days window");
    assert_eq!(App::new(Output::Silent, Store::at(&file)).top_window, crate::top::Window::Days7, "the window is saved");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_top_artist_or_album_row_opens_its_page() {
    let mut rt = runtime();
    let artist = rt.state.top_model().artists[0].item.clone();
    click(&mut rt, "top-artist0");
    assert_eq!(rt.state.open_artist(), Some(artist.as_str()), "the artist's page opens");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Home);
    let album = rt.state.top_model().albums[0].item;
    click(&mut rt, "top-album0");
    assert_eq!(rt.state.open_album(), Some(album), "the album opens");
}

/// A fresh library with the demo's songs and no plays: the demo's are stamped in 2027, after
/// today, so they'd count in every window.
fn unplayed_home() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    app.show_home();
    let mut rt = Runtime::new(app, view, (1200.0, 1600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Plays every song of `rt`'s library once, `at` seconds since the epoch.
fn play_all_at(rt: &mut Runtime<App>, at: i64) {
    let count = rt.state.tracks().len();
    rt.update(move |s, _| {
        for i in 0..count {
            let _ = s.record_play(i, at);
        }
    });
    settle(rt);
}

#[test]
fn the_people_block_lists_credited_names_by_role_and_links_them() {
    let mut rt = unplayed_home();
    assert_eq!(text_of(&rt, "people-empty"), "No credited names played in the window.", "no plays, no names");
    // Credit the first song, then play it: the names it carries are the ones listed.
    rt.update(|s, _| s.edit_track(0, |t| t.credits = vec![crate::credits::Credit::parse(crate::credits::Role::Producer, "Kenny Segal")]));
    let now = crate::listen::unix_now().expect("the clock");
    rt.update(move |s, _| {
        let _ = s.record_play(0, now - 60);
    });
    settle(&mut rt);
    let boards = rt.state.people_model();
    assert!(!boards.is_empty(), "the played song's credits are counted");
    assert!(has(&rt, "people"), "the block shows");
    assert!(!has(&rt, "people-empty"));
    let first = "Kenny Segal".to_string();
    let key = "people-producer-0".to_string();
    assert!(has(&rt, &key), "the producer's name shows");
    click(&mut rt, &key);
    assert_eq!(rt.state.open_artist(), Some(first.as_str()), "a name opens its artist's page");
}

#[test]
fn the_people_block_follows_the_window() {
    let mut rt = unplayed_home();
    let year_ago = crate::listen::unix_now().expect("the clock") - 365 * 86_400;
    play_all_at(&mut rt, year_ago);
    click(&mut rt, "7");
    assert_eq!(text_of(&rt, "people-empty"), "No credited names played in the window.", "a year-old play isn't in 7 days");
    click(&mut rt, "all");
    assert!(has(&rt, "people"), "all time shows them");
}

#[test]
fn the_discovery_block_opens_on_unlistened_with_its_order_control() {
    let rt = runtime();
    assert_eq!(rt.state.discover, crate::discover::Tab::Unlistened, "Unlistened unless chosen");
    assert!(has(&rt, "discover"), "the block shows");
    assert!(has(&rt, "unlistened-albums"), "the Unlistened tab keeps its albums");
    assert!(has(&rt, "unlistened-order"), "and its order control");
    assert!(has(&rt, "discover-tab"), "the tab switch shows");
}

#[test]
fn each_discovery_tab_shows_its_albums_or_says_why_there_are_none() {
    let mut rt = unplayed_home();
    click(&mut rt, "continue");
    assert_eq!(rt.state.discover, crate::discover::Tab::Continue);
    assert_eq!(text_of(&rt, "discover-continue-empty"), "No album is part way through yet.", "nothing started yet");
    click(&mut rt, "rediscover");
    assert_eq!(text_of(&rt, "discover-rediscover-empty"), "No album has 5 plays and was last played more than 60 days ago.");
    click(&mut rt, "new");
    assert!(has(&rt, "discover-new-wall") || has(&rt, "discover-new-empty"), "the New tab shows one or the other");
    click(&mut rt, "unlistened");
    assert!(has(&rt, "unlistened-albums-wall"), "back on Unlistened, its albums are back");
}

#[test]
fn a_discovery_tab_is_remembered_across_launches() {
    let dir = std::env::temp_dir().join(format!("peri-discover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    app.set_library(crate::demo_tracks(), false);
    app.show_home();
    let mut rt = Runtime::new(app, view, (1200.0, 1600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    click(&mut rt, "rediscover");
    assert_eq!(App::new(Output::Silent, Store::at(&file)).discover, crate::discover::Tab::Rediscover, "the tab is saved");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unlistened_album_moves_to_continue_once_part_of_it_plays() {
    let mut rt = unplayed_home();
    let album_songs: Vec<usize> = {
        let albums = rt.state.albums();
        let big = albums.iter().max_by_key(|a| a.tracks.len()).expect("albums");
        big.tracks.clone()
    };
    assert!(album_songs.len() >= 2, "the demo has an album with several songs");
    let first = album_songs[0];
    let title = rt.state.albums()[rt.state.album_of(first).expect("its album")].title.clone();
    let now = crate::listen::unix_now().expect("the clock");
    rt.update(move |s, _| {
        let _ = s.record_play(first, now - 60);
    });
    settle(&mut rt);
    click(&mut rt, "continue");
    let shown = rt.state.discover_albums(crate::discover::Tab::Continue);
    let titles: Vec<String> = shown.iter().map(|&a| rt.state.albums()[a].title.clone()).collect();
    assert!(titles.contains(&title), "a part-played album is in Continue: {titles:?}");
}

/// Today in the local time zone, as days since the epoch, and the zone's offset.
fn local_today() -> (i64, i64) {
    let now = crate::listen::unix_now().expect("the clock");
    let offset = crate::activity::local_offset(now);
    (crate::activity::day_of(now, offset), offset)
}

/// A fresh home page with a play `days_ago` local days back from today, for each entry. Each
/// play lands on the demo's songs at local noon, so its day doesn't depend on the time of day.
fn home_with_play_days(days_ago: &[i64]) -> Runtime<App> {
    let mut rt = unplayed_home();
    let (today, offset) = local_today();
    let plays: Vec<i64> = days_ago.iter().map(|&d| (today - d) * 86_400 + 12 * 3600 - offset).collect();
    rt.update(move |s, _| {
        for (n, at) in plays.iter().enumerate() {
            let _ = s.record_play(n % s.tracks().len(), *at);
        }
    });
    settle(&mut rt);
    rt
}

#[test]
fn the_activity_block_shows_the_heatmap_and_streaks() {
    let rt = home_with_play_days(&[0, 1, 2, 3, 10]);
    assert!(has(&rt, "activity-grid"), "the grid shows with enough days");
    assert_eq!(text_of(&rt, "activity-streaks"), "Current streak 4 days  •  Longest 4 days", "four days running to today");
    assert!(has(&rt, "stat-streak"), "the streak tile shows in the stats strip");
    assert_eq!(text_of(&rt, "stat-streak"), "4 days");
}

#[test]
fn a_few_active_days_show_a_note_instead_of_the_heatmap() {
    let rt = home_with_play_days(&[0, 5]);
    assert!(!has(&rt, "activity-grid"), "two days aren't enough for a heatmap");
    assert_eq!(text_of(&rt, "activity-empty"), "Play a few more days to see your activity here.");
}

#[test]
fn a_lapsed_streak_is_not_current() {
    let rt = home_with_play_days(&[10, 11, 12, 13]);
    assert_eq!(text_of(&rt, "activity-streaks"), "Current streak 0 days  •  Longest 4 days");
}

#[test]
fn heatmap_cells_show_for_days_with_plays_and_are_keyed_by_day() {
    let rt = home_with_play_days(&[0, 1, 2]);
    let (today, _) = local_today();
    assert!(has(&rt, &format!("activity-day-{today}")), "today has a cell");
    assert!(has(&rt, &format!("activity-day-{}", today - 2)), "a day with plays has a cell");
}

#[test]
fn the_mix_shows_genres_and_decades_labelled_by_their_source() {
    let mut rt = unplayed_home();
    assert!(has(&rt, "mix"), "the block shows");
    assert_eq!(text_of(&rt, "mix-genres-title"), "Genres, of the library (nothing played yet)", "the library's share with no plays");
    assert!(has(&rt, "mix-genres-bar0"), "a bar per genre");
    let now = crate::listen::unix_now().expect("the clock");
    play_all_at(&mut rt, now - 60);
    assert_eq!(text_of(&rt, "mix-genres-title"), "Genres, of plays", "plays once there are some");
    assert_eq!(text_of(&rt, "mix-decades-title"), "Decades, of plays");
}

#[test]
fn a_mix_with_no_songs_says_so() {
    let mut rt = unplayed_home();
    rt.update(|s, _| s.set_library(Vec::new(), false));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "mix-genres-empty"), "No genres yet.");
    assert_eq!(text_of(&rt, "mix-decades-empty"), "No decades yet.");
    assert!(!has(&rt, "mix-genres-bar0"));
}


#[test]
fn the_mix_follows_the_plays_and_its_bars_are_proportional() {
    let mut rt = unplayed_home();
    let now = crate::listen::unix_now().expect("the clock");
    rt.update(move |s, _| {
        let _ = s.record_play(0, now - 60);
    });
    settle(&mut rt);
    let mix = rt.state.mix_model();
    let top = mix.0.bars.first().expect("the played song has a genre");
    assert_eq!(top.share, 1.0, "the only played song's genre has all the plays");
    assert!(has(&rt, "mix-genres-bar0"));
    assert!((rect(&rt, "mix-genres-bar0")[2] - rect(&rt, "mix-genres-track0")[2]).abs() < 1.0, "a full bar fills its track");
}

/// Scrolls Home down by `dy` points.
fn scroll_home(rt: &mut Runtime<App>, dy: f32) {
    let (x, y) = rt.find("home-page").expect("Home shows");
    rt.wheel(x, y, -dy);
    settle(rt);
}

#[test]
fn back_from_a_name_in_the_people_block_returns_home_to_where_it_was() {
    let mut rt = unplayed_home();
    rt.set_viewport((1200.0, 900.0), 1.0);
    rt.update(|s, _| s.edit_track(0, |t| t.credits = vec![crate::credits::Credit::parse(crate::credits::Role::Producer, "Kenny Segal")]));
    let now = crate::listen::unix_now().expect("the clock");
    rt.update(move |s, _| {
        let _ = s.record_play(0, now - 60);
    });
    settle(&mut rt);
    scroll_home(&mut rt, 400.0);
    let before = rt.state.home_scroll.get()[1];
    assert!(before > 300.0, "Home scrolled, to {before}");
    click(&mut rt, "people-producer-0");
    assert_eq!(rt.state.open_artist(), Some("Kenny Segal"), "the name opened its artist's page");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Home);
    let after = rt.state.home_scroll.get()[1];
    assert!((after - before).abs() < 1.0, "back at {before}, not {after}");
    assert!(has(&rt, "people-producer-0"), "the same name shows again");
}

#[test]
fn the_home_tab_still_opens_at_the_top_from_another_tab() {
    let mut rt = runtime_sized((1200.0, 900.0));
    scroll_home(&mut rt, 1200.0);
    click(&mut rt, "library-tab");
    click(&mut rt, "home-tab");
    assert_eq!(rt.state.page, Page::Home);
    assert!(rt.state.home_scroll.get()[1] < 1.0, "the Home tab opens at the top, at {}", rt.state.home_scroll.get()[1]);
}

/// The middle of the title in the row keyed `row`: a plain spot of the row, away from the
/// name links in its columns.
fn title_of(rt: &Runtime<App>, row: &str) -> (f32, f32) {
    let e = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("title") && e.path.contains(&format!("/{row}/"))).unwrap_or_else(|| panic!("no title in {row}"));
    (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0)
}

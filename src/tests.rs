//! The window driven headlessly: clicks, keys and what the view shows.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page};
use crate::queue::Repeat;
use crate::view::view;

fn runtime(dark: bool) -> Runtime<App> {
    let mut app = App::new(Output::Silent, crate::base::store::Store::memory());
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(dark, 1.0));
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

/// Clicks a song row where it has no links.
fn click_row(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = row_point(rt, key);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn key(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

/// The library index of the demo song titled `title`.
fn idx(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.tracks.iter().position(|t| t.title == title).unwrap_or_else(|| panic!("no song {title}"))
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

#[test]
fn the_library_lists_every_track_and_counts_them() {
    let rt = runtime(true);
    assert_eq!(text_of(&rt, "count"), "13 tracks");
    assert!(rt.find("t0").is_some());
    assert!(rt.find("t10").is_some());
}

#[test]
fn double_clicking_a_row_plays_it_and_fills_the_bar() {
    let mut rt = runtime(true);
    let shy = idx(&rt, "Super Shy");
    let (x, y) = row_point(&rt, &format!("t{shy}"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
    }
    settle(&mut rt);
    assert_eq!(rt.state.current, Some(shy));
    assert!(rt.state.playing);
    assert_eq!(text_of(&rt, "np-title"), "Super Shy");
    assert_eq!(text_of(&rt, "total"), "2:34");
}

#[test]
fn space_plays_the_selection_and_pauses() {
    let mut rt = runtime(false);
    click_row(&mut rt, "t2");
    assert_eq!(rt.state.selected, Some(2));
    assert_eq!(rt.state.current, None);
    key(&mut rt, Key::Text(" ".into()));
    assert_eq!((rt.state.current, rt.state.playing), (Some(2), true));
    key(&mut rt, Key::Text(" ".into()));
    assert!(!rt.state.playing);
}

#[test]
fn next_and_previous_walk_the_list_and_stop_at_the_end() {
    let mut rt = runtime(true);
    let (last, before) = (rt.state.filtered[12], rt.state.filtered[11]);
    // Arrow keys move the selection and scroll it into view.
    for _ in 0..12 {
        key(&mut rt, Key::Down);
    }
    assert_eq!(rt.state.selected, Some(before));
    key(&mut rt, Key::Enter);
    click(&mut rt, "next");
    assert_eq!(rt.state.current, Some(last));
    click(&mut rt, "next");
    assert!(!rt.state.playing, "past the last track playback stops");
    assert_eq!(rt.state.current, Some(last));
    click(&mut rt, "prev");
    assert_eq!(rt.state.current, Some(before));
}

#[test]
fn searching_opens_results_that_back_returns_to_and_clearing_leaves() {
    let mut rt = runtime(true);
    click(&mut rt, "search");
    for c in "nils".chars() {
        key(&mut rt, Key::Text(c.to_string()));
    }
    assert_eq!(rt.state.page, Page::Search, "typing opens the results");
    assert!(rt.find("toolbar").is_none(), "the view's toolbar hides on the results");
    assert_eq!(text_of(&rt, "artist-name"), "Nils Frahm");
    assert!(rt.find("artist-albums").is_some(), "her albums show like on the artist page");
    assert!(rt.find("artist-songs").is_some());
    // Results update as the query changes.
    key(&mut rt, Key::Backspace);
    key(&mut rt, Key::Backspace);
    key(&mut rt, Key::Backspace);
    key(&mut rt, Key::Backspace);
    for c in "radiance".chars() {
        key(&mut rt, Key::Text(c.to_string()));
    }
    assert!(rt.find("artist-name").is_none(), "no artist is named radiance");
    assert!(rt.find("search-albums").is_some());
    // An album opened from the results goes back to them.
    let a = rt.state.albums.iter().position(|a| a.title == "Day of Radiance").expect("Day of Radiance");
    rt.update(|s, tx| s.show_album(a, tx));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "page-title"), "Day of Radiance");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Search);
    // Typing on another page returns to the results instead of stacking more.
    rt.update(|s, tx| s.show_artist("Laraaji", tx));
    settle(&mut rt);
    click(&mut rt, "search");
    key(&mut rt, Key::Text("x".into()));
    assert_eq!(rt.state.page, Page::Search);
    assert!(rt.state.history.is_empty());
    // Esc clears the search and leaves the results.
    key(&mut rt, Key::Escape);
    assert_eq!(rt.state.search.text, "");
    assert_eq!(rt.state.page, Page::Library);
    assert_eq!(text_of(&rt, "count"), "13 tracks", "the view isn't filtered by the search");
    assert!(rt.find("toolbar").is_some(), "the toolbar returns with the view");
}

#[test]
fn home_returns_to_the_views_and_clears_the_search() {
    let mut rt = runtime(true);
    rt.update(|s, tx| {
        s.search = rune::TextEdit::new("laraaji");
        s.searched(tx);
    });
    settle(&mut rt);
    click(&mut rt, "open-artist");
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
    click(&mut rt, "home");
    assert_eq!(rt.state.page, Page::Library);
    assert!(rt.state.history.is_empty());
    assert_eq!(rt.state.search.text, "");
}

#[test]
fn shuffle_and_repeat_toggle_from_their_buttons() {
    let mut rt = runtime(true);
    click(&mut rt, "shuffle");
    assert!(rt.state.shuffle);
    click(&mut rt, "repeat");
    click(&mut rt, "repeat");
    assert_eq!(rt.state.repeat, Repeat::One);
}

#[test]
fn clicking_the_volume_slider_sets_the_volume_where_clicked() {
    let mut rt = runtime(true);
    let info = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("volume")).unwrap();
    let [x, y, w, h] = info.rect;
    rt.pointer_down(x + w * 0.25, y + h / 2.0);
    rt.pointer_up(x + w * 0.25, y + h / 2.0);
    settle(&mut rt);
    assert!((rt.state.volume - 0.25).abs() < 0.02, "volume {}", rt.state.volume);
}

#[test]
fn an_error_shows_and_dismisses() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.error = Some("Can't play x.mp3".into()));
    settle(&mut rt);
    click(&mut rt, "dismiss");
    assert_eq!(rt.state.error, None);
}

/// Scans a real folder and decodes a file of each format, without sound:
/// `RUNE_MUSIC_DIR=~/Music cargo test --release real_library -- --ignored --nocapture`.
#[test]
#[ignore = "needs RUNE_MUSIC_DIR"]
fn real_library_scans_and_every_format_decodes() {
    use rodio::Source;
    let dir = std::env::var("RUNE_MUSIC_DIR").expect("set RUNE_MUSIC_DIR to a music folder");
    let started = std::time::Instant::now();
    let rx = crate::library::scan(vec![dir.into()], || {});
    let (tracks, errors) = loop {
        if let crate::library::ScanMsg::Done { tracks, errors } = rx.recv().expect("the scan finishes") {
            break (tracks, errors);
        }
    };
    println!("scanned {} tracks in {:.2?}, {} unreadable", tracks.len(), started.elapsed(), errors.len());
    assert!(!tracks.is_empty());
    let mut seen = std::collections::BTreeSet::new();
    for t in &tracks {
        let ext = t.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !seen.insert(ext.clone()) {
            continue;
        }
        let file = std::fs::File::open(&t.path).expect("open");
        let decoder = rodio::Decoder::try_from(file);
        let cover = crate::library::read_cover(&t.path).map_or(0, |c| c.len());
        match decoder {
            Ok(d) => println!("{ext}: ok, {} Hz, tag duration {:?}, decoder duration {:?}, cover {cover} bytes — {}", d.sample_rate(), t.duration, d.total_duration(), t.title),
            Err(e) => panic!("{ext}: {} doesn't decode: {e}", t.path.display()),
        }
    }
}

/// Plays a real file through the default output, muted, and checks time moves, pause holds it
/// and seeking lands: `RUNE_MUSIC_FILE=song.flac cargo test --release real_output -- --ignored`.
#[test]
#[ignore = "needs RUNE_MUSIC_FILE and an audio device"]
fn real_output_plays_pauses_and_seeks() {
    use std::thread::sleep;
    use std::time::Duration;
    let file = std::env::var("RUNE_MUSIC_FILE").expect("set RUNE_MUSIC_FILE to an audio file");
    let mut engine = crate::audio::Engine::open().expect("open the default output");
    engine.load(std::path::Path::new(&file), 0.0).expect("load");
    sleep(Duration::from_millis(600));
    let played = engine.position().expect("loaded");
    assert!(played > 0.3, "time advances while playing: {played}");
    engine.pause();
    sleep(Duration::from_millis(100));
    let paused = engine.position().expect("loaded");
    sleep(Duration::from_millis(400));
    assert!((engine.position().expect("loaded") - paused).abs() < 0.05, "pause holds the position");
    engine.seek(20.0).expect("seek");
    assert!((engine.position().expect("loaded") - 20.0).abs() < 0.1);
    engine.play();
    sleep(Duration::from_millis(300));
    assert!(engine.position().expect("loaded") > 20.1);
    assert!(!engine.finished());
}

#[test]
fn clicking_the_search_field_takes_typing_and_letters_do_not_trigger_shortcuts() {
    let mut rt = runtime(true);
    click(&mut rt, "search");
    for c in "nils r".chars() {
        key(&mut rt, Key::Text(c.to_string()));
    }
    assert_eq!(rt.state.search.text, "nils r");
    assert_eq!(rt.state.repeat, Repeat::Off, "typing doesn't hit the R shortcut");
    assert_eq!(rt.state.page, Page::Search);
}

#[test]
fn space_still_plays_and_pauses_after_clicking_a_transport_button() {
    let mut rt = runtime(true);
    click_row(&mut rt, "t1");
    click(&mut rt, "shuffle");
    key(&mut rt, Key::Text(" ".into()));
    assert!(rt.state.shuffle, "Space didn't press the shuffle button again");
    assert_eq!((rt.state.current, rt.state.playing), (Some(1), true));
}

fn mouse(rt: &mut Runtime<App>, button: rune::PointerButton) {
    let (x, y) = rt.find("count").expect("the header's count");
    rt.button_down(x, y, button);
    rt.button_up(x, y, button);
    settle(rt);
}

fn alt(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods { alt: true, ..Mods::default() } });
    settle(rt);
}

#[test]
fn the_mouse_back_and_forward_buttons_walk_the_pages() {
    use rune::PointerButton::{Back, Forward};
    let mut rt = runtime(true);
    rt.update(|s, tx| s.show_artist("Laraaji", tx));
    settle(&mut rt);
    let a = rt.state.albums.iter().position(|a| a.title == "Day of Radiance").expect("Day of Radiance");
    rt.update(|s, tx| s.show_album(a, tx));
    settle(&mut rt);
    let album = rt.state.page.clone();
    mouse(&mut rt, Back);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
    mouse(&mut rt, Back);
    assert_eq!(rt.state.page, Page::Library);
    mouse(&mut rt, Back);
    assert_eq!(rt.state.page, Page::Library, "Back stops at the library");
    mouse(&mut rt, Forward);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
    mouse(&mut rt, Forward);
    assert_eq!(rt.state.page, album);
    mouse(&mut rt, Forward);
    assert_eq!(rt.state.page, album, "nothing further forward");
    mouse(&mut rt, Back);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()), "Back works after Forward");
}

#[test]
fn alt_arrows_go_back_and_forward_and_opening_a_page_drops_forward() {
    let mut rt = runtime(true);
    rt.update(|s, tx| s.show_artist("Laraaji", tx));
    settle(&mut rt);
    rt.update(|s, tx| s.show_label("All Saints", tx));
    settle(&mut rt);
    alt(&mut rt, Key::Left);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
    alt(&mut rt, Key::Right);
    assert_eq!(rt.state.page, Page::Label("All Saints".into()));
    alt(&mut rt, Key::Left);
    // A new page from here replaces what was ahead, as in a browser.
    click(&mut rt, "settings");
    assert_eq!(rt.state.page, Page::Settings);
    assert!(rt.state.forward.is_empty());
    alt(&mut rt, Key::Right);
    assert_eq!(rt.state.page, Page::Settings);
    alt(&mut rt, Key::Left);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
}

#[test]
fn forward_skips_search_results_that_were_cleared() {
    let mut rt = runtime(true);
    rt.update(|s, tx| {
        s.search = rune::TextEdit::new("laraaji");
        s.searched(tx);
    });
    settle(&mut rt);
    rt.update(|s, tx| s.show_artist("Laraaji", tx));
    settle(&mut rt);
    alt(&mut rt, Key::Left);
    assert_eq!(rt.state.page, Page::Search);
    // Esc clears the search, leaving the results: Forward can't come back to them.
    key(&mut rt, Key::Escape);
    assert_eq!(rt.state.page, Page::Library);
    assert!(!rt.state.forward.contains(&Page::Search));
    alt(&mut rt, Key::Right);
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()));
}

#[test]
fn a_left_click_on_empty_space_still_leaves_the_search_field_and_stays() {
    let mut rt = runtime(true);
    rt.update(|s, tx| s.show_artist("Laraaji", tx));
    settle(&mut rt);
    click(&mut rt, "search");
    assert!(rt.state.search.focused);
    click(&mut rt, "count");
    assert!(!rt.state.search.focused, "the field lets go of the keyboard");
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()), "a primary click doesn't navigate");
}

/// Reads the real library's new attributes and times grouping it:
/// `RUNE_MUSIC_DIR=~/Music cargo test --release real_library_groups -- --ignored --nocapture`.
#[test]
#[ignore = "needs RUNE_MUSIC_DIR"]
fn real_library_groups_by_every_property() {
    use crate::base::property::Property;
    use crate::base::view::{Direction, GroupBy, View, evaluate};
    let dir = std::env::var("RUNE_MUSIC_DIR").expect("set RUNE_MUSIC_DIR to a music folder");
    let rx = crate::library::scan(vec![dir.into()], || {});
    let mut tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    crate::credits::resolve(&mut tracks);
    use crate::credits::Role;
    let share = |f: &dyn Fn(&crate::library::Track) -> bool| 100 * tracks.iter().filter(|t| f(t)).count() / tracks.len();
    println!(
        "{} tracks: genre {}%, year {}%, album artist {}%, label {}%, composer {}%, producer {}%, several artists {}%, featuring {}%, several album artists {}%",
        tracks.len(),
        share(&|t| !t.genres.is_empty()),
        share(&|t| t.year.is_some()),
        share(&|t| t.album_artist.is_some()),
        share(&|t| t.label.is_some()),
        share(&|t| !t.credited(Role::Composer).is_empty()),
        share(&|t| !t.credited(Role::Producer).is_empty()),
        share(&|t| t.artists.len() > 1),
        share(&|t| !t.featured.is_empty()),
        share(&|t| t.album_artists.len() > 1),
    );
    for t in tracks.iter().filter(|t| !t.featured.is_empty() || t.album_artists.len() > 1).take(25) {
        println!("  {:?} by {:?}: artists {:?}, featured {:?}, album artists {:?}", t.title, t.artist(), t.artists, t.featured, t.album_artists);
    }
    for property in Property::ALL {
        let mut view = View::new("v");
        view.group_by = Some(GroupBy { property, direction: Direction::Asc });
        let started = std::time::Instant::now();
        let e = evaluate(&view, &tracks, "");
        let took = started.elapsed();
        let sample: Vec<String> = e.sections.iter().take(4).map(|s| format!("{} ({})", s.title, s.items.len())).collect();
        println!("{:>13}: {:>4} groups in {took:>9.2?}  {}", property.label(), e.sections.len(), sample.join(", "));
        assert_eq!(e.order.len(), tracks.len(), "every song plays once");
    }
}

#[test]
fn a_play_counts_at_half_the_song_and_shows_in_columns_and_details() {
    let mut rt = runtime(true);
    let eta = rt.state.tracks.iter().position(|t| t.title == "ETA").expect("ETA is in the demo library");
    assert_eq!(rt.state.tracks[eta].plays, 0);
    rt.update(|s, tx| s.play_index(eta, tx));
    rt.frame(0.0);
    // ETA lasts 151 seconds: the play counts after 75.5 of them, heard in 0.2 s ticks.
    for step in 1..=400 {
        rt.update(|s, _| s.position = f64::from(step) * 0.2);
        rt.frame(0.2);
        if step == 350 {
            let eta = rt.state.tracks.iter().find(|t| t.title == "ETA").expect("still there");
            assert_eq!(eta.plays, 0, "70 seconds isn't half of 151");
        }
    }
    let eta = rt.state.tracks.iter().position(|t| t.title == "ETA").expect("still there");
    assert_eq!(rt.state.tracks[eta].plays, 1, "counted once");
    assert!(rt.state.tracks[eta].last_played.is_some());
    let album = rt.state.albums.iter().find(|a| a.tracks.contains(&eta)).expect("ETA's album");
    assert_eq!(album.plays, 1, "the album adds up its songs' plays");

    rt.update(|s, _| s.base.active_mut().columns.push(crate::base::property::Property::Plays));
    rt.update(|s, _| s.refilter());
    settle(&mut rt);
    rt.update(|s, tx| s.show_details(eta, tx));
    settle(&mut rt);
    let count = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("details-play-count")).and_then(|e| e.text);
    assert_eq!(count.as_deref(), Some("Once"));
}

#[test]
fn plays_are_saved_beside_the_preferences_and_count_at_the_next_launch() {
    use crate::base::store::Store;
    let dir = std::env::temp_dir().join(format!("rune-music-plays-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    let eta = rt.state.tracks.iter().position(|t| t.title == "ETA").expect("ETA is in the demo library");
    rt.update(|s, tx| s.play_index(eta, tx));
    rt.frame(0.0);
    for step in 1..=400 {
        rt.update(|s, _| s.position = f64::from(step) * 0.2);
        rt.frame(0.2);
    }
    assert!(dir.join(crate::plays::FILE).exists());

    let mut next = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut next);
    let eta = next.tracks.iter().find(|t| t.title == "ETA").expect("ETA is in the demo library");
    assert_eq!(eta.plays, 1);
    assert_eq!(next.tracks.iter().map(|t| t.plays).sum::<u32>(), 1, "only ETA played");
    std::fs::remove_dir_all(&dir).unwrap();
}

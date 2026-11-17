//! The window driven headlessly: clicks, keys and what the view shows.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use std::rc::Rc;

use crate::app::{App, Output, Page};
use crate::queue::Repeat;
use crate::view::{VOLUME_DRAG, view};

fn runtime(dark: bool) -> Runtime<App> {
    let mut app = App::new(Output::Silent, crate::query::store::Store::memory());
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
fn the_library_lists_every_track() {
    let rt = runtime(true);
    assert_eq!(rt.state.filtered.len(), 13);
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
    assert!(rt.find("artist-music").is_some() && rt.find("artist-tab").is_some(), "the tab block");
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
    assert_eq!(rt.state.filtered.len(), 13, "the view isn't filtered by the search");
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
    click(&mut rt, "library-tab");
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

/// Presses the volume knob at its centre, moves the mouse `dy` points vertically while it holds
/// the pointer, and lets go where it pressed: the cursor never moved.
fn turn_volume(rt: &mut Runtime<App>, dy: f32) {
    let (x, y) = press_volume(rt);
    rt.pointer_motion(0.0, dy / 2.0);
    rt.frame(1.0 / 60.0);
    rt.pointer_motion(0.0, dy / 2.0);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Presses the volume knob at its centre and returns where.
fn press_volume(rt: &mut Runtime<App>) -> (f32, f32) {
    let r = rect_of_key(rt, "volume");
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    (x, y)
}

fn rect_of_key(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}")).rect
}

#[test]
fn dragging_the_volume_knob_up_raises_and_down_lowers_it() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.set_volume(0.5));
    settle(&mut rt);
    turn_volume(&mut rt, -30.0);
    assert!((rt.state.volume - (0.5 + 30.0 / VOLUME_DRAG)).abs() < 0.01, "volume {}", rt.state.volume);
    turn_volume(&mut rt, 60.0);
    assert!((rt.state.volume - (0.5 + 30.0 / VOLUME_DRAG - 60.0 / VOLUME_DRAG)).abs() < 0.01, "volume {}", rt.state.volume);
}

#[test]
fn a_click_on_the_volume_knob_leaves_the_volume_alone() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.set_volume(0.5));
    settle(&mut rt);
    click(&mut rt, "volume");
    assert!((rt.state.volume - 0.5).abs() < 1e-6, "volume {}", rt.state.volume);
}

#[test]
fn the_volume_knob_clamps_between_silent_and_full() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.set_volume(0.9));
    settle(&mut rt);
    turn_volume(&mut rt, -1000.0);
    assert_eq!(rt.state.volume, 1.0);
    turn_volume(&mut rt, 1000.0);
    assert_eq!(rt.state.volume, 0.0);
}

#[test]
fn holding_the_volume_knob_holds_the_pointer_still_until_release() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.set_volume(0.5));
    settle(&mut rt);
    let (x, y) = press_volume(&mut rt);
    assert!(rt.pointer_locked(), "the press holds the pointer");
    assert_eq!(rt.cursor(), rune::Cursor::Hidden);
    // A mouse moved further down than the screen has room for below the knob.
    for _ in 0..20 {
        rt.pointer_motion(0.0, VOLUME_DRAG / 10.0);
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.volume, 0.0, "turned all the way down in one movement");
    assert_eq!(rt.pointer(), Some((x, y)), "the cursor stayed on the knob");
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(!rt.pointer_locked(), "the release gives the pointer back");
    rt.pointer_motion(0.0, -VOLUME_DRAG);
    settle(&mut rt);
    assert_eq!(rt.state.volume, 0.0, "the mouse no longer turns it");
}

#[test]
fn escape_while_turning_a_muted_volume_knob_keeps_it_muted_at_its_volume() {
    let mut rt = runtime(true);
    rt.update(|s, _| {
        s.set_volume(0.6);
        s.toggle_mute();
    });
    settle(&mut rt);
    let (x, y) = press_volume(&mut rt);
    rt.pointer_motion(0.0, -30.0);
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.muted, "turning the knob unmutes");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(rt.state.muted, "Escape puts the mute back");
    assert!((rt.state.volume - 0.6).abs() < 1e-6, "volume {}", rt.state.volume);
}

#[test]
fn escape_while_turning_the_volume_knob_puts_the_volume_back() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.set_volume(0.5));
    settle(&mut rt);
    let (x, y) = press_volume(&mut rt);
    rt.pointer_motion(0.0, 30.0);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.volume < 0.5);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(!rt.pointer_locked());
    assert!((rt.state.volume - 0.5).abs() < 1e-6, "volume {}", rt.state.volume);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!((rt.state.volume - 0.5).abs() < 1e-6, "volume {}", rt.state.volume);
}

#[test]
fn clicking_the_seek_bar_seeks_to_the_fraction_pressed() {
    let mut rt = runtime(true);
    let eta = idx(&rt, "ETA");
    rt.update(move |s, _| {
        s.current = Some(eta);
        s.playing = true;
        s.position = 0.0;
    });
    settle(&mut rt);
    let duration = rt.state.duration().expect("ETA has a duration");
    let r = rect_of_key(&rt, "seek");
    let (x, y) = (r[0] + r[2] * 0.25, r[1] + r[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!((rt.state.position - duration * 0.25).abs() < duration * 0.02, "position {}", rt.state.position);
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
/// `PERI_MUSIC_DIR=~/Music cargo test --release real_library -- --ignored --nocapture`.
#[test]
#[ignore = "needs PERI_MUSIC_DIR"]
fn real_library_scans_and_every_format_decodes() {
    use rodio::Source;
    let dir = std::env::var("PERI_MUSIC_DIR").expect("set PERI_MUSIC_DIR to a music folder");
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
/// and seeking lands: `PERI_MUSIC_FILE=song.flac cargo test --release real_output -- --ignored`.
#[test]
#[ignore = "needs PERI_MUSIC_FILE and an audio device"]
fn real_output_plays_pauses_and_seeks() {
    use std::thread::sleep;
    use std::time::Duration;
    let file = std::env::var("PERI_MUSIC_FILE").expect("set PERI_MUSIC_FILE to an audio file");
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
    let [x, y, _, h] = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("header")).expect("the header").rect;
    let (x, y) = (x + 6.0, y + h / 2.0);
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
    mouse(&mut rt, rune::PointerButton::Primary);
    assert!(!rt.state.search.focused, "the field lets go of the keyboard");
    assert_eq!(rt.state.page, Page::Artist("Laraaji".into()), "a primary click doesn't navigate");
}

/// Reads the real library's new attributes and times grouping it:
/// `PERI_MUSIC_DIR=~/Music cargo test --release real_library_groups -- --ignored --nocapture`.
#[test]
#[ignore = "needs PERI_MUSIC_DIR"]
fn real_library_groups_by_every_property() {
    use crate::query::property::Property;
    use crate::query::view::{Direction, GroupBy, View, evaluate};
    let dir = std::env::var("PERI_MUSIC_DIR").expect("set PERI_MUSIC_DIR to a music folder");
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

    rt.update(|s, _| s.views.active_mut().columns.push(crate::query::property::Property::Plays));
    rt.update(|s, _| s.refilter());
    settle(&mut rt);
    rt.update(|s, tx| s.show_details(eta, tx));
    settle(&mut rt);
    let count = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("details-play-count")).and_then(|e| e.text);
    assert_eq!(count.as_deref(), Some("Once"));
}

#[test]
fn plays_are_saved_beside_the_preferences_and_count_at_the_next_launch() {
    use crate::query::store::Store;
    let dir = std::env::temp_dir().join(format!("peri-plays-ui-{}", std::process::id()));
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

#[test]
fn the_play_button_darkens_as_pause_while_playing_and_lightens_once_paused() {
    let mut rt = runtime(true);
    let play = |rt: &Runtime<App>| {
        let e = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("play")).unwrap_or_else(|| panic!("no play button"));
        (e.fill.r + e.fill.g + e.fill.b, e.label)
    };
    let (raised, label) = play(&rt);
    assert_eq!(label.as_deref(), Some("Play"));
    let eta = idx(&rt, "ETA");
    rt.update(move |s, _| {
        s.current = Some(eta);
        s.playing = true;
    });
    settle(&mut rt);
    let (pressed, label) = play(&rt);
    assert_eq!(label.as_deref(), Some("Pause"));
    assert!(pressed < raised, "pressed in and darker while playing: {pressed} vs {raised}");
    click(&mut rt, "play");
    assert!(!rt.state.playing);
    let (paused, label) = play(&rt);
    assert_eq!(label.as_deref(), Some("Play"));
    assert!((paused - raised).abs() < 1e-3, "back to the raised colour once paused: {paused} vs {raised}");
}

/// The titles a queue plays from its current track to its end, without the playing state.
fn queue_titles(rt: &Runtime<App>) -> Vec<String> {
    let mut queue = rt.state.queue.clone().expect("a queue is playing");
    let mut titles = vec![rt.state.tracks[queue.current()].title.clone()];
    while let Some(i) = queue.next(Repeat::Off) {
        titles.push(rt.state.tracks[i].title.clone());
    }
    titles
}

#[test]
fn play_after_the_queue_ran_out_starts_the_last_track_again() {
    let mut rt = runtime(true);
    let eta = idx(&rt, "ETA");
    rt.update(move |s, tx| s.play_list(vec![eta], eta, tx));
    rt.update(|s, tx| s.next(tx));
    assert!(!rt.state.playing);
    assert_eq!(rt.state.current, Some(eta));
    rt.update(|s, tx| s.toggle(tx));
    assert!(rt.state.playing, "play starts the finished track again");
    assert_eq!(rt.state.current, Some(eta));
    assert!(!rt.state.ended);
}

#[test]
fn a_rescan_keeps_the_shuffled_queue_a_list_started() {
    let mut rt = runtime(true);
    let all: Vec<usize> = (0..rt.state.tracks.len()).collect();
    let eta = idx(&rt, "ETA");
    rt.update(move |s, tx| {
        s.shuffle = true;
        s.play_list(all, eta, tx);
    });
    let before = queue_titles(&rt);
    assert_eq!(before[0], "ETA");
    rt.update(|s, _| s.set_library(crate::demo_tracks(), false));
    assert_eq!(queue_titles(&rt), before);
}

#[test]
fn a_rescan_drops_queued_tracks_that_left_the_library() {
    let mut rt = runtime(true);
    let all: Vec<usize> = (0..rt.state.tracks.len()).collect();
    let eta = idx(&rt, "ETA");
    rt.update(move |s, tx| s.play_list(all, eta, tx));
    let before = queue_titles(&rt);
    let mut library = crate::demo_tracks();
    library.retain(|t| t.title != "Meditation #1");
    rt.update(move |s, _| s.set_library(library, false));
    let expected: Vec<String> = before.into_iter().filter(|t| t != "Meditation #1").collect();
    assert_eq!(queue_titles(&rt), expected);
}

#[test]
fn adding_a_folder_while_the_launch_scan_runs_keeps_both_folders() {
    let dir = std::env::temp_dir().join(format!("peri-b2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (a, b) = (dir.join("a"), dir.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(a.join("one.mp3"), b"not audio").unwrap();
    std::fs::write(b.join("two.mp3"), b"not audio").unwrap();
    let mut rt = Runtime::new(App::new(Output::Silent, crate::query::store::Store::memory()), view, (980.0, 640.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    rt.update(|s, _| s.set_waker(std::sync::Arc::new(|| {})));
    let first = a.clone();
    rt.update(move |s, _| s.replace_library(vec![first]));
    // The launch scan is still running (no frame has drained it) when the second folder is added.
    let second = b.clone();
    rt.update(move |s, _| s.add_to_library(vec![second]));
    for _ in 0..400 {
        rt.frame(0.0);
        if rt.state.scan.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(rt.state.scan.is_none(), "the scan finishes");
    let mut paths: Vec<std::path::PathBuf> = rt.state.tracks.iter().map(|t| t.path.clone()).collect();
    paths.sort();
    let mut expected = vec![a.join("one.mp3"), b.join("two.mp3")];
    expected.sort();
    assert_eq!(paths, expected, "both folders are in the library");
    assert_eq!(rt.state.library, vec![a.clone(), b.clone()]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn album_of_matches_the_albums_after_a_library_change_and_a_play() {
    use crate::query::store::Store;
    let mut app = App::new(Output::Silent, Store::memory());
    let album_of_by_scan = |app: &App, i: usize| app.albums.iter().position(|a| a.tracks.contains(&i));
    app.set_library(crate::demo_tracks(), false);
    assert!(!app.albums.is_empty());
    for i in 0..app.tracks.len() {
        assert_eq!(app.album_of(i), album_of_by_scan(&app, i), "track {i} after set_library");
    }
    app.record_play(0, 1_000).expect("the play is recorded");
    for i in 0..app.tracks.len() {
        assert_eq!(app.album_of(i), album_of_by_scan(&app, i), "track {i} after record_play");
    }
    assert_eq!(app.album_of(app.tracks.len()), None, "out of range is no album");
}

#[test]
fn cached_page_models_match_fresh_builds_and_follow_library_changes() {
    use crate::query::store::Store;
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    let artist = app.tracks[0].artist().expect("the demo's first song has an artist");

    app.search.text = "  NILS  ".into();
    let fresh = crate::search::search("nils", &app.tracks, &app.albums, &app.relations);
    assert_eq!(*app.search_results(), fresh, "cached search equals a fresh search");
    assert!(Rc::ptr_eq(&app.search_results(), &app.search_results()), "a repeat asks for the kept model");

    let (page, others) = (crate::artist::page(&artist, &app.tracks, &app.albums, &app.relations, true), crate::artist::involved(&artist, &app.tracks, &app.albums, &app.relations, true));
    let model = app.artist_model(&artist, true);
    assert_eq!(model.0, page, "cached artist page equals a fresh build");
    assert_eq!(model.1, others, "cached involved artists equal a fresh build");

    let before = app.search_results();
    app.record_play(0, 1_000).expect("the play is recorded");
    let after = app.search_results();
    assert!(!Rc::ptr_eq(&before, &after), "a library change rebuilds the search results");
    assert_eq!(*after, crate::search::search("nils", &app.tracks, &app.albums, &app.relations), "rebuilt search equals a fresh search");
    assert_eq!(app.recent_songs().first().copied(), Some(0), "the played song leads Recently Played after the change");
}

#[test]
fn a_filter_edit_applies_now_and_saves_when_the_panel_closes() {
    use crate::query::filter::{Condition, Node, Operator};
    use crate::query::property::Property;
    use crate::query::store::Store;
    let dir = std::env::temp_dir().join(format!("peri-debounce-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut app);
    let all = app.filtered.len();
    app.views.active_mut().filter.children = vec![Node::Condition(Condition { property: Property::Title, op: Operator::Contains, value: "Dance".into() })];
    app.views_changed_later();
    assert!(app.filtered.len() < all, "the filter applies at once");
    assert!(Store::at(&file).load().active().filter.children.is_empty(), "the save waits");
    app.close_panel();
    assert_eq!(Store::at(&file).load().active().filter.children.len(), 1, "closing the panel saves");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_play_updates_its_album_in_place_like_a_rebuild() {
    use crate::query::store::Store;
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    let before = app.albums.clone();
    let i = 0;
    let a = app.album_of(i).expect("the first song is in an album");
    app.record_play(i, 1_000).expect("the play is recorded");
    assert_eq!(app.albums, crate::album::build(&app.tracks), "the in-place update equals a full rebuild");
    assert_eq!(app.albums.len(), before.len(), "albums keep their count");
    assert_eq!(app.albums[a].plays, before[a].plays + 1, "the album's plays went up by one");
    assert_eq!(app.albums[a].last_played, Some(1_000), "the album's last played is the play");
    assert_eq!(app.album_of(i), Some(a), "the song is still in its album");
}

#[test]
fn copies_of_a_song_in_two_albums_both_update_their_plays() {
    use crate::query::store::Store;
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(crate::demo_tracks(), false);
    let mut tracks = app.tracks.clone();
    let mut copy = tracks[0].clone();
    copy.path = "/elsewhere/copy.flac".into();
    copy.album_artist = Some("Someone Else".into());
    tracks.push(copy);
    app.set_library(tracks, false);
    let played = app.tracks.iter().position(|t| t.path == std::path::Path::new("/demo/Big Thief;Laraaji/Live Sessions/00 Mary.flac")).expect("the first song is in the library");
    let other = app.tracks.iter().position(|t| t.path == std::path::Path::new("/elsewhere/copy.flac")).expect("the copy is in the library");
    let first = app.album_of(played).expect("the song is in an album");
    let second = app.album_of(other).expect("the copy is in an album");
    assert_ne!(first, second, "the copies land in different albums");
    app.record_play(played, 1_000).expect("the play is recorded");
    assert_eq!(app.albums, crate::album::build(&app.tracks), "the in-place update equals a full rebuild");
    assert_eq!(app.albums[first].plays, 1, "the played album counts the play");
    assert_eq!(app.albums[second].plays, 1, "the copy's album counts the shared play");
    assert_eq!(app.albums[second].last_played, Some(1_000), "the copy's album shows the shared last played");
}

#[test]
fn a_window_reads_the_details_file_in_the_background_and_poll_delivers_it() {
    let mut app = App::new(Output::Closed, crate::query::store::Store::memory());
    app.set_waker(std::sync::Arc::new(|| {}));
    let missing = std::path::PathBuf::from("/nonexistent/x.flac");
    let track = crate::library::Track { path: missing.clone(), title: "x".into(), ..Default::default() };
    app.set_library(vec![track], false);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    rt.update(|s, tx| s.show_details(0, tx));
    let open = rt.state.details.as_ref().expect("the dialog opened");
    assert!(open.details.is_none(), "the dialog starts out reading");
    for _ in 0..400 {
        rt.frame(0.0);
        if rt.state.details.as_ref().is_some_and(|d| d.details.is_some()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    settle(&mut rt);
    let open = rt.state.details.as_ref().expect("the dialog is still open");
    assert!(matches!(&open.details, Some(Err(e)) if e.contains("/nonexistent/x.flac")), "the read error arrives through poll");
    let keys: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).collect();
    assert!(keys.iter().any(|k| k == "details-error"));
    assert!(!keys.iter().any(|k| k == "details-reading"));
}

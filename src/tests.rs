//! The window driven headlessly: clicks, keys and what the view shows.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output};
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
    let (x, y) = rt.find(&format!("t{shy}")).unwrap();
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
    click(&mut rt, "t2");
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
fn searching_filters_the_list_and_plays_within_it() {
    let mut rt = runtime(true);
    rt.update(|s, _| {
        s.search = rune::TextEdit::new("nils");
        s.refilter();
    });
    settle(&mut rt);
    assert_eq!(text_of(&rt, "count"), "4 of 13 tracks");
    assert!(rt.find("t0").is_none(), "Laraaji is filtered out");
    let first = rt.state.filtered[0];
    click(&mut rt, &crate::app::row_key(first, None));
    key(&mut rt, Key::Enter);
    for _ in 0..3 {
        click(&mut rt, "next");
    }
    assert!(rt.state.current.is_some_and(|i| rt.state.tracks[i].artist().as_deref() == Some("Nils Frahm")));
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
    assert_eq!(text_of(&rt, "count"), "4 of 13 tracks");
}

#[test]
fn space_still_plays_and_pauses_after_clicking_a_transport_button() {
    let mut rt = runtime(true);
    click(&mut rt, "t1");
    click(&mut rt, "shuffle");
    key(&mut rt, Key::Text(" ".into()));
    assert!(rt.state.shuffle, "Space didn't press the shuffle button again");
    assert_eq!((rt.state.current, rt.state.playing), (Some(1), true));
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
    let tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    let share = |f: &dyn Fn(&crate::library::Track) -> bool| 100 * tracks.iter().filter(|t| f(t)).count() / tracks.len();
    println!(
        "{} tracks: genre {}%, year {}%, album artist {}%, label {}%, composer {}%, several artists {}%",
        tracks.len(),
        share(&|t| !t.genres.is_empty()),
        share(&|t| t.year.is_some()),
        share(&|t| t.album_artist.is_some()),
        share(&|t| t.label.is_some()),
        share(&|t| !t.composers.is_empty()),
        share(&|t| t.artists.len() > 1),
    );
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

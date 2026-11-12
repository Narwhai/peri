//! Album views driven headlessly: switching modes and layouts, the album page, playing albums.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, album_key};
use crate::query::filter::{Condition, Node, Operator};
use crate::query::property::{Mode, Property};
use crate::query::store::Store;
use crate::query::view::Layout;
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
    assert_eq!(rt.state.views.active().mode, Mode::Albums);
    assert_eq!(rt.state.views.active().layout, Layout::Grid);
    assert!(!rt.elements().iter().any(|e| e.path.contains("header") && e.text.as_deref() == Some("4 albums")), "the header has no album count");
    assert_eq!(shown_titles(&rt), ["Live Sessions", "Day of Radiance", "Get Up", "Spaces"]);
    for a in 0..4 {
        assert!(rt.find(&album_key(a, None)).is_some(), "card {a} shows");
    }
    assert!(rt.state.columns >= 4, "980 points fit at least four columns, got {}", rt.state.columns);
    click(&mut rt, "layout/list");
    assert_eq!(rt.state.views.active().layout, Layout::List);
    assert!(rt.find("play-album").is_some(), "list cards have a play button");
}

#[test]
fn sorting_albums_by_artist_clusters_each_artists_albums() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    rt.update(|s, _| {
        s.views.active_mut().sorts = vec![crate::query::view::Sort { property: Property::Year, direction: crate::query::view::Direction::Asc }];
        s.views_changed();
    });
    settle(&mut rt);
    assert_eq!(shown_titles(&rt), ["Day of Radiance", "Spaces", "Get Up", "Live Sessions"], "1980, 2013, 2023, 2024");
    rt.update(|s, _| {
        s.views.active_mut().group_by = Some(crate::query::view::GroupBy { property: Property::Genre, direction: crate::query::view::Direction::Asc });
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
fn search_results_lead_with_the_artists_own_albums() {
    let mut rt = runtime();
    rt.update(|s, tx| {
        s.search = rune::TextEdit::new("laraaji");
        s.searched(tx);
    });
    settle(&mut rt);
    assert_eq!(text_of(&rt, "artist-name"), "Laraaji");
    let day = album(&rt, "Day of Radiance");
    let live = album(&rt, "Live Sessions");
    assert!(rt.find(&format!("artist-albums{day}")).is_some());
    assert!(rt.find(&format!("artist-albums{live}")).is_some(), "Live Sessions lists Laraaji as an album artist");
    // A song title finds its album.
    rt.update(|s, tx| {
        s.search = rune::TextEdit::new("hammers");
        s.searched(tx);
    });
    settle(&mut rt);
    assert!(rt.find(&format!("search-albums{}", album(&rt, "Spaces"))).is_none(), "an album only shows when its title or artist matches");
    assert!(rt.find("search-songs").is_some());
}

#[test]
fn album_filters_use_album_properties_and_switching_modes_drops_song_only_ones() {
    let mut rt = runtime();
    rt.update(|s, _| {
        let cond = |property, op, value: &str| Node::Condition(Condition { property, op, value: value.into() });
        s.views.active_mut().filter.children = vec![cond(Property::Year, Operator::Ge, "2013"), cond(Property::Track, Operator::Is, "1")];
        s.views_changed();
    });
    click(&mut rt, "mode/albums");
    let filter = &rt.state.views.active().filter.children;
    assert_eq!(filter.len(), 1, "albums have no track number, so that condition goes");
    assert_eq!(shown_titles(&rt), ["Live Sessions", "Get Up", "Spaces"]);
    rt.update(|s, _| {
        s.views.active_mut().filter.children = vec![Node::Condition(Condition { property: Property::Tracks, op: Operator::Gt, value: "3".into() })];
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

#[test]
fn hovering_a_grid_cards_play_button_keeps_it_shown_and_pressing_it_plays_without_opening_the_album() {
    let mut rt = runtime();
    click(&mut rt, "mode/albums");
    let a = album(&rt, "Get Up");
    let card = album_key(a, None);
    let play = |rt: &Runtime<App>| {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some("play") && e.path.contains(&format!("/{card}/"))).unwrap_or_else(|| panic!("no play button on {card}"))
    };
    let r = play(&rt).rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    settle(&mut rt);
    let opacity = play(&rt).opacity;
    assert!(opacity > 0.99, "the play button stays shown while hovered, opacity {opacity}");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Library, "pressing play doesn't open the album");
    assert_eq!(rt.state.tracks[rt.state.current.expect("playing")].title, "New Jeans");
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}")).rect
}

fn album_page_runtime(a: &str) -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    long_credits(&mut app);
    let mut rt = Runtime::new(app, view, (1100.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    let a = album(&rt, a);
    rt.update(|s, tx| s.show_album(a, tx));
    settle(&mut rt);
    rt
}

#[test]
fn an_album_pages_buttons_stack_under_its_genres_with_the_credits_past_a_divider_to_their_right() {
    let rt = album_page_runtime("Get Up");
    let (genres, play, shuffle) = (rect(&rt, "page-genres"), rect(&rt, "page-play"), rect(&rt, "page-shuffle"));
    let (divider, facts) = (rect(&rt, "page-divider"), rect(&rt, "facts"));
    assert!(play[1] > genres[1] + genres[3], "the buttons sit under the genres");
    assert!(shuffle[1] > play[1] + play[3] && (shuffle[0] - play[0]).abs() < 0.5, "Shuffle stacks under Play, left edges level");
    assert!(shuffle[3] < play[3], "Shuffle is shorter than Play: {shuffle:?} {play:?}");
    assert!((shuffle[2] - play[2]).abs() < 0.5, "the two share a width: {shuffle:?} {play:?}");
    assert!(divider[0] > play[0] + play[2].max(shuffle[2]), "the divider follows the buttons");
    assert!(facts[0] > divider[0] + divider[2], "the credits follow the divider");
    assert!(facts[0] - (play[0] + play[2].max(shuffle[2])) < 48.0, "the credits start just past the buttons");
    assert!((divider[1] - facts[1]).abs() < 0.5 && (divider[3] - facts[3]).abs() < 0.5, "the divider spans the credits: {divider:?} {facts:?}");
    assert!((play[1] - facts[1]).abs() < 0.5, "the buttons top-align with the credits");
    let (composer, mixer) = (rect(&rt, "fact-composer"), rect(&rt, "fact-mixer"));
    assert!((composer[0] - mixer[0]).abs() < 0.5 && mixer[1] > composer[1], "the credits stack in one column");
}

#[test]
fn an_album_page_without_credits_shows_its_buttons_without_a_divider() {
    let rt = album_page_runtime("Spaces");
    assert!(rt.find("page-play").is_some());
    assert!(rt.find("page-divider").is_none(), "no divider with nothing past it");
}

/// Get Up credited like the real EP: two long name lists and four more roles.
fn long_credits(s: &mut App) {
    use crate::credits::{Credit, Role};
    crate::scene_library(s);
    let mut tracks = s.tracks.clone();
    let composers = "Erika de Casier, Jinsu Park, Fine Glindvad Jensen, Frankie Scoca, Kristine Bogan, 250, Ylva Dimberg, Donghyun Kim, freekind., Henriette Motzfeldt, Catharina Stoltenberg";
    let lyricists = "Erika de Casier, Gigi, Haerin, Fine Glindvad Jensen, Kristine Bogan, Danielle, Donghyun Kim, Beenzino, Ylva Dimberg, freekind., Henriette Motzfeldt, Catharina Stoltenberg";
    let roles = [(Role::Composer, composers), (Role::Lyricist, lyricists), (Role::Producer, "250, Erika de Casier"), (Role::Performer, "NewJeans (background vocals), Emily Kim (background vocals)"), (Role::Engineer, "Bill Zimmerman"), (Role::Mixer, "Phil Tan, Nathan Boddy, Tony Maserati, Jonny Breakwell, David K. Younghyun")];
    for t in tracks.iter_mut().filter(|t| t.album.as_deref() == Some("Get Up")) {
        for (role, names) in roles {
            t.credits.extend(names.split(", ").map(|n| Credit::parse(role, n)));
        }
    }
    s.set_library(tracks, false);
}

#[test]
fn an_album_page_shows_its_genres_as_pills_and_its_label_under_the_tracks() {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_label(&mut app);
    let mut rt = Runtime::new(app, view, (1100.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    let a = album(&rt, "Spaces");
    rt.update(|s, tx| s.show_album(a, tx));
    settle(&mut rt);
    let genres = rt.state.albums[a].genres.clone();
    assert!(!genres.is_empty(), "the demo album has genres");
    for n in 0..genres.len() {
        assert!(rt.find(&format!("page-genre{n}")).is_some(), "genre {n} shows as a pill");
    }
    assert!(rt.find("fact-label").is_none(), "the label left the facts");
    let year = rt.state.albums[a].year.expect("a year");
    assert_eq!(text_of(&rt, "page-release"), format!("℗ {year} Erased Tapes"));
    assert!(rect(&rt, "page-release")[1] > rect(&rt, "tracks")[1], "it sits under the track list");
}

#[test]
fn the_album_pages_play_button_toggles_to_pause_while_the_album_plays() {
    let mut rt = album_page_runtime("Get Up");
    assert_eq!(play_label(&rt).as_deref(), Some("Play"));
    let (play, shuffle) = (rect(&rt, "page-play"), rect(&rt, "page-shuffle"));
    click(&mut rt, "page-play");
    assert!(rt.state.playing, "Play starts the album");
    assert_eq!(rect(&rt, "page-play"), play, "turning into Pause keeps the button's size and place");
    assert_eq!(rect(&rt, "page-shuffle"), shuffle, "and Shuffle's");
    let get_up = album(&rt, "Get Up");
    assert_eq!(rt.state.current.and_then(|i| rt.state.album_of(i)), Some(get_up));
    assert_eq!(play_label(&rt).as_deref(), Some("Pause"), "it turns into Pause");
    click(&mut rt, "page-play");
    assert!(!rt.state.playing, "Pause pauses");
    assert_eq!(rt.state.current.and_then(|i| rt.state.album_of(i)), Some(get_up), "and keeps the album loaded");
    assert_eq!(play_label(&rt).as_deref(), Some("Play"), "and turns back into Play");
    click(&mut rt, "page-play");
    assert!(rt.state.playing, "Play again resumes");
}

fn play_label(rt: &Runtime<App>) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("page-play")).and_then(|e| e.label)
}

#[test]
fn the_album_pages_play_button_plays_this_album_when_another_is_playing() {
    let mut rt = album_page_runtime("Get Up");
    let spaces = album(&rt, "Spaces");
    rt.update(|s, tx| s.play_album(spaces, None, tx));
    settle(&mut rt);
    assert_eq!(play_label(&rt).as_deref(), Some("Play"), "another album playing leaves this one's button at Play");
    click(&mut rt, "page-play");
    assert_eq!(rt.state.current.and_then(|i| rt.state.album_of(i)), Some(album(&rt, "Get Up")), "Play switches to this album");
    assert_eq!(play_label(&rt).as_deref(), Some("Pause"));
}

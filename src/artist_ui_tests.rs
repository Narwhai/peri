//! Artist names driven headlessly: clicking a featured or album artist's name opens their page,
//! and Back returns through the pages visited.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Page, album_key};
use crate::base::property::Mode;
use crate::base::store::Store;
use crate::credits::{Credit, Role};
use crate::library::{TaggedArtists, Track};
use crate::view::view;

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Mercy as tagged: a collaborative album with featured guests and a producer credit.
fn mercy() -> Vec<Track> {
    let song = |n: u32, title: &str, credit: &str, artists: &[&str], ids: &[&str]| Track {
        path: format!("/m/Mercy/{n:02} {title}.flac").into(),
        title: title.into(),
        credit: Some(credit.into()),
        album_artist: Some("Armand Hammer & The Alchemist".into()),
        tagged: TaggedArtists { artists: strings(artists), artist_ids: strings(ids), album_artists: Vec::new(), album_artist_ids: strings(&["ah", "alc"]) },
        album: Some("Mercy".into()),
        year: Some(2025),
        number: Some(n),
        duration: Some(180.0),
        credits: vec![Credit::parse(Role::Producer, "The Alchemist"), Credit::parse(Role::Performer, "Cleo Reed (vocals)")],
        ..Track::default()
    };
    vec![
        song(1, "Laraaji", "Armand Hammer & The Alchemist", &["Armand Hammer", "The Alchemist"], &["ah", "alc"]),
        song(3, "Calypso Gene", "Armand Hammer & The Alchemist feat. Silka & Cleo Reed", &["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"], &["ah", "alc", "silka", "cleo"]),
    ]
}

/// Wide enough that the Artist column shows "… feat. Silka & Cleo Reed" whole, so its names
/// can be clicked.
fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(mercy(), false);
    let mut rt = Runtime::new(app, view, (1500.0, 1100.0), Env::new(true, 1.0));
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

/// Clicks the element keyed `key`, or the link whose id (artist name) is `key`.
fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element or link {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

#[test]
fn a_featured_artists_name_in_a_song_row_opens_their_page() {
    let mut rt = runtime();
    let calypso = rt.state.tracks.iter().position(|t| t.title == "Calypso Gene").expect("Calypso Gene");
    assert_eq!(rt.state.tracks[calypso].featured, ["Silka", "Cleo Reed"]);
    assert_eq!(rt.state.tracks[calypso].album_artists, ["Armand Hammer", "The Alchemist"]);
    click(&mut rt, "Cleo Reed");
    assert_eq!(rt.state.page, Page::Artist("Cleo Reed".into()));
    assert_eq!(text_of(&rt, "artist-name"), "Cleo Reed");
    assert!(rt.find("appears-on").is_some(), "Mercy is an album Cleo Reed appears on");
    assert!(rt.find("artist-albums").is_none(), "Cleo Reed has no albums of their own");
    assert!(rt.find("credits-performer").is_some(), "their performer credits list too");
    assert!(rt.state.selected.is_none(), "clicking a name doesn't select the row under it");
}

#[test]
fn each_artist_of_a_collaborative_album_opens_from_the_album_page_and_back_retraces() {
    let mut rt = runtime();
    rt.update(|s, _| {
        s.base.active_mut().set_mode(Mode::Albums);
        s.views_changed();
    });
    settle(&mut rt);
    click(&mut rt, &album_key(0, None));
    assert_eq!(text_of(&rt, "page-artist"), "Armand Hammer & The Alchemist");
    click(&mut rt, "The Alchemist");
    assert_eq!(rt.state.page, Page::Artist("The Alchemist".into()));
    assert!(rt.find("artist-albums").is_some(), "Mercy counts as The Alchemist's album");
    assert!(rt.find("credits-producer").is_some());
    // From the artist page into a guest's page, then Back twice to the album.
    click(&mut rt, "Silka");
    assert_eq!(rt.state.page, Page::Artist("Silka".into()));
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Artist("The Alchemist".into()));
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(rt.state.open_album().is_some(), "Esc goes back to the album page");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Library);
}

#[test]
fn the_now_playing_bar_links_the_playing_songs_artists() {
    let mut rt = runtime();
    let calypso = rt.state.tracks.iter().position(|t| t.title == "Calypso Gene").expect("Calypso Gene");
    rt.update(|s, tx| s.play_index(calypso, tx));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "np-artist"), "Armand Hammer & The Alchemist feat. Silka & Cleo Reed", "the artist line reads as tagged");
    // The song row is gone once the artist page opens, so the bar's link is the one left.
    click(&mut rt, "Armand Hammer");
    assert_eq!(rt.state.page, Page::Artist("Armand Hammer".into()));
    click(&mut rt, "artist-play");
    assert!(rt.state.playing);
}


#[test]
fn songs_start_collapsed_under_the_credits_when_the_artist_has_credits() {
    let mut rt = runtime();
    // The Alchemist plays on Mercy and produced it: credits first, songs folded.
    rt.update(|s, tx| s.show_artist("The Alchemist", tx));
    settle(&mut rt);
    let y = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect[1];
    assert!(y(&rt, "credits-producer") < y(&rt, "artist-songs"), "credits come before songs");
    assert!(rt.find("artist-songs-table").is_none(), "songs start collapsed");
    click(&mut rt, "artist-songs-toggle");
    assert!(rt.find("artist-songs-table").is_some());
    click(&mut rt, "artist-songs-toggle");
    assert!(rt.find("artist-songs-table").is_none());
    // Silka has no credits, so her songs show open.
    rt.update(|s, tx| s.show_artist("Silka", tx));
    settle(&mut rt);
    assert!(rt.find("artist-songs-table").is_some());
}

#[test]
fn artist_song_rows_lead_with_the_album_cover() {
    let mut rt = runtime();
    rt.update(|s, tx| s.show_artist("Silka", tx));
    settle(&mut rt);
    let calypso = rt.state.tracks.iter().position(|t| t.title == "Calypso Gene").expect("Calypso Gene");
    assert!(rt.find(&format!("artist-songs-cover{calypso}")).is_some());
}

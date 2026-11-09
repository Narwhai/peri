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

/// Somebody's album "Elsewhere", which The Alchemist produced: credits on another artist's
/// songs, so they list under Credits rather than on his own album's card.
fn elsewhere() -> Vec<Track> {
    let song = |n: u32, title: &str| Track {
        path: format!("/m/Elsewhere/{n:02} {title}.flac").into(),
        title: title.into(),
        credit: Some("Somebody".into()),
        album_artist: Some("Somebody".into()),
        tagged: TaggedArtists { artists: strings(&["Somebody"]), ..TaggedArtists::default() },
        album: Some("Elsewhere".into()),
        year: Some(2020),
        number: Some(n),
        duration: Some(200.0),
        credits: vec![Credit::parse(Role::Producer, "The Alchemist")],
        ..Track::default()
    };
    vec![song(1, "Over There"), song(2, "Far Off")]
}

/// The runtime with Mercy and Elsewhere, on The Alchemist's page.
fn alchemist(elsewhere: Vec<Track>) -> Runtime<App> {
    let mut tracks = mercy();
    tracks.extend(elsewhere);
    let mut rt = runtime();
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    rt.update(|s, tx| s.show_artist("The Alchemist", tx));
    settle(&mut rt);
    rt
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
    assert!(rt.find("credits").is_some(), "their performer credits list too");
    let calypso = rt.state.tracks.iter().position(|t| t.title == "Calypso Gene").expect("Calypso Gene");
    let pill = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("tag-performer-·-vocals"));
    assert!(pill.is_some(), "the credit's pill says what they played");
    assert!(rt.find(&format!("credits-t{calypso}")).is_some());
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
    assert!(rt.find("credits").is_none(), "his only credits are on his own album");
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
    // The Alchemist plays on Mercy and produced Elsewhere: credits first, songs folded.
    let mut rt = alchemist(elsewhere());
    let y = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect[1];
    assert!(y(&rt, "credits") < y(&rt, "artist-songs"), "credits come before songs");
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

#[test]
fn eps_and_singles_list_apart_from_albums_on_the_artist_page_and_in_search() {
    let mut tracks = mercy();
    let mut ep = tracks[0].clone();
    ep.path = "/m/Shrines EP/01 Shrines.flac".into();
    ep.title = "Shrines".into();
    ep.album = Some("Shrines EP".into());
    ep.year = Some(2020);
    ep.album_artist = Some("Armand Hammer".into());
    ep.tagged = TaggedArtists { artists: strings(&["Armand Hammer"]), ..TaggedArtists::default() };
    ep.release_type = crate::album::ReleaseType::Ep;
    tracks.push(ep);
    let mut rt = runtime();
    rt.update(|s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let shrines = rt.state.albums.iter().position(|a| a.title == "Shrines EP").expect("Shrines EP");
    let mercy = rt.state.albums.iter().position(|a| a.title == "Mercy").expect("Mercy");

    rt.update(|s, tx| s.show_artist("Armand Hammer", tx));
    settle(&mut rt);
    assert!(rt.find(&format!("artist-albums{mercy}")).is_some());
    assert!(rt.find(&format!("artist-eps{shrines}")).is_some(), "the EP lists under EPs");
    assert!(rt.find(&format!("artist-albums{shrines}")).is_none());
    assert_eq!(text_of(&rt, "artist-counts"), "1 album  •  1 EP  •  3 songs");

    rt.update(|s, tx| {
        s.search = rune::TextEdit::new("shrines");
        s.searched(tx);
    });
    settle(&mut rt);
    assert!(rt.find(&format!("search-eps{shrines}")).is_some(), "search results split by type too");

    rt.update(|s, tx| s.show_album(shrines, tx));
    settle(&mut rt);
    assert!(text_of(&rt, "page-meta").starts_with("EP  •"));
}

#[test]
fn a_song_with_several_of_the_artists_credits_lists_once_with_a_pill_per_role() {
    let mut songs = elsewhere();
    songs[0].credits.extend([Credit::parse(Role::Composer, "The Alchemist"), Credit::parse(Role::Mixer, "The Alchemist")]);
    let rt = alchemist(songs);
    let over = rt.state.tracks.iter().position(|t| t.title == "Over There").expect("Over There");
    let far = rt.state.tracks.iter().position(|t| t.title == "Far Off").expect("Far Off");
    let keys: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).collect();
    assert_eq!(keys.iter().filter(|k| *k == &format!("credits-t{over}")).count(), 1, "Over There lists once");
    assert!(keys.iter().any(|k| k == &format!("credits-t{far}")));
    for role in ["tag-composer", "tag-producer", "tag-mixer"] {
        assert!(keys.iter().any(|k| k == role), "a {role} pill");
    }
    assert!(!keys.iter().any(|k| k.starts_with("credits-producer")), "no per-role sections");
    assert_eq!(text_of(&rt, "artist-counts"), "1 album  •  2 songs  •  credited on 2 songs");
}

#[test]
fn a_roles_pills_line_up_down_the_credits_table() {
    // Over There: producer only. Far Off: composer and producer, so its producer pill sits
    // after a composer pill; Over There's must still sit under it.
    let mut songs = elsewhere();
    songs[1].credits.push(Credit::parse(Role::Composer, "The Alchemist"));
    let rt = alchemist(songs);
    let xs: Vec<f32> = rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("tag-producer") && e.path.contains("credits-table")).map(|e| e.rect[0]).collect();
    assert_eq!(xs.len(), 2, "both songs have a producer pill");
    assert!((xs[0] - xs[1]).abs() < 0.5, "producer pills share a column: {xs:?}");
    assert!(rt.find("credits-header-col-roles").is_some(), "the table heads the pills Roles");
}

#[test]
fn the_credits_section_folds_and_opens_again_on_the_next_artist_page() {
    let mut rt = alchemist(elsewhere());
    let label = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("artist-credits-toggle")).and_then(|e| e.label);
    assert_eq!(label.as_deref(), Some("Credits, 2"), "Credits counts its songs");
    assert!(rt.find("credits-table").is_some(), "credits start open");
    click(&mut rt, "artist-credits-toggle");
    assert!(rt.find("credits-table").is_none());
    assert!(rt.find("artist-credits-toggle").is_some(), "the heading stays to open it again");
    click(&mut rt, "artist-credits-toggle");
    assert!(rt.find("credits-table").is_some());
    click(&mut rt, "artist-credits-toggle");
    rt.update(|s, tx| s.show_artist("Cleo Reed", tx));
    settle(&mut rt);
    assert!(rt.find("credits-table").is_some(), "another artist's credits start open");
}

#[test]
fn credits_on_their_own_album_show_on_its_card() {
    let mut songs = elsewhere();
    songs.push({
        // A third Mercy song he didn't mix, so the mixer pill counts songs.
        let mut t = mercy()[0].clone();
        t.path = "/m/Mercy/05 Dry Spell.flac".into();
        t.title = "Dry Spell".into();
        t.number = Some(5);
        t.credits = vec![Credit::parse(Role::Producer, "The Alchemist"), Credit::parse(Role::Mixer, "The Alchemist")];
        t
    });
    let rt = alchemist(songs);
    let mercy = rt.state.albums.iter().position(|a| a.title == "Mercy").expect("Mercy");
    let pills: Vec<String> = rt
        .elements()
        .into_iter()
        .filter(|e| e.path.contains(&format!("/artist-albums-roles{mercy}/")) && e.role == "text")
        .filter_map(|e| e.text)
        .collect();
    assert_eq!(pills, ["Producer", "Mixer · 1/3"], "a role on every song, then one on some");
    let dry = rt.state.tracks.iter().position(|t| t.title == "Dry Spell").expect("Dry Spell");
    assert!(rt.find(&format!("credits-t{dry}")).is_none(), "own-album songs stay out of Credits");
    assert_eq!(text_of(&rt, "artist-counts"), "1 album  •  3 songs  •  credited on 2 songs");
}

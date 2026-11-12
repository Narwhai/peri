//! Artist names driven headlessly: clicking a featured or album artist's name opens their page,
//! and Back returns through the pages visited.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, ArtistTab, Output, Page, album_key};
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
    // From the artist page into a guest's page, then Back twice to the album. Silka's name is
    // on a song row, which lists under the Songs tab.
    click_tab(&mut rt, "songs");
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
fn the_ampersand_between_album_artists_opens_what_they_made_together() {
    let mut rt = runtime();
    rt.update(|s, _| {
        s.set_library(with_older_album(), false);
        s.base.active_mut().set_mode(Mode::Albums);
        s.views_changed();
    });
    settle(&mut rt);
    let mercy = rt.state.albums.iter().position(|a| a.title == "Mercy").expect("Mercy");
    click(&mut rt, &album_key(mercy, None));
    let both = crate::view::collab_id(&["Armand Hammer", "The Alchemist"]);
    click(&mut rt, &both);
    assert_eq!(rt.state.page, Page::Collab(strings(&["Armand Hammer", "The Alchemist"])));
    assert_eq!(text_of(&rt, "collab-name"), "Armand Hammer & The Alchemist");
    assert!(rt.find(&format!("collab-albums{mercy}")).is_some(), "Mercy is theirs together");
    let ruins = rt.state.albums.iter().position(|a| a.title == "Ruins").expect("Ruins");
    assert!(rt.find(&format!("collab-albums{ruins}")).is_none(), "Armand Hammer's solo album isn't");
    // Each name in the title opens that artist's page, and Back returns here.
    click(&mut rt, "The Alchemist");
    assert_eq!(rt.state.page, Page::Artist("The Alchemist".into()));
    click(&mut rt, "back");
    assert!(matches!(rt.state.page, Page::Collab(_)));
    click(&mut rt, "collab-play");
    assert!(rt.state.playing);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(rt.state.open_album().is_some(), "Esc goes back to the album page");
}

#[test]
fn a_song_rows_ampersand_links_its_main_artists_not_the_featured_ones() {
    let mut rt = runtime();
    let calypso = rt.state.tracks.iter().position(|t| t.title == "Calypso Gene").expect("Calypso Gene");
    rt.update(|s, tx| s.play_index(calypso, tx));
    settle(&mut rt);
    let main = crate::view::collab_id(&["Armand Hammer", "The Alchemist"]);
    assert!(rt.find(&crate::view::collab_id(&["Silka", "Cleo Reed"])).is_none(), "the & between featured guests is plain text");
    click(&mut rt, &main);
    assert_eq!(rt.state.page, Page::Collab(strings(&["Armand Hammer", "The Alchemist"])));
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


/// Mercy plus Armand Hammer's older album "Ruins", so his page has two releases to order.
fn with_older_album() -> Vec<Track> {
    let song = |n: u32, title: &str| Track {
        path: format!("/m/Ruins/{n:02} {title}.flac").into(),
        title: title.into(),
        credit: Some("Armand Hammer".into()),
        album_artist: Some("Armand Hammer".into()),
        tagged: TaggedArtists { artists: strings(&["Armand Hammer"]), ..TaggedArtists::default() },
        album: Some("Ruins".into()),
        year: Some(2010),
        number: Some(n),
        duration: Some(200.0),
        ..Track::default()
    };
    let mut tracks = mercy();
    tracks.extend([song(1, "Ruin Porch"), song(2, "Dust Hymn")]);
    tracks
}

/// Armand Hammer's page with Mercy and Ruins in the library.
fn armand_hammer() -> Runtime<App> {
    let mut rt = runtime();
    rt.update(|s, _| s.set_library(with_older_album(), false));
    settle(&mut rt);
    rt.update(|s, tx| s.show_artist("Armand Hammer", tx));
    settle(&mut rt);
    rt
}

/// Clicks the option `value` of the artist page's Releases / Songs tab. The toolbar's own
/// Songs option shares the key, so this looks only inside `artist-tab`.
fn click_tab(rt: &mut Runtime<App>, value: &str) {
    let (x, y) = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(value) && e.path.contains("artist-tab"))
        .map(|e| (e.rect[0] + e.rect[2] * 0.5, e.rect[1] + e.rect[3] * 0.5))
        .unwrap_or_else(|| panic!("no {value} option in artist-tab"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

#[test]
fn the_releases_tab_is_the_default_and_songs_list_every_song_newest_album_first() {
    let mut rt = armand_hammer();
    let mercy = rt.state.albums.iter().position(|a| a.title == "Mercy").expect("Mercy");
    let ruins = rt.state.albums.iter().position(|a| a.title == "Ruins").expect("Ruins");
    assert!(rt.find("artist-tab").is_some());
    assert!(rt.find(&format!("artist-albums{mercy}")).is_some(), "Releases is the default");
    assert!(rt.find("artist-songs-table").is_none());

    click_tab(&mut rt, "songs");
    assert!(rt.find(&format!("artist-albums{mercy}")).is_none(), "the strips give way to the table");
    assert!(rt.find("artist-songs-table").is_some());
    let order: Vec<usize> = rt.state.albums[mercy].tracks.iter().chain(&rt.state.albums[ruins].tracks).copied().collect();
    assert_eq!(order.len(), 4);
    let y = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect[1];
    let ys: Vec<f32> = order.iter().map(|i| y(&rt, &format!("artist-songs-t{i}"))).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "rows in album order, newest first: {ys:?}");
    assert!(rt.find(&format!("artist-songs-cover{}", order[0])).is_some(), "rows lead with the album cover");
}

#[test]
fn switching_back_restores_the_releases_and_another_artist_opens_on_them() {
    let mut rt = armand_hammer();
    let mercy = rt.state.albums.iter().position(|a| a.title == "Mercy").expect("Mercy");
    click_tab(&mut rt, "songs");
    click_tab(&mut rt, "releases");
    assert!(rt.find(&format!("artist-albums{mercy}")).is_some());
    assert!(rt.find("artist-songs-table").is_none());

    click_tab(&mut rt, "songs");
    rt.update(|s, tx| s.show_artist("The Alchemist", tx));
    settle(&mut rt);
    assert_eq!(rt.state.artist_tab, ArtistTab::Releases, "another artist's page resets the tab");
    assert!(rt.find("artist-songs-table").is_none());
    assert!(rt.find(&format!("artist-albums{mercy}")).is_some());
}

#[test]
fn an_artist_with_no_releases_of_their_own_has_no_tab() {
    let mut rt = runtime();
    rt.update(|s, tx| s.show_artist("Silka", tx));
    settle(&mut rt);
    assert!(rt.find("artist-tab").is_none());
    assert!(rt.find("artist-music").is_none());
    assert!(rt.find("appears-on").is_some(), "Silka's songs on Mercy still list");
}

#[test]
fn the_active_folder_tab_opens_into_the_panel_it_sits_on() {
    let mut rt = armand_hammer();
    // The toolbar's Songs option shares the key, so the tab is looked up inside `artist-tab`.
    let tab = |rt: &Runtime<App>, value: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(value) && e.path.contains("artist-tab")).unwrap_or_else(|| panic!("no {value} tab")).rect;
    let panel = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("artist-music-panel")).expect("panel").rect;
    let releases = tab(&rt, "releases");
    let songs = tab(&rt, "songs");
    let bottom = |r: [f32; 4]| r[1] + r[3];
    assert_eq!(bottom(releases), panel[1] + 1.0, "the active tab's bottom edge covers the panel's top border");
    assert!(releases[0] >= panel[0] && releases[0] + releases[2] <= panel[0] + panel[2], "Releases sits within the panel's width");
    assert!(songs[0] >= panel[0] && songs[0] + songs[2] <= panel[0] + panel[2], "Songs sits within the panel's width");
    assert_eq!(releases[0], panel[0], "the first tab's left edge lines up with the panel's");
    let seam = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("artist-tab-seam")).expect("seam").rect;
    assert_eq!(seam[1], panel[1], "the seam strip is on the panel's top line");
    assert_eq!(seam[3], 1.0, "the seam strip is one point tall");
    assert!(seam[0] > releases[0] && seam[0] + seam[2] < releases[0] + releases[2], "the seam sits between the tab's side borders");
    click_tab(&mut rt, "songs");
    let songs = tab(&rt, "songs");
    assert_eq!(bottom(songs), panel[1] + 1.0, "Songs covers the panel's top border once active");
}

#[test]
fn the_tabs_sit_above_credits_and_credits_keep_their_toggle() {
    let mut rt = alchemist(elsewhere());
    let y = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect[1];
    assert!(rt.find("artist-tab").is_some());
    assert!(y(&rt, "artist-music") < y(&rt, "credits"), "the tabs come before credits");
    assert!(rt.find("credits-table").is_some(), "credits start open");
    click(&mut rt, "artist-credits-toggle");
    assert!(rt.find("credits-table").is_none());
    click(&mut rt, "artist-credits-toggle");
    assert!(rt.find("credits-table").is_some());
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
        t.credits = vec![Credit::parse(Role::Producer, "The Alchemist"), Credit::parse(Role::Mixer, "The Alchemist"), Credit::parse(Role::Engineer, "The Alchemist")];
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
    assert_eq!(pills, ["Producer", "Engineer · 1/3", "Mixer · 1/3"], "a role on every song, then ones on some");
    // Too wide for the card side by side, the pills wrap onto rows inside it.
    let rect = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && e.path.contains(&format!("/artist-albums-roles{mercy}/"))).unwrap_or_else(|| panic!("no {key}")).rect;
    let card = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("artist-albums{mercy}") as &str)).expect("Mercy's card").rect;
    let (producer, engineer, mixer) = (rect("tag-producer"), rect("tag-engineer-·-1/3"), rect("tag-mixer-·-1/3"));
    assert!(engineer[1] > producer[1] || mixer[1] > engineer[1], "a pill starts a new row");
    for r in [producer, engineer, mixer] {
        assert!(r[0] >= card[0] && r[0] + r[2] <= card[0] + card[2], "pill {r:?} inside card {card:?}");
    }
    let dry = rt.state.tracks.iter().position(|t| t.title == "Dry Spell").expect("Dry Spell");
    assert!(rt.find(&format!("credits-t{dry}")).is_none(), "own-album songs stay out of Credits");
    assert_eq!(text_of(&rt, "artist-counts"), "1 album  •  3 songs  •  credited on 2 songs");
}

/// A prefs file under the temp folder with these `artists` relations, removed on drop.
struct Prefs(std::path::PathBuf);

impl Prefs {
    fn new(name: &str, json: &str) -> Prefs {
        let dir = std::env::temp_dir().join(format!("rune-music-relations-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("prefs.json"), format!(r#"{{ "artists": {json} }}"#)).unwrap();
        Prefs(dir)
    }

    fn runtime(&self) -> Runtime<App> {
        let mut app = App::new(Output::Silent, Store::at(self.0.join("prefs.json")));
        let mut tracks = mercy();
        tracks.extend(elsewhere());
        app.set_library(tracks, false);
        let mut rt = Runtime::new(app, view, (1500.0, 1100.0), Env::new(true, 1.0));
        rt.on_key = Some(crate::app::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }
}

impl Drop for Prefs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn merged_names_open_one_page_and_links_lead_to_related_artists() {
    let prefs = Prefs::new(
        "merge",
        r#"{ "merges": [["Somebody", "The Alchemist"]], "links": [{ "from": "Cleo Reed", "kind": "member_of", "to": "the alchemist" }, { "from": "Somebody", "kind": "alias", "to": "Nobody Known" }] }"#,
    );
    let mut rt = prefs.runtime();
    assert!(rt.state.error.is_none());
    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "artist-name"), "The Alchemist", "the album artist of Mercy outweighs Somebody's one album");
    assert!(rt.find("aka-Somebody").is_some(), "Also credited as Somebody");
    assert!(rt.find("artist-albums").is_some());
    assert!(rt.find("credits").is_none(), "Elsewhere's producer credit is on one of the merged artist's own albums now");
    assert!(rt.find("artist-related").is_some());
    click(&mut rt, "artist-pill-Nobody Known");
    assert_eq!(text_of(&rt, "artist-name"), "The Alchemist", "a name the library lacks doesn't open a page");
    rt.update(|_, tx| tx.scroll_into_view_in("artist-page", "artist-pill-Cleo Reed"));
    settle(&mut rt);
    click(&mut rt, "artist-pill-Cleo Reed");
    assert_eq!(rt.state.page, Page::Artist("Cleo Reed".into()));
    assert!(rt.find("artist-pill-the alchemist").is_some(), "Cleo Reed's page lists the band she's a member of");
    assert!(rt.find("artist-aka").is_none());
}

#[test]
fn unreadable_relations_say_so_instead_of_vanishing() {
    let prefs = Prefs::new("typo", r#"{ "merges": [["A", "B"]], "linkz": [] }"#);
    let rt = prefs.runtime();
    assert!(rt.state.error.as_deref().is_some_and(|e| e.contains("\"artists\"")), "{:?}", rt.state.error);
    assert!(rt.state.relations.merges.is_empty());
}

#[test]
fn include_aliases_adds_what_they_released_under_other_names_and_play_follows() {
    let prefs = Prefs::new("aliases", r#"{ "links": [{ "from": "Somebody", "kind": "alias", "to": "The Alchemist" }] }"#);
    let mut rt = prefs.runtime();
    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "artist-counts"), "1 album  •  2 songs");
    click(&mut rt, "artist-aliases");
    assert!(rt.state.artist_with_aliases);
    assert_eq!(text_of(&rt, "artist-name"), "Somebody", "still Somebody's page");
    assert_eq!(text_of(&rt, "artist-counts"), "2 albums  •  4 songs", "Mercy joins Elsewhere");
    click(&mut rt, "artist-discography");
    let mut queue = rt.state.queue.clone().expect("a queue");
    let mut order = vec![queue.current()];
    while let Some(i) = queue.next(crate::queue::Repeat::Off) {
        order.push(i);
    }
    let played: Vec<String> = order.into_iter().map(|i| rt.state.tracks[i].title.clone()).collect();
    assert_eq!(played, ["Over There", "Far Off", "Laraaji", "Calypso Gene"], "Elsewhere (2020), then Mercy (2025)");
    // It stays on for the alias's page, which offers it too.
    click(&mut rt, "artist-pill-The Alchemist");
    assert!(rt.find("artist-aliases").is_some());
    assert!(rt.find("credits").is_none(), "his Elsewhere producer credit is on an album of his own now");
}

fn type_text(rt: &mut Runtime<App>, text: &str) {
    for c in text.chars() {
        rt.key(KeyEvent { key: Key::Text(c.to_string()), mods: Mods::default() });
    }
    settle(rt);
}

#[test]
fn link_artist_merges_links_and_removes_names_and_saves_them() {
    let prefs = Prefs::new("ui", "{}");
    let mut rt = prefs.runtime();
    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    let saved = || crate::base::store::Store::at(prefs.0.join("prefs.json")).load_relations().expect("readable");

    // Same artist, picked from the library's names as typed.
    click(&mut rt, "artist-link");
    assert!(rt.find("artist-link-panel").is_some());
    type_text(&mut rt, "alch");
    assert!(rt.find("link-option-The Alchemist").is_some(), "the library's name matches");
    assert!(rt.find("link-option-new").is_some(), "and what's typed can be added as is");
    click(&mut rt, "link-option-The Alchemist");
    assert!(rt.find("artist-link-panel").is_none(), "picking closes the popover");
    assert_eq!(saved().merges, [strings(&["Somebody", "The Alchemist"])]);
    assert_eq!(text_of(&rt, "artist-name"), "The Alchemist", "the merged page now leads with the album artist");

    // An alias that isn't in the library: Enter adds it as typed.
    click(&mut rt, "artist-link");
    click(&mut rt, "alias");
    type_text(&mut rt, "Nobody Known");
    assert!(rt.find("link-option-The Alchemist").is_none(), "their own names aren't offered");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(saved().links, [crate::relations::Link { from: "Somebody".into(), kind: crate::relations::LinkKind::Alias, to: "Nobody Known".into() }]);
    assert!(rt.find("artist-pill-Nobody Known").is_some());

    // A band member, from the band's side.
    click(&mut rt, "artist-link");
    click(&mut rt, "has-member");
    type_text(&mut rt, "cleo");
    click(&mut rt, "link-option-Cleo Reed");
    assert!(rt.find("unlink-member-Cleo Reed").is_some());

    // Removing: the alias, then Somebody out of the merge.
    click(&mut rt, "unlink-alias-Nobody Known");
    assert!(rt.find("artist-pill-Nobody Known").is_none());
    click(&mut rt, "unmerge-Somebody");
    assert!(saved().merges.is_empty());
    assert_eq!(rt.state.page, Page::Artist("The Alchemist".into()), "the page opened as Somebody moves to the name left");
    assert_eq!(saved().links.len(), 1, "Cleo Reed's membership stays with Somebody");
    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
}

#[test]
fn link_artist_refuses_while_the_saved_links_are_unreadable() {
    let prefs = Prefs::new("refuse", r#"{ "linkz": [] }"#);
    let mut rt = prefs.runtime();
    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    click(&mut rt, "artist-link");
    type_text(&mut rt, "Anyone");
    click(&mut rt, "link-option-new");
    assert!(rt.state.error.as_deref().is_some_and(|e| e.contains("couldn't be read")), "{:?}", rt.state.error);
    let file = std::fs::read_to_string(prefs.0.join("prefs.json")).unwrap();
    assert!(file.contains("linkz"), "the hand-edited entry is left alone");
}

#[test]
fn a_band_section_is_collapsed_until_opened_and_play_everything_plays_it() {
    // Somebody (on Elsewhere) is a member of The Alchemist's band, Armand Hammer's Mercy included.
    let prefs = Prefs::new("bands", r#"{ "links": [{ "from": "Somebody", "kind": "member_of", "to": "Armand Hammer" }] }"#);
    let mut rt = prefs.runtime();
    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    assert!(rt.find("related-Armand Hammer-toggle").is_some(), "the band is listed");
    assert!(rt.find("related-Armand Hammer-albums").is_none(), "collapsed by default");
    click(&mut rt, "related-Armand Hammer-toggle");
    assert_eq!(rt.state.artist_related_open, ["Armand Hammer"]);
    assert!(rt.find("related-Armand Hammer-albums").is_some(), "the band's album strip");
    click(&mut rt, "related-Armand Hammer-toggle");
    assert!(rt.find("related-Armand Hammer-albums").is_none(), "closed again");
    click(&mut rt, "artist-play-everything");
    let mut queue = rt.state.queue.clone().expect("a queue");
    let mut order = vec![queue.current()];
    while let Some(i) = queue.next(crate::queue::Repeat::Off) {
        order.push(i);
    }
    let played: Vec<String> = order.into_iter().map(|i| rt.state.tracks[i].title.clone()).collect();
    assert_eq!(played, ["Over There", "Far Off", "Laraaji", "Calypso Gene"], "Somebody's own, then the band's, each once");
}

#[test]
fn play_everything_is_absent_without_membership_links() {
    let mut rt = runtime();
    rt.update(|s, tx| s.show_artist("The Alchemist", tx));
    settle(&mut rt);
    assert!(rt.find("artist-play-everything").is_none());
    assert!(rt.find("related-Armand Hammer-toggle").is_none(), "no bands or members, no sections");
}


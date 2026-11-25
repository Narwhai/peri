//! Artist pictures driven headlessly: chosen from the ✎ menu's file picker, dropped on the
//! picture, pasted as an image or link, removed again, and kept apart for aliases.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rune::{ClipboardContent, ClipboardImage, Env, Runtime};

use crate::app::{App, Output};
use crate::library::{TaggedArtists, Track};
use crate::pictures::{INDEX_FILE, Source};
use crate::query::store::Store;
use crate::view::view;

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Two albums by two artists, so each page has a cover to fall back to.
fn library() -> Vec<Track> {
    let song = |artist: &str, id: &str, album: &str, title: &str| Track {
        path: format!("/m/{album}/01 {title}.flac").into(),
        title: title.into(),
        credit: Some(artist.into()),
        album_artist: Some(artist.into()),
        tagged: TaggedArtists { artists: strings(&[artist]), artist_ids: strings(&[id]), ..TaggedArtists::default() },
        album: Some(album.into()),
        year: Some(2020),
        number: Some(1),
        duration: Some(200.0),
        ..Track::default()
    };
    vec![song("Somebody", "mb-somebody", "Elsewhere", "Over There"), song("Nobody", "mb-nobody", "Nowhere", "Under Here")]
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 60, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A data folder under the temp folder with these prefs, removed on drop.
struct Data(PathBuf);

impl Data {
    fn new(name: &str, prefs: &str) -> Data {
        let dir = std::env::temp_dir().join(format!("peri-picture-ui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("prefs.json"), prefs).unwrap();
        Data(dir)
    }

    /// An image file to pick or drop.
    fn image(&self, name: &str, w: u32, h: u32) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, png(w, h)).unwrap();
        path
    }

    /// The runtime on `artist`'s page.
    fn runtime(&self, artist: &str) -> Runtime<App> {
        self.runtime_with(artist, library(), None)
    }

    /// The runtime on `artist`'s page with `tracks`, downloads and lookups answered by `fetch`.
    fn runtime_with(&self, artist: &str, tracks: Vec<Track>, fetch: Option<crate::pictures::Fetch>) -> Runtime<App> {
        let mut app = App::new(Output::Silent, Store::at(self.0.join("prefs.json")));
        if let Some(fetch) = fetch {
            app.pictures.set_fetch(fetch);
        }
        app.set_library(tracks, false);
        app.setup_with(Arc::new(|| {}), Vec::new());
        let mut rt = Runtime::new(app, view, (1300.0, 900.0), Env::new(true, 1.0));
        rt.on_key = Some(crate::app::on_key);
        rt.frame(0.0);
        let artist = artist.to_string();
        rt.update(move |s, tx| s.show_artist(&artist, tx));
        settle(&mut rt);
        rt
    }

    fn index(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.0.join(INDEX_FILE)).expect("the index is saved")).unwrap()
    }
}

impl Drop for Data {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Opens the picture's ✎ menu and picks its `n`th item.
fn menu(rt: &mut Runtime<App>, n: usize) {
    click(rt, "artist-picture-edit");
    assert!(rt.state.picture_menu, "the ✎ button opens the menu");
    click(rt, &format!("artist-picture-item{n}"));
}

const CHOOSE: usize = 0;
const PASTE: usize = 1;
const KEEP: usize = 3;
/// Choose from Wikimedia…: the picker.
const LOOK_UP: usize = 4;
const REMOVE: usize = 6;

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn names(v: &[&str]) -> Vec<String> {
    strings(v)
}

/// The picture shown: the image's pixel size, or none for the placeholder.
fn shown(rt: &Runtime<App>, names: &[String]) -> Option<[u32; 2]> {
    rt.state.pictures.get(names).and_then(|i| i.wait().ok())
}

fn stored(data: &Data, file: &str) -> image::DynamicImage {
    image::load_from_memory(&std::fs::read(data.0.join(file)).expect("copied into the data folder")).unwrap()
}

#[test]
fn choose_image_picks_a_file_that_is_copied_in_shrunk_and_shown() {
    let data = Data::new("choose", "{}");
    let file = data.image("photo.png", 2400, 1600);
    let mut rt = data.runtime("Somebody");
    let somebody = names(&["Somebody"]);
    assert!(rt.find("artist-picture").is_some());
    assert!(shown(&rt, &somebody).is_none(), "no picture yet: the album cover, which headless tracks lack");

    menu(&mut rt, CHOOSE);
    let (kind, options) = rt.file_dialog().expect("Choose image… opens a file dialog");
    assert_eq!(kind, rune::FileDialogKind::Open);
    assert_eq!(options.filters, [("Images".to_string(), strings(&["png", "jpg", "jpeg", "webp", "gif"]))]);
    rt.resolve_file_dialog(Some(file.clone())).unwrap();
    settle(&mut rt);

    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
    let entry = rt.state.pictures.entry_for(&somebody).expect("saved");
    assert_eq!((entry.source, entry.mbid.as_deref()), (Source::Local, Some("mb-somebody")));
    let img = stored(&data, &entry.file);
    assert_eq!((img.width(), img.height()), (1000, 667), "shrunk to 1000 px on its longer side");
    assert!(file.exists(), "the chosen file is copied, not moved");
    assert_eq!(data.index()["artists"][0]["names"], serde_json::json!(["Somebody"]));
    assert_eq!(shown(&rt, &somebody), Some([480, 320]), "drawn from a 480 px thumbnail");
    assert!(!rt.state.pictures.has(&names(&["Nobody"])), "other artists keep their own");

    // Cancelling changes nothing.
    menu(&mut rt, CHOOSE);
    rt.resolve_file_dialog(None).unwrap();
    settle(&mut rt);
    assert_eq!(rt.state.pictures.entry_for(&somebody), Some(entry));
}

#[test]
fn an_image_dropped_on_the_picture_highlights_it_and_becomes_the_picture() {
    let data = Data::new("drop", "{}");
    let file = data.image("dropped.jpg", 300, 300);
    let mut rt = data.runtime("Somebody");
    let at = rt.find("artist-picture").unwrap();

    rt.drag_files(vec![file.clone()], Some(at));
    settle(&mut rt);
    assert_eq!(rt.drop_hovered().as_deref(), Some("artist-picture"));
    assert!(rt.state.picture_drop_hover && rt.find("artist-picture-drop").is_some(), "the picture lights up");
    assert!(!rt.state.drop_hover, "not the window's add-to-library veil");

    rt.drop_files(vec![file], Some(at));
    settle(&mut rt);
    assert!(!rt.state.picture_drop_hover && rt.find("artist-picture-drop").is_none());
    let entry = rt.state.pictures.entry_for(&names(&["Somebody"])).expect("the drop is the picture");
    assert_eq!(entry.source, Source::Local);
    assert!(rt.state.tracks().len() == 2, "an image isn't added to the library");

    // A song dragged over the picture is the window's to add, not the picture's.
    let song = data.0.join("song.flac");
    rt.drag_files(vec![song], Some(at));
    settle(&mut rt);
    assert_ne!(rt.drop_hovered().as_deref(), Some("artist-picture"));
    assert!(!rt.state.picture_drop_hover && rt.state.drop_hover, "the window's veil shows instead");
    rt.drag_files_leave();
    settle(&mut rt);
}

#[test]
fn paste_takes_an_image_or_a_link_from_the_menu_and_ctrl_v() {
    let data = Data::new("paste", "{}");
    let mut rt = data.runtime("Somebody");
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    let somebody = names(&["Somebody"]);

    // An image copied from a browser, through the menu.
    clipboard.set_content(ClipboardContent::new("").with_image(ClipboardImage::from_png(png(64, 48)).unwrap()));
    menu(&mut rt, PASTE);
    let first = rt.state.pictures.entry_for(&somebody).expect("the pasted image");
    assert_eq!((first.source, first.url.as_deref()), (Source::User, None));
    assert_eq!(shown(&rt, &somebody), Some([64, 48]));

    // A link, with Ctrl+V on the page: downloaded only because the user pasted it. Tests
    // never reach the network, so a stand-in answers.
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = asked.clone();
    rt.state.pictures.set_fetch(Arc::new(move |url: &str| {
        log.lock().unwrap().push(url.to_string());
        Ok(png(900, 900))
    }));
    let link = "https://lastfm.freetls.fastly.net/i/u/770x0/somebody.jpg";
    rt.paste_content(ClipboardContent::new(link));
    settle(&mut rt);
    assert_eq!(*asked.lock().unwrap(), [link]);
    let second = rt.state.pictures.entry_for(&somebody).expect("the linked image");
    assert_eq!((second.source, second.url.as_deref()), (Source::User, Some(link)));
    assert_ne!(second.file, first.file, "it replaces the pasted one");
    assert!(!data.0.join(&first.file).exists());

    // Text that's neither says so and keeps the picture.
    rt.paste_content(ClipboardContent::new("just some words"));
    settle(&mut rt);
    assert!(rt.state.error.as_deref().is_some_and(|e| e.contains("no image")), "{:?}", rt.state.error);
    assert_eq!(rt.state.pictures.entry_for(&somebody), Some(second));
}

#[test]
fn a_pasted_link_is_never_downloaded_headless() {
    let data = Data::new("headless", "{}");
    let mut rt = data.runtime("Somebody");
    rt.paste_content(ClipboardContent::new("https://example.com/a.jpg"));
    settle(&mut rt);
    assert!(rt.state.error.as_deref().is_some_and(|e| e.contains("headless")), "{:?}", rt.state.error);
    assert!(!rt.state.pictures.has(&names(&["Somebody"])));
}

#[test]
fn remove_picture_deletes_it_and_the_album_cover_shows_again() {
    let data = Data::new("remove", "{}");
    let file = data.image("photo.png", 100, 100);
    let mut rt = data.runtime("Somebody");
    menu(&mut rt, CHOOSE);
    rt.resolve_file_dialog(Some(file)).unwrap();
    settle(&mut rt);
    let entry = rt.state.pictures.entry_for(&names(&["Somebody"])).unwrap();
    menu(&mut rt, REMOVE);
    assert!(!rt.state.pictures.has(&names(&["Somebody"])));
    assert!(!data.0.join(&entry.file).exists(), "the copy is deleted");
    assert_eq!(data.index()["artists"], serde_json::json!([]));
    assert!(shown(&rt, &names(&["Somebody"])).is_none());
}

#[test]
fn merged_names_share_a_picture_while_an_alias_keeps_its_own() {
    let prefs = r#"{ "artists": { "merges": [["Somebody", "Some Body"]], "links": [{ "from": "Somebody", "kind": "alias", "to": "Nobody" }] } }"#;
    let data = Data::new("merged", prefs);
    let file = data.image("photo.png", 100, 100);
    let mut rt = data.runtime("Somebody");
    menu(&mut rt, CHOOSE);
    rt.resolve_file_dialog(Some(file)).unwrap();
    settle(&mut rt);
    let entry = rt.state.pictures.entry_for(&names(&["some body"])).expect("found by the merged name");
    assert_eq!(entry.names, ["Somebody", "Some Body"]);
    assert!(!rt.state.pictures.has(&names(&["Nobody"])), "the alias has its own page and picture");

    // The next launch shows it again.
    drop(rt);
    let rt = data.runtime("Some Body");
    assert_eq!(shown(&rt, &names(&["Somebody", "Some Body"])), Some([100, 100]));
    assert!(Path::new(&data.0.join(&entry.file)).exists());
}

/// Somebody's album under `music/Somebody/Elsewhere`, so their folder can hold an artist.jpg.
fn in_folder(data: &Data) -> Vec<Track> {
    let mut tracks = library();
    let dir = data.0.join("music/Somebody/Elsewhere");
    std::fs::create_dir_all(&dir).unwrap();
    tracks[0].path = dir.join("01 Over There.flac");
    tracks
}

impl Data {
    /// The runtime on Somebody's page, their songs in the music folder, lookups answered by
    /// `fetch` (the network is never reached).
    fn online(&self, fetch: crate::pictures::Fetch) -> Runtime<App> {
        self.runtime_with("Somebody", in_folder(self), Some(fetch))
    }
}

fn wikimedia() -> (crate::pictures::Fetch, Arc<std::sync::Mutex<Vec<String>>>) {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    (crate::pictures::online::tests::fake(png(1200, 800), asked.clone()), asked)
}

#[test]
fn an_artist_jpg_in_their_music_folder_shows_until_they_have_their_own() {
    let data = Data::new("folder", "{}");
    let mut rt = data.online(Arc::new(crate::pictures::no_downloads));
    let somebody = names(&["Somebody"]);
    assert_eq!(rt.state.artist_picture(&rt.state.artist_model("Somebody", false).0).1, crate::pictures::Shown::Cover);

    let folder_pic = data.0.join("music/Somebody/artist.jpg");
    std::fs::write(&folder_pic, png(90, 90)).unwrap();
    // Found once per library version, as a rescan would.
    let tracks = in_folder(&data);
    rt.update(move |s, _| s.set_library(tracks, false));
    settle(&mut rt);
    let info = rt.state.artist_model("Somebody", false).0.clone();
    let (image, shown) = rt.state.artist_picture(&info);
    assert_eq!(shown, crate::pictures::Shown::Folder(folder_pic.clone()));
    assert_eq!(image.and_then(|i| i.wait().ok()), Some([90, 90]));
    assert!(!rt.state.pictures.has(&somebody), "shown in place, not copied in");
    assert!(rt.find("artist-picture-attribution").is_none());

    // Their own wins.
    let file = data.image("mine.png", 40, 40);
    menu(&mut rt, CHOOSE);
    rt.resolve_file_dialog(Some(file)).unwrap();
    settle(&mut rt);
    assert!(matches!(rt.state.artist_picture(&info).1, crate::pictures::Shown::User(None)));
    assert!(folder_pic.exists(), "the folder's picture is left alone");
}

#[test]
fn lookups_are_off_by_default_and_the_settings_switch_turns_them_on_and_saves() {
    let data = Data::new("off", "{}");
    let (fetch, asked) = wikimedia();
    let mut rt = data.online(fetch);
    assert!(!rt.state.picture_lookups);
    assert!(asked.lock().unwrap().is_empty(), "nothing is looked up unless turned on");
    assert!(rt.find("artist-picture-attribution").is_none());

    rt.update(|s, _| s.show_settings());
    settle(&mut rt);
    rt.update(|_, tx| tx.scroll_into_view_in("settings-page", "picture-lookups"));
    settle(&mut rt);
    click(&mut rt, "picture-lookups");
    assert!(rt.state.picture_lookups);
    let prefs: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.0.join("prefs.json")).unwrap()).unwrap();
    assert_eq!(prefs["artist_picture_lookups"], serde_json::json!(true));

    rt.update(|s, tx| s.show_artist("Somebody", tx));
    settle(&mut rt);
    let urls = asked.lock().unwrap().clone();
    assert!(urls[0].starts_with("https://musicbrainz.org/ws/2/artist/"), "{urls:?}");
    assert!(urls.iter().any(|u| u.starts_with("https://www.wikidata.org/")));
    assert!(urls.iter().any(|u| u.starts_with("https://commons.wikimedia.org/")));
    assert!(rt.find("artist-picture-attribution").is_some(), "a Commons picture shows with its credit");
    assert!(!rt.state.pictures.has(&names(&["Somebody"])), "only shown until kept");
}

#[test]
fn a_looked_up_picture_is_kept_with_its_credit_and_never_replaced() {
    let data = Data::new("keep", r#"{ "artist_picture_lookups": true }"#);
    let (fetch, asked) = wikimedia();
    let mut rt = data.online(fetch);
    let somebody = names(&["Somebody"]);
    let attribution = text_of(&rt, "artist-picture-attribution");
    assert_eq!(attribution, "Photo: Jane Doe & co · CC BY-SA 4.0 · Wikimedia Commons");

    menu(&mut rt, KEEP);
    let entry = rt.state.pictures.entry_for(&somebody).expect("kept");
    assert_eq!(entry.source, Source::Wikimedia);
    assert_eq!(entry.attribution.as_ref().map(|a| a.page.as_str()), Some("https://commons.wikimedia.org/wiki/File:Somebody_live_2019.jpg"));
    assert_eq!(stored(&data, &entry.file).width(), 1000);
    assert_eq!(text_of(&rt, "artist-picture-attribution"), attribution, "the credit stays with it");

    // Next launch: theirs, with no lookup at all.
    drop(rt);
    asked.lock().unwrap().clear();
    let (fetch, asked) = wikimedia();
    let rt = data.online(fetch);
    assert!(asked.lock().unwrap().is_empty(), "a page with a picture of its own isn't looked up");
    assert_eq!(rt.state.pictures.entry_for(&somebody), Some(entry));
}

#[test]
fn removing_a_looked_up_picture_declines_it_for_good() {
    let data = Data::new("decline", r#"{ "artist_picture_lookups": true }"#);
    let (fetch, asked) = wikimedia();
    let mut rt = data.online(fetch.clone());
    assert!(rt.find("artist-picture-attribution").is_some());
    menu(&mut rt, REMOVE);
    assert!(rt.find("artist-picture-attribution").is_none(), "removed");
    assert_eq!(data.index()["declined"], serde_json::json!([["Somebody"]]));

    drop(rt);
    asked.lock().unwrap().clear();
    let rt = data.online(fetch);
    assert!(asked.lock().unwrap().is_empty(), "a declined page isn't looked up next launch");
    assert!(rt.find("artist-picture-attribution").is_none());
}

#[test]
fn the_picker_offers_every_wikimedia_picture_and_the_pick_is_kept_with_its_credit() {
    // Lookups off: the picker is asked for, so it looks anyway.
    let data = Data::new("picker", "{}");
    let (fetch, asked) = wikimedia();
    let mut rt = data.online(fetch);
    assert!(asked.lock().unwrap().is_empty());
    menu(&mut rt, LOOK_UP);
    assert!(rt.find("picture-picker-dialog").is_some(), "the picker opens");
    let found = match &rt.state.pictures.picker().unwrap().candidates {
        Some(Ok(found)) => found.clone(),
        other => panic!("{:?}", other.as_ref().map(|r| r.as_ref().map(Vec::len))),
    };
    assert_eq!(found.len(), 2 + 12, "the main image, MusicBrainz's other, and a dozen category photos");
    for n in 0..found.len() {
        assert!(rt.find(&format!("picker-{n}")).is_some() || n >= 8, "tile {n}");
    }
    assert!(rt.state.pictures.picker().unwrap().thumb(&found[0].file).is_some_and(|t| t.is_some()), "thumbnails download");
    assert!(!rt.state.pictures.has(&names(&["Somebody"])), "nothing is kept by looking");

    // Pick the second: MusicBrainz's band photo.
    click(&mut rt, "picker-1");
    assert!(rt.state.pictures.picker().is_none(), "picking closes the picker");
    assert!(rt.state.error.is_none(), "{:?}", rt.state.error);
    let entry = rt.state.pictures.entry_for(&names(&["Somebody"])).expect("the pick is theirs");
    assert_eq!(entry.source, Source::Wikimedia);
    assert_eq!(entry.attribution.as_ref().map(|a| a.page.as_str()), Some("https://commons.wikimedia.org/wiki/File:Somebody_live_2019.jpg"), "its credit, as Commons gives it (the stand-in gives every file the same one)");
    assert!(asked.lock().unwrap().iter().any(|u| u.contains("iiurlwidth=1000") && u.contains("band")), "the picked file is fetched at full size");
    assert!(rt.find("artist-picture-attribution").is_some(), "shown with its credit");
}

#[test]
fn the_picker_says_when_nothing_is_found_or_the_lookup_fails() {
    let data = Data::new("picker-fail", "{}");
    let mut rt = data.online(Arc::new(|_: &str| Err("offline".to_string())));
    menu(&mut rt, LOOK_UP);
    assert!(rt.find("picker-error").is_some());
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    settle(&mut rt);
    assert!(rt.state.pictures.picker().is_none(), "Escape closes it");
    assert_eq!(rt.state.page, crate::app::Page::Artist("Somebody".into()), "and stays on the page");
    assert!(!rt.state.pictures.has(&names(&["Somebody"])));

    // A name MusicBrainz can't tell apart: nothing to offer.
    rt.update(|s, _| s.pictures.set_fetch(Arc::new(|_: &str| Ok(br#"{"artists":[]}"#.to_vec()))));
    menu(&mut rt, LOOK_UP);
    assert!(rt.find("picker-empty").is_some());
}

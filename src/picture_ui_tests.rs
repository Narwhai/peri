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
        let mut app = App::new(Output::Silent, Store::at(self.0.join("prefs.json")));
        app.set_library(library(), false);
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
const REMOVE: usize = 3;

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

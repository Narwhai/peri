//! The library folders: saved as they're opened, dropped or removed, and scanned at launch.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rune::{Env, Runtime};

use crate::app::{App, Output, Page};
use crate::palette::{Appearance, Theme};
use crate::query::store::Store;
use crate::view::view;

/// A temp folder holding `prefs.json` and a music folder with `songs` (fake audio files).
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("peri-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Fixture { dir }
    }

    fn prefs(&self) -> PathBuf {
        self.dir.join("prefs.json")
    }

    fn music(&self, folder: &str, songs: &[&str]) -> PathBuf {
        let root = self.dir.join(folder);
        std::fs::create_dir_all(&root).unwrap();
        for song in songs {
            std::fs::write(root.join(song), b"not really audio").unwrap();
        }
        root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn runtime(prefs: &Path) -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::at(prefs));
    app.set_waker(Arc::new(|| {}));
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    rt
}

/// Frames until the scan finishes.
fn finish_scan(rt: &mut Runtime<App>) {
    for _ in 0..500 {
        rt.frame(1.0 / 60.0);
        if rt.state.scan.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(rt.state.scan.is_none(), "the scan never finished");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn titles(rt: &Runtime<App>) -> Vec<String> {
    let mut t: Vec<String> = rt.state.tracks.iter().map(|t| t.title.clone()).collect();
    t.sort();
    t
}

#[test]
fn nothing_is_saved_until_a_folder_is_opened() {
    let f = Fixture::new("empty");
    assert!(Store::at(f.prefs()).load_library().is_empty());
}

#[test]
fn an_opened_folder_is_remembered_and_scanned_at_the_next_launch() {
    let f = Fixture::new("open");
    let music = f.music("music", &["a.mp3", "b.flac"]);
    let mut rt = runtime(&f.prefs());
    rt.update(|s, _| s.replace_library(vec![music.clone()]));
    finish_scan(&mut rt);
    assert_eq!(titles(&rt), ["a", "b"]);
    assert_eq!(Store::at(f.prefs()).load_library(), std::slice::from_ref(&music));

    // Next launch: setup scans what was saved.
    let mut app = App::new(Output::Silent, Store::at(f.prefs()));
    // No command-line folders: the test harness's own arguments aren't the app's.
    app.setup_with(Arc::new(|| {}), Vec::new());
    assert_eq!(app.library, [music]);
    assert_eq!(app.tracks.len(), 2, "headless setup waits for the saved library's scan");
}

#[test]
fn opening_a_folder_replaces_the_saved_one_and_dropping_adds_to_it() {
    let f = Fixture::new("drop");
    let first = f.music("first", &["a.mp3"]);
    let second = f.music("second", &["b.mp3"]);
    let dropped = f.music("dropped", &["c.mp3"]);
    let mut rt = runtime(&f.prefs());
    rt.update(|s, _| s.replace_library(vec![first]));
    finish_scan(&mut rt);
    rt.update(|s, _| s.replace_library(vec![second.clone()]));
    finish_scan(&mut rt);
    rt.update(|s, _| s.add_to_library(vec![dropped.clone()]));
    finish_scan(&mut rt);
    assert_eq!(titles(&rt), ["b", "c"]);
    assert_eq!(Store::at(f.prefs()).load_library(), [second.clone(), dropped.clone()]);
    // Dropping the same folder again doesn't list it twice.
    rt.update(|s, _| s.add_to_library(vec![dropped.clone()]));
    finish_scan(&mut rt);
    assert_eq!(Store::at(f.prefs()).load_library(), [second, dropped]);
}

#[test]
fn the_settings_page_lists_folders_and_removes_them() {
    let f = Fixture::new("remove");
    let a = f.music("a", &["one.mp3"]);
    let b = f.music("b", &["two.mp3"]);
    let mut rt = runtime(&f.prefs());
    rt.update(|s, _| s.replace_library(vec![a.clone()]));
    finish_scan(&mut rt);
    rt.update(|s, _| s.add_to_library(vec![b.clone()]));
    finish_scan(&mut rt);
    click(&mut rt, "settings");
    assert_eq!(rt.state.page, Page::Settings);
    assert!(rt.find("toolbar").is_none(), "settings have no view toolbar");
    assert!(rt.find("library-root0").is_some());
    assert!(rt.find("library-root1").is_some());
    click(&mut rt, "library-root0-remove");
    finish_scan(&mut rt);
    assert_eq!(rt.state.library, std::slice::from_ref(&b));
    assert_eq!(Store::at(f.prefs()).load_library(), [b]);
    assert_eq!(titles(&rt), ["two"], "the removed folder's songs leave the library");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Library);
}

#[test]
fn forgetting_the_library_empties_it_and_its_saved_folders() {
    let f = Fixture::new("forget");
    let music = f.music("music", &["a.mp3"]);
    let mut rt = runtime(&f.prefs());
    rt.update(|s, _| s.replace_library(vec![music]));
    finish_scan(&mut rt);
    click(&mut rt, "settings");
    click(&mut rt, "library-forget");
    assert!(rt.state.tracks.is_empty());
    assert!(Store::at(f.prefs()).load_library().is_empty());
    assert!(rt.find("library-empty").is_some());
}

#[test]
fn a_missing_folder_stays_saved_and_shows_as_not_found() {
    let f = Fixture::new("missing");
    let gone = f.dir.join("unplugged-drive");
    Store::at(f.prefs()).save_library(std::slice::from_ref(&gone)).unwrap();
    let mut rt = runtime(&f.prefs());
    rt.update(|s, _| s.rescan_library());
    finish_scan(&mut rt);
    assert!(rt.state.error.as_deref().is_some_and(|e| e.contains("unplugged-drive")), "the scan error shows: {:?}", rt.state.error);
    assert_eq!(rt.state.library, std::slice::from_ref(&gone));
    assert_eq!(Store::at(f.prefs()).load_library(), [gone]);
    click(&mut rt, "settings");
    assert!(rt.find("library-root0-missing").is_some());
}

/// A runtime on the settings page, with the system's dark setting `system_dark`.
fn appearance_runtime(prefs: &Path, system_dark: bool) -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::at(prefs));
    app.set_waker(Arc::new(|| {}));
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(system_dark, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    click(&mut rt, "settings");
    rt
}

/// The text colour of the settings page's title, which follows the palette's text colour.
fn title_colour(rt: &Runtime<App>) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("settings-title")).and_then(|e| e.text_color).map(|c| c.hex()).expect("the settings title's colour")
}

fn palette_text(theme: Theme, dark: bool) -> String {
    theme.palette(dark).tx.hex()
}

#[test]
fn appearance_defaults_to_system_and_follows_it() {
    let f = Fixture::new("appearance-default");
    let rt = appearance_runtime(&f.prefs(), true);
    assert_eq!(rt.state.appearance, Appearance::System);
    assert_eq!(rt.state.theme, Theme::Bolis);
    assert_eq!(title_colour(&rt), palette_text(Theme::Bolis, true));
    let rt = appearance_runtime(&f.prefs(), false);
    assert_eq!(title_colour(&rt), palette_text(Theme::Bolis, false));
}

#[test]
fn dark_draws_dark_while_the_system_is_light() {
    let f = Fixture::new("appearance-dark");
    let mut rt = appearance_runtime(&f.prefs(), false);
    click(&mut rt, "dark");
    assert_eq!(rt.state.appearance, Appearance::Dark);
    assert_eq!(title_colour(&rt), palette_text(Theme::Bolis, true));
}

#[test]
fn light_draws_light_while_the_system_is_dark() {
    let f = Fixture::new("appearance-light");
    let mut rt = appearance_runtime(&f.prefs(), true);
    click(&mut rt, "light");
    assert_eq!(rt.state.appearance, Appearance::Light);
    assert_eq!(title_colour(&rt), palette_text(Theme::Bolis, false));
}

#[test]
fn the_appearance_is_saved_and_reloaded() {
    let f = Fixture::new("appearance-saved");
    let mut rt = appearance_runtime(&f.prefs(), false);
    click(&mut rt, "dark");
    assert_eq!(Store::at(f.prefs()).load_appearance(), Ok(Appearance::Dark));
    let app = App::new(Output::Silent, Store::at(f.prefs()));
    assert_eq!(app.appearance, Appearance::Dark);
    assert_eq!(app.theme, Theme::Bolis);
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn an_unreadable_appearance_shows_an_error_and_falls_back_to_system() {
    let f = Fixture::new("appearance-bad");
    std::fs::write(f.prefs(), r#"{"appearance": "sepia"}"#).unwrap();
    let app = App::new(Output::Silent, Store::at(f.prefs()));
    assert_eq!(app.appearance, Appearance::System);
    assert!(app.error.as_deref().is_some_and(|e| e.contains("\"appearance\"")), "{:?}", app.error);
}

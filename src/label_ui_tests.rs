//! Label pages driven headlessly: a label's name opens its page of albums, and Back returns.

use rune::{Env, Runtime};

use crate::app::{App, Output, Page};
use crate::query::store::Store;
use crate::view::view;

fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_label(&mut app);
    app.page = Page::Library;
    let mut rt = Runtime::new(app, view, (1200.0, 900.0), Env::new(true, 1.0));
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

/// Clicks the element keyed `key`, or the link whose id (a name) is `key`.
fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element or link {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn album(rt: &Runtime<App>, title: &str) -> usize {
    rt.state.albums.iter().position(|a| a.title == title).unwrap_or_else(|| panic!("no album {title}"))
}

#[test]
fn the_album_pages_label_opens_the_labels_albums() {
    let mut rt = runtime();
    let spaces = album(&rt, "Spaces");
    rt.update(|s, tx| s.show_album(spaces, tx));
    settle(&mut rt);
    click(&mut rt, "Erased Tapes");
    assert_eq!(rt.state.page, Page::Label("Erased Tapes".into()));
    assert_eq!(text_of(&rt, "label-name"), "Erased Tapes");
    assert_eq!(text_of(&rt, "label-counts"), "3 albums  •  3 artists");
    for title in ["Live Sessions", "Spaces", "Day of Radiance"] {
        assert!(rt.find(&format!("label-albums{}", album(&rt, title))).is_some(), "{title} is listed");
    }
    assert!(rt.find(&format!("label-albums{}", album(&rt, "Get Up"))).is_none(), "Get Up has no label");
    click(&mut rt, "back");
    assert_eq!(rt.state.page, Page::Album(rt.state.albums[spaces].id()));
}

#[test]
fn a_card_opens_its_album_and_play_plays_newest_first() {
    let mut rt = runtime();
    rt.update(|s, tx| s.show_label("erased tapes", tx));
    settle(&mut rt);
    assert_eq!(text_of(&rt, "label-name"), "Erased Tapes", "matched ignoring case");
    click(&mut rt, "label-play");
    let playing = rt.state.current.map(|i| rt.state.tracks[i].title.clone());
    assert_eq!(playing.as_deref(), Some("Mary"), "Live Sessions (2024) comes first");
    let day = album(&rt, "Day of Radiance");
    click(&mut rt, &format!("label-albums{day}"));
    assert_eq!(rt.state.open_album(), Some(day));
}

#[test]
fn an_unknown_label_says_so() {
    let mut rt = runtime();
    rt.update(|s, tx| s.show_label("Nobody", tx));
    settle(&mut rt);
    assert!(rt.find("label-empty").is_some());
    assert!(rt.find("label-play").is_none());
}

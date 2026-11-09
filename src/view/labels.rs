//! The label page: the albums released under a record label, one block per release type.

use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::albums::{album_wall, back_label, cover, cover_image, play_button};
use super::*;
use crate::album::ReleaseType;
use crate::label::page;
use crate::palette::Palette;

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);

pub(super) fn label_page(s: &App, name: &str, p: &Palette) -> El {
    let info = page(name, &s.albums);
    let back = row()
        .key("back")
        .h(28.0)
        .pl(6.0)
        .pr(10.0)
        .gap(4.0)
        .radius(6.0)
        .center_y()
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .label("Back")
        .help("Back (Esc)")
        .on_press(|s: &mut App| s.go_back())
        .child(icon(&icons::BACK, 16.0).color(p.tx2))
        .child(text(back_label(s)).font(TITLE).color(p.tx2));

    let first = info.albums.first().copied();
    let art = cover("label-cover".into(), first.and_then(|a| cover_image(s, &s.albums[a])), Some(160.0), 80.0, p);
    let mut counts = Vec::new();
    for kind in ReleaseType::ALL {
        let n = info.albums.iter().filter(|&&a| s.albums[a].release_type == kind).count();
        if n > 0 {
            counts.push(kind.count(n));
        }
    }
    let mut artists: Vec<String> = Vec::new();
    for &a in &info.albums {
        let name = s.albums[a].artist_name();
        if !artists.iter().any(|x| x.eq_ignore_ascii_case(name)) {
            artists.push(name.to_string());
        }
    }
    if !artists.is_empty() {
        counts.push(plural(artists.len(), "artist"));
    }
    let mut details = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Label").font(SMALL).color(p.tx3))
        .child(text(info.name.clone()).key("label-name").font(PAGE_TITLE).color(p.tx).wrap())
        .child(text(counts.join("  •  ")).key("label-counts").font(BODY).color(p.tx3));
    if !info.albums.is_empty() {
        let who = info.name.clone();
        details = details.child(el().h(10.0)).child(row().gap(8.0).child(play_button("label-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, &s.albums).play_list(&s.albums);
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        })));
    }
    let head = row().gap(28.0).center_y().child(art).child(details);

    let mut body = col().key("label-page-body").w_full().px(28.0).py(16.0).gap(22.0).child(back).child(head);
    if info.albums.is_empty() {
        body = body.child(text(format!("Nothing in the library was released on {}.", info.name)).key("label-empty").font(BODY).color(p.tx3));
    }
    for kind in ReleaseType::ALL {
        let of_kind: Vec<usize> = info.albums.iter().copied().filter(|&a| s.albums[a].release_type == kind).collect();
        if !of_kind.is_empty() {
            body = body.child(album_wall(s, &format!("label-{}", kind.plural().to_lowercase()), kind.plural(), &of_kind, true, p));
        }
    }
    col().key("label-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

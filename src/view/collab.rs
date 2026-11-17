//! The collaboration page: what two or more artists released together, then the songs
//! they're on together elsewhere.

use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::albums::{back_label, cover, cover_image, play_button, tinted_button};
use super::artists::{boxed, release_strips, songs};
use super::*;
use crate::album::ReleaseType;
use crate::collab::page;

use crate::palette::Palette;

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);

pub(super) fn collab_page(s: &App, names: &[String], p: &Palette) -> El {
    let info = s.collab_model(names);
    let back = row()
        .key("back")
        .h(28.0)
        .pl(6.0)
        .pr(10.0)
        .gap(4.0)
        .radius(3.0)
        .center_y()
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .label("Back")
        .help("Back (Esc)")
        .on_press_tx(|s: &mut App, tx| s.go_back(tx))
        .child(icon(&icons::BACK, 16.0).color(p.tx2))
        .child(text(back_label(s)).font(TITLE).color(p.tx2));

    let first = info.albums.first().copied().or_else(|| info.appears_on.first().and_then(|&i| s.album_of(i)));
    let art = cover("collab-cover".into(), first.and_then(|a| cover_image(s, &s.albums()[a])), Some(160.0), p);
    let mut counts = Vec::new();
    for kind in ReleaseType::ALL {
        let n = info.albums.iter().filter(|&&a| s.albums()[a].release_type == kind).count();
        if n > 0 {
            counts.push(kind.count(n));
        }
    }
    let songs_together = info.play_list(s.albums()).len();
    if songs_together > 0 {
        counts.push(plural(songs_together, "song"));
    }
    // "A & B", each name opening its artist's page.
    let title = info.title();
    let links = name_links(&title, names);
    let mut details = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Collaboration").font(SMALL).color(p.tx3))
        .child(linked(title, links, p.tx).key("collab-name").font(PAGE_TITLE).wrap())
        .child(text(counts.join("  •  ")).key("collab-counts").font(BODY).color(p.tx3));
    let mut buttons = row().gap(8.0);
    if songs_together > 0 {
        let who = names.to_vec();
        buttons = buttons.child(play_button("collab-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, s.tracks(), s.albums(), s.relations()).play_list(s.albums());
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    if !info.albums.is_empty() {
        let who = names.to_vec();
        buttons = buttons.child(tinted_button("collab-discography", &icons::PLAY, "Play Discography", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, s.tracks(), s.albums(), s.relations()).discography(s.albums());
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    details = details.child(el().h(10.0)).child(buttons);
    let head = row().gap(28.0).center_y().child(art).child(details);

    let mut body = col().key("collab-page-body").w_full().px(28.0).py(16.0).gap(22.0).child(back).child(head);
    if info.is_empty() {
        body = body.child(text(format!("Nothing in the library is by {} together.", info.title())).key("collab-empty").font(BODY).color(p.tx3));
    }
    if !info.albums.is_empty() {
        body = body.child(boxed(col().key("collab-releases").w_full().gap(22.0).children(release_strips(s, "collab", &info.albums, false, &[], p)), p));
    }
    if !info.appears_on.is_empty() {
        body = body.child(songs(s, "collab-appears-on", "Together on", &info.appears_on, p));
    }
    col().key("collab-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

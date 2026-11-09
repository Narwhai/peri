//! The search page: the artist the query finds laid out like their page, the other matching
//! artists, then matching albums and songs. It follows the search field as it changes.

use rune::{Cursor, El, ScrollSpec, col, row, text};

use super::artists;
use super::*;
use crate::palette::Palette;
use crate::search::search;

pub(super) fn search_page(s: &App, p: &Palette) -> El {
    let query = s.search.text.trim().to_string();
    let found = search(&query, &s.tracks, &s.albums);
    let mut body = col().key("search-page-body").w_full().px(28.0).py(16.0).gap(22.0);
    if found.is_empty() {
        body = body.child(
            col().key("no-results").w_full().gap(6.0).child(text("No matches").font(HEADING).color(p.tx2)).child(text(format!("Nothing in the library matches “{query}”.")).font(BODY).color(p.tx3)),
        );
    }
    if let Some(info) = &found.artist {
        body = body.child(artists::head(s, info, true, p)).children(artists::sections(s, info, p));
    }
    if !found.artists.is_empty() {
        let mut list = row().gap(8.0).wrap();
        for name in &found.artists {
            let who = name.clone();
            list = list.child(
                row()
                    .key(format!("other-artist-{name}"))
                    .h(30.0)
                    .px(12.0)
                    .radius(15.0)
                    .center_y()
                    .fill(p.ui.alpha(0.5))
                    .hover(p.ui.alpha(0.8))
                    .cursor(Cursor::Pointer)
                    .role("link")
                    .label(name.clone())
                    .on_press_tx(move |s: &mut App, tx| s.show_artist(&who, tx))
                    .child(text(name.clone()).font(TITLE).color(p.tx)),
            );
        }
        let title = if found.artist.is_some() { "Other artists" } else { "Artists" };
        body = body.child(col().key("search-artists").gap(10.0).child(text(title).font(HEADING).color(p.tx)).child(list));
    }
    body = body.children(artists::release_strips(s, "search", &found.albums, true, p));
    if !found.songs.is_empty() {
        body = body.child(artists::songs(s, "search-songs", "Songs", &found.songs, None, "", p));
    }
    col().key("search-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

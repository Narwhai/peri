//! The search page: the artist the query finds laid out like their page, the other matching
//! artists, then matching albums and songs. It follows the search field as it changes.

use rune::{El, ScrollSpec, col, row, text};

use super::artists;
use super::*;
use crate::palette::Palette;

pub(super) fn search_page(s: &App, p: &Palette) -> El {
    let query = s.search.text.trim().to_string();
    let found = s.search_results();
    let mut body = col().key("search-page-body").w_full().px(28.0).py(16.0).gap(22.0);
    if found.is_empty() {
        body = body.child(
            col().key("no-results").w_full().gap(6.0).child(text("No matches").font(HEADING).color(p.tx2)).child(text(format!("Nothing in the library matches “{query}”.")).font(BODY).color(p.tx3)),
        );
    }
    if let Some(info) = &found.artist {
        body = body.child(artists::head(s, info, &[], None, p)).children(artists::sections(s, info, &[], p));
    }
    if !found.artists.is_empty() {
        let mut list = row().gap(8.0).flex_wrap();
        for name in &found.artists {
            list = list.child(artists::artist_pill(name, true, p));
        }
        let title = if found.artist.is_some() { "Other artists" } else { "Artists" };
        body = body.child(artists::boxed(col().key("search-artists").w_full().gap(10.0).child(text(title).font(HEADING).color(p.tx)).child(list), p));
    }
    body = body.children(artists::release_strips(s, "search", &found.albums, true, &[], p).into_iter().map(|strip| artists::boxed(strip, p)));
    if !found.songs.is_empty() {
        body = body.child(artists::songs(s, "search-songs", "Songs", &found.songs, p));
    }
    col().key("search-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

//! The home page: the newest albums as one strip of cards, and the songs played last as a table.

use rune::{Cursor, El, ScrollPos, ScrollSpec, Tx, col, el, icon, row, text};

use super::albums::{CARD_GAP, small_card};
use super::table;
use super::*;
use crate::base::property::Property;
use crate::palette::Palette;

/// The song table's columns after the title, which don't follow the views' or the pages'.
const RECENT_COLUMNS: [Property; 3] = [Property::Artist, Property::Album, Property::LastPlayed];
/// Song rows' height, as the artist page's.
const SONG_ROW_H: f32 = 40.0;
/// The recently added cards' width.
const CARD_W: f32 = 200.0;
/// The recently added strip's viewport, which scrolls sideways.
const STRIP_KEY: &str = "recent-albums-strip";

pub(super) fn home_page(s: &App, p: &Palette) -> El {
    let body = col()
        .key("home-page-body")
        .w_full()
        .px(28.0)
        .py(16.0)
        .gap(28.0)
        .child(recent_albums(s, p))
        .child(recent_songs(s, p));
    col().key("home-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// Recently Added: the newest albums as one row of cards across the page, scrolling sideways
/// from the arrow buttons at either end.
fn recent_albums(s: &App, p: &Palette) -> El {
    let albums = s.recently_added();
    if albums.is_empty() {
        return section_empty("recent-albums", "Recently Added", "No albums in the library yet.", p);
    }
    let mut strip = row().gap(CARD_GAP).items_start();
    for &o in &albums {
        strip = strip.child(small_card(s, "recent-albums", o, true, CARD_W, p));
    }
    let (at, width) = (s.home_strip.clone(), s.home_viewport.clone());
    let viewport = col()
        .key(STRIP_KEY)
        .grow(1.0)
        .min_w(0.0)
        .scroll_x()
        .on_scroll_offset(move |pos: ScrollPos| {
            at.set([pos.offset[0], pos.max[0]]);
            width.set(pos.viewport[0]);
        })
        .child(strip);
    let nav = row()
        .w_full()
        .items_center()
        .gap(6.0)
        .child(arrow_button("recent-albums-prev", &icons::BACK, "Scroll back", p).on_press_tx(|s: &mut App, tx| page_strip(s, tx, -1.0)))
        .child(viewport)
        .child(arrow_button("recent-albums-next", &icons::FORWARD, "Scroll forward", p).on_press_tx(|s: &mut App, tx| page_strip(s, tx, 1.0)));
    col().key("recent-albums").w_full().gap(10.0).child(text("Recently Added").font(HEADING).color(p.tx)).child(nav)
}

/// Scrolls the Home strip by a page of whole cards: `dir` is -1 back, 1 forward, clamped to its ends.
fn page_strip(s: &mut App, tx: &mut Tx, dir: f32) {
    let [at, max] = s.home_strip.get();
    let viewport = s.home_viewport.get();
    let pitch = CARD_W + CARD_GAP;
    let cards = ((viewport + CARD_GAP) / pitch).floor().max(1.0);
    tx.scroll_to(STRIP_KEY, (at + dir * cards * pitch).clamp(0.0, max), 0.0);
}

/// A small round button with an icon, at the ends of the Home strip.
fn arrow_button(key: &'static str, glyph: &'static Icon, help: &str, p: &Palette) -> El {
    el()
        .key(key)
        .size(28.0, 28.0)
        .radius(14.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(help.to_string())
        .help(help.to_string())
        .child(icon(glyph, 16.0).color(p.tx2))
}

/// Recently Played: the songs played last, most recent at the top, as a table.
fn recent_songs(s: &App, p: &Palette) -> El {
    let list = s.recently_played();
    if list.is_empty() {
        return section_empty("recent-songs", "Recently Played", "Nothing played yet.", p);
    }
    let columns = table::Columns { shown: &RECENT_COLUMNS, widths: &s.base.page_widths, pills: &[], covers: false };
    let mut body = col().child(table::header("recent-songs-header", "", columns, 8.0, p));
    for &i in &list {
        body = body.child(song_line(s, i, &list, columns, p));
    }
    let table = table::scrolling("recent-songs-table".into(), columns, 8.0, body);
    col().key("recent-songs").w_full().gap(8.0).child(text("Recently Played").font(HEADING).color(p.tx)).child(table)
}

/// A section's title over a muted line saying why it's empty.
fn section_empty(key: &str, title: &str, line: &str, p: &Palette) -> El {
    col()
        .key(key.to_string())
        .w_full()
        .gap(8.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx))
        .child(text(line.to_string()).key(format!("{key}-empty")).font(BODY).color(p.tx3))
}

/// A song row: a click selects it, a double-click plays the list from it.
fn song_line(s: &App, i: usize, list: &[usize], columns: table::Columns, p: &Palette) -> El {
    let t = &s.tracks[i];
    let current = s.current == Some(i);
    let lead = if current {
        icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(p.blue)
    } else {
        el()
    };
    let look = table::RowLook { lead, cover: None, selected: s.selected == Some(i), current, height: SONG_ROW_H, px: 8.0, pills: Vec::new() };
    let list = list.to_vec();
    table::song_row(t, i, columns, look, p)
        .key(format!("recent-songs-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
}

//! The home page: the library's totals as a strip of tiles, the newest albums as one strip of
//! cards, the songs played last as a table, and the albums nobody has played as a grid.

use rune::{Cursor, El, ScrollPos, ScrollSpec, Tx, col, el, icon, row, text};

use super::albums::{CARD_GAP, small_card, song_cover};
use super::table;
use super::toolbar::segmented;
use super::*;
use crate::app::{SongMenuTarget, UnlistenedOrder};
use crate::query::property::Property;
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
        .child(stats_strip(s, p))
        .child(top_block(s, p))
        .child(recent_albums(s, p))
        .child(recent_songs(s, p))
        .child(unlistened_albums(s, p));
    col().key("home-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// Top artists, albums and songs over the chosen window, side by side; they wrap on narrow
/// widths. The window switch sits above them.
fn top_block(s: &App, p: &Palette) -> El {
    let choose = segmented("top-window", &crate::top::Window::ALL.map(|w| (w.id(), w.label())), s.top_window.id(), p, |s: &mut App, id: &str| {
        if let Some(window) = crate::top::Window::ALL.into_iter().find(|w| w.id() == id) {
            s.set_top_window(window);
        }
    });
    let head = row().w_full().items_center().gap(12.0).child(text("Top").font(HEADING).color(p.tx).grow(1.0).truncate()).child(choose);
    let top = s.top_model();
    if top.songs.is_empty() {
        let line = format!("No plays in the {}.", s.top_window.label().to_lowercase());
        return col().key("top").w_full().gap(10.0).child(head).child(text(line).key("top-empty").font(BODY).color(p.tx3));
    }
    let artists = top.artists.iter().enumerate().fold(col().key("top-artists").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let name = r.item.clone();
        c.child(top_row(format!("top-artist{n}"), &r.item, r.plays, p).role("link").cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx| s.show_artist(&name, tx)))
    });
    let albums = top.albums.iter().enumerate().fold(col().key("top-albums").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let (a, title) = (r.item, s.albums()[r.item].title.clone());
        c.child(top_row(format!("top-album{n}"), &title, r.plays, p).role("link").cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx| s.show_album(a, tx)))
    });
    let songs = top.songs.iter().enumerate().fold(col().key("top-songs").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let t = &s.tracks()[r.item];
        c.child(top_row(format!("top-song{n}"), &t.title, r.plays, p))
    });
    let columns = row().key("top-columns").w_full().gap(24.0).flex_wrap().child(artists).child(albums).child(songs);
    col().key("top").w_full().gap(10.0).child(head).child(columns)
}

/// One leaderboard row: its name and its plays.
fn top_row(key: String, name: &str, plays: u32, p: &Palette) -> El {
    row()
        .key(key)
        .w_full()
        .px(6.0)
        .py(3.0)
        .radius(4.0)
        .gap(8.0)
        .items_center()
        .hover(p.ui.alpha(0.3))
        .child(text(name.to_string()).font(BODY).color(p.tx).grow(1.0).truncate())
        .child(text(plays.to_string()).font(SMALL).color(p.tx3))
}

/// The stats strip: the library's totals as a row of tiles, each a figure over its label.
fn stats_strip(s: &App, p: &Palette) -> El {
    let t = s.totals();
    if t.songs == 0 {
        return section_empty("stats", "Library", "No songs in the library yet.", p);
    }
    let played = match t.played_share() {
        Some(share) => format!("{:.0}%", share * 100.0),
        None => "–".to_string(),
    };
    let tiles = [
        ("Songs", t.songs.to_string()),
        ("Albums", t.albums.to_string()),
        ("Artists", t.artists.to_string()),
        ("Hours", format!("{:.1}", t.hours())),
        ("Albums played", played),
    ];
    let mut strip = row().key("stats-tiles").w_full().gap(10.0);
    for (label, value) in tiles {
        strip = strip.child(stat_tile(label, &value, p));
    }
    col().key("stats").w_full().gap(10.0).child(text("Library").font(HEADING).color(p.tx)).child(strip)
}

/// A stats tile: its figure large, its label under it, on a faint panel.
fn stat_tile(label: &str, value: &str, p: &Palette) -> El {
    let slug = label.to_lowercase().replace(' ', "-");
    col()
        .key(format!("stat-tile-{slug}"))
        .grow(1.0)
        .min_w(0.0)
        .padding(12.0)
        .gap(2.0)
        .radius(6.0)
        .fill(p.ui.alpha(0.35))
        .child(text(value.to_string()).key(format!("stat-{slug}")).font(HEADING).color(p.tx).truncate())
        .child(text(label.to_string()).font(SMALL).color(p.tx3).truncate())
}

/// Recently Added: the newest albums as one row of cards across the page, scrolling sideways
/// from the arrow buttons at either end.
fn recent_albums(s: &App, p: &Palette) -> El {
    let albums = s.recent_albums();
    if albums.is_empty() {
        return section_empty("recent-albums", "Recently Added", "No albums in the library yet.", p);
    }
    let mut strip = row().gap(CARD_GAP).items_start();
    for &o in albums.iter() {
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

/// A small round icon button, at the ends of the Home strip or beside Unlistened Releases.
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
    let list = s.recent_songs();
    if list.is_empty() {
        return section_empty("recent-songs", "Recently Played", "Nothing played yet.", p);
    }
    let columns = table::Columns { shown: &RECENT_COLUMNS, widths: &s.views.page_widths, pills: &[], covers: true };
    let mut body = col().child(table::header("recent-songs-header", "", columns, 8.0, p));
    for (n, &i) in list.iter().enumerate() {
        body = body.child(song_line(s, i, &list, !n.is_multiple_of(2), columns, p));
    }
    let table = table::scrolling("recent-songs-table".into(), columns, 8.0, body, p);
    col().key("recent-songs").w_full().gap(8.0).child(text("Recently Played").font(HEADING).color(p.tx)).child(table)
}

/// Unlistened Releases: the albums with no plays, as a wrapping grid of cards, in the chosen
/// order. Random has a button to re-roll its selection.
fn unlistened_albums(s: &App, p: &Palette) -> El {
    let albums = s.unlistened_albums();
    let order = segmented("unlistened-order", &UnlistenedOrder::ALL.map(|o| (o.id(), o.label())), s.unlistened_order.id(), p, |s: &mut App, id: &str| {
        if let Some(order) = UnlistenedOrder::ALL.into_iter().find(|o| o.id() == id) {
            s.set_unlistened_order(order);
        }
    });
    let mut controls = row().gap(8.0).center_y().child(order);
    if s.unlistened_order == UnlistenedOrder::Random {
        controls = controls.child(arrow_button("unlistened-reroll", &icons::SHUFFLE, "Shuffle", p).on_press(|s: &mut App| s.reroll_unlistened()));
    }
    let head = row().w_full().items_center().gap(12.0).child(text("Unlistened Releases").font(HEADING).color(p.tx).grow(1.0).truncate()).child(controls);
    let body: El = if albums.is_empty() {
        text("Every album has been played.").key("unlistened-albums-empty").font(BODY).color(p.tx3)
    } else {
        let mut wall = row().key("unlistened-albums-wall").w_full().gap(CARD_GAP).flex_wrap();
        for &o in albums.iter() {
            wall = wall.child(small_card(s, "unlistened-albums", o, true, CARD_W, p));
        }
        wall
    };
    col().key("unlistened-albums").w_full().gap(10.0).child(head).child(body)
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

/// A song row: a click selects it, a double-click plays the list from it. Its album's cover
/// leads, with the playing or paused icon over it when it's the current song.
fn song_line(s: &App, i: usize, list: &[usize], stripe: bool, columns: table::Columns, p: &Palette) -> El {
    let t = &s.tracks()[i];
    let current = s.current == Some(i);
    let cover = song_cover(s, format!("recent-cover{i}"), i, table::COVER_W, p);
    let look = table::RowLook { lead: el(), cover: Some(cover), selected: s.selected == Some(i), current, stripe, height: SONG_ROW_H, px: 8.0, pills: Vec::new() };
    let list = list.to_vec();
    table::song_row(t, i, columns, look, &[], p)
        .key(format!("recent-songs-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Track(i), at)))
}

//! Album views: list cards with track tables, a grid of covers, and the album page.

use rune::{Align, Cursor, El, Image, ScrollSpec, SurfaceFit, Tx, col, el, icon, row, text};

use super::table;
use super::*;
use crate::album::Album;
use crate::base::property::Property;
use crate::credits::{self, Role};
use crate::app::{App, album_key, grid_key};
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;

/// Grid card cover edge range: columns fill the width with covers between these.
pub const CARD_MIN: f32 = 168.0;
const CARD_GAP: f32 = 18.0;
/// Card height below the cover: title, meta and artist lines.
const CARD_TEXT_H: f32 = 70.0;
const LIST_COVER: f32 = 132.0;
const TRACK_ROW_H: f32 = 34.0;
const PAGE_COVER: f32 = 232.0;

const ALBUM_TITLE: TextStyle = TextStyle::new(17.0, 650.0);
const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);

/// How many covers fit across `width` (the list's inner width).
pub fn columns_for(width: f32) -> usize {
    (((width + CARD_GAP) / (CARD_MIN + CARD_GAP)).floor() as usize).max(1)
}

/// Grid card width for `columns` across `width`.
fn card_w(width: f32, columns: usize) -> f32 {
    ((width - CARD_GAP * (columns as f32 - 1.0)) / columns as f32).floor()
}

/// The height a grid row takes, for the list's row estimate.
pub fn grid_row_h(width: f32, columns: usize) -> f32 {
    card_w(width, columns) + CARD_TEXT_H + CARD_GAP + 16.0
}

/// "2024 • 11 tracks • 32:08", skipping what isn't known.
fn meta(a: &Album, with_length: bool) -> String {
    let mut parts = Vec::new();
    if let Some(y) = a.year {
        parts.push(y.to_string());
    }
    parts.push(plural(a.tracks.len(), "track"));
    if with_length && a.duration > 0.0 {
        parts.push(clock(a.duration));
    }
    parts.join("  •  ")
}

/// The album's artist line ("Armand Hammer & The Alchemist"), each act's name opening its page.
fn album_artist(a: &Album, color: rune::Color) -> El {
    let line = a.artist_name().to_string();
    let links = if a.artist.is_some() { name_links(&line, &a.album_artists) } else { Vec::new() };
    linked(line, links, color)
}

pub(super) fn cover_image(s: &App, a: &Album) -> Option<Image> {
    let first = *a.tracks.first()?;
    s.covers.as_ref()?.get(&s.tracks[first].path).filter(|i| i.error().is_none())
}

/// A square cover, or a placeholder with a note while it loads or when there is none.
pub(super) fn cover(key: String, image: Option<Image>, size: Option<f32>, radius: f32, p: &Palette) -> El {
    let base = el().key(key).radius(radius).fill(p.ui).clip_rounded(radius);
    let base = match size {
        Some(px) => base.size(px, px),
        None => base.w_full().aspect_ratio(1.0),
    };
    match image {
        Some(image) => base.pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => {
            let glyph = size.map_or(36.0, |px| (px * 0.28).clamp(16.0, 56.0));
            base.center().child(icon(&icons::NOTE, glyph).color(p.ui2))
        }
    }
}

// Grid

/// A row of grid cards; `width` is the list's inner width.
pub(super) fn grid_row(s: &App, start: usize, section: Option<usize>, width: f32, p: &Palette) -> El {
    let columns = s.columns.max(1);
    let w = card_w(width, columns);
    let mut r = row().key(grid_key(start, section)).w_full().gap(CARD_GAP).pb(CARD_GAP).items_start();
    for &a in s.grid_albums(start, section) {
        r = r.child(grid_card(s, a, section, w, p));
    }
    r
}

fn grid_card(s: &App, a: usize, section: Option<usize>, w: f32, p: &Palette) -> El {
    let album = &s.albums[a];
    let playing = s.current.is_some_and(|c| album.tracks.contains(&c));
    let play = el()
        .key("play")
        .size(44.0, 44.0)
        .radius(22.0)
        .fill(p.blue)
        .center()
        .pin(None, None, Some(10.0), Some(10.0))
        .group_opacity(if playing { 1.0 } else { 0.0 }, 1.0)
        .hover_alone()
        .press_scale(0.92)
        .cursor(Cursor::Pointer)
        .label(format!("Play {}", album.title))
        .on_press_tx(move |s: &mut App, tx| s.play_album(a, None, tx))
        .child(icon(if playing && s.playing { &icons::BARS } else { &icons::PLAY }, 18.0).color(rune::Color::WHITE));
    let art = cover(format!("cover{a}"), cover_image(s, album), None, 8.0, p).child(play);
    let title_color = if playing { p.blue } else { p.tx };
    col()
        .key(album_key(a, section))
        .w(w)
        .gap(4.0)
        .padding(8.0)
        .radius(12.0)
        .hover(p.ui.alpha(0.3))
        .hover_group()
        .press_scale(0.98)
        .cursor(Cursor::Pointer)
        .role("link")
        .label(format!("{} by {}", album.title, album.artist_name()))
        .on_press_tx(move |s: &mut App, tx| s.show_album(a, tx))
        .child(art)
        .child(el().h(6.0))
        .child(text(album.title.clone()).font(TITLE).color(title_color).truncate())
        .child(text(meta(album, false)).font(SMALL).color(p.tx3).truncate())
        .child(album_artist(album, p.tx2).key("artist").font(SMALL).truncate())
}

// List

/// An album card: header with cover, then its track table.
pub(super) fn list_card(s: &App, a: usize, section: Option<usize>, p: &Palette) -> El {
    let album = &s.albums[a];
    let open = move |s: &mut App, tx: &mut Tx| s.show_album(a, tx);
    let art = cover(format!("cover{a}"), cover_image(s, album), Some(LIST_COVER), 8.0, p).cursor(Cursor::Pointer).on_press_tx(open);
    let head = col()
        .gap(4.0)
        .grow(1.0)
        .shrink()
        .child(album_artist(album, p.tx2).key("artist").font(BODY).truncate())
        .child(text(album.title.clone()).key("title").font(ALBUM_TITLE).color(p.tx).truncate().cursor(Cursor::Pointer).on_press_tx(open))
        .child(text(meta(album, true)).font(BODY).color(p.tx3))
        .child(el().h(10.0))
        .child(row().gap(8.0).child(play_button("play-album", "Play album", p).on_press_tx(move |s: &mut App, tx| s.play_album(a, None, tx))));
    col()
        .key(album_key(a, section))
        .w_full()
        .mb(14.0)
        .padding(16.0)
        .radius(12.0)
        .fill(p.bg2)
        .gap(14.0)
        .child(row().gap(18.0).items_start().child(art).child(head))
        .child(track_table(s, a, &s.base.active().columns, p))
}

/// The album's songs as a table with the page columns. Double-click plays the album from
/// that song. Album cards in list views use the view's columns instead.
fn track_table(s: &App, a: usize, columns: &[Property], p: &Palette) -> El {
    let album = &s.albums[a];
    let discs = album.discs(&s.tracks);
    let mut rows = col().w_full();
    let mut disc = None;
    for (n, &i) in album.tracks.iter().enumerate() {
        let t = &s.tracks[i];
        if discs.len() > 1 && t.disc != disc {
            disc = t.disc;
            let label = disc.map_or("No disc".to_string(), |d| format!("Disc {d}"));
            rows = rows.child(row().key(format!("disc{}", disc.unwrap_or(0))).h(30.0).px(8.0).center_y().child(text(label).font(GROUP).color(p.tx2)));
        }
        rows = rows.child(track_line(s, t, i, n, a, columns, p));
    }
    let body = col()
        .child(table::header("tracks-header", "#", columns, 8.0, p))
        .child(el().w_full().h(1.0).fill(p.ui.alpha(0.7)))
        .child(rows);
    table::scrolling("tracks".into(), columns, 8.0, body)
}

fn track_line(s: &App, t: &Track, i: usize, n: usize, a: usize, columns: &[Property], p: &Palette) -> El {
    let current = s.current == Some(i);
    let lead: El = if current {
        icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(p.blue)
    } else {
        text(t.number.map_or_else(|| format!("{}", n + 1), |x| x.to_string())).font(NUM).color(p.tx3).w(28.0).align(Align::End)
    };
    let look = table::RowLook { lead, selected: s.selected == Some(i), current, height: TRACK_ROW_H, px: 8.0, note: None };
    table::song_row(t, i, columns, look, p)
        .key(format!("t{i}"))
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or(s.albums[a].artist_name())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_album(a, Some(i), tx))
}

pub(super) fn play_button(key: &'static str, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(14.0)
        .gap(8.0)
        .radius(16.0)
        .center_y()
        .fill(p.blue)
        .hover(rune::Color::WHITE.alpha(0.12))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .label(label)
        .child(icon(&icons::PLAY, 14.0).color(rune::Color::WHITE))
        .child(text(label).font(TITLE).color(rune::Color::WHITE))
}

fn secondary_button(key: &'static str, glyph: &'static rune::Icon, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(14.0)
        .gap(8.0)
        .radius(16.0)
        .center_y()
        .fill(p.ui.alpha(0.6))
        .hover(p.ui.alpha(0.4))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .label(label)
        .child(icon(glyph, 15.0).color(p.tx))
        .child(text(label).font(TITLE).color(p.tx))
}

// Album page

pub(super) fn album_page(s: &App, a: usize, p: &Palette) -> El {
    let album = &s.albums[a];
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
    let mut facts = col().gap(4.0);
    if !album.genres.is_empty() {
        facts = facts.child(fact("Genre", text(album.genres.join(", ")), p));
    }
    if let Some(label) = &album.label {
        facts = facts.child(fact("Label", text(label.clone()), p));
    }
    if !album.formats.is_empty() {
        facts = facts.child(fact("Format", text(album.formats.iter().map(|f| f.to_uppercase()).collect::<Vec<_>>().join(", ")), p));
    }
    let featured: Vec<(String, Vec<String>)> = album.featured.iter().map(|f| (f.clone(), Vec::new())).collect();
    if !featured.is_empty() {
        facts = facts.child(fact("Featuring", name_list(&featured, p.tx2).key("fact-featuring"), p));
    }
    for role in Role::ALL {
        let people = credits::people(&album.credits, role);
        if !people.is_empty() {
            facts = facts.child(fact(role.label(), name_list(&people, p.tx2).key(format!("fact-{}", role.label().to_lowercase())), p));
        }
    }
    let head = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Album").font(SMALL).color(p.tx3))
        .child(text(album.title.clone()).key("page-title").font(PAGE_TITLE).color(p.tx).wrap())
        .child(album_artist(album, p.tx2).key("page-artist").font(HEADING))
        .child(text(meta(album, true)).font(BODY).color(p.tx3))
        .child(el().h(6.0))
        .child(facts)
        .child(el().h(12.0))
        .child(
            row()
                .gap(8.0)
                .child(play_button("page-play", "Play", p).on_press_tx(move |s: &mut App, tx| s.play_album(a, None, tx)))
                .child(secondary_button("page-shuffle", &icons::SHUFFLE, "Shuffle", p).on_press_tx(move |s: &mut App, tx| s.shuffle_album(a, tx))),
        );
    let top = row().gap(28.0).items_start().child(cover("page-cover".into(), cover_image(s, album), Some(PAGE_COVER), 10.0, p)).child(head);
    let mut page = col()
        .key("album-page-body")
        .w_full()
        .px(28.0)
        .py(16.0)
        .gap(22.0)
        .child(back)
        .child(top)
        .child(track_table(s, a, &s.base.page_columns, p));
    let more = more_by(s, a, p);
    if let Some(more) = more {
        page = page.child(more);
    }
    col()
        .key("album-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(page)
}

/// What Back returns to: the previous page's name, else the view's.
pub(super) fn back_label(s: &App) -> String {
    match s.history.last() {
        Some(crate::app::Page::Artist(name)) => name.clone(),
        Some(crate::app::Page::Album(id)) => s.albums.iter().find(|a| &a.id() == id).map_or_else(|| "Back".to_string(), |a| a.title.clone()),
        Some(crate::app::Page::Library) | None => s.base.active().name.clone(),
    }
}

fn fact(label: &str, value: El, p: &Palette) -> El {
    row().gap(10.0).items_start().child(text(label.to_string()).font(SMALL).color(p.tx3).w(64.0)).child(value.font(SMALL).color(p.tx2).wrap().grow(1.0).shrink())
}

/// "More by {artist}": the artist's other albums, newest first, as a scrolling row of covers.
fn more_by(s: &App, a: usize, p: &Palette) -> Option<El> {
    let album = &s.albums[a];
    let artist = album.artist.as_deref()?.to_lowercase();
    let mut others: Vec<usize> = (0..s.albums.len())
        .filter(|&o| o != a && s.albums[o].artist.as_deref().is_some_and(|x| x.to_lowercase() == artist))
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by_key(|&o| std::cmp::Reverse(s.albums[o].year));
    Some(album_strip(s, "more", &format!("More by {}", album.artist_name()), &others, false, p))
}

/// A titled, sideways-scrolling row of album covers keyed `{key}`, `{key}{album}` per card.
/// With `artists`, each card names its artists too.
pub(super) fn album_strip(s: &App, key: &str, title: &str, albums: &[usize], artists: bool, p: &Palette) -> El {
    let mut strip = row().gap(CARD_GAP).items_start();
    for &o in albums {
        let other = &s.albums[o];
        let mut card = col()
            .key(format!("{key}{o}"))
            .w(150.0)
            .gap(4.0)
            .padding(6.0)
            .radius(10.0)
            .hover(p.ui.alpha(0.3))
            .cursor(Cursor::Pointer)
            .role("link")
            .label(other.title.clone())
            .on_press_tx(move |s: &mut App, tx| s.show_album(o, tx))
            .child(cover(format!("{key}-cover{o}"), cover_image(s, other), Some(138.0), 8.0, p))
            .child(text(other.title.clone()).font(TITLE).color(p.tx).truncate());
        if artists {
            card = card.child(text(other.artist_name().to_string()).font(SMALL).color(p.tx2).truncate());
        }
        strip = strip.child(card.child(text(other.year.map(|y| y.to_string()).unwrap_or_default()).font(SMALL).color(p.tx3)));
    }
    let section = if key == "more" { "more-by".to_string() } else { key.to_string() };
    col()
        .key(section)
        .gap(10.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx))
        .child(col().key(format!("{key}-strip")).w_full().scroll_x().child(strip))
}

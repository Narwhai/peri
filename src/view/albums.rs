//! Album views: list cards with track tables, a grid of covers, and the album page.

use rune::{Align, Cursor, El, Image, ScrollSpec, SurfaceFit, Tx, col, el, icon, row, text};

use super::*;
use crate::album::Album;
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

fn cover_image(s: &App, a: &Album) -> Option<Image> {
    let first = *a.tracks.first()?;
    s.covers.as_ref()?.get(&s.tracks[first].path).filter(|i| i.error().is_none())
}

/// A square cover, or a placeholder with a note while it loads or when there is none.
fn cover(key: String, image: Option<Image>, size: Option<f32>, radius: f32, p: &Palette) -> El {
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
        .child(text(album.artist_name().to_string()).font(SMALL).color(p.tx2).truncate())
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
        .child(text(album.artist_name().to_string()).font(BODY).color(p.tx2).truncate())
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
        .child(track_table(s, a, p))
}

/// The album's songs: #, title (with the song's artist when it isn't the album's), format and
/// length. Double-click plays the album from that song.
fn track_table(s: &App, a: usize, p: &Palette) -> El {
    let album = &s.albums[a];
    let discs = album.discs(&s.tracks);
    let mut table = col().key("tracks").w_full();
    table = table.child(
        row()
            .h(28.0)
            .px(8.0)
            .gap(12.0)
            .center_y()
            .child(text("#").font(SMALL).color(p.tx3).w(28.0).align(Align::End))
            .child(text("Title").font(SMALL).color(p.tx3).grow(1.0))
            .child(text("Format").font(SMALL).color(p.tx3).w(56.0))
            .child(text("Length").font(SMALL).color(p.tx3).w(48.0).align(Align::End)),
    );
    table = table.child(el().w_full().h(1.0).fill(p.ui.alpha(0.7)));
    let mut disc = None;
    for (n, &i) in album.tracks.iter().enumerate() {
        let t = &s.tracks[i];
        if discs.len() > 1 && t.disc != disc {
            disc = t.disc;
            let label = disc.map_or("No disc".to_string(), |d| format!("Disc {d}"));
            table = table.child(row().key(format!("disc{}", disc.unwrap_or(0))).h(30.0).px(8.0).center_y().child(text(label).font(GROUP).color(p.tx2)));
        }
        table = table.child(track_line(s, album, t, i, n, a, p));
    }
    table
}

fn track_line(s: &App, album: &Album, t: &Track, i: usize, n: usize, a: usize, p: &Palette) -> El {
    let current = s.current == Some(i);
    let number: El = if current {
        el().w(28.0).center().child(icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(p.blue))
    } else {
        text(t.number.map_or_else(|| format!("{}", n + 1), |x| x.to_string())).font(NUM).color(p.tx3).w(28.0).align(Align::End)
    };
    let artist = t.artist().filter(|ar| album.artist.as_deref().is_none_or(|aa| !aa.eq_ignore_ascii_case(ar)));
    let mut title = row().grow(1.0).shrink().gap(8.0).center_y().child(text(t.title.clone()).font(BODY).color(if current { p.blue } else { p.tx }).truncate().shrink());
    if let Some(artist) = artist {
        title = title.child(text(artist).font(BODY).color(p.tx3).truncate().shrink());
    }
    let format = t.path.extension().and_then(|e| e.to_str()).map(str::to_uppercase).unwrap_or_default();
    let selected = s.selected == Some(i);
    row()
        .key(format!("t{i}"))
        .w_full()
        .h(TRACK_ROW_H)
        .px(8.0)
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.ui.alpha(0.55) } else { p.bg.alpha(0.0) })
        .hover(p.ui.alpha(0.3))
        .role("row")
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or(album.artist_name())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_album(a, Some(i), tx))
        .child(number)
        .child(title)
        .child(text(format).font(SMALL).color(p.tx3).w(56.0))
        .child(text(t.duration.map(clock).unwrap_or_default()).font(NUM).color(p.tx3).w(48.0).align(Align::End))
}

fn play_button(key: &'static str, label: &'static str, p: &Palette) -> El {
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
        .on_press(|s: &mut App| s.show_library())
        .child(icon(&icons::BACK, 16.0).color(p.tx2))
        .child(text(s.base.active().name.clone()).font(TITLE).color(p.tx2));
    let mut facts = col().gap(4.0);
    if !album.genres.is_empty() {
        facts = facts.child(fact("Genre", album.genres.join(", "), p));
    }
    if let Some(label) = &album.label {
        facts = facts.child(fact("Label", label.clone(), p));
    }
    if !album.formats.is_empty() {
        facts = facts.child(fact("Format", album.formats.iter().map(|f| f.to_uppercase()).collect::<Vec<_>>().join(", "), p));
    }
    let head = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Album").font(SMALL).color(p.tx3))
        .child(text(album.title.clone()).key("page-title").font(PAGE_TITLE).color(p.tx).wrap())
        .child(text(album.artist_name().to_string()).key("page-artist").font(HEADING).color(p.tx2))
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
        .child(track_table(s, a, p));
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

fn fact(label: &str, value: String, p: &Palette) -> El {
    row().gap(10.0).child(text(label.to_string()).font(SMALL).color(p.tx3).w(52.0)).child(text(value).font(SMALL).color(p.tx2).truncate().shrink())
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
    let mut strip = row().gap(CARD_GAP);
    for o in others {
        let other = &s.albums[o];
        strip = strip.child(
            col()
                .key(format!("more{o}"))
                .w(150.0)
                .gap(4.0)
                .padding(6.0)
                .radius(10.0)
                .hover(p.ui.alpha(0.3))
                .cursor(Cursor::Pointer)
                .role("link")
                .label(other.title.clone())
                .on_press_tx(move |s: &mut App, tx| s.show_album(o, tx))
                .child(cover(format!("more-cover{o}"), cover_image(s, other), Some(138.0), 8.0, p))
                .child(text(other.title.clone()).font(TITLE).color(p.tx).truncate())
                .child(text(other.year.map(|y| y.to_string()).unwrap_or_default()).font(SMALL).color(p.tx3)),
        );
    }
    Some(
        col()
            .key("more-by")
            .gap(10.0)
            .child(text(format!("More by {}", album.artist_name())).font(HEADING).color(p.tx))
            .child(col().key("more-strip").w_full().scroll_x().child(strip)),
    )
}

//! Album views: list cards with track tables, a grid of covers, and the album page.

use rune::{Align, Cursor, El, Image, ScrollSpec, Shadow, SurfaceFit, Tx, col, el, icon, row, text};

use super::table;
use super::*;
use crate::album::{Album, ReleaseType};
use crate::credits::{self, Role};
use crate::app::{App, SongMenuTarget, album_key, grid_key};
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;
use std::collections::HashSet;
use crate::query::property::Property;
use crate::query::view::Widths;

pub(super) const CARD_GAP: f32 = 18.0;
/// Card height below the cover: title, meta and artist lines.
const CARD_TEXT_H: f32 = 70.0;
const LIST_COVER: f32 = 132.0;
const TRACK_ROW_H: f32 = 34.0;
const PAGE_COVER: f32 = 232.0;
/// The corners of the album page's Play and Shuffle.
const PAGE_BUTTON_RADIUS: f32 = 4.0;
/// The album page's genre pills.
const PILL_H: f32 = 20.0;
const PILL_PX: f32 = 8.0;
const PILL_GAP: f32 = 6.0;

const ALBUM_TITLE: TextStyle = TextStyle::new(17.0, 650.0);
const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);
/// A small button's label.
const SMALL_TITLE: TextStyle = TextStyle::new(11.5, 500.0);

/// How many cards `size` wide fit across `width` (the list's inner width).
pub fn columns_for(width: f32, size: f32) -> usize {
    (((width + CARD_GAP) / (size + CARD_GAP)).floor() as usize).max(1)
}

/// Grid card width: the view's card size, narrower only when the list is.
fn card_w(width: f32, size: f32) -> f32 {
    size.min(width).floor()
}

/// The height a grid row takes, for the list's row estimate.
pub fn grid_row_h(width: f32, size: f32) -> f32 {
    card_w(width, size) + CARD_TEXT_H + CARD_GAP + 16.0
}

/// "EP • 2024 • 4 tracks • 12:08", skipping what isn't known.
fn meta(a: &Album, with_length: bool) -> String {
    let mut parts = vec![a.release_type.label().to_string()];
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
    let links = if a.artist.is_some() { artist_links(&line, &a.album_artists) } else { Vec::new() };
    linked(line, links, color)
}

pub(super) fn cover_image(s: &App, a: &Album) -> Option<Image> {
    let first = *a.tracks.first()?;
    s.covers.as_ref()?.get(&s.tracks()[first].path).filter(|i| i.error().is_none())
}

/// A square cover, or a placeholder with a note while it loads or when there is none.
pub(super) fn cover(key: String, image: Option<Image>, size: Option<f32>, p: &Palette) -> El {
    let base = el().key(key).fill(p.ui);
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

/// Song `i`'s album cover at `size`, dimmed under the playing or paused icon when it's the
/// current song.
pub(super) fn song_cover(s: &App, key: String, i: usize, size: f32, p: &Palette) -> El {
    let image = s.album_of(i).and_then(|a| cover_image(s, &s.albums()[a]));
    let art = cover(key, image, Some(size), p);
    if s.current != Some(i) {
        return art;
    }
    art.child(
        el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(rune::Color::BLACK.alpha(0.55)).center()
            .child(icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(rune::Color::WHITE)),
    )
}

/// A row of grid cards; `width` is the list's inner width.
pub(super) fn grid_row(s: &App, start: usize, section: Option<usize>, width: f32, p: &Palette) -> El {
    let columns = s.columns.max(1);
    // Narrower only while the column count catches up with a resize.
    let fit = ((width - CARD_GAP * (columns as f32 - 1.0)) / columns as f32).floor();
    let w = card_w(width, s.card_size()).min(fit);
    // Cards keep their size; what's left of the width spreads between them.
    let gap = if columns > 1 { ((width - w * columns as f32) / (columns as f32 - 1.0)).max(CARD_GAP) } else { CARD_GAP };
    let mut r = row().key(grid_key(start, section)).w_full().gap(gap).pb(CARD_GAP).items_start();
    for &a in s.grid_albums(start, section) {
        r = r.child(grid_card(s, a, section, w, p));
    }
    r
}

fn grid_card(s: &App, a: usize, section: Option<usize>, w: f32, p: &Palette) -> El {
    let album = &s.albums()[a];
    let playing = s.current.is_some_and(|c| album.tracks.contains(&c));
    let play = el()
        .key("play")
        .size(44.0, 44.0)
        .radius(22.0)
        .fill(p.accent)
        .center()
        .pin(None, None, Some(10.0), Some(10.0))
        .group_opacity(if playing { 1.0 } else { 0.0 }, 1.0)
        // No `hover_alone`: it would rest the card's group while the pointer is on the button,
        // fading the button out from under it.
        .hover(rune::Color::WHITE.alpha(0.12))
        .press_scale(0.92)
        .cursor(Cursor::Pointer)
        .label(format!("Play {}", album.title))
        .on_press_tx(move |s: &mut App, tx| s.play_album(a, None, tx))
        .child(icon(if playing && s.playing { &icons::BARS } else { &icons::PLAY }, 18.0).color(p.on_accent));
    let art = cover(format!("cover{a}"), cover_image(s, album), None, p).child(play);
    let title_color = if playing { p.accent } else { p.tx };
    col()
        .key(album_key(a, section))
        .w(w)
        .gap(4.0)
        .padding(8.0)
        .radius(2.0)
        .hover(p.ui.alpha(0.3))
        .hover_group()
        .press_scale(0.98)
        .cursor(Cursor::Pointer)
        .role("link")
        .label(format!("{} by {}", album.title, album.artist_name()))
        .on_press_tx(move |s: &mut App, tx| s.show_album(a, tx))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Album(a), at)))
        .child(art)
        .child(el().h(6.0))
        .child(marked(text(album.title.clone()), &album.title, &s.search_words, p).font(TITLE).color(title_color).truncate())
        .child(text(meta(album, false)).font(SMALL).color(p.tx3).truncate())
        .child(marked(album_artist(album, p.tx2), album.artist_name(), &s.search_words, p).key("artist").font(SMALL).truncate())
}

// List

/// An album card: header with cover, then its track table.
pub(super) fn list_card(s: &App, a: usize, section: Option<usize>, p: &Palette) -> El {
    let album = &s.albums()[a];
    let open = move |s: &mut App, tx: &mut Tx| s.show_album(a, tx);
    let art = cover(format!("cover{a}"), cover_image(s, album), Some(LIST_COVER), p).cursor(Cursor::Pointer).on_press_tx(open);
    let head = col()
        .gap(4.0)
        .grow(1.0)
        .shrink()
        .child(marked(album_artist(album, p.tx2), album.artist_name(), &s.search_words, p).key("artist").font(BODY).truncate())
        .child(marked(text(album.title.clone()), &album.title, &s.search_words, p).key("title").font(ALBUM_TITLE).color(p.tx).truncate().cursor(Cursor::Pointer).on_press_tx(open))
        .child(text(meta(album, true)).font(BODY).color(p.tx3))
        .child(el().h(10.0))
        .child(row().gap(8.0).child(play_button("play-album", "Play album", p).on_press_tx(move |s: &mut App, tx| s.play_album(a, None, tx))));
    let card = col()
        .key(album_key(a, section))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Album(a), at)))
        .w_full()
        .padding(16.0)
        .radius(2.0)
        .fill(p.bg2)
        .gap(14.0)
        .child(row().gap(18.0).items_start().child(art).child(head))
        .child(track_table(s, a, &format!("album{a}"), &s.views.active().columns, &s.views.active().widths, s.view_matches.as_ref(), &s.search_words, p));
    // Padding, not margin: virtual rows size each row by its box, so the gap sits inside it.
    col().w_full().pb(16.0).child(card)
}

/// The album's songs as a table with the page columns, its columns sized to its songs (`table`
/// keys what they measure to). Double-click plays the album from that song. Album cards in
/// list views use the view's columns instead, and list only the songs in `matches` when it's
/// given, with `words` highlighted.
#[allow(clippy::too_many_arguments)]
fn track_table(s: &App, a: usize, table: &str, shown: &[Property], widths: &Widths, matches: Option<&HashSet<usize>>, words: &[String], p: &Palette) -> El {
    let album = &s.albums()[a];
    let keep = |i: usize| matches.is_none_or(|m| m.contains(&i));
    let listed: Vec<usize> = album.tracks.iter().copied().filter(|&i| keep(i)).collect();
    let fits = s.column_fits(table, &listed, shown);
    let columns = table::Columns { shown, widths, fits: &fits, pills: &[], covers: false };
    let discs = album.discs(s.tracks(), keep);
    let mut rows = col().w_full();
    let mut disc = None;
    for (n, &i) in album.tracks.iter().enumerate().filter(|&(_, &i)| keep(i)) {
        let t = &s.tracks()[i];
        if discs.len() > 1 && t.disc != disc {
            disc = t.disc;
            let label = disc.map_or("No disc".to_string(), |d| format!("Disc {d}"));
            rows = rows.child(row().key(format!("disc{}", disc.unwrap_or(0))).h(30.0).px(8.0).center_y().child(text(label).font(GROUP).color(p.tx2)));
        }
        rows = rows.child(track_line(s, t, i, n, a, columns, words, p));
    }
    let body = col()
        .child(table::header("tracks-header", "#", columns, 8.0, p))
        .child(el().w_full().h(1.0).fill(p.ui.alpha(0.7)))
        .child(rows);
    table::scrolling("tracks".into(), columns, 8.0, body, p)
}

#[allow(clippy::too_many_arguments)]
fn track_line(s: &App, t: &Track, i: usize, n: usize, a: usize, columns: table::Columns, words: &[String], p: &Palette) -> El {
    let current = s.current == Some(i);
    let lead: El = if current {
        icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(p.accent)
    } else {
        text(t.number.map_or_else(|| format!("{}", n + 1), |x| x.to_string())).font(NUM).color(p.tx3).w(28.0).align(Align::End)
    };
    let look = table::RowLook { lead, cover: None, selected: s.selected == Some(i), current, stripe: !n.is_multiple_of(2), height: TRACK_ROW_H, px: 8.0, pills: Vec::new() };
    table::song_row(t, i, columns, look, words, p)
        .key(format!("t{i}"))
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or(s.albums()[a].artist_name())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_album(a, Some(i), tx))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Track(i), at)))
}

pub(super) fn play_button(key: &'static str, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(14.0)
        .gap(8.0)
        .radius(16.0)
        .center_y()
        .fill(p.accent)
        .hover(rune::Color::WHITE.alpha(0.12))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .label(label)
        .child(icon(&icons::PLAY, 14.0).color(p.on_accent))
        .child(text(label).font(TITLE).color(p.on_accent))
}

/// A playback button beside Play: the neutral secondary fill with Play's blue for its ink.
pub(super) fn tinted_button(key: &'static str, glyph: &'static rune::Icon, label: &'static str, p: &Palette) -> El {
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
        .child(icon(glyph, 15.0).color(p.accent))
        .child(text(label).font(TITLE).color(p.accent))
}

/// The album page's Play: raised while album `a` isn't playing, pressed in and darker as
/// Pause while it is. Pressing it pauses or resumes the album when it's the one loaded,
/// else plays it from the start.
fn play_toggle(s: &App, a: usize, p: &Palette) -> El {
    let loaded = s.current.is_some_and(|i| s.album_of(i) == Some(a));
    let down = loaded && s.playing;
    let (glyph, label) = if down { (&icons::PAUSE, "Pause") } else { (&icons::PLAY, "Play") };
    let shadow = if down {
        Shadow { y: 2.0, blur: 5.0, color: rune::Color::BLACK.alpha(0.45), inset: true, ..Shadow::default() }
    } else {
        Shadow { y: 1.0, blur: 2.0, color: rune::Color::BLACK.alpha(0.25), ..Shadow::default() }
    };
    // Wide enough for either label, so turning into Pause doesn't resize it: the icon and
    // label recentre in the same box.
    let widest = ["Play", "Pause"].into_iter().map(|l| rune::text::measure(l, &[], TITLE, None).0).fold(0.0, f32::max);
    row()
        .key("page-play")
        .h(32.0)
        .min_w(widest + 14.0 + 8.0 + 2.0 * 14.0)
        .px(14.0)
        .gap(8.0)
        .radius(PAGE_BUTTON_RADIUS)
        .center_y()
        .justify_center()
        .fill(if down { p.accent.mix(rune::Color::BLACK, 0.35) } else { p.accent })
        .box_shadow(shadow)
        .hover(rune::Color::WHITE.alpha(0.12))
        .press(rune::Color::BLACK.alpha(0.12))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(label)
        .on_press_tx(move |s: &mut App, tx| if loaded { s.toggle(tx) } else { s.play_album(a, None, tx) })
        .child(icon(glyph, 14.0).color(p.on_accent))
        .child(text(label).font(TITLE).color(p.on_accent))
}

/// [`tinted_button`] smaller, for a lesser action under a full-size one.
fn small_tinted_button(key: &'static str, glyph: &'static rune::Icon, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(24.0)
        .px(10.0)
        .gap(6.0)
        .radius(PAGE_BUTTON_RADIUS)
        .center_y()
        .fill(p.ui.alpha(0.6))
        .hover(p.ui.alpha(0.4))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .label(label)
        .child(icon(glyph, 12.0).color(p.accent))
        .child(text(label).font(SMALL_TITLE).color(p.accent))
}

pub(super) fn secondary_button(key: &'static str, glyph: &'static rune::Icon, label: &'static str, p: &Palette) -> El {
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
    let album = &s.albums()[a];
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
    let mut facts = col().key("facts").gap(4.0).grow(1.0).shrink();
    let featured: Vec<(String, Vec<String>)> = album.featured.iter().map(|f| (f.clone(), Vec::new())).collect();
    let mut has_credits = !featured.is_empty();
    if !featured.is_empty() {
        facts = facts.child(fact("Featuring", name_list(&featured, p.tx2).key("fact-featuring"), p));
    }
    for role in Role::ALL {
        let people = credits::people(&album.credits, role);
        if !people.is_empty() {
            has_credits = true;
            facts = facts.child(fact(role.label(), name_list(&people, p.tx2).key(format!("fact-{}", role.label().to_lowercase())), p));
        }
    }
    let buttons = col()
        .key("page-buttons")
        .gap(8.0)
        .self_start()
        .items_stretch()
        .child(play_toggle(s, a, p))
        .child(small_tinted_button("page-shuffle", &icons::SHUFFLE, "Shuffle", p).justify_center().on_press_tx(move |s: &mut App, tx| s.shuffle_album(a, tx)));
    // The buttons, then a rule as tall as the credits beside it, then the credits.
    let mut actions = row().key("page-actions").gap(20.0).items_stretch().child(buttons);
    if has_credits {
        actions = actions.child(el().key("page-divider").w(1.0).fill(p.ui.alpha(0.9))).child(facts);
    }
    let head = col()
        .key("page-head")
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text(album.title.clone()).key("page-title").font(PAGE_TITLE).color(p.tx).wrap())
        .child(album_artist(album, p.tx2).key("page-artist").font(HEADING))
        .child(text(meta(album, true)).key("page-meta").font(BODY).color(p.tx3))
        .children((!album.genres.is_empty()).then(|| genre_pills(&album.genres, p)))
        .child(el().h(6.0))
        .child(actions);
    let top = row().gap(28.0).items_start().child(cover("page-cover".into(), cover_image(s, album), Some(PAGE_COVER), p)).child(head);
    let mut page = col()
        .key("album-page-body")
        .w_full()
        .px(28.0)
        .py(16.0)
        .gap(22.0)
        .child(back)
        .child(top)
        .child(track_table(s, a, "album-page", &s.views.page_columns, &s.views.page_widths, None, &[], p))
        .children(album.label.as_deref().map(|label| release_line(album.year, label, p)));
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
        Some(crate::app::Page::Artist(name) | crate::app::Page::Label(name)) => name.clone(),
        Some(crate::app::Page::Collab(names)) => crate::collab::title(names),
        Some(crate::app::Page::Album(id)) => s.albums().iter().find(|a| &a.id() == id).map_or_else(|| "Back".to_string(), |a| a.title.clone()),
        Some(crate::app::Page::Search) => format!("Results for “{}”", s.search.text.trim()),
        Some(crate::app::Page::Settings) => "Settings".to_string(),
        Some(crate::app::Page::Graph) => "Graph".to_string(),
        Some(crate::app::Page::Home) => "Home".to_string(),
        Some(crate::app::Page::Library) | None => s.views.active().name.clone(),
    }
}

/// The album's genres as pills under its meta line.
// TODO: open a page of the genre's songs on press.
fn genre_pills(genres: &[String], p: &Palette) -> El {
    let pill = |(n, genre): (usize, &String)| {
        row().key(format!("page-genre{n}")).h(PILL_H).px(PILL_PX).radius(PILL_H / 2.0).center_y().fill(p.ui.alpha(0.6)).child(text(genre.clone()).font(SMALL).color(p.tx2))
    };
    row().key("page-genres").w_full().gap(PILL_GAP).flex_wrap().children(genres.iter().enumerate().map(pill))
}

/// "℗ 2023 ADOR" under the track list, the label opening its page.
fn release_line(year: Option<u32>, label: &str, p: &Palette) -> El {
    let lead = year.map_or_else(|| "℗ ".to_string(), |y| format!("℗ {y} "));
    let content = format!("{lead}{label}");
    let color = p.tx3;
    text(content)
        .key("page-release")
        .font(SMALL)
        .color(color)
        .link(Link::new(lead.len()..lead.len() + label.len(), label.to_string()).hover(LinkPaint::new().underline(color)))
        .on_link(|s: &mut App, name, tx| s.show_label(name, tx))
}

fn fact(label: &str, value: El, p: &Palette) -> El {
    row().gap(10.0).items_start().child(text(label.to_string()).font(SMALL).color(p.tx3).w(64.0)).child(value.font(SMALL).color(p.tx2).wrap().grow(1.0).shrink())
}

/// "More by {artist}": the artist's other albums, newest first, as a scrolling row of covers.
fn more_by(s: &App, a: usize, p: &Palette) -> Option<El> {
    let album = &s.albums()[a];
    let artist = album.artist.as_deref()?.to_lowercase();
    let mut others: Vec<usize> = (0..s.albums().len())
        .filter(|&o| o != a && s.albums()[o].artist.as_deref().is_some_and(|x| x.to_lowercase() == artist))
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by_key(|&o| std::cmp::Reverse(s.albums()[o].year));
    Some(album_strip(s, "more", &format!("More by {}", album.artist_name()), &others, false, &[], p))
}

/// A titled, sideways-scrolling row of album covers keyed `{key}`, `{key}{album}` per card.
/// With `artists`, each card names its artists too. `pills` go under the cards of the albums
/// they name.
pub(super) fn album_strip(s: &App, key: &str, title: &str, albums: &[usize], artists: bool, pills: &[(usize, Vec<table::Pill>)], p: &Palette) -> El {
    let mut strip = row().gap(CARD_GAP).items_start();
    for &o in albums {
        let card = small_card(s, key, o, artists, 150.0, p);
        strip = strip.child(match pills.iter().find(|(a, _)| *a == o) {
            Some((_, list)) => card.child(row().key(format!("{key}-roles{o}")).w_full().gap(4.0).pt(2.0).flex_wrap().children(list.iter().cloned().map(table::pill_el))),
            None => card,
        });
    }
    let section = if key == "more" { "more-by".to_string() } else { key.to_string() };
    col()
        .key(section)
        .gap(10.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx))
        .child(col().key(format!("{key}-strip")).w_full().scroll_x().child(strip))
}

/// A titled block of album covers keyed `{key}`, `{key}{album}` per card, wrapping onto as
/// many rows as it takes. With `artists`, each card names its artists too.
pub(super) fn album_wall(s: &App, key: &str, title: &str, albums: &[usize], artists: bool, p: &Palette) -> El {
    let mut wall = row().key(format!("{key}-wall")).w_full().gap(CARD_GAP).flex_wrap();
    for &o in albums {
        wall = wall.child(small_card(s, key, o, artists, 150.0, p));
    }
    col().key(key.to_string()).w_full().gap(10.0).child(text(title.to_string()).font(HEADING).color(p.tx)).child(wall)
}

/// A strip or wall's card: cover, title, optionally the artists, then kind and year. Opens
/// the album.
pub(super) fn small_card(s: &App, key: &str, o: usize, artists: bool, width: f32, p: &Palette) -> El {
    let other = &s.albums()[o];
    let mut card = col()
        .key(format!("{key}{o}"))
        .w(width)
        .gap(4.0)
        .padding(6.0)
        .radius(2.0)
        .hover(p.ui.alpha(0.3))
        .cursor(Cursor::Pointer)
        .role("link")
        .label(other.title.clone())
        .on_press_tx(move |s: &mut App, tx| s.show_album(o, tx))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Album(o), at)))
        .child(cover(format!("{key}-cover{o}"), cover_image(s, other), Some(width - 12.0), p))
        .child(text(other.title.clone()).font(TITLE).color(p.tx).truncate());
    if artists {
        card = card.child(text(other.artist_name().to_string()).font(SMALL).color(p.tx2).truncate());
    }
    let year = other.year.map(|y| y.to_string()).unwrap_or_default();
    let line = if other.release_type == ReleaseType::Album { year } else { [other.release_type.label().to_string(), year].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  •  ") };
    card.child(text(line).font(SMALL).color(p.tx3))
}

//! The artist page: their albums, the songs they appear on, their songs, and the songs
//! that credit them. The search page shows the same sections for the artist it finds.

use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::albums::{album_strip, back_label, cover, cover_image, play_button};
use super::table;
use super::*;
use crate::artist::{ArtistPage, page};
use crate::base::property::Property;
use crate::credits::Role;
use crate::palette::Palette;

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);
const SONG_ROW_H: f32 = 40.0;
/// Song rows' covers, as wide as the table's lead column.
const COVER_W: f32 = table::LEAD_W;

pub(super) fn artist_page(s: &App, name: &str, p: &Palette) -> El {
    let info = page(name, &s.tracks, &s.albums);
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
    let mut body = col().key("artist-page-body").w_full().px(28.0).py(16.0).gap(22.0).child(back).child(head(s, &info, false, p));
    if info.is_empty() {
        body = body.child(text(format!("Nothing in the library is by or credits {}.", info.name)).key("artist-empty").font(BODY).color(p.tx3));
    }
    col().key("artist-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body.children(sections(s, &info, p)))
}

/// Their albums, the songs they appear on, their credits, then their songs.
pub(super) fn sections(s: &App, info: &ArtistPage, p: &Palette) -> Vec<El> {
    let mut out = Vec::new();
    if !info.albums.is_empty() {
        out.push(album_strip(s, "artist-albums", "Albums", &info.albums, false, p));
    }
    if !info.appears_on.is_empty() {
        out.push(songs(s, "appears-on", "Appears on", &info.appears_on, None, &info.name, p));
    }
    // Credits come first; the Songs section starts collapsed when there are any.
    for (role, list) in &info.credits {
        out.push(songs(s, &format!("credits-{}", role.label().to_lowercase()), &format!("{} credits", role.label()), list, Some(*role), &info.name, p));
    }
    if !info.songs.is_empty() {
        let open = s.artist_songs_open.unwrap_or(info.credits.is_empty());
        let mut section = col().key("artist-songs").w_full().gap(8.0).child(songs_toggle(info.songs.len(), open, p));
        if open {
            section = section.child(song_table(s, "artist-songs", &info.songs, None, &info.name, p));
        }
        out.push(section);
    }
    out
}

/// Picture (the first album's cover), name, counts, Play. In search results the name and
/// picture open the artist's page.
pub(super) fn head(s: &App, info: &ArtistPage, linked: bool, p: &Palette) -> El {
    let first = info.albums.first().copied().or_else(|| info.appears_on.first().and_then(|&i| s.album_of(i)));
    let mut art = cover("artist-cover".into(), first.and_then(|a| cover_image(s, &s.albums[a])), Some(160.0), 80.0, p);
    let mut counts = Vec::new();
    if !info.albums.is_empty() {
        counts.push(plural(info.albums.len(), "album"));
    }
    if !info.songs.is_empty() {
        counts.push(plural(info.songs.len(), "song"));
    }
    if !info.featured_on.is_empty() {
        counts.push(format!("featured on {}", info.featured_on.len()));
    }
    let credited: usize = info.credits.iter().map(|(_, l)| l.len()).sum();
    if credited > 0 {
        counts.push(plural(credited, "credit"));
    }
    let mut name = text(info.name.clone()).key("artist-name").font(PAGE_TITLE).color(p.tx).wrap();
    if linked {
        let open = |n: String| move |s: &mut App, tx: &mut rune::Tx| s.show_artist(&n, tx);
        art = art.cursor(Cursor::Pointer).role("link").label(format!("Open {}", info.name)).on_press_tx(open(info.name.clone()));
        name = name.cursor(Cursor::Pointer).on_press_tx(open(info.name.clone()));
    }
    let mut details = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Artist").font(SMALL).color(p.tx3))
        .child(name)
        .child(text(counts.join("  •  ")).key("artist-counts").font(BODY).color(p.tx3));
    let mut buttons = row().gap(8.0);
    let playable = play_list(info);
    if let Some(&first) = playable.first() {
        let who = info.name.clone();
        buttons = buttons.child(play_button("artist-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let list = play_list(&page(&who, &s.tracks, &s.albums));
            s.play_list(list, first, tx);
        }));
    }
    if linked {
        let who = info.name.clone();
        buttons = buttons.child(link_button("open-artist", "Go to artist", p).on_press_tx(move |s: &mut App, tx| s.show_artist(&who, tx)));
    }
    details = details.child(el().h(10.0)).child(buttons);
    row().gap(28.0).center_y().child(art).child(details)
}

/// What Play plays: their songs, else the songs crediting them, in library order.
fn play_list(info: &ArtistPage) -> Vec<usize> {
    if !info.songs.is_empty() {
        return info.songs.clone();
    }
    let mut all: Vec<usize> = info.credits.iter().flat_map(|(_, l)| l.iter().copied()).collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// An outlined button beside Play.
fn link_button(key: &'static str, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(14.0)
        .radius(16.0)
        .center_y()
        .border(1.0, p.ui)
        .hover(p.ui.alpha(0.4))
        .cursor(Cursor::Pointer)
        .role("link")
        .label(label)
        .child(text(label).font(TITLE).color(p.tx))
}

/// A titled song table with the page columns. Double-click plays the table from that song.
/// `artist` names whose performer credits the notes describe.
pub(super) fn songs(s: &App, key: &str, title: &str, list: &[usize], role: Option<Role>, artist: &str, p: &Palette) -> El {
    col()
        .key(key.to_string())
        .w_full()
        .gap(8.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx))
        .child(song_table(s, key, list, role, artist, p))
}

fn song_table(s: &App, key: &str, list: &[usize], role: Option<Role>, artist: &str, p: &Palette) -> El {
    let columns = &s.base.page_columns;
    let mut rows = col().child(table::header(&format!("{key}-header"), "", columns, 8.0, p));
    for &i in list {
        rows = rows.child(song_line(s, key, i, list, role, artist, columns, p));
    }
    table::scrolling(format!("{key}-table"), columns, 8.0, rows)
}

/// The Songs heading, which opens and closes the section.
fn songs_toggle(count: usize, open: bool, p: &Palette) -> El {
    let chevron = icon(&rune_kit::glyphs::TRIANGLE_RIGHT, 12.0).color(p.tx3).icon_rotation(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 }).motion(SNAPPY);
    row()
        .key("artist-songs-toggle")
        .h(32.0)
        .pr(10.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.ui.alpha(0.3))
        .cursor(Cursor::Pointer)
        .role("button")
        .expanded(open)
        .label(format!("Songs, {count}"))
        .on_press(move |s: &mut App| s.artist_songs_open = Some(!open))
        .child(el().w(16.0).center().child(chevron))
        .child(text("Songs").font(HEADING).color(p.tx))
        .child(text(format!("{count}")).key("count").font(NUM).color(p.tx3))
}

#[allow(clippy::too_many_arguments)]
fn song_line(s: &App, key: &str, i: usize, list: &[usize], role: Option<Role>, artist: &str, columns: &[Property], p: &Palette) -> El {
    let t = &s.tracks[i];
    let current = s.current == Some(i);
    // The song's album cover, dimmed under the playing icon for the current song.
    let image = s.album_of(i).and_then(|a| cover_image(s, &s.albums[a]));
    let mut lead = cover(format!("{key}-cover{i}"), image, Some(COVER_W), 4.0, p);
    if current {
        lead = lead.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(4.0).fill(rune::Color::BLACK.alpha(0.55)).center()
                .child(icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(rune::Color::WHITE)),
        );
    }
    // A performer's credit says what they played.
    let note = role.filter(|r| *r == Role::Performer).map(|_| {
        t.credits.iter().filter(|c| c.role == Role::Performer && c.name.eq_ignore_ascii_case(artist)).filter_map(|c| c.detail.clone()).collect::<Vec<_>>().join(", ")
    });
    let list = list.to_vec();
    let look = table::RowLook { lead, selected: s.selected == Some(i), current, height: SONG_ROW_H, px: 8.0, note };
    table::song_row(t, i, columns, look, p)
        .key(format!("{key}-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
}

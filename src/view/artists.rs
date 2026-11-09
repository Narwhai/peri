//! The artist page: their albums, the albums they appear on, their songs, and the songs
//! that credit them.

use rune::{Align, Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::albums::{album_strip, back_label, cover, cover_image, play_button};
use super::*;
use crate::artist::{ArtistPage, page};
use crate::credits::Role;
use crate::palette::Palette;

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);
const SONG_ROW_H: f32 = 40.0;

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
    let mut body = col().key("artist-page-body").w_full().px(28.0).py(16.0).gap(22.0).child(back).child(head(s, &info, p));
    if info.is_empty() {
        body = body.child(text(format!("Nothing in the library is by or credits {}.", info.name)).key("artist-empty").font(BODY).color(p.tx3));
    }
    if !info.albums.is_empty() {
        body = body.child(album_strip(s, "artist-albums", "Albums", &info.albums, false, p));
    }
    if !info.appears_on.is_empty() {
        body = body.child(album_strip(s, "appears-on", "Appears on", &info.appears_on, true, p));
    }
    if !info.songs.is_empty() {
        body = body.child(songs(s, "artist-songs", "Songs", &info.songs, None, p));
    }
    for (role, list) in &info.credits {
        body = body.child(songs(s, &format!("credits-{}", role.label().to_lowercase()), &format!("{} credits", role.label()), list, Some(*role), p));
    }
    col().key("artist-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// Picture (the first album's cover), name, counts, Play.
fn head(s: &App, info: &ArtistPage, p: &Palette) -> El {
    let first = info.albums.first().or(info.appears_on.first()).copied();
    let art = cover("artist-cover".into(), first.and_then(|a| cover_image(s, &s.albums[a])), Some(160.0), 80.0, p);
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
    let mut details = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Artist").font(SMALL).color(p.tx3))
        .child(text(info.name.clone()).key("artist-name").font(PAGE_TITLE).color(p.tx).wrap())
        .child(text(counts.join("  •  ")).key("artist-counts").font(BODY).color(p.tx3));
    let playable = play_list(info);
    if let Some(&first) = playable.first() {
        let play = play_button("artist-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let name = s.open_artist().map(String::from);
            let list = name.map_or_else(Vec::new, |n| play_list(&page(&n, &s.tracks, &s.albums)));
            s.play_list(list, first, tx);
        });
        details = details.child(el().h(10.0)).child(row().gap(8.0).child(play));
    }
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

/// A titled table of songs: title (with "feat." names), artist and album, length.
/// Double-click plays the table from that song.
fn songs(s: &App, key: &str, title: &str, list: &[usize], role: Option<Role>, p: &Palette) -> El {
    let mut table = col().key(key.to_string()).w_full().child(text(title.to_string()).font(HEADING).color(p.tx)).child(el().h(8.0));
    for &i in list {
        table = table.child(song_line(s, key, i, list, role, p));
    }
    table
}

fn song_line(s: &App, key: &str, i: usize, list: &[usize], role: Option<Role>, p: &Palette) -> El {
    let t = &s.tracks[i];
    let current = s.current == Some(i);
    let lead: El = if current {
        icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 13.0).color(p.blue)
    } else {
        icon(&icons::NOTE, 13.0).color(p.ui2)
    };
    let artist = song_artist(t, p.tx2).unwrap_or_else(|| text("Unknown artist").color(p.tx2));
    let album = t.album.clone().unwrap_or_default();
    // A performer's credit says what they played.
    let detail = role.filter(|r| *r == Role::Performer).map(|_| {
        let name = s.open_artist().unwrap_or_default();
        t.credits.iter().filter(|c| c.role == Role::Performer && c.name.eq_ignore_ascii_case(name)).filter_map(|c| c.detail.clone()).collect::<Vec<_>>().join(", ")
    });
    let list = list.to_vec();
    let mut middle = col().grow(1.0).shrink().gap(2.0).child(song_title(t, if current { p.blue } else { p.tx }).key("title").font(BODY).truncate());
    middle = middle.child(row().gap(6.0).child(artist.key("artist").font(SMALL).truncate().shrink()).child(text(if album.is_empty() { String::new() } else { format!("· {album}") }).font(SMALL).color(p.tx3).truncate().shrink()));
    row()
        .key(format!("{key}-t{i}"))
        .w_full()
        .h(SONG_ROW_H)
        .px(8.0)
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .fill(if s.selected == Some(i) { p.ui.alpha(0.55) } else { p.bg.alpha(0.0) })
        .hover(p.ui.alpha(0.3))
        .role("row")
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
        .child(el().w(20.0).center().child(lead))
        .child(middle)
        .children(detail.filter(|d| !d.is_empty()).map(|d| text(d).font(SMALL).color(p.tx3).shrink().truncate()))
        .child(text(t.duration.map(clock).unwrap_or_default()).font(NUM).color(p.tx3).w(48.0).align(Align::End))
}

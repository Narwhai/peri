//! The artist page: their albums, the songs they appear on, their songs, and the songs
//! that credit them. The search page shows the same sections for the artist it finds.

use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};

use super::albums::{album_strip, back_label, cover, cover_image, play_button, secondary_button};
use super::table;
use super::*;
use crate::album::ReleaseType;
use crate::artist::{ArtistPage, page};
use crate::credits::Role;
use crate::library::Track;
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
        .on_press_tx(|s: &mut App, tx| s.go_back(tx))
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
    let pills: Vec<(usize, Vec<table::Pill>)> = info.album_credits.iter().map(|(a, roles)| (*a, album_pills(s.albums[*a].tracks.len(), roles, p))).collect();
    out.extend(release_strips(s, "artist", &info.albums, false, &pills, p));
    if !info.appears_on.is_empty() {
        out.push(songs(s, "appears-on", "Appears on", &info.appears_on, p));
    }
    // Credits come first, each song once with a pill per role; the Songs section starts
    // collapsed when there are any.
    if !info.credits.is_empty() {
        let open = !s.artist_credits_closed;
        let toggle = section_toggle("artist-credits-toggle", "Credits", info.credits.len(), open, p).on_press(move |s: &mut App| s.artist_credits_closed = open);
        let mut section = col().key("credits").w_full().gap(8.0).child(toggle);
        if open {
            section = section.child(credits(s, info, p));
        }
        out.push(section);
    }
    if !info.songs.is_empty() {
        let open = s.artist_songs_open.unwrap_or(info.credits.is_empty());
        let toggle = section_toggle("artist-songs-toggle", "Songs", info.songs.len(), open, p).on_press(move |s: &mut App| s.artist_songs_open = Some(!open));
        let mut section = col().key("artist-songs").w_full().gap(8.0).child(toggle);
        if open {
            section = section.child(song_table(s, "artist-songs", untagged(&info.songs), &[], &info.songs, p));
        }
        out.push(section);
    }
    out
}

/// One strip per release type (Albums, EPs, Singles, Compilations) of `albums`, keeping
/// their order; keyed `{key}-albums`, `{key}-eps`…
pub(super) fn release_strips(s: &App, key: &str, albums: &[usize], artists: bool, pills: &[(usize, Vec<table::Pill>)], p: &Palette) -> Vec<El> {
    ReleaseType::ALL
        .into_iter()
        .filter_map(|kind| {
            let of_kind: Vec<usize> = albums.iter().copied().filter(|&a| s.albums[a].release_type == kind).collect();
            let section = format!("{key}-{}", kind.plural().to_lowercase());
            (!of_kind.is_empty()).then(|| album_strip(s, &section, kind.plural(), &of_kind, artists, pills, p))
        })
        .collect()
}

/// Picture (the first album's cover), name, counts, Play. In search results the name and
/// picture open the artist's page.
pub(super) fn head(s: &App, info: &ArtistPage, linked: bool, p: &Palette) -> El {
    let first = info.albums.first().copied().or_else(|| info.appears_on.first().and_then(|&i| s.album_of(i)));
    let mut art = cover("artist-cover".into(), first.and_then(|a| cover_image(s, &s.albums[a])), Some(160.0), 80.0, p);
    let mut counts = Vec::new();
    for kind in ReleaseType::ALL {
        let n = info.albums.iter().filter(|&&a| s.albums[a].release_type == kind).count();
        if n > 0 {
            counts.push(kind.count(n));
        }
    }
    if !info.songs.is_empty() {
        counts.push(plural(info.songs.len(), "song"));
    }
    if !info.featured_on.is_empty() {
        counts.push(format!("featured on {}", info.featured_on.len()));
    }
    if !info.credits.is_empty() {
        counts.push(format!("credited on {}", plural(info.credits.len(), "song")));
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
    if !info.play_list(&s.albums).is_empty() {
        let who = info.name.clone();
        buttons = buttons.child(play_button("artist-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, &s.tracks, &s.albums).play_list(&s.albums);
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    if !info.discography(&s.albums).is_empty() {
        let who = info.name.clone();
        buttons = buttons.child(secondary_button("artist-discography", &icons::PLAY, "Play Discography", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, &s.tracks, &s.albums).discography(&s.albums);
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    if linked {
        let who = info.name.clone();
        buttons = buttons.child(link_button("open-artist", "Go to artist", p).on_press_tx(move |s: &mut App, tx| s.show_artist(&who, tx)));
    }
    details = details.child(el().h(10.0)).child(buttons);
    row().gap(28.0).center_y().child(art).child(details)
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
pub(super) fn songs(s: &App, key: &str, title: &str, list: &[usize], p: &Palette) -> El {
    titled(key, title, song_table(s, key, untagged(list), &[], list, p), p)
}

fn titled(key: &str, title: &str, table: El, p: &Palette) -> El {
    col().key(key.to_string()).w_full().gap(8.0).child(text(title.to_string()).font(HEADING).color(p.tx)).child(table)
}

fn untagged(list: &[usize]) -> Vec<(usize, Vec<Option<table::Pill>>)> {
    list.iter().map(|&i| (i, Vec::new())).collect()
}

/// The Credits table: each song crediting the artist once, with a Roles column: one slot per
/// role any of the songs credits them with, so a role's pills line up down the table.
fn credits(s: &App, info: &ArtistPage, p: &Palette) -> El {
    let used: Vec<Role> = Role::ALL.into_iter().filter(|r| info.credits.iter().any(|(_, roles)| roles.contains(r))).collect();
    let rows: Vec<(usize, Vec<Option<table::Pill>>)> = info
        .credits
        .iter()
        .map(|(i, roles)| {
            let pills = used.iter().map(|r| roles.contains(r).then(|| role_pill(&s.tracks[*i], *r, &info.name, p))).collect();
            (*i, pills)
        })
        .collect();
    // Each slot is as wide as its widest pill.
    let slots: Vec<f32> = (0..used.len())
        .map(|k| rows.iter().filter_map(|(_, pills)| pills[k].as_ref()).map(|pill| table::pill_width(&pill.label)).fold(0.0, f32::max))
        .collect();
    song_table(s, "credits", rows, &slots, &info.credited(), p)
}

/// The artist's roles on one of their albums of `songs` songs: "Producer" when they hold it
/// on every song, else "Producer · 3/10".
fn album_pills(songs: usize, roles: &[(Role, usize)], p: &Palette) -> Vec<table::Pill> {
    roles
        .iter()
        .map(|&(role, n)| {
            let label = if n == songs { role.label().to_string() } else { format!("{} · {n}/{songs}", role.label()) };
            let (fill, color) = p.role(role);
            table::Pill { label, fill, color }
        })
        .collect()
}

/// `artist`'s `role` on a song: "Producer"; a performer's says what they played:
/// "Performer · vocals".
fn role_pill(t: &Track, role: Role, artist: &str, p: &Palette) -> table::Pill {
    let played: Vec<&str> = t.credits.iter().filter(|c| c.role == role && c.name.eq_ignore_ascii_case(artist)).filter_map(|c| c.detail.as_deref()).collect();
    let label = if played.is_empty() { role.label().to_string() } else { format!("{} · {}", role.label(), played.join(", ")) };
    let (fill, color) = p.role(role);
    table::Pill { label, fill, color }
}

/// `rows` are the songs with what fills their Roles slots; `list` is what double-click plays.
fn song_table(s: &App, key: &str, rows: Vec<(usize, Vec<Option<table::Pill>>)>, pills: &[f32], list: &[usize], p: &Palette) -> El {
    let columns = table::Columns { shown: &s.base.page_columns, widths: &s.base.page_widths, pills };
    let mut body = col().child(table::header(&format!("{key}-header"), "", columns, 8.0, p));
    for (i, pills) in rows {
        body = body.child(song_line(s, key, i, list, pills, columns, p));
    }
    table::scrolling(format!("{key}-table"), columns, 8.0, body)
}

/// A section heading with its count, which opens and closes the section; the caller adds
/// the press.
fn section_toggle(key: &'static str, title: &'static str, count: usize, open: bool, p: &Palette) -> El {
    let chevron = icon(&rune_kit::glyphs::TRIANGLE_RIGHT, 12.0).color(p.tx3).icon_rotation(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 }).motion(SNAPPY);
    row()
        .key(key)
        .h(32.0)
        .pr(10.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.ui.alpha(0.3))
        .cursor(Cursor::Pointer)
        .role("button")
        .expanded(open)
        .label(format!("{title}, {count}"))
        .child(el().w(16.0).center().child(chevron))
        .child(text(title).font(HEADING).color(p.tx))
        .child(text(format!("{count}")).key("count").font(NUM).color(p.tx3))
}

fn song_line(s: &App, key: &str, i: usize, list: &[usize], pills: Vec<Option<table::Pill>>, columns: table::Columns, p: &Palette) -> El {
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
    let list = list.to_vec();
    let look = table::RowLook { lead, selected: s.selected == Some(i), current, height: SONG_ROW_H, px: 8.0, pills };
    table::song_row(t, i, columns, look, p)
        .key(format!("{key}-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
}

//! Song tables: a header and rows with the title and the view's columns. The song list, album
//! track tables and the artist page share them. Names in list columns (artists, credits) open
//! the artist's page, and each row has a details button that opens the song details dialog.

use rune::{Align, Cursor, El, Tx, col, el, icon, row, text};

use super::*;
use crate::base::property::{Kind, Property, Value};
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;

/// Room for the row's number or playing icon.
pub const LEAD_W: f32 = 28.0;
/// The title column takes the room left, but never less than this.
pub const TITLE_MIN: f32 = 220.0;
const DETAILS_W: f32 = 28.0;
pub const GAP: f32 = 12.0;

/// A column's width, after the property's kind: short for numbers, wide for names.
pub fn width(p: Property) -> f32 {
    match p {
        Property::Track | Property::Disc => 44.0,
        Property::Year | Property::Format => 56.0,
        Property::Duration | Property::Tracks => 60.0,
        Property::Added => 96.0,
        Property::Folder => 260.0,
        Property::Album | Property::Label | Property::Genre => 180.0,
        Property::Title => TITLE_MIN,
        _ => 170.0,
    }
}

/// How much of the spare width a column takes beside the title (which takes 1): artist lines
/// run long ("A & B feat. C & D"), so they grow with the window.
fn grow(p: Property) -> f32 {
    match p {
        Property::Artist | Property::AlbumArtist => 0.8,
        _ => 0.0,
    }
}

/// Sizes a header or row cell for column `c`.
fn sized(e: El, c: Property) -> El {
    let e = e.w(width(c));
    if grow(c) > 0.0 { e.grow(grow(c)) } else { e }
}

/// How wide a table with `columns` must be before it scrolls sideways.
pub fn min_width(columns: &[Property], px: f32) -> f32 {
    let cells: f32 = columns.iter().map(|&c| width(c) + GAP).sum();
    2.0 * px + LEAD_W + GAP + TITLE_MIN + cells + GAP + DETAILS_W
}

fn numeric(p: Property) -> bool {
    matches!(p.kind(), Kind::Number | Kind::Duration)
}

/// A song table that scrolls sideways when its columns need more than the width it has. The
/// scroll content fills the viewport on its own; a set minimum width on it would override
/// that, so the minimum sits on an inner column.
pub fn scrolling(key: String, columns: &[Property], px: f32, body: El) -> El {
    let inner = body.w_full().min_w(min_width(columns, px));
    el().key(key).w_full().scroll_x().child(col().w_full().child(inner))
}

/// The header row: "#", "Title" and each column's name.
/// `lead` heads the lead column: "#" over numbers, "" over covers.
pub fn header(key: &str, lead: &str, columns: &[Property], px: f32, p: &Palette) -> El {
    let mut r = row().key(key.to_string()).w_full().h(28.0).px(px).gap(GAP).center_y();
    r = r.child(text(lead.to_string()).font(SMALL).color(p.tx3).w(LEAD_W).align(Align::End));
    r = r.child(text("Title").font(SMALL).color(p.tx3).grow(1.0).min_w(TITLE_MIN).shrink());
    for &c in columns {
        let label = sized(text(c.label()).font(SMALL).color(p.tx3).truncate(), c);
        r = r.child(if numeric(c) { label.align(Align::End) } else { label });
    }
    r.child(el().w(DETAILS_W))
}

/// What a row shows besides its columns.
pub struct RowLook {
    /// "3", or the playing icon.
    pub lead: El,
    pub selected: bool,
    pub current: bool,
    pub height: f32,
    pub px: f32,
    /// A quiet note after the title, as in what a performer played.
    pub note: Option<String>,
}

/// A song row: lead, title with "feat." links, the columns, and the details button. The caller
/// adds its key, presses and label.
pub fn song_row(t: &Track, i: usize, columns: &[Property], look: RowLook, p: &Palette) -> El {
    let title = song_title(t, if look.current { p.blue } else { p.tx }).key("title").font(BODY).truncate();
    let mut r = row()
        .w_full()
        .h(look.height)
        .px(look.px)
        .gap(GAP)
        .center_y()
        .radius(6.0)
        .fill(if look.selected { p.ui.alpha(if p.dark { 0.7 } else { 0.55 }) } else { p.bg.alpha(0.0) })
        .hover(p.ui.alpha(0.35))
        .hover_group()
        .role("row")
        .child(el().w(LEAD_W).center().child(look.lead))
        .child(
            row()
                .grow(1.0)
                .min_w(TITLE_MIN)
                .shrink()
                .gap(8.0)
                .center_y()
                .child(title.shrink())
                .children(look.note.filter(|n| !n.is_empty()).map(|n| text(n).key("note").font(SMALL).color(p.tx3).truncate().shrink())),
        );
    for &c in columns {
        r = r.child(sized(cell(t, c, p), c).key(format!("col-{}", c.label().to_lowercase().replace(' ', "-"))));
    }
    r.child(details_button(i, p))
}

/// One column's value: names link to their pages, numbers align right.
fn cell(t: &Track, c: Property, p: &Palette) -> El {
    let color = p.tx2;
    let shown = match c {
        // The artist line as tagged ("A & B feat. C"), each name a link.
        Property::Artist => return song_artist(t, color).unwrap_or_else(|| text("")).font(BODY).truncate(),
        Property::AlbumArtist if t.album_artist.is_some() => {
            let line = t.album_artist.clone().unwrap_or_default();
            let links = name_links(&line, &t.album_artists);
            return linked(line, links, color).font(BODY).truncate();
        }
        _ => c.value(t),
    };
    match shown {
        Value::List(names) if names_link(c) => name_list(&names.into_iter().map(|n| (n, Vec::new())).collect::<Vec<_>>(), color).font(BODY).truncate(),
        Value::None => text(""),
        Value::Text(f) if c == Property::Format => text(f.to_uppercase()).font(SMALL).color(p.tx3),
        v => {
            let label = text(c.display(&v)).font(if numeric(c) { NUM } else { BODY }).color(if numeric(c) { p.tx3 } else { color }).truncate();
            if numeric(c) { label.align(Align::End) } else { label }
        }
    }
}

/// Whether a list property's items are people with pages.
fn names_link(c: Property) -> bool {
    c.role().is_some() || matches!(c, Property::Artist | Property::Featured | Property::AlbumArtist)
}

/// "ⓘ": faint until the row is hovered (fully transparent elements take no presses), and
/// opens the song's details.
fn details_button(i: usize, p: &Palette) -> El {
    el()
        .key("details")
        .size(DETAILS_W, DETAILS_W)
        .radius(DETAILS_W / 2.0)
        .center()
        .group_opacity(0.25, 1.0)
        .hover(p.ui.alpha(0.6))
        .hover_alone()
        .cursor(Cursor::Pointer)
        .label("Song details")
        .help("Song details (I)")
        .on_press_tx(move |s: &mut App, tx: &mut Tx| s.show_details(i, tx))
        .child(icon(&icons::INFO, 16.0).color(p.tx2))
}

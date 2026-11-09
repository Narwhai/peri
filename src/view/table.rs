//! Song tables: a header and rows with the title and the view's columns. The song list, album
//! track tables and the artist page share them. Names in list columns (artists, credits) open
//! the artist's page, and each row has a details button that opens the song details dialog.
//! Dragging a header's right edge sets that column's width; double-clicking it lets the
//! column size itself again.

use rune::{Align, Color, Cursor, Drag, El, Phase, Tx, col, el, icon, row, text};

use super::*;
use crate::base::property::{Kind, Property, Value};
use crate::base::view::Widths;
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;

/// Room for the row's number or playing icon.
pub const LEAD_W: f32 = 28.0;
/// The title column takes the room left, but never less than this.
pub const TITLE_MIN: f32 = 220.0;
const DETAILS_W: f32 = 28.0;
pub const GAP: f32 = 12.0;
/// A dragged column is never narrower than this.
const MIN_W: f32 = 40.0;
/// The grab area at a header's right edge, centred in the gap after it.
const HANDLE_W: f32 = 8.0;

/// A table's columns after the title, and the widths the user dragged some of them to.
#[derive(Clone, Copy)]
pub struct Columns<'a> {
    pub shown: &'a [Property],
    pub widths: &'a Widths,
    /// The widths of the pill slots in a Roles column after the title; none, no such column.
    pub pills: &'a [f32],
}

impl<'a> Columns<'a> {
    /// What changes a row's look, for memo versions.
    pub fn version(&self) -> (Vec<Property>, Vec<(Property, u32)>) {
        (self.shown.to_vec(), self.widths.iter().map(|(&c, w)| (c, w.to_bits())).collect())
    }
}

/// A column's width: as dragged, else its default.
fn width_in(widths: &Widths, c: Property) -> f32 {
    widths.get(&c).copied().unwrap_or_else(|| width(c))
}

/// A column's default width, after the property's kind: short for numbers, wide for names.
pub fn width(p: Property) -> f32 {
    match p {
        Property::Track | Property::Disc => 44.0,
        Property::Year | Property::Format => 56.0,
        Property::Duration | Property::Tracks => 60.0,
        Property::Added | Property::ReleaseType => 96.0,
        Property::Folder => 260.0,
        Property::Album | Property::Label | Property::Genre => 180.0,
        Property::Title => TITLE_MIN,
        _ => 170.0,
    }
}

/// How much of the spare width a column takes beside the title (which takes 1), until it's
/// dragged to a width: artist lines and album and label names run long, so they grow with
/// the window.
fn grow(p: Property) -> f32 {
    match p {
        Property::Artist | Property::AlbumArtist => 0.8,
        Property::Album | Property::Label => 0.6,
        _ => 0.0,
    }
}

/// Sizes a header or row cell for column `c`: its dragged width, else its default growing
/// with the spare room.
fn sized(e: El, c: Property, widths: &Widths) -> El {
    match widths.get(&c) {
        Some(&w) => e.w(w),
        None if grow(c) > 0.0 => e.w(width(c)).grow(grow(c)),
        None => e.w(width(c)),
    }
}

/// How wide a table with `columns` must be before it scrolls sideways.
pub fn min_width(columns: Columns, px: f32) -> f32 {
    let cells: f32 = columns.shown.iter().map(|&c| width_in(columns.widths, c) + GAP).sum();
    let pills = if columns.pills.is_empty() { 0.0 } else { pills_width(columns.pills) + GAP };
    2.0 * px + LEAD_W + GAP + TITLE_MIN + pills + cells + GAP + DETAILS_W
}

/// The Roles column's width: its slots side by side.
fn pills_width(slots: &[f32]) -> f32 {
    slots.iter().sum::<f32>() + PILL_GAP * slots.len().saturating_sub(1) as f32
}

fn numeric(p: Property) -> bool {
    matches!(p.kind(), Kind::Number | Kind::Duration)
}

/// A song table that scrolls sideways when its columns need more than the width it has. The
/// scroll content fills the viewport on its own; a set minimum width on it would override
/// that, so the minimum sits on an inner column.
pub fn scrolling(key: String, columns: Columns, px: f32, body: El) -> El {
    let inner = body.w_full().min_w(min_width(columns, px));
    el().key(key).w_full().scroll_x().child(col().w_full().child(inner))
}

/// The header row: "#", "Title" and each column's name, with a resize handle at each
/// column's right edge. `lead` heads the lead column: "#" over numbers, "" over covers.
pub fn header(key: &str, lead: &str, columns: Columns, px: f32, p: &Palette) -> El {
    let mut r = row().key(key.to_string()).w_full().h(28.0).px(px).gap(GAP).center_y();
    r = r.child(text(lead.to_string()).font(SMALL).color(p.tx3).w(LEAD_W).align(Align::End));
    r = r.child(text("Title").font(SMALL).color(p.tx3).grow(1.0).min_w(TITLE_MIN).shrink());
    if !columns.pills.is_empty() {
        r = r.child(text("Roles").key(format!("{key}-col-roles")).font(SMALL).color(p.tx3).w(pills_width(columns.pills)));
    }
    for &c in columns.shown {
        let label = text(c.label()).font(SMALL).color(p.tx3).truncate().w_full();
        let cell = el()
            .key(format!("{key}-{}", column_key(c)))
            .h_full()
            .center_y()
            .on_layout_tx(move |s: &mut App, r, tx| {
                s.laid_out_widths.insert(c, r[2]);
                tx.live_only();
            })
            .child(if numeric(c) { label.align(Align::End) } else { label })
            .child(resize_handle(format!("{key}-{}-resize", column_key(c)), c, p));
        r = r.child(sized(cell, c, columns.widths));
    }
    r.child(el().w(DETAILS_W))
}

/// "col-album-artist": a column's key part.
fn column_key(c: Property) -> String {
    format!("col-{}", c.label().to_lowercase().replace(' ', "-"))
}

/// Drag to set column `c`'s width; double-click to let it size itself again.
fn resize_handle(key: String, c: Property, p: &Palette) -> El {
    el().key(key)
        .pin(None, Some(0.0), Some(-(GAP + HANDLE_W) / 2.0), Some(0.0))
        .w(HANDLE_W)
        .center()
        .cursor(Cursor::ResizeColumn)
        .role("separator")
        .label(format!("Resize {}", c.label()))
        .help("Drag to resize; double-click to reset")
        .child(el().w(1.0).h(14.0).radius(0.5).fill(p.ui.alpha(0.9)).group_opacity(0.0, 1.0))
        .hover_group()
        .on_drag(move |s: &mut App, d: &mut Drag, _tx: &mut Tx| match d.phase {
            Phase::Down if d.clicks >= 2 => {
                s.shown_widths_mut().remove(&c);
                d.data = f32::NAN;
            }
            Phase::Down => {
                let shown = s.shown_widths().get(&c).copied();
                d.data = shown.or_else(|| s.laid_out_widths.get(&c).copied()).unwrap_or_else(|| width(c));
            }
            Phase::Move if !d.data.is_nan() => {
                let w = (d.data + d.dx).max(MIN_W).round();
                s.shown_widths_mut().insert(c, w);
            }
            Phase::Move => {}
            Phase::Up => {
                if d.cancelled() && !d.data.is_nan() {
                    s.shown_widths_mut().insert(c, d.data);
                }
                s.views_changed();
            }
        })
}

/// What a row shows besides its columns.
pub struct RowLook {
    /// "3", or the playing icon.
    pub lead: El,
    pub selected: bool,
    pub current: bool,
    pub height: f32,
    pub px: f32,
    /// What fills each slot of the Roles column, if the table has one.
    pub pills: Vec<Option<Pill>>,
}

/// A song row: lead, title with "feat." links, the columns, and the details button. The caller
/// adds its key, presses and label.
pub fn song_row(t: &Track, i: usize, columns: Columns, look: RowLook, p: &Palette) -> El {
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
                .child(title.shrink()),
        );
    if !columns.pills.is_empty() {
        let mut slots = row().key("roles").w(pills_width(columns.pills)).gap(PILL_GAP).center_y();
        for (k, &w) in columns.pills.iter().enumerate() {
            let slot = el().w(w).center_y();
            slots = slots.child(match look.pills.get(k).cloned().flatten() {
                Some(pill) => slot.child(pill_el(pill)),
                None => slot,
            });
        }
        r = r.child(slots);
    }
    for &c in columns.shown {
        r = r.child(sized(cell(t, c, p), c, columns.widths).key(column_key(c)));
    }
    r.child(details_button(i, p))
}

/// A small coloured label in the Roles column: "Producer", "Performer · vocals".
#[derive(Clone, Debug, PartialEq)]
pub struct Pill {
    pub label: String,
    pub fill: Color,
    pub color: Color,
}

const PILL_PX: f32 = 7.0;
const PILL_GAP: f32 = 6.0;

/// How wide `label`'s pill draws.
pub fn pill_width(label: &str) -> f32 {
    rune::text::measure(label, &[], SMALL, None).0 + 2.0 * PILL_PX
}

fn pill_el(pill: Pill) -> El {
    row()
        .key(format!("tag-{}", pill.label.to_lowercase().replace(' ', "-")))
        .h(18.0)
        .px(PILL_PX)
        .radius(9.0)
        .center_y()
        .fill(pill.fill)
        .child(text(pill.label).font(SMALL).color(pill.color))
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
        Property::Label => return t.label.as_deref().map_or_else(|| text(""), |l| label_link(l, color)).font(BODY).truncate(),
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

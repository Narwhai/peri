//! The track list: song rows, and sticky collapsible group headers in a grouped view.

use rune::{Align, Cursor, Cx, El, GrowthSpec, Live, ScrollPos, ScrollSpec, Tx, col, el, icon, memo, row, text};

use super::albums::{columns_for, grid_row, grid_row_h, list_card};
use super::table;
use super::*;
use crate::app::{App, Row, album_key, grid_key, header_key, row_key};
use crate::base::property::{Mode, Property};
use crate::base::view::Layout;
use crate::base::view::Section;
use crate::icons;
use crate::library::{AUDIO_EXTENSIONS, Track};
use crate::palette::Palette;

const HEADER_H: f32 = 36.0;

/// The list's horizontal padding, and the grid's inner width from the list's.
const LIST_PX: f32 = 8.0;
const GRID_PX: f32 = 16.0;

pub(super) fn list(s: &App, p: &Palette, cx: &mut Cx<App>) -> El {
    if s.tracks.is_empty() {
        return empty_state(s, p);
    }
    let albums = s.mode() == Mode::Albums;
    let grid = albums && s.base.active().layout == Layout::Grid;
    let empty = if albums { s.shown_albums.is_empty() } else { s.filtered.is_empty() };
    if empty {
        let what = if albums { "albums" } else { "songs" };
        let detail = format!("No {what} match the filters of “{}”.", s.base.active().name);
        return col()
            .key("no-results")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(6.0)
            .child(text("No matches").font(HEADING).color(p.tx2))
            .child(text(detail).font(BODY).color(p.tx3));
    }
    let p = *p;
    let count = s.rows.len();
    // The grid's width, read from layout; the column count follows it (see `set_columns`).
    let pad = if grid { GRID_PX } else { LIST_PX };
    let width = cx.layout_with("tracks", |r| (r[2] / 4.0).round() * 4.0).unwrap_or(960.0) - 2.0 * pad;
    let estimate = match (albums, grid) {
        (_, true) => grid_row_h(width, s.columns),
        (true, false) => 520.0,
        (false, _) => ROW_H,
    };
    // Rows are rebuilt from state at layout time, so the closures read the list through `s`.
    let list = col()
        .key("rows")
        .w_full()
        .px(pad)
        .py(if grid { 12.0 } else { 6.0 })
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            estimate,
            |s: &App, n| match s.rows[n] {
                Row::Header(g) => header_key(g).into(),
                Row::Track { track, section } => row_key(track, section).into(),
                Row::Album { album, section } => album_key(album, section).into(),
                Row::Grid { start, section } => grid_key(start, section).into(),
            },
            move |s: &App, n| match s.rows[n] {
                Row::Album { album, section } => {
                    let current = s.current.filter(|c| s.albums[album].tracks.contains(c));
                    let covers = s.covers.as_ref().map_or(0, |c| c.generation());
                    memo(album_key(album, section), (s.list_version, current, s.playing, s.selected, covers, p.dark), || list_card(s, album, section, &p))
                }
                Row::Grid { start, section } => {
                    let playing = s.current.filter(|c| s.grid_albums(start, section).iter().any(|&a| s.albums[a].tracks.contains(c)));
                    let covers = s.covers.as_ref().map_or(0, |c| c.generation());
                    memo(grid_key(start, section), (s.list_version, playing, s.playing, covers, width.to_bits(), p.dark), || {
                        grid_row(s, start, section, width, &p)
                    })
                }
                Row::Header(g) => {
                    let section = &s.sections[g];
                    let open = !s.collapsed.contains(&section.title);
                    memo(header_key(g), (s.list_version, open, p.dark), || group_header(section, g, open, &p))
                }
                Row::Track { track, section } => {
                    let selected = s.selected == Some(track);
                    let current = s.current == Some(track);
                    let playing = current && s.playing;
                    let number = number_in(s, track, section);
                    let columns = &s.base.active().columns;
                    memo(row_key(track, section), (s.list_version, number, selected, current, playing, p.dark, columns.clone()), || {
                        track_row(&s.tracks[track], track, section, number, selected, current, playing, columns, &p)
                    })
                }
            },
        )
        .virtual_rows_version(s.list_version);
    let viewport = col()
        .key("tracks")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_margin(HEADER_H)
        .on_layout(move |s: &mut App, r| {
            if s.mode() == Mode::Albums && s.base.active().layout == Layout::Grid {
                s.set_columns(columns_for(r[2] - 2.0 * GRID_PX));
            }
        });
    if albums {
        return viewport.child(list);
    }
    // Songs: a table as wide as its columns need, scrolling sideways when the window is
    // narrower. The column header sits above the viewport and follows its sideways scroll.
    let columns = &s.base.active().columns;
    // A viewport scrolling both ways lays its content out at its natural width, so the table
    // fills the viewport itself, and is wider when its columns need it.
    let min_w = table::min_width(columns, LIST_PX).max(width + 2.0 * pad);
    let shift = Live::new([0.0f32, 0.0]);
    let follow = shift.clone();
    let viewport = viewport.scroll_xy().on_scroll_offset(move |at: ScrollPos| {
        follow.set([-at.offset[0], 0.0]);
    });
    let head = table::header("table-header", "#", columns, LIST_PX + 10.0, &p).min_w(min_w).live_offset(&shift);
    col()
        .key("song-table")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(el().key("table-header-clip").w_full().clip_soft(0.0).border_bottom(&p).child(head))
        .child(viewport.child(list.min_w(min_w)))
}

/// A song's number in its group, or in the whole list when ungrouped.
fn number_in(s: &App, track: usize, section: Option<usize>) -> usize {
    let within = match section {
        Some(g) => &s.sections[g].items,
        None => &s.filtered,
    };
    within.iter().position(|&i| i == track).map_or(0, |n| n + 1)
}

fn group_header(section: &Section, g: usize, open: bool, p: &Palette) -> El {
    let title = section.title.clone();
    let key = header_key(g);
    let keep = key.clone();
    let chevron = icon(&rune_kit::glyphs::TRIANGLE_RIGHT, 12.0).color(p.tx3).icon_rotation(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 }).motion(SNAPPY);
    let title_color = if section.value.is_none() { p.tx3 } else { p.tx };
    row()
        .key(key)
        .w_full()
        .h(HEADER_H)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .fill(p.bg)
        .sticky_top(0.0)
        .stuck_shadow(rune::Shadow { x: 0.0, y: 1.0, blur: 0.0, spread: 0.0, color: p.ui.alpha(0.8), inset: false })
        .role("button")
        .expanded(open)
        .label(format!("{title}, {}", section.items.len()))
        .cursor(Cursor::Pointer)
        .hover(p.ui.alpha(0.25))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            s.toggle_section(&title);
            tx.keep_on_screen(&keep);
        })
        .child(el().w(28.0).center().child(chevron))
        .child(text(section.title.clone()).font(GROUP).color(title_color).truncate().shrink())
        .child(text(format!("{}", section.items.len())).key("count").font(NUM).color(p.tx3))
}

#[allow(clippy::too_many_arguments)]
fn track_row(t: &Track, i: usize, section: Option<usize>, number: usize, selected: bool, current: bool, playing: bool, columns: &[Property], p: &Palette) -> El {
    let lead: El = if current {
        icon(if playing { &icons::BARS } else { &icons::PAUSE }, 14.0).color(p.blue)
    } else {
        text(format!("{number}")).font(NUM).color(p.tx3).align(Align::End).w(28.0)
    };
    let look = table::RowLook { lead, selected, current, height: ROW_H, px: 10.0, note: None };
    table::song_row(t, i, columns, look, p)
        .key(row_key(i, section))
        .press(p.ui.alpha(0.5))
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or("unknown artist")))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_index(i, tx))
}

fn empty_state(s: &App, p: &Palette) -> El {
    let (title, detail) = match &s.scan {
        Some(_) => ("Reading your music…", "Tags and durations load as each file is read.".to_string()),
        None => (
            "No music yet",
            format!("Open a folder, or drop folders and files here ({}).", AUDIO_EXTENSIONS.join(", ")),
        ),
    };
    col()
        .key("empty")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(10.0)
        .child(icon(&icons::NOTE, 40.0).color(p.ui2))
        .child(text(title).font(HEADING).color(p.tx))
        .child(text(detail).font(BODY).color(p.tx3).max_w(360.0).wrap().align(Align::Center))
        .child(el().h(6.0))
        .child(pill_button("open-empty", "Open folder…", p).on_press_tx(|s: &mut App, tx| s.open_folder(tx)))
}

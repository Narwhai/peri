//! The track list: song rows, and sticky collapsible group headers in a grouped view.

use rune::{Align, Cursor, El, GrowthSpec, ScrollSpec, Tx, col, el, icon, memo, row, text};

use super::*;
use crate::app::{App, Row, header_key, row_key};
use crate::base::view::Section;
use crate::icons;
use crate::library::{AUDIO_EXTENSIONS, Track};
use crate::palette::Palette;

const HEADER_H: f32 = 36.0;

pub(super) fn list(s: &App, p: &Palette) -> El {
    if s.tracks.is_empty() {
        return empty_state(s, p);
    }
    if s.filtered.is_empty() {
        let search = s.search.text.trim();
        let detail = if search.is_empty() {
            format!("No songs match the filters of “{}”.", s.base.active().name)
        } else {
            format!("Nothing in “{}” matches “{search}”.", s.base.active().name)
        };
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
    // Rows are rebuilt from state at layout time, so the closures read the list through `s`.
    let list = col()
        .key("rows")
        .w_full()
        .px(8.0)
        .py(6.0)
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            ROW_H,
            |s: &App, n| match s.rows[n] {
                Row::Header(g) => header_key(g).into(),
                Row::Track { track, section } => row_key(track, section).into(),
            },
            move |s: &App, n| match s.rows[n] {
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
                    memo(row_key(track, section), (s.list_version, number, selected, current, playing, p.dark), || {
                        track_row(&s.tracks[track], track, section, number, selected, current, playing, &p)
                    })
                }
            },
        )
        .virtual_rows_version(s.list_version);
    col()
        .key("tracks")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_margin(HEADER_H)
        .child(list)
}

/// A song's number in its group, or in the whole list when ungrouped.
fn number_in(s: &App, track: usize, section: Option<usize>) -> usize {
    let within = match section {
        Some(g) => &s.sections[g].tracks,
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
        .label(format!("{title}, {}", super::plural(section.tracks.len(), "song")))
        .cursor(Cursor::Pointer)
        .hover(p.ui.alpha(0.25))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            s.toggle_section(&title);
            tx.keep_on_screen(&keep);
        })
        .child(el().w(28.0).center().child(chevron))
        .child(text(section.title.clone()).font(GROUP).color(title_color).truncate().shrink())
        .child(text(format!("{}", section.tracks.len())).key("count").font(NUM).color(p.tx3))
}

#[allow(clippy::too_many_arguments)]
fn track_row(t: &Track, i: usize, section: Option<usize>, number: usize, selected: bool, current: bool, playing: bool, p: &Palette) -> El {
    let lead: El = if current {
        icon(if playing { &icons::BARS } else { &icons::PAUSE }, 14.0).color(p.blue)
    } else {
        text(format!("{number}")).font(NUM).color(p.tx3).align(Align::End).w(28.0)
    };
    let title_color = if current { p.blue } else { p.tx };
    let subtitle = match (t.artist(), &t.album) {
        (Some(a), Some(b)) => format!("{a} · {b}"),
        (Some(a), None) => a,
        (None, Some(b)) => b.clone(),
        (None, None) => "Unknown artist".into(),
    };
    let fill = if selected { p.ui.alpha(if p.dark { 0.7 } else { 0.55 }) } else { p.bg.alpha(0.0) };
    row()
        .key(row_key(i, section))
        .w_full()
        .h(ROW_H)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .radius(8.0)
        .fill(fill)
        .hover(p.ui.alpha(0.35))
        .press(p.ui.alpha(0.5))
        .role("row")
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or("unknown artist")))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_index(i, tx))
        .child(el().w(28.0).center().child(lead))
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(2.0)
                .child(text(t.title.clone()).font(TITLE).color(title_color).truncate())
                .child(text(subtitle).font(SMALL).color(p.tx2).truncate()),
        )
        .child(text(t.duration.map(clock).unwrap_or_default()).font(NUM).color(p.tx3))
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

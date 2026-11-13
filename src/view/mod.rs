//! The window: a header with search, the track list, and the now-playing bar.

mod albums;
mod artists;
mod collab;
mod details;
mod filters;
mod graph;
mod home;
mod labels;
mod list;
mod player;
mod search;
mod settings;
mod table;
mod toolbar;

use std::ops::Range;

use rune::{Color, Cursor, Cx, El, Icon, LineCap, Link, LinkPaint, Path, PointerButton, Shape, Spring, TextStyle, col, el, icon, row, text};
use rune_kit::{Kit, PaletteSeed, TextInput};

use crate::app::App;
use crate::credits;
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;

pub use albums::columns_for;

const ROW_H: f32 = 44.0;
/// The header's tabs: as tall as the header's bottom edge they stand on, so their bottoms line up with it.
const TAB_H: f32 = 38.0;
/// The gap between the header's tabs. The active tab's flares reach past it into its neighbours' padding.
const TAB_GAP: f32 = 2.0;
/// A header tab's side padding: room for a neighbouring active tab's flare beside its icon and label.
const TAB_PAD: f32 = 11.0;
/// How far an inactive header tab's hover fill stays inside its top and bottom edges.
const TAB_HOVER_INSET: f32 = 5.0;
/// The top corners' radius of a header tab.
const TAB_RADIUS: f32 = 6.0;
/// How far an active header tab's bottom corners flare out onto the header's edge, Chrome-style.
const TAB_FLARE: f32 = 10.0;
const BAR_H: f32 = 88.0;
const COVER: f32 = 56.0;
const SNAPPY: Spring = Spring::new(0.25, 0.0);

const TITLE: TextStyle = TextStyle::new(13.0, 500.0);
const BODY: TextStyle = TextStyle::new(13.0, 400.0);
const SMALL: TextStyle = TextStyle::new(11.5, 400.0);
const NUM: TextStyle = TextStyle::new(11.5, 400.0).features("'tnum' 1");
const HEADING: TextStyle = TextStyle::new(15.0, 600.0);
const GROUP: TextStyle = TextStyle::new(12.5, 600.0);

/// The smallest window the app lays out in; a zoom never lays it out smaller.
pub const MIN_SIZE: (f32, f32) = (720.0, 420.0);

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let p = Palette::new(cx.system_dark());
    let window = cx.layout_size("zoom-frame");
    let zoom = window.map_or(1.0, |size| applied_zoom(s.zoom, size));
    let kit = Kit::from_palette(PaletteSeed { bg: p.bg2, fg: p.tx, accent: p.blue });
    let mut layer = Vec::new();
    // The view's toolbar does nothing to search results or settings, so they go without it.
    let toolbar = (!matches!(s.page, crate::app::Page::Search | crate::app::Page::Settings | crate::app::Page::Graph | crate::app::Page::Home)).then(|| toolbar::toolbar(s, &p, &kit, &mut layer));
    let body = col()
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(header(s, zoom, &p, &kit, cx))
        .children(toolbar)
        .child(match (s.open_album(), s.open_artist(), s.open_label(), s.open_collab()) {
            (Some(a), _, _, _) => albums::album_page(s, a, &p),
            (None, Some(name), _, _) => artists::artist_page(s, name, &p, &kit, &mut layer),
            (None, None, Some(name), _) => labels::label_page(s, name, &p),
            (None, None, None, Some(names)) => collab::collab_page(s, names, &p),
            (None, None, None, None) if s.page == crate::app::Page::Search => search::search_page(s, &p),
            (None, None, None, None) if s.page == crate::app::Page::Settings => settings::settings_page(s, &p, &kit),
            (None, None, None, None) if s.page == crate::app::Page::Graph => graph::graph_page(s, &p, &kit, &mut layer),
            (None, None, None, None) if s.page == crate::app::Page::Home => home::home_page(s, &p),
            (None, None, None, None) => list::list(s, &p, cx),
        })
        .child(player::now_playing(s, &p))
        // The mouse's back and forward buttons, anywhere a control doesn't take them.
        .pointer_buttons([PointerButton::Back, PointerButton::Forward])
        .on_press_tx(|s: &mut App, tx| match tx.button() {
            PointerButton::Back => s.go_back(tx),
            PointerButton::Forward => s.go_forward(),
            _ => {}
        })
        .on_drop_files(|s: &mut App, files, _tx| {
            s.drop_hover = false;
            s.add_to_library(files.to_vec());
        })
        .accepts_files(|path| path.is_dir() || crate::library::is_audio(path))
        .on_drop_hover(|s: &mut App, over, _tx| s.drop_hover = over);
    let drop_veil = s.drop_hover.then(|| {
        el().key("drop-veil")
            .pin(Some(8.0), Some(8.0), Some(8.0), Some(8.0))
            .radius(6.0)
            .border(2.0, p.blue)
            .fill(p.blue.alpha(0.08))
            .center()
            .child(text("Drop to add to the library").font(HEADING).color(p.tx))
            .inert()
    });
    layer.extend(drop_veil);
    details::details_dialog(s, &p, &kit, &mut layer);
    layer.extend(zoom_toast(s, zoom, &p));
    zoomed(window, zoom, &p, rune_kit::root(body, layer))
}

/// The zoom the app draws at in a `window`-sized window: the user's, less when that would lay
/// the app out smaller than [`MIN_SIZE`] (but never less than 100% for it).
pub fn applied_zoom(zoom: f32, [w, h]: [f32; 2]) -> f32 {
    let fits = (w / MIN_SIZE.0).min(h / MIN_SIZE.1).max(1.0);
    zoom.min(fits)
}

/// The app at `zoom`: laid out `1 / zoom` the window's size, drawn `zoom` times as large from
/// the top-left corner, so it fills the window again.
fn zoomed(window: Option<[f32; 2]>, zoom: f32, p: &Palette, app: El) -> El {
    let frame = el().key("zoom-frame").w_full().h_full().fill(p.bg);
    match window {
        Some([w, h]) if zoom != 1.0 => frame.child(el().key("zoomed").pin(Some(0.0), Some(0.0), None, None).size(w / zoom, h / zoom).scale(zoom).origin(0.0, 0.0).child(app)),
        _ => frame.child(el().key("zoomed").w_full().h_full().child(app)),
    }
}

/// The zoom level, for a moment after a keyboard zoom.
fn zoom_toast(s: &App, zoom: f32, p: &Palette) -> Option<El> {
    s.zoom_toast?;
    let toast = row()
            .key("zoom-toast")
            .h(32.0)
            .px(14.0)
            .gap(8.0)
            .radius(16.0)
            .center_y()
            .fill(p.bg2)
            .border(1.0, p.ui)
            .shadow(0.0, 6.0, Color::BLACK.alpha(0.18))
            .label(format!("Zoom {}", zoom_label(s.zoom, zoom)))
            .child(icon(&icons::ZOOM, 15.0).color(p.tx2))
            .child(text(zoom_label(s.zoom, zoom)).font(TITLE).color(p.tx));
    // Centred above the now-playing bar.
    Some(row().key("zoom-toast-row").pin(Some(0.0), None, Some(0.0), Some(BAR_H + 20.0)).justify_center().child(toast).inert())
}

/// "120%"; "max 136%" when the window is too small for the zoom asked for.
fn zoom_label(asked: f32, applied: f32) -> String {
    let percent = |z: f32| (z * 100.0).round();
    if percent(applied) < percent(asked) { format!("max {}%", percent(applied)) } else { format!("{}%", percent(asked)) }
}

/// The header's zoom control: the level, and a slider that's live as it drags. Clicking the
/// level resets it to 100%.
fn zoom_control(s: &App, zoom: f32, p: &Palette) -> El {
    use crate::app::{ZOOM_MAX, ZOOM_MIN};
    let shown = s.zoom_preview.unwrap_or(s.zoom);
    let fraction = (shown - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
    let level = match s.zoom_preview {
        Some(preview) => zoom_label(preview, preview),
        None => zoom_label(s.zoom, zoom),
    };
    let at = |d: &rune::Drag| ZOOM_MIN + (d.x / d.w).clamp(0.0, 1.0) * (ZOOM_MAX - ZOOM_MIN);
    let track = player::slider_track("zoom", fraction, true, p)
        .w(72.0)
        .label("Zoom")
        .help("Zoom (Ctrl+= / Ctrl+- / Ctrl+0)")
        .a11y_value(level.clone())
        .on_step(|s: &mut App, steps, _tx| s.set_zoom(s.zoom + steps as f32 * 0.1, true))
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut rune::Tx| match d.phase {
            rune::Phase::Down | rune::Phase::Move => s.zoom_preview = Some(crate::app::clamp_zoom(at(d))),
            rune::Phase::Up => {
                if let (false, Some(zoom)) = (d.cancelled(), s.zoom_preview.take()) {
                    s.set_zoom(zoom, true);
                }
            }
        });
    row()
        .key("zoom-control")
        .gap(6.0)
        .center_y()
        .child(icon(&icons::ZOOM, 15.0).color(p.tx3))
        .child(track)
        .child(
            text(level)
                .key("zoom-level")
                .font(NUM)
                .color(p.tx2)
                .min_w(36.0)
                .cursor(Cursor::Pointer)
                .role("button")
                .help(if zoom < s.zoom { "The window is too small for more zoom. Click to reset to 100% (Ctrl+0)" } else { "Reset zoom to 100% (Ctrl+0)" })
                .on_press(|s: &mut App| s.set_zoom(1.0, true)),
        )
}

fn header(s: &App, zoom: f32, p: &Palette, kit: &Kit, cx: &mut Cx<App>) -> El {
    let scan = s.scan.as_ref().map(|scan| {
        let status = if scan.found > 0 { format!("Scanning… {} files", scan.found) } else { "Scanning…".to_string() };
        text(status).key("scan-status").font(SMALL).color(p.tx3).shrink().truncate()
    });
    let search = TextInput::new(&s.search)
        .placeholder("Search")
        .label("Search")
        .leading_icon(&rune_kit::glyphs::SEARCH)
        .width(Some(260.0))
        .clear_button()
        .on_edit(|s: &mut App| &mut s.search)
        .on_change(|s: &mut App, tx| s.searched(tx))
        .on_action(|s: &mut App, tx| s.clear_search(tx))
        .on_key(|s: &mut App, ev, tx| match ev.key {
            rune::Key::Escape => {
                s.clear_search(tx);
                tx.blur();
                true
            }
            rune::Key::Enter | rune::Key::Down => {
                tx.blur();
                true
            }
            _ => false,
        })
        .build(kit)
        .key("search")
        .radius(2.0)
        // Gives way to the side controls in a narrow (or zoomed-in) window.
        .shrink()
        .min_w(80.0);
    // Equal side widths keep search centred. Reserve at least the wider group's measured
    // width on both sides, allowing search to shrink rather than overlap the tabs.
    let side_min = cx.layout_size("tabs").zip(cx.layout_size("zoom-control")).zip(cx.layout_size("settings"))
        .map(|((tabs, zoom), settings)| tabs[0].max(zoom[0] + 12.0 + settings[0]));
    let mut left = row().h_full().grow(1.0).child(tabs(s, p));
    let mut right = row().grow(1.0).gap(12.0).center_y().justify_end()
        .children(scan)
        .child(zoom_control(s, zoom, p))
        .child(icon_button("settings", &icons::SETTINGS, "Settings", p).role("button").on_press(|s: &mut App| s.show_settings()));
    if let Some(side_min) = side_min {
        left = left.min_w(side_min);
        right = right.min_w(side_min);
    }
    row()
        .key("header")
        .w_full()
        .h(52.0)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .fill(p.bg2)
        .border_bottom(p)
        .child(left)
        .child(search)
        .child(right)
}

/// A header tab's flare: a concave quarter circle from its side border's centre, curving out
/// onto the header's bottom line's centre, with the tab's colour in the corner above the line.
/// The box sits flush with the tab's bottom, its last column the side border, and the
/// right flare mirrors the left.
fn tab_flare(key: String, left: bool, edge: Color, fill: Color) -> El {
    const KAPPA: f32 = 0.552_284_8;
    let r = TAB_FLARE;
    let x = |v: f32| if left { v } else { r + 1.0 - v };
    let (start, end) = ([x(r + 0.5), 0.5], [x(0.5), r + 0.5]);
    let (c1, c2) = ([x(r + 0.5), 0.5 + KAPPA * r], [x(0.5 + KAPPA * r), r + 0.5]);
    let body = Path::new().move_to(start).cubic_to(c1, c2, end).line_to([x(0.5), r + 1.0]).line_to([x(r + 1.0), r + 1.0]).line_to([x(r + 1.0), 0.5]).close();
    let curve = Path::new().move_to(start).cubic_to(c1, c2, end);
    let shape = Shape::new().fill_path(&body, fill).stroke_path(&curve, 1.0, LineCap::Butt, edge);
    // Overlaps the tab's 1-point side border, so the curve starts on its centre.
    let outside = Some(-r);
    let (pin_left, pin_right) = if left { (outside, None) } else { (None, outside) };
    el().key(key).size(r + 1.0, r + 1.0).pin(pin_left, None, pin_right, Some(0.0)).shape(shape)
}

/// The header's tabs: Home, Library (the views, and the pages opened from them) and Graph.
fn tabs(s: &App, p: &Palette) -> El {
    let (home, graph) = (s.in_home(), s.in_graph());
    let ons = [home, !home && !graph, graph];
    // Like Chrome's tabs: the active one is a folder tab joined to the page, the others bare labels on the header.
    let tab = |i: usize, key: &'static str, glyph: &'static Icon, label: &'static str, help: &'static str| {
        let on = ons[i];
        let mut folder = row()
            .key(key)
            .h(TAB_H)
            .px(TAB_PAD)
            .gap(8.0)
            .center_y()
            .cursor(Cursor::Pointer)
            .role("tab")
            .checked(on)
            .label(label)
            .help(help);
        if on {
            let edge = p.ui.alpha(0.7);
            folder = folder
                .corners([TAB_RADIUS, TAB_RADIUS, 0.0, 0.0])
                // No bottom side, so its fill runs down over the header's line.
                .border_sides([1.0, 1.0, 0.0, 1.0], edge)
                .fill(p.bg)
                .child(tab_flare(format!("{key}-flare-left"), true, edge, p.bg))
                .child(tab_flare(format!("{key}-flare-right"), false, edge, p.bg));
        } else {
            // Inside as far as a neighbouring active tab's flare reaches, so it never covers one.
            let (side, ends) = (Some(TAB_FLARE - TAB_GAP), Some(TAB_HOVER_INSET));
            folder = folder
                .hover_group()
                .child(el().key(format!("{key}-hover")).pin(side, ends, side, ends).radius(TAB_RADIUS).fill(p.ui.alpha(0.3)).group_opacity(0.0, 1.0));
        }
        // Between two inactive tabs, as Chrome divides them.
        if !on && ons.get(i + 1) == Some(&false) {
            let middle = Some(-(1.0 + TAB_GAP) / 2.0);
            folder = folder.child(el().key(format!("{key}-divider")).pin(None, Some(11.0), middle, Some(11.0)).w(1.0).fill(p.ui.alpha(0.5)));
        }
        folder
            .child(icon(glyph, 18.0).color(if on { p.ui3 } else { p.tx3 }))
            .child(text(label).font(HEADING).color(if on { p.tx } else { p.tx2 }))
    };
    row()
        .key("tabs")
        .gap(TAB_GAP)
        .self_end()
        .role("tablist")
        .label("Sections")
        .child(tab(0, "home-tab", &icons::HOUSE, "Home", "Recently added and played").on_press(|s: &mut App| s.show_home()))
        .child(tab(1, "library-tab", &icons::NOTE, "Library", "Back to the library's views").on_press(|s: &mut App| s.go_home()))
        .child(tab(2, "graph-tab", &icons::GRAPH, "Graph", "Artists and who they've worked with").on_press(|s: &mut App| s.show_graph()))
}

fn icon_button(key: &'static str, glyph: &'static Icon, help: &str, p: &Palette) -> El {
    el()
        .key(key)
        .size(32.0, 32.0)
        .radius(16.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .label(help.split(" (").next().unwrap_or(help).to_string())
        .help(help.to_string())
        .child(icon(glyph, 18.0).color(p.tx2))
}

fn toggle_icon(key: &'static str, glyph: &'static Icon, on: bool, help: &str, p: &Palette) -> El {
    let mut b = el()
        .key(key)
        .size(32.0, 32.0)
        .radius(16.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .role("switch")
        .checked(on)
        .label(help.split([':', '(']).next().unwrap_or(help).trim().to_string())
        .help(help.to_string())
        .child(icon(glyph, 17.0).color(if on { p.blue } else { p.tx3 }));
    if on {
        b = b.child(el().size(4.0, 4.0).radius(2.0).fill(p.blue).pin(Some(14.0), None, None, Some(1.0)));
    }
    b
}

fn pill_button(key: &'static str, label: &'static str, p: &Palette) -> El {
    el()
        .key(key)
        .h(28.0)
        .px(12.0)
        .radius(14.0)
        .center()
        .fill(p.ui.alpha(0.6))
        .hover(p.ui.alpha(0.5))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .label(label)
        .child(text(label).font(TITLE).color(p.tx))
}

/// `content` in `color`, its `links` (byte ranges and the artist each names) opening that
/// artist's page. Hovering a name underlines it.
fn linked(content: String, links: Vec<(Range<usize>, String)>, color: Color) -> El {
    let mut t = text(content).color(color);
    if links.is_empty() {
        return t;
    }
    for (range, name) in links {
        t = t.link(Link::new(range, name).hover(LinkPaint::new().underline(color)));
    }
    t.on_link(|s: &mut App, id, tx| match collab_names(id) {
        Some(names) => s.show_collab(&names, tx),
        None => s.show_artist(id, tx),
    })
}

/// Starts a link id naming a collaboration rather than one artist; the names follow, each
/// after one.
const COLLAB: char = '\u{1f}';

/// The link id opening the collaboration page of `names`.
pub fn collab_id(names: &[&str]) -> String {
    names.iter().map(|n| format!("{COLLAB}{n}")).collect()
}

/// The artists a [`collab_id`] names; `None` for an artist's own link.
fn collab_names(id: &str) -> Option<Vec<String>> {
    id.starts_with(COLLAB).then(|| id.split(COLLAB).skip(1).map(str::to_string).collect())
}

/// A record label's name, opening its page. Hovering it underlines it.
fn label_link(name: &str, color: Color) -> El {
    let t = text(name.to_string()).color(color).link(Link::new(0..name.len(), name.to_string()).hover(LinkPaint::new().underline(color)));
    t.on_link(|s: &mut App, name, tx| s.show_label(name, tx))
}

/// Song `i`'s album title, opening the album's page. Hovering it underlines it.
fn album_link(title: &str, i: usize, color: Color) -> El {
    let t = text(title.to_string()).color(color).link(Link::new(0..title.len(), format!("album:{i}")).hover(LinkPaint::new().underline(color)));
    t.on_link(move |s: &mut App, _, tx| {
        if let Some(a) = s.album_of(i) {
            s.show_album(a, tx);
        }
    })
}

/// Where each of `names` appears in `content`, as links.
fn name_links(content: &str, names: &[String]) -> Vec<(Range<usize>, String)> {
    credits::find_names(content, names).into_iter().map(|(r, n)| (r, n.to_string())).collect()
}

/// An artist line ("A & B"): each of `names` opens its page, and with two or more found the
/// joiners between them ("&", ",", "x"…) open the page of what they made together.
fn artist_links(content: &str, names: &[String]) -> Vec<(Range<usize>, String)> {
    let found = credits::find_names(content, names);
    let mut links: Vec<(Range<usize>, String)> = found.iter().map(|(r, n)| (r.clone(), n.to_string())).collect();
    if found.len() > 1 {
        let id = collab_id(&found.iter().map(|&(_, n)| n).collect::<Vec<_>>());
        links.extend(credits::joiners(content, &found).into_iter().map(|r| (r, id.clone())));
        links.sort_by_key(|(r, _)| r.start);
    }
    links
}

/// A song's title, the names in its "(feat. C)" opening their pages.
fn song_title(t: &Track, color: Color) -> El {
    let links = credits::feat(&t.title).map_or_else(Vec::new, |(_, names)| {
        name_links(&t.title[names.clone()], &t.artists).into_iter().map(|(r, n)| (r.start + names.start..r.end + names.start, n)).collect()
    });
    linked(t.title.clone(), links, color)
}

/// A song's artist line as tagged ("A & B feat. C"), each name opening its page and the main
/// artists' joiners their collaboration's; `None` when the song names no artist.
fn song_artist(t: &Track, color: Color) -> Option<El> {
    let line = t.artist()?;
    let main = credits::feat(&line).map_or(line.len(), |(part, _)| part.start);
    let mut links = artist_links(&line[..main], &t.artists);
    links.extend(name_links(&line[main..], &t.artists).into_iter().map(|(r, n)| (r.start + main..r.end + main, n)));
    Some(linked(line, links, color))
}

/// Names joined by ", ", each opening its page, with a performer's instruments in brackets
/// after their name: "Elly (keyboard, sampler), AKAI SOLO (vocals)".
fn name_list(people: &[(String, Vec<String>)], color: Color) -> El {
    let mut content = String::new();
    let mut links = Vec::new();
    for (name, details) in people {
        if !content.is_empty() {
            content.push_str(", ");
        }
        links.push((content.len()..content.len() + name.len(), name.clone()));
        content.push_str(name);
        if !details.is_empty() {
            content.push_str(&format!(" ({})", details.join(", ")));
        }
    }
    linked(content, links, color)
}

trait Borders {
    fn border_top(self, p: &Palette) -> El;
    fn border_bottom(self, p: &Palette) -> El;
}

impl Borders for El {
    fn border_top(self, p: &Palette) -> El {
        self.child(el().pin(Some(0.0), Some(0.0), Some(0.0), None).h(1.0).fill(p.ui.alpha(0.7)))
    }
    fn border_bottom(self, p: &Palette) -> El {
        self.child(el().pin(Some(0.0), None, Some(0.0), Some(0.0)).h(1.0).fill(p.ui.alpha(0.7)))
    }
}

/// A column's default width, for tests.
#[cfg(test)]
pub fn table_width(c: crate::query::property::Property) -> f32 {
    table::width(c)
}

pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats_minutes_and_hours() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.9), "1:01");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock(-3.0), "0:00");
    }
}

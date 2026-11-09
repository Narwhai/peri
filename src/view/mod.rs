//! The window: a header with search, the track list, and the now-playing bar.

mod albums;
mod artists;
mod details;
mod filters;
mod labels;
mod list;
mod player;
mod search;
mod settings;
mod table;
mod toolbar;

use std::ops::Range;

use rune::{Color, Cursor, Cx, El, Icon, Link, LinkPaint, Spring, TextStyle, col, el, icon, row, text};
use rune_kit::{Kit, PaletteSeed, TextInput};

use crate::app::App;
use crate::credits;
use crate::icons;
use crate::library::Track;
use crate::palette::Palette;

const ROW_H: f32 = 44.0;
const BAR_H: f32 = 88.0;
const COVER: f32 = 56.0;
const SNAPPY: Spring = Spring::new(0.25, 0.0);

const TITLE: TextStyle = TextStyle::new(13.0, 500.0);
const BODY: TextStyle = TextStyle::new(13.0, 400.0);
const SMALL: TextStyle = TextStyle::new(11.5, 400.0);
const NUM: TextStyle = TextStyle::new(11.5, 400.0).features("'tnum' 1");
const HEADING: TextStyle = TextStyle::new(15.0, 600.0);
const GROUP: TextStyle = TextStyle::new(12.5, 600.0);

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let p = Palette::new(cx.system_dark());
    let kit = Kit::from_palette(PaletteSeed { bg: p.bg2, fg: p.tx, accent: p.blue });
    let mut layer = Vec::new();
    // The view's toolbar does nothing to search results or settings, so they go without it.
    let toolbar = (!matches!(s.page, crate::app::Page::Search | crate::app::Page::Settings)).then(|| toolbar::toolbar(s, &p, &kit, &mut layer));
    let body = col()
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(header(s, &p, &kit))
        .children(toolbar)
        .child(match (s.open_album(), s.open_artist(), s.open_label()) {
            (Some(a), _, _) => albums::album_page(s, a, &p),
            (None, Some(name), _) => artists::artist_page(s, name, &p),
            (None, None, Some(name)) => labels::label_page(s, name, &p),
            (None, None, None) if s.page == crate::app::Page::Search => search::search_page(s, &p),
            (None, None, None) if s.page == crate::app::Page::Settings => settings::settings_page(s, &p),
            (None, None, None) => list::list(s, &p, cx),
        })
        .child(player::now_playing(s, &p))
        .on_drop_files(|s: &mut App, files, _tx| {
            s.drop_hover = false;
            s.add_to_library(files.to_vec());
        })
        .accepts_files(|path| path.is_dir() || crate::library::is_audio(path))
        .on_drop_hover(|s: &mut App, over, _tx| s.drop_hover = over);
    let drop_veil = s.drop_hover.then(|| {
        el().key("drop-veil")
            .pin(Some(8.0), Some(8.0), Some(8.0), Some(8.0))
            .radius(12.0)
            .border(2.0, p.blue)
            .fill(p.blue.alpha(0.08))
            .center()
            .child(text("Drop to add to the library").font(HEADING).color(p.tx))
            .inert()
    });
    layer.extend(drop_veil);
    details::details_dialog(s, &p, &kit, &mut layer);
    rune_kit::root(body, layer)
}

fn header(s: &App, p: &Palette, kit: &Kit) -> El {
    let count = match (&s.scan, s.tracks.len()) {
        (Some(scan), _) if scan.found > 0 => format!("Scanning… {} files", scan.found),
        (Some(_), _) => "Scanning…".to_string(),
        (None, 0) => String::new(),
        (None, _) if s.mode() == crate::base::property::Mode::Albums => match (s.shown_albums.len(), s.albums.len()) {
            (shown, all) if shown == all => plural(all, "album"),
            (shown, all) => format!("{shown} of {}", plural(all, "album")),
        },
        (None, n) if s.filtered.len() == n => plural(n, "track"),
        (None, n) => format!("{} of {}", s.filtered.len(), plural(n, "track")),
    };
    let search = TextInput::new(&s.search)
        .placeholder("Search artists, albums, songs")
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
        .key("search");
    row()
        .key("header")
        .w_full()
        .h(52.0)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .fill(p.bg2)
        .border_bottom(p)
        .child(
            row()
                .key("home")
                .h(32.0)
                .pl(6.0)
                .pr(10.0)
                .gap(8.0)
                .radius(8.0)
                .center_y()
                .hover(p.ui.alpha(0.45))
                .cursor(Cursor::Pointer)
                .role("button")
                .label("Library")
                .help("Back to the library's views")
                .on_press(|s: &mut App| s.go_home())
                .child(icon(&icons::NOTE, 18.0).color(p.ui3))
                .child(text("Library").font(HEADING).color(p.tx)),
        )
        .child(text(count).key("count").font(SMALL).color(p.tx3).grow(1.0).shrink().truncate())
        .child(search)
        .child(pill_button("open", "Open folder…", p).on_press_tx(|s: &mut App, tx| s.open_folder(tx)))
        .child(icon_button("settings", &icons::SETTINGS, "Settings", p).role("button").on_press(|s: &mut App| s.show_settings()))
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
    t.on_link(|s: &mut App, name, tx| s.show_artist(name, tx))
}

/// A record label's name, opening its page. Hovering it underlines it.
fn label_link(name: &str, color: Color) -> El {
    let t = text(name.to_string()).color(color).link(Link::new(0..name.len(), name.to_string()).hover(LinkPaint::new().underline(color)));
    t.on_link(|s: &mut App, name, tx| s.show_label(name, tx))
}

/// Where each of `names` appears in `content`, as links.
fn name_links(content: &str, names: &[String]) -> Vec<(Range<usize>, String)> {
    credits::find_names(content, names).into_iter().map(|(r, n)| (r, n.to_string())).collect()
}

/// A song's title, the names in its "(feat. C)" opening their pages.
fn song_title(t: &Track, color: Color) -> El {
    let links = credits::feat(&t.title).map_or_else(Vec::new, |(_, names)| {
        name_links(&t.title[names.clone()], &t.artists).into_iter().map(|(r, n)| (r.start + names.start..r.end + names.start, n)).collect()
    });
    linked(t.title.clone(), links, color)
}

/// A song's artist line as tagged ("A & B feat. C"), each name opening its page; `None` when
/// the song names no artist.
fn song_artist(t: &Track, color: Color) -> Option<El> {
    let line = t.artist()?;
    let links = name_links(&line, &t.artists);
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
pub fn table_width(c: crate::base::property::Property) -> f32 {
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

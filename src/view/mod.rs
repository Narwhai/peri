//! The window: a header with search, the track list, and the now-playing bar.

mod filters;
mod list;
mod player;
mod toolbar;

use rune::{Cursor, Cx, El, Icon, Spring, TextStyle, col, el, icon, row, text};
use rune_kit::{Kit, PaletteSeed, TextInput};

use crate::app::App;
use crate::icons;
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
    let toolbar = toolbar::toolbar(s, &p, &kit, &mut layer);
    let body = col()
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(header(s, &p, &kit))
        .child(toolbar)
        .child(list::list(s, &p))
        .child(player::now_playing(s, &p))
        .on_drop_files(|s: &mut App, files, _tx| {
            s.drop_hover = false;
            s.start_scan(files.to_vec(), true);
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
    rune_kit::root(body, layer)
}

fn header(s: &App, p: &Palette, kit: &Kit) -> El {
    let count = match (&s.scan, s.tracks.len()) {
        (Some(scan), _) if scan.found > 0 => format!("Scanning… {} files", scan.found),
        (Some(_), _) => "Scanning…".to_string(),
        (None, 0) => String::new(),
        (None, n) if s.filtered.len() == n => plural(n, "track"),
        (None, n) => format!("{} of {}", s.filtered.len(), plural(n, "track")),
    };
    let search = TextInput::new(&s.search)
        .placeholder("Search title, artist, album")
        .label("Search")
        .leading_icon(&rune_kit::glyphs::SEARCH)
        .width(Some(260.0))
        .clear_button()
        .on_edit(|s: &mut App| &mut s.search)
        .on_change(|s: &mut App, _tx| s.refilter())
        .on_key(|s: &mut App, ev, tx| match ev.key {
            rune::Key::Escape => {
                s.search.clear();
                s.refilter();
                tx.blur();
                true
            }
            rune::Key::Down => {
                tx.blur();
                s.move_selection(1, tx);
                true
            }
            rune::Key::Enter => {
                tx.blur();
                if let Some(&i) = s.filtered.first() {
                    s.play_index(i, tx);
                }
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
        .child(icon(&icons::NOTE, 18.0).color(p.ui3))
        .child(text("Library").font(HEADING).color(p.tx))
        .child(text(count).key("count").font(SMALL).color(p.tx3).grow(1.0).shrink().truncate())
        .child(search)
        .child(pill_button("open", "Open folder…", p).on_press_tx(|s: &mut App, tx| s.open_folder(tx)))
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

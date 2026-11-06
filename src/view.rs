//! The window: a header with search, the track list, and the now-playing bar.

use rune::{Align, Cursor, Cx, Drag, El, GrowthSpec, Icon, Phase, ScrollSpec, Spring, SurfaceFit, TextStyle, Tx, col, el, icon, memo, row, text};
use rune_kit::{Kit, PaletteSeed, TextInput};

use crate::app::{App, SEEK_STEP, row_key};
use crate::icons;
use crate::library::{AUDIO_EXTENSIONS, Track};
use crate::palette::Palette;
use crate::queue::Repeat;

const ROW_H: f32 = 44.0;
const BAR_H: f32 = 88.0;
const COVER: f32 = 56.0;
const SNAPPY: Spring = Spring::new(0.25, 0.0);

const TITLE: TextStyle = TextStyle::new(13.0, 500.0);
const BODY: TextStyle = TextStyle::new(13.0, 400.0);
const SMALL: TextStyle = TextStyle::new(11.5, 400.0);
const NUM: TextStyle = TextStyle::new(11.5, 400.0).features("'tnum' 1");
const HEADING: TextStyle = TextStyle::new(15.0, 600.0);

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let p = Palette::new(cx.system_dark());
    let kit = Kit::from_palette(PaletteSeed { bg: p.bg2, fg: p.tx, accent: p.blue });
    let body = col()
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(header(s, &p, &kit))
        .child(list(s, &p))
        .child(now_playing(s, &p))
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
    rune_kit::root(body, drop_veil)
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

fn list(s: &App, p: &Palette) -> El {
    if s.tracks.is_empty() {
        return empty_state(s, p);
    }
    if s.filtered.is_empty() {
        return col()
            .key("no-results")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(6.0)
            .child(text("No matches").font(HEADING).color(p.tx2))
            .child(text(format!("Nothing in the library matches “{}”.", s.search.text.trim())).font(BODY).color(p.tx3));
    }
    let p = *p;
    let rows = col()
        .key("rows")
        .w_full()
        .px(8.0)
        .py(6.0)
        .growth(GrowthSpec::default())
        .virtual_rows(
            s.filtered.len(),
            ROW_H,
            |s: &App, n| row_key(s.filtered[n]).into(),
            move |s: &App, n| {
                let i = s.filtered[n];
                let selected = s.selected == Some(i);
                let current = s.current == Some(i);
                let playing = current && s.playing;
                let dark = p.dark;
                memo(row_key(i), (s.list_version, n, selected, current, playing, dark), || track_row(&s.tracks[i], i, n, selected, current, playing, &p))
            },
        )
        .virtual_rows_version(s.list_version);
    col()
        .key("tracks")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_margin(6.0)
        .child(rows)
}

fn track_row(t: &Track, i: usize, n: usize, selected: bool, current: bool, playing: bool, p: &Palette) -> El {
    let lead: El = if current {
        icon(if playing { &icons::BARS } else { &icons::PAUSE }, 14.0).color(p.blue)
    } else {
        text(format!("{}", n + 1)).font(NUM).color(p.tx3).align(Align::End).w(28.0)
    };
    let title_color = if current { p.blue } else { p.tx };
    let subtitle = match (&t.artist, &t.album) {
        (Some(a), Some(b)) => format!("{a} · {b}"),
        (Some(a), None) => a.clone(),
        (None, Some(b)) => b.clone(),
        (None, None) => "Unknown artist".into(),
    };
    let fill = if selected { p.ui.alpha(if p.dark { 0.7 } else { 0.55 }) } else { p.bg.alpha(0.0) };
    row()
        .key(row_key(i))
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
        .label(format!("{} by {}", t.title, t.artist.as_deref().unwrap_or("unknown artist")))
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

fn now_playing(s: &App, p: &Palette) -> El {
    let track = s.current_track();
    let cover = el().key("cover").size(COVER, COVER).radius(8.0).fill(p.ui).center();
    let cover = match s.cover.as_ref().filter(|c| c.error().is_none()) {
        Some(image) => cover.pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => cover.child(icon(&icons::NOTE, 22.0).color(p.ui2)),
    };
    let (title, artist) = match track {
        Some(t) => (t.title.clone(), t.artist.clone().unwrap_or_else(|| "Unknown artist".into())),
        None => ("Not playing".into(), "Double-click a track to play it".into()),
    };
    let info = col()
        .w(220.0)
        .shrink()
        .gap(3.0)
        .child(text(title).key("np-title").font(TITLE).color(p.tx).truncate())
        .child(text(artist).key("np-artist").font(SMALL).color(p.tx2).truncate());
    let left = row().w(300.0).shrink().gap(12.0).center_y().child(cover).child(info);

    let shuffle = toggle_icon("shuffle", &icons::SHUFFLE, s.shuffle, "Shuffle (S)", p).on_press(|s: &mut App| s.toggle_shuffle());
    let repeat_icon = if s.repeat == Repeat::One { &icons::REPEAT_ONE } else { &icons::REPEAT };
    let repeat_help = match s.repeat {
        Repeat::Off => "Repeat: off (R)",
        Repeat::All => "Repeat: all (R)",
        Repeat::One => "Repeat: one (R)",
    };
    let repeat = toggle_icon("repeat", repeat_icon, s.repeat != Repeat::Off, repeat_help, p).on_press(|s: &mut App| s.cycle_repeat());
    let play = el()
        .key("play")
        .size(38.0, 38.0)
        .radius(19.0)
        .fill(p.tx)
        .center()
        .hover(p.bg.alpha(0.12))
        .press_scale(0.94)
        .cursor(Cursor::Pointer)
        .label(if s.playing { "Pause" } else { "Play" })
        .help("Play/Pause (Space)")
        .on_press_tx(|s: &mut App, tx| s.toggle(tx))
        .child(icon(if s.playing { &icons::PAUSE } else { &icons::PLAY }, 18.0).color(p.bg));
    let controls = row()
        .gap(14.0)
        .center_y()
        .child(shuffle)
        .child(icon_button("prev", &icons::PREV, "Previous (Ctrl+←)", p).on_press_tx(|s: &mut App, tx| s.prev(tx)))
        .child(play)
        .child(icon_button("next", &icons::NEXT, "Next (Ctrl+→)", p).on_press_tx(|s: &mut App, tx| s.next(tx)))
        .child(repeat);
    let center = col().grow(1.0).shrink().min_w(260.0).max_w(560.0).gap(6.0).items_center().child(controls).child(seek_bar(s, p));

    let volume_icon = if s.muted || s.volume == 0.0 { &icons::MUTED } else { &icons::VOLUME };
    let right = row()
        .w(300.0)
        .shrink()
        .gap(8.0)
        .center_y()
        .justify_end()
        .child(icon_button("mute", volume_icon, "Mute (M)", p).on_press(|s: &mut App| s.toggle_mute()))
        .child(volume_slider(s, p));

    let error = s.error.as_ref().map(|e| {
        row()
            .key("error")
            .w_full()
            .px(16.0)
            .py(6.0)
            .gap(8.0)
            .center_y()
            .fill(p.red.alpha(0.12))
            .child(text(e.clone()).font(SMALL).color(p.red).grow(1.0).shrink().truncate())
            .child(text("Dismiss").key("dismiss").font(SMALL).color(p.tx2).cursor(Cursor::Pointer).on_press(|s: &mut App| s.error = None))
    });
    col()
        .key("now-playing")
        .w_full()
        .fill(p.bg2)
        .border_top(p)
        .children(error)
        .child(row().w_full().h(BAR_H).px(16.0).gap(16.0).center_y().justify_between().child(left).child(center).child(right))
}

/// The progress bar, which seeks on click and drag and with the arrow keys when focused.
fn seek_bar(s: &App, p: &Palette) -> El {
    let duration = s.duration();
    let position = s.shown_position();
    let fraction = duration.filter(|d| *d > 0.0).map_or(0.0, |d| (position / d).clamp(0.0, 1.0) as f32);
    let enabled = s.current.is_some() && duration.is_some();
    let track = slider_track("seek", fraction, enabled, p)
        .a11y_value(clock(position))
        .on_step(|s: &mut App, steps, _tx| s.seek_by(steps as f64 * SEEK_STEP))
        .on_drag(|s: &mut App, d: &mut Drag, _tx: &mut Tx| {
            if s.duration().is_none() {
                return;
            }
            let f = (d.x / d.w).clamp(0.0, 1.0);
            match d.phase {
                Phase::Down | Phase::Move => s.seek_preview = Some(f),
                Phase::Up => {
                    let preview = s.seek_preview.take();
                    if let (false, Some(f), Some(dur)) = (d.cancelled(), preview, s.duration()) {
                        s.seek(f as f64 * dur);
                    }
                }
            }
        });
    row()
        .w_full()
        .gap(10.0)
        .center_y()
        .child(text(clock(position)).key("elapsed").font(NUM).color(p.tx3).w(40.0).align(Align::End))
        .child(track.grow(1.0))
        .child(text(duration.map(clock).unwrap_or_else(|| "–:––".into())).key("total").font(NUM).color(p.tx3).w(40.0))
}

fn volume_slider(s: &App, p: &Palette) -> El {
    let shown = if s.muted { 0.0 } else { s.volume };
    slider_track("volume", shown, true, p)
        .w(110.0)
        .label("Volume")
        .a11y_value(format!("{}%", (shown * 100.0).round()))
        .on_step(|s: &mut App, steps, _tx| s.set_volume(s.volume + steps as f32 * 0.05))
        .on_drag(|s: &mut App, d: &mut Drag, _tx: &mut Tx| match d.phase {
            Phase::Down => {
                d.data = s.volume;
                s.set_volume(d.x / d.w);
            }
            Phase::Move => s.set_volume(d.x / d.w),
            Phase::Up if d.cancelled() => s.set_volume(d.data),
            Phase::Up => {}
        })
}

/// A 4 pt bar with a fill and a knob, in a taller hit area.
fn slider_track(key: &'static str, fraction: f32, enabled: bool, p: &Palette) -> El {
    let fill = if enabled { p.tx2 } else { p.ui };
    el()
        .key(key)
        .role("slider")
        .label(key)
        .h(16.0)
        .min_w(40.0)
        .center_y()
        .cursor(if enabled { Cursor::Pointer } else { Cursor::Default })
        .child(
            el().key("bar").w_full().h(4.0).radius(2.0).fill(p.ui).child(
                // The knob rides the fill's end, centred on it.
                el().key("fill").w_frac(fraction).h(4.0).radius(2.0).fill(fill).motion(SNAPPY).child(
                    el().key("knob")
                        .size(12.0, 12.0)
                        .radius(6.0)
                        .fill(p.tx)
                        .opacity(if enabled { 1.0 } else { 0.0 })
                        .pin(None, Some(-4.0), Some(-6.0), None)
                        .motion(SNAPPY),
                ),
            ),
        )
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

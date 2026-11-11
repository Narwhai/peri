//! The now-playing bar: cover, title, transport, seek bar and volume.

use rune::{Align, Cursor, Drag, El, Phase, SurfaceFit, Tx, col, el, icon, row, text};

use super::*;
use crate::app::{App, SEEK_STEP};
use crate::icons;
use crate::palette::Palette;
use crate::queue::Repeat;

pub(super) fn now_playing(s: &App, p: &Palette) -> El {
    let track = s.current_track();
    let cover = el().key("cover").size(COVER, COVER).fill(p.ui).center();
    let mut cover = match s.cover.as_ref().filter(|c| c.error().is_none()) {
        Some(image) => cover.pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => cover.child(icon(&icons::NOTE, 22.0).color(p.ui2)),
    };
    if let Some(a) = s.current.and_then(|c| s.album_of(c)) {
        cover = cover
            .cursor(Cursor::Pointer)
            .label("Show album")
            .help("Show album")
            .press_scale(0.96)
            .on_press_tx(move |s: &mut App, tx| s.show_album(a, tx));
    }
    let (title, artist) = match track {
        Some(t) => (song_title(t, p.tx), song_artist(t, p.tx2).unwrap_or_else(|| text("Unknown artist").color(p.tx2))),
        None => (text("Not playing").color(p.tx), text("Double-click a track to play it").color(p.tx2)),
    };
    let mut title = title.key("np-title").font(TITLE).truncate();
    if let Some(a) = s.current.and_then(|c| s.album_of(c)) {
        title = title.cursor(Cursor::Pointer).label("Show album").help("Show album").on_press_tx(move |s: &mut App, tx| s.show_album(a, tx));
    }
    let info = col()
        .w(220.0)
        .shrink()
        .gap(3.0)
        .child(title)
        .child(artist.key("np-artist").font(SMALL).truncate());
    let details = s.current.map(|i| {
        el().key("np-details")
            .size(28.0, 28.0)
            .radius(14.0)
            .center()
            .hover(p.ui.alpha(0.6))
            .cursor(Cursor::Pointer)
            .label("Song details")
            .help("Song details (I)")
            .on_press_tx(move |s: &mut App, tx| s.show_details(i, tx))
            .child(icon(&icons::INFO, 16.0).color(p.tx3))
    });
    let left = row().w(300.0).shrink().gap(12.0).center_y().child(cover).child(info).children(details);

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
pub(super) fn slider_track(key: &'static str, fraction: f32, enabled: bool, p: &Palette) -> El {
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

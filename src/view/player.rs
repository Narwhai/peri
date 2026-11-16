//! The now-playing bar: cover, title, transport, seek bar and volume.

use std::f32::consts::PI;

use rune::{Align, Color, Cursor, Drag, El, LineCap, Phase, Shadow, Shape, SurfaceFit, Tx, canvas, col, el, icon, row, text};

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
    // Like the album page's Play: raised in the accent while stopped or paused, pressed in and
    // darker as Pause while playing.
    let down = s.playing;
    let shadow = if down {
        Shadow { y: 2.0, blur: 5.0, color: Color::BLACK.alpha(0.45), inset: true, ..Shadow::default() }
    } else {
        Shadow { y: 1.0, blur: 2.0, color: Color::BLACK.alpha(0.25), ..Shadow::default() }
    };
    let play = el()
        .key("play")
        .size(38.0, 38.0)
        .radius(19.0)
        .fill(if down { p.accent.mix(Color::BLACK, 0.35) } else { p.accent })
        .box_shadow(shadow)
        .center()
        .hover(Color::WHITE.alpha(0.12))
        .press(Color::BLACK.alpha(0.12))
        .press_scale(0.94)
        .cursor(Cursor::Pointer)
        .label(if s.playing { "Pause" } else { "Play" })
        .help("Play/Pause (Space)")
        .on_press_tx(|s: &mut App, tx| s.toggle(tx))
        .child(icon(if s.playing { &icons::PAUSE } else { &icons::PLAY }, 18.0).color(p.on_accent));
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
    let track = fader("seek", fraction, enabled, p)
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

/// Points of vertical drag that sweep the volume knob across its whole 0 to 1 range.
pub(crate) const VOLUME_DRAG: f32 = 150.0;
/// The knob's diameter, and the radius of its 270° sweep track.
const KNOB: f32 = 38.0;
const SWEEP_R: f32 = 15.0;
/// The sweep starts at 135° (7:30, clockwise from 3 o'clock in screen space) and spans 270°.
const SWEEP_START: f32 = 3.0 * PI / 4.0;
const SWEEP: f32 = 3.0 * PI / 2.0;

/// The volume as a rotary knob: drag up to turn it up, down to turn it down. While the button
/// is held the cursor stays still and hidden, so the knob turns as far as the mouse moves, not
/// only as far as the screen's edge.
fn volume_slider(s: &App, p: &Palette) -> El {
    let shown = if s.muted { 0.0 } else { s.volume };
    let c = KNOB / 2.0;
    let mut arcs = Shape::new().stroke_with(sweep(c, SWEEP_START, SWEEP), false, 2.5, LineCap::Round, p.ui);
    if shown > 0.0 {
        arcs = arcs.stroke_with(sweep(c, SWEEP_START, SWEEP * shown), false, 2.5, LineCap::Round, p.accent);
    }
    // The indicator points up at half volume; the body turns from there.
    let turn = (shown - 0.5) * SWEEP;
    let body = el()
        .key("volume-body")
        .size(26.0, 26.0)
        .radius(13.0)
        .pin(Some(c - 13.0), Some(c - 13.0), None, None)
        .gradient(p.ui, p.bg2)
        .border(1.0, p.ui2.alpha(0.5))
        .shadow(1.5, 4.0, Color::BLACK.alpha(0.4))
        .rotate(turn)
        .rotate_motion(SNAPPY)
        .child(el().size(3.0, 8.0).radius(1.5).fill(p.tx).pin(Some(11.5), Some(3.0), None, None));
    el().key("volume")
        .role("slider")
        .label("Volume")
        .size(KNOB, KNOB)
        .cursor(Cursor::ResizeRow)
        .a11y_value(format!("{}%", (shown * 100.0).round()))
        .on_step(|s: &mut App, steps, _tx| s.set_volume(s.volume + steps as f32 * 0.05))
        // The press keeps where the volume started, so Escape puts it back.
        .on_drag(|s: &mut App, d: &mut Drag, _tx: &mut Tx| match d.phase {
            Phase::Down => {
                d.data = if s.muted { 0.0 } else { s.volume };
            }
            Phase::Up if d.cancelled() => s.set_volume(d.data),
            Phase::Move | Phase::Up => {}
        })
        .pointer_lock_while_held(|s: &mut App, [_, dy]: [f32; 2], _tx: &mut Tx| {
            let shown = if s.muted { 0.0 } else { s.volume };
            s.set_volume(shown - dy / VOLUME_DRAG);
        })
        .child(canvas(arcs).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)))
        .child(body)
}

/// Points along the circle of radius `SWEEP_R` about `c`, from angle `from` through `span` radians.
fn sweep(c: f32, from: f32, span: f32) -> Vec<[f32; 2]> {
    let steps = ((span.abs() / (PI / 48.0)).ceil() as usize).max(1);
    (0..=steps)
        .map(|i| {
            let a = from + span * i as f32 / steps as f32;
            [c + SWEEP_R * a.cos(), c + SWEEP_R * a.sin()]
        })
        .collect()
}

/// The seek bar as a hardware fader: tick notches above a recessed slot, and a tall cap at the played fraction
/// that stands past the slot's top and bottom.
/// The hit area is the track's full width, so a press at `d.x / d.w` lands where the ticks say.
fn fader(key: &'static str, fraction: f32, enabled: bool, p: &Palette) -> El {
    let fill = if enabled { p.tx2 } else { p.ui };
    let slot = el()
        .key("bar")
        .pin(Some(0.0), Some(SLOT_TOP), Some(0.0), Some(FADER_H - SLOT_TOP - SLOT_H))
        .radius(4.0)
        .fill(p.bg)
        .border(1.0, p.ui.alpha(0.5))
        .box_shadow(Shadow { y: 1.0, blur: 3.0, color: Color::BLACK.alpha(0.5), inset: true, ..Shadow::default() })
        .child(
            el().key("fill").w_frac(fraction).h_full().radius(4.0).fill(fill).motion(SNAPPY).child(
                // The cap rides the fill's end, centred on it.
                el().key("knob")
                    .size(CAP_W, CAP_H)
                    .radius(3.0)
                    .linear_gradient(0.0, [(0.0, p.ui2), (0.5, p.tx), (1.0, p.ui2)])
                    .border(1.0, p.bg.alpha(0.6))
                    .shadow(1.0, 3.0, Color::BLACK.alpha(0.45))
                    .opacity(if enabled { 1.0 } else { 0.0 })
                    .pin(None, Some((SLOT_H - CAP_H) / 2.0), Some(-CAP_W / 2.0), None)
                    .motion(SNAPPY)
                    // The grip line across the cap's middle, like a fader's index mark.
                    .child(canvas(Shape::new().line([2.5, CAP_H / 2.0], [CAP_W - 2.5, CAP_H / 2.0], 1.5, LineCap::Round, p.bg.alpha(0.7))).size(CAP_W, CAP_H).pin(Some(0.0), Some(0.0), None, None)),
            ),
        );
    el().key(key)
        .role("slider")
        .label(key)
        .h(FADER_H)
        .min_w(40.0)
        .center_y()
        .cursor(if enabled { Cursor::Pointer } else { Cursor::Default })
        .child(slot)
        .child(canvas(fader_ticks(p)).pin(Some(0.0), Some(SLOT_TOP - TICKS_GAP - TICKS_H), Some(0.0), None).h(TICKS_H))
}

/// The seek fader's geometry in points: the ticks on top, the slot under them, and the cap
/// standing past the slot by the same amount above and below.
const TICKS_H: f32 = 7.0;
const SLOT_H: f32 = 8.0;
const CAP_W: f32 = 12.0;
const CAP_H: f32 = 22.0;
/// The ticks end just above the cap's top, so the cap never covers them.
const TICKS_GAP: f32 = (CAP_H - SLOT_H) / 2.0 + 1.0;
const SLOT_TOP: f32 = TICKS_H + TICKS_GAP;
const FADER_H: f32 = SLOT_TOP + (SLOT_H + CAP_H) / 2.0;

/// Notches in a 100-unit wide box, rising from its bottom edge: short at every 10%, medium at
/// 25% and 75%, tallest at 50%.
fn fader_ticks(p: &Palette) -> Shape {
    let notch = |shape: Shape, x: f32, len: f32| shape.line([x, TICKS_H - len], [x, TICKS_H], 1.0, LineCap::Butt, p.tx3);
    let mut shape = Shape::new().view_box(100.0, TICKS_H);
    for i in 0..=10 {
        let x = i as f32 * 10.0;
        shape = notch(shape, x, if i == 5 { 8.0 } else { 3.0 });
    }
    shape = notch(shape, 25.0, 5.0);
    notch(shape, 75.0, 5.0)
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

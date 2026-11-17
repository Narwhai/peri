//! The play queue: the panel that lists it (floating over the page, or docked beside it), the
//! question an edited queue asks before a new list replaces it, and the Settings choice.

use rune::{Align, Color, Cursor, El, GrowthSpec, ScrollSpec, col, el, icon, row, text};
use rune_kit::{Checkbox, Dialog, Kit};

use super::toolbar::{GRIP_W, is_dragged, reorder_handle, reorder_shift, segmented, small_icon_button};
use super::*;
use super::albums::{cover, cover_image};
use crate::app::{App, QUEUE_LIST_KEY, QUEUE_ROW_H, QueueChoice, ReorderList};
use crate::icons;
use crate::palette::Palette;
use crate::queue::{QueueReplace, Repeat};

/// The panel's width, in points.
const PANEL_W: f32 = 360.0;
/// The fixed slots of a row's right side: the Added marker, the duration and the remove button.
const ADDED_W: f32 = 52.0;
const DURATION_W: f32 = 46.0;
const REMOVE_W: f32 = 26.0;
/// A row's album cover.
const COVER: f32 = 36.0;
/// The gap between the floating panel and the window's edges.
pub(super) const PANEL_GAP: f32 = 12.0;

/// The panel: a header, then the rows (or the empty state). It floats over the page unless
/// pinned, when `view` docks it beside the page instead.
pub(super) fn panel(s: &App, p: &Palette) -> El {
    let upcoming = s.queue.as_ref().map_or(0, |q| q.upcoming().len());
    let count = match upcoming {
        0 => "Nothing up next".to_string(),
        n => format!("{n} up next"),
    };
    let pin_help = if s.queue_pinned { "Unpin the queue" } else { "Pin the queue beside the songs" };
    let pin = small_icon_button("queue-pin".into(), &icons::PIN, pin_help, p)
        .role("switch")
        .checked(s.queue_pinned)
        .on_press_tx(|s: &mut App, tx| {
            let pinned = !s.queue_pinned;
            s.set_queue_pinned(pinned, tx);
        });
    let close = small_icon_button("queue-close".into(), &rune_kit::glyphs::CLOSE, "Close the queue (Q)", p).on_press(|s: &mut App| s.queue_shown = false);
    let head = row()
        .key("queue-header")
        .w_full()
        .h(48.0)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .child(text("Queue").key("queue-title").font(HEADING).color(p.tx))
        .child(text(count).key("queue-count").font(SMALL).color(p.tx3).grow(1.0).truncate())
        .child(pin)
        .child(close);
    let panel = col().key("queue-panel").w(PANEL_W).h_full().fill(p.bg2).child(head.border_bottom(p)).child(body(s, p));
    if s.queue_pinned {
        panel.shrink().child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).w(1.0).fill(p.ui.alpha(0.7)))
    } else {
        panel.radius(6.0).border(1.0, p.ui.alpha(0.7)).shadow(0.0, 10.0, Color::BLACK.alpha(0.28))
    }
}

/// The rows, or the empty state when nothing is queued.
fn body(s: &App, p: &Palette) -> El {
    let Some(queue) = s.queue.as_ref() else {
        return col()
            .key("queue-empty")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(8.0)
            .px(24.0)
            .child(icon(&icons::QUEUE, 32.0).color(p.ui2))
            .child(text("Nothing queued.").font(HEADING).color(p.tx2))
            .child(text("Double-click a song to start playing.").font(BODY).color(p.tx3).wrap().align(Align::Center));
    };
    // With Repeat::All, one more row after the last entry says the queue starts over.
    let repeats = s.repeat == Repeat::All;
    let count = queue.entries().len() + usize::from(repeats);
    let p = *p;
    let rows = col()
        .key("queue-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            QUEUE_ROW_H,
            |s: &App, n| match s.queue.as_ref() {
                Some(q) if n < q.entries().len() => format!("queue-row{n}").into(),
                _ => "queue-repeat".into(),
            },
            move |s: &App, n| match s.queue.as_ref() {
                Some(q) if n < q.entries().len() => queue_row(s, n, &p),
                _ => repeat_note(&p),
            },
        )
        .virtual_rows_version(s.list_version);
    col().key(QUEUE_LIST_KEY).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows)
}

/// The last row when the queue repeats: it sits where the next song would, but says so.
fn repeat_note(p: &Palette) -> El {
    row().w_full().h(QUEUE_ROW_H).center().child(text("Then repeats from the start").font(SMALL).color(p.tx3))
}

/// Entry `n` of the queue: its grip (upcoming only), title and artist, duration, an Added mark,
/// and a remove button. Played entries are muted; the current one is accented.
fn queue_row(s: &App, n: usize, p: &Palette) -> El {
    let Some(queue) = s.queue.as_ref() else { return el() };
    let Some(entry) = queue.entries().get(n).copied() else { return el() };
    let t = &s.tracks()[entry.track];
    let pos = queue.pos();
    let current = n == pos;
    let played = n < pos;
    let upcoming = n > pos;
    let ink = if current { p.accent } else if played { p.tx3 } else { p.tx };
    let sub = if played { p.tx3 } else { p.tx2 };

    // The album cover; the current entry's is dimmed under the playing or paused icon. Not
    // `song_cover`: a track queued twice would mark both entries.
    let image = s.album_of(entry.track).and_then(|a| cover_image(s, &s.albums()[a]));
    let mut art = cover(format!("queue-row{n}-cover"), image, Some(COVER), p);
    if current {
        art = art.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(Color::BLACK.alpha(0.55)).center()
                .child(icon(if s.playing { &icons::BARS } else { &icons::PAUSE }, 14.0).color(Color::WHITE)),
        );
    }
    let grip = if upcoming {
        reorder_handle(format!("queue-row{n}-handle"), ReorderList::Queue, n, p)
    } else {
        el().w(GRIP_W)
    };
    let shift = reorder_shift(s, ReorderList::Queue, n);
    let mut info = col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(
        text(t.title.clone()).font(BODY).color(ink).truncate(),
    );
    if let Some(artist) = t.artist() {
        info = info.child(text(artist).font(SMALL).color(sub).truncate());
    }
    let mut r = row()
        .key(format!("queue-row{n}"))
        .w_full()
        .h(QUEUE_ROW_H)
        .pl(4.0)
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .offset(0.0, shift)
        .fill(if current { p.ui.alpha(0.35) } else { p.bg2.alpha(0.0) })
        .hover(p.ui.alpha(0.3))
        .hover_group()
        .role("row")
        .label(format!("{} by {}", t.title, t.artist().as_deref().unwrap_or("unknown artist")))
        .cursor(Cursor::Pointer)
        .on_double_press_tx(move |s: &mut App, tx| s.queue_jump(n, tx))
        .child(el().w(GRIP_W).center().child(grip))
        .child(art)
        .child(info);
    // Fixed slots, so the durations and remove buttons line up down the list.
    let marker = if entry.added { text("Added").key(format!("queue-row{n}-added")).font(SMALL).color(p.accent) } else { el() };
    r = r.child(el().w(ADDED_W).center_y().child(marker));
    r = r.child(text(clock(t.duration.unwrap_or(0.0))).font(NUM).color(sub).align(Align::End).w(DURATION_W));
    let remove = (!current).then(|| {
        small_icon_button(format!("queue-row{n}-remove"), &rune_kit::glyphs::CLOSE, "Remove from the queue", p).on_press(move |s: &mut App| s.queue_remove(n))
    });
    r = r.child(el().w(REMOVE_W).center().children(remove));
    if is_dragged(s, ReorderList::Queue, n) {
        r = r.fill(p.ui.alpha(0.5));
    }
    r
}

/// The replace prompt, while a new list waits on the answer to an edited queue. Closing it, or
/// Escape, is Cancel.
pub(super) fn replace_dialog(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) {
    if s.pending_play.is_none() {
        return;
    }
    let keep = s.pending_has_added();
    let check = row().key("queue-dont-ask").child(
        Checkbox::new(s.queue_dont_ask)
            .label("Don't ask again")
            .on_toggle(|s: &mut App| s.queue_dont_ask = !s.queue_dont_ask)
            .build(kit),
    );
    let note = text("The queue has songs you added or moved. Starting a new list replaces the upcoming songs.").font(BODY).color(p.tx2).wrap();
    let mut buttons = row().key("queue-replace-buttons").w_full().gap(8.0).justify_end();
    buttons = buttons
        .child(pill_button("queue-cancel", "Cancel", p).on_press_tx(|s: &mut App, tx| s.resolve_pending_play(QueueChoice::Cancel, false, tx)));
    if keep {
        buttons = buttons.child(pill_button("queue-keep-button", "Keep added songs", p).on_press_tx(|s: &mut App, tx| {
            let remember = s.queue_dont_ask;
            s.queue_dont_ask = false;
            s.resolve_pending_play(QueueChoice::KeepAdded, remember, tx);
        }));
    }
    buttons = buttons.child(pill_button("queue-replace-button", "Replace queue", p).on_press_tx(|s: &mut App, tx| {
        let remember = s.queue_dont_ask;
        s.queue_dont_ask = false;
        s.resolve_pending_play(QueueChoice::Replace, remember, tx);
    }));
    let body = col().key("queue-replace-body").w_full().px(16.0).pb(16.0).gap(16.0).child(note).child(check).child(buttons);
    Dialog::new("queue-replace", "Replace the queue?")
        .open(true)
        .width(440.0)
        .fit()
        .on_close(|s: &mut App| s.cancel_pending_play())
        .child(body)
        .build(kit, layer);
}

/// The Settings choice for what a new list does over an edited queue.
pub(super) fn policy_control(s: &App, p: &Palette) -> El {
    segmented(
        "queue-policy",
        &[("ask", "Ask"), ("replace", "Replace queue"), ("keep", "Keep added songs")],
        match s.queue_replace {
            QueueReplace::Ask => "ask",
            QueueReplace::Replace => "replace",
            QueueReplace::KeepAdded => "keep",
        },
        p,
        |s: &mut App, id: &str| {
            let policy = match id {
                "replace" => QueueReplace::Replace,
                "keep" => QueueReplace::KeepAdded,
                _ => QueueReplace::Ask,
            };
            s.set_queue_replace(policy);
        },
    )
}

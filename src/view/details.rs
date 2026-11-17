//! The song details dialog: everything about one song, read from its tags and its file when
//! the dialog opens. Names open their artist's page; values can be selected and copied.

use rune::{Align, Cursor, El, ScrollSpec, col, el, icon, row, text};
use rune_kit::{Dialog, Kit};

use super::*;
use crate::app::DetailsDialog;
use crate::credits::{self, Role};
use crate::details::{Details, format_channels, format_size};
use crate::icons;
use crate::palette::Palette;

const LABEL_W: f32 = 132.0;

/// Mounts the dialog in `layer` while one is open.
pub(super) fn details_dialog(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) {
    let Some(open) = &s.details else { return };
    let body = match s.details_track() {
        Some(i) => body(s, i, open, p),
        None => text("This song is no longer in the library.").key("details-gone").font(BODY).color(p.tx3).padding(16.0),
    };
    let title = s.details_track().map_or_else(|| "Song details".to_string(), |i| s.tracks()[i].title.clone());
    Dialog::new("details", title)
        .open(true)
        .width(640.0)
        .fit()
        .on_close(|s: &mut App| s.close_details())
        .child(col().key("details-scroll").w_full().max_h(560.0).scroll(ScrollSpec { stick_bottom: false }).child(body))
        .build(kit, layer);
}

fn body(s: &App, i: usize, open: &DetailsDialog, p: &Palette) -> El {
    let t = &s.tracks()[i];
    let mut b = col().key("details-body").w_full().px(16.0).pb(16.0).gap(18.0);

    let mut song = section("Song", p);
    song = song.child(fact("Title", song_title(t, p.tx), p));
    if let Some(artist) = song_artist(t, p.tx) {
        song = song.child(fact("Artist", artist, p));
    }
    if !t.featured.is_empty() {
        song = song.child(fact("Featuring", names(&t.featured, p), p));
    }
    b = b.child(song);

    let mut release = section("Release", p);
    release = release.children(t.album.clone().map(|a| fact("Album", plain(a, p), p)));
    if let Some(line) = &t.album_artist {
        release = release.child(fact("Album artist", linked(line.clone(), artist_links(line, &t.album_artists), p.tx), p));
    }
    let numbering = match (t.number, t.disc) {
        (Some(n), Some(d)) => Some(format!("Track {n}, disc {d}")),
        (Some(n), None) => Some(format!("Track {n}")),
        (None, Some(d)) => Some(format!("Disc {d}")),
        (None, None) => None,
    };
    release = release.children(numbering.map(|n| fact("Track", plain(n, p), p)));
    release = release.children(t.year.map(|y| fact("Year", plain(y.to_string(), p), p)));
    release = release.children(t.label.as_deref().map(|l| fact("Label", label_link(l, p.tx).key("details-label"), p)));
    if !t.genres.is_empty() {
        release = release.child(fact("Genre", plain(t.genres.join(", "), p), p));
    }
    b = b.child(release);

    let mut plays = section("Plays", p).key("details-plays");
    let count = match t.plays {
        0 => "Never played".to_string(),
        1 => "Once".to_string(),
        n => format!("{n} times"),
    };
    plays = plays.child(fact("Plays", plain(count, p).key("details-play-count"), p));
    plays = plays.children(t.last_played.map(|at| fact("Last played", plain(crate::query::property::format_date(at), p), p)));
    b = b.child(plays);

    let roles = Role::ALL.into_iter().filter(|&r| t.credits.iter().any(|c| c.role == r));
    let mut c = section("Credits", p).key("details-credits");
    let mut any = false;
    for role in roles {
        any = true;
        let people = credits::people(&t.credits, role);
        c = c.child(fact(role.label(), name_list(&people, p.tx).key(format!("credit-{}", role.label().to_lowercase())), p));
    }
    if any {
        b = b.child(c);
    }

    match &open.details {
        None => {
            b = b.child(file_section(t, &Details::default(), p));
            b = b.child(text("Reading the file…").key("details-reading").font(SMALL).color(p.tx3));
        }
        Some(Ok(d)) => {
            b = b.child(file_section(t, d, p));
            if !d.ids.is_empty() {
                let mut ids = section("Identifiers", p).key("details-ids");
                for (label, value) in &d.ids {
                    ids = ids.child(fact(label, plain(value.clone(), p).font(NUM), p));
                }
                b = b.child(ids);
            }
            if let Some(lyrics) = &d.lyrics {
                b = b.child(section("Lyrics", p).key("details-lyrics").child(text(lyrics.clone()).font(BODY).color(p.tx).wrap().selectable(true).w_full()));
            }
            b = b.child(raw_section(d, open.raw_open, p));
        }
        Some(Err(e)) => {
            b = b.child(file_section(t, &Details::default(), p));
            b = b.child(text(e.clone()).key("details-error").font(SMALL).color(p.red).wrap().w_full());
        }
    }
    b
}

fn file_section(t: &crate::library::Track, d: &Details, p: &Palette) -> El {
    let mut f = section("File", p).key("details-file");
    f = f.child(fact("Path", plain(t.path.display().to_string(), p).wrap(), p));
    let format = t.path.extension().and_then(|e| e.to_str()).map(str::to_uppercase);
    f = f.children(format.map(|x| fact("Format", plain(x, p), p)));
    f = f.children(t.duration.map(|x| fact("Duration", plain(clock(x), p), p)));
    let mut audio = Vec::new();
    audio.extend(d.bitrate.map(|b| format!("{b} kbps")));
    audio.extend(d.sample_rate.map(|r| format!("{:.1} kHz", f64::from(r) / 1000.0)));
    audio.extend(d.bit_depth.map(|b| format!("{b}-bit")));
    audio.extend(d.channels.map(format_channels));
    if !audio.is_empty() {
        f = f.child(fact("Audio", plain(audio.join("  •  "), p), p));
    }
    f = f.children(d.size.map(|x| fact("Size", plain(format_size(x), p), p)));
    let reveal = row()
        .key("details-reveal")
        .h(30.0)
        .px(12.0)
        .gap(8.0)
        .radius(15.0)
        .center_y()
        .fill(p.ui.alpha(0.6))
        .hover(p.ui.alpha(0.4))
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .label("Open containing folder")
        .on_press(|s: &mut App| s.reveal_details_file())
        .child(icon(&icons::FOLDER_OPEN, 15.0).color(p.tx))
        .child(text("Open containing folder").font(TITLE).color(p.tx));
    f.child(row().pl(LABEL_W + 12.0).pt(4.0).child(reveal))
}

/// "All tags": collapsed until opened, then every tag as stored, grouped by tag format.
fn raw_section(d: &Details, open: bool, p: &Palette) -> El {
    let chevron = icon(&rune_kit::glyphs::TRIANGLE_RIGHT, 12.0).color(p.tx3).icon_rotation(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 }).motion(SNAPPY);
    let toggle = row()
        .key("details-raw-toggle")
        .h(28.0)
        .gap(6.0)
        .center_y()
        .radius(3.0)
        .hover(p.ui.alpha(0.3))
        .cursor(Cursor::Pointer)
        .role("button")
        .expanded(open)
        .label("All tags")
        .on_press(|s: &mut App| {
            if let Some(d) = &mut s.details {
                d.raw_open = !d.raw_open;
            }
        })
        .child(el().w(16.0).center().child(chevron))
        .child(text("All tags").font(GROUP).color(p.tx2))
        .child(text(format!("{}", d.raw.len())).font(NUM).color(p.tx3));
    let mut s = col().key("details-raw").w_full().gap(4.0).child(toggle);
    if !open {
        return s;
    }
    if d.raw.is_empty() {
        return s.child(text("The file has no tags.").font(SMALL).color(p.tx3));
    }
    let mut format = "";
    for (n, tag) in d.raw.iter().enumerate() {
        if tag.format != format {
            format = tag.format;
            s = s.child(text(format).font(SMALL).color(p.tx3).pt(6.0));
        }
        s = s.child(
            row()
                .key(format!("raw{n}"))
                .w_full()
                .gap(12.0)
                .items_start()
                .child(text(tag.key.clone()).font(NUM).color(p.tx2).w(LABEL_W + 60.0).wrap().selectable(true))
                .child(text(tag.value.clone()).font(SMALL).color(p.tx).wrap().selectable(true).grow(1.0).shrink()),
        );
    }
    s
}

fn section(title: &str, p: &Palette) -> El {
    col().w_full().gap(6.0).child(text(title.to_string()).font(GROUP).color(p.tx2))
}

/// A label and its value, the value wrapping.
fn fact(label: &str, value: El, p: &Palette) -> El {
    row()
        .w_full()
        .gap(12.0)
        .items_start()
        .child(text(label.to_string()).font(SMALL).color(p.tx3).w(LABEL_W).align(Align::Start))
        .child(value.font(BODY).wrap().grow(1.0).shrink())
}

fn plain(value: String, p: &Palette) -> El {
    text(value).color(p.tx).selectable(true)
}

fn names(list: &[String], p: &Palette) -> El {
    name_list(&list.iter().map(|n| (n.clone(), Vec::new())).collect::<Vec<_>>(), p.tx)
}

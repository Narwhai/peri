//! The artist picture picker: the pictures Wikimedia has of the open artist, each with its
//! credit, to pick one as theirs. Wikidata's main image comes first, then the other images
//! Wikidata and MusicBrainz name, then photos from the artist's Commons category.

use rune::{Cursor, El, ScrollSpec, SurfaceFit, col, el, icon, row, text};
use rune_kit::{Dialog, Kit};

use super::*;
use crate::palette::Palette;
use crate::pictures::online::{Candidate, Origin};

const TILE: f32 = 148.0;
const GAP: f32 = 12.0;
const COLUMNS: usize = 4;

/// Mounts the picker in `layer` while it's open.
pub(super) fn picture_picker(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) {
    let Some(picker) = s.pictures.picker() else { return };
    let who = picker.names.first().cloned().unwrap_or_default();
    let body = match &picker.candidates {
        None => note("picker-loading", "Looking on MusicBrainz, Wikidata and Wikimedia Commons…", p),
        Some(Err(e)) => note("picker-error", &format!("Couldn't look them up: {e}"), p),
        Some(Ok(found)) if found.is_empty() => note("picker-empty", &format!("Wikimedia Commons has no pictures of {who} that Peri could find."), p),
        Some(Ok(found)) => {
            let mut grid = col().key("picker-grid").gap(GAP);
            for (r, chunk) in found.chunks(COLUMNS).enumerate() {
                let mut line = row().key(format!("picker-row{r}")).gap(GAP).items_start();
                for (c, candidate) in chunk.iter().enumerate() {
                    let n = r * COLUMNS + c;
                    line = line.child(tile(n, candidate, picker.thumb(&candidate.file), p));
                }
                grid = grid.child(line);
            }
            col().key("picker-scroll").w_full().max_h(520.0).scroll(ScrollSpec { stick_bottom: false }).child(grid.px(16.0).pb(16.0))
        }
    };
    Dialog::new("picture-picker", format!("Pictures of {who}"))
        .description("From Wikimedia Commons. The one you pick is copied in as yours, with its credit.")
        .open(true)
        .width(TILE * COLUMNS as f32 + GAP * (COLUMNS - 1) as f32 + 32.0)
        .fit()
        .on_close(|s: &mut App| s.pictures.close_picker())
        .child(body)
        .build(kit, layer);
}

fn note(key: &str, message: &str, p: &Palette) -> El {
    text(message.to_string()).key(key.to_string()).font(BODY).color(p.tx3).wrap().px(16.0).pb(16.0)
}

/// One candidate: its thumbnail, where it's from, and its credit. Pressing it picks it.
/// Keyed `picker-{n}`.
fn tile(n: usize, c: &Candidate, thumb: Option<Option<rune::Image>>, p: &Palette) -> El {
    let picture = el().key("thumb").size(TILE, TILE).radius(4.0).fill(p.ui);
    let picture = match thumb {
        Some(Some(image)) => picture.pixels_fit(image.pixels(), SurfaceFit::Fill),
        Some(None) => picture.center().child(icon(&icons::NOTE, 28.0).color(p.ui2)),
        None => picture,
    };
    let from = match (n, c.origin) {
        (0, Origin::Wikidata) => "Main image",
        (_, Origin::Wikidata) => "Wikidata",
        (_, Origin::MusicBrainz) => "MusicBrainz",
        (_, Origin::Category) => "Commons category",
    };
    let credit = [c.attribution.author.clone(), c.attribution.license.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let file = c.file.clone();
    col()
        .key(format!("picker-{n}"))
        .w(TILE)
        .gap(4.0)
        .padding(0.0)
        .radius(4.0)
        .hover(p.ui.alpha(0.35))
        .cursor(Cursor::Pointer)
        .role("button")
        .label(format!("Use {}", c.file))
        .help(format!("{}\n{}", c.attribution.line(), c.attribution.page))
        .on_press(move |s: &mut App| s.pick_artist_picture(&file))
        .child(picture)
        .child(text(from).key("origin").font(SMALL).color(p.tx2))
        .child(text(credit).key("credit").font(SMALL).color(p.tx3).wrap().max_lines(2))
}

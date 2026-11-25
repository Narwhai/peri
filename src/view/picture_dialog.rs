//! The artist picture's dialog, opened by pressing the picture: the picture large, where it's
//! from (a file they chose, their music folder, Wikimedia Commons with its credit and link, a
//! release's cover), and other pictures to use: any of their releases' covers, or the pictures
//! Wikimedia has.

use rune::{Cursor, El, ScrollSpec, col, el, row, text};
use rune_kit::{Dialog, Kit};

use super::albums::{cover, cover_image, secondary_button};
use super::*;
use crate::artist::ArtistPage;
use crate::palette::Palette;
use crate::pictures::{Shown, Source, online::Attribution};

const PREVIEW: f32 = 240.0;
const COVER: f32 = 104.0;
const GAP: f32 = 10.0;
const COLUMNS: usize = 5;

/// Mounts the dialog in `layer` while it's open.
pub(super) fn picture_dialog(s: &App, info: &ArtistPage, shown: &Shown, p: &Palette, kit: &Kit, layer: &mut Vec<El>) {
    if !s.picture_dialog {
        return;
    }
    let (image, _) = s.artist_picture(info);
    let first = info.albums.first().copied().or_else(|| info.appears_on.first().and_then(|&i| s.album_of(i)));
    let image = image.or_else(|| first.and_then(|a| cover_image(s, &s.albums()[a])));
    let preview = cover("picture-preview".into(), image, Some(PREVIEW), p);
    let about = col().key("picture-source").gap(8.0).grow(1.0).shrink().children(source(s, shown, first, p));
    let top = row().gap(20.0).items_start().child(preview).child(about);
    let mut body = col().key("picture-dialog-body").w_full().px(16.0).pb(16.0).gap(18.0).child(top);
    let releases = releases(s, info, shown, p);
    if !releases.is_empty() {
        let mut grid = col().key("picture-covers").gap(GAP);
        for (r, chunk) in releases.chunks(COLUMNS).enumerate() {
            grid = grid.child(row().key(format!("picture-covers-row{r}")).gap(GAP).items_start().children(chunk.to_vec()));
        }
        body = body
            .child(text("Use a release's cover").font(HEADING).color(p.tx))
            .child(col().key("picture-covers-scroll").w_full().max_h(300.0).scroll(ScrollSpec { stick_bottom: false }).child(grid));
    }
    Dialog::new("picture", format!("Picture of {}", info.name))
        .open(true)
        .width(COVER * COLUMNS as f32 + GAP * (COLUMNS - 1) as f32 + 32.0 + 16.0)
        .fit()
        .on_close(|s: &mut App| s.close_picture_dialog())
        .child(body)
        .build(kit, layer);
}

/// Where the picture is from, its credit and link, and the other ways to change it.
fn source(s: &App, shown: &Shown, first: Option<usize>, p: &Palette) -> Vec<El> {
    let line = |key: &str, t: String| text(t).key(key.to_string()).font(BODY).color(p.tx2).wrap();
    let mut out = Vec::new();
    match shown {
        Shown::User(entry) => {
            out.push(line("picture-from", match entry.source {
                Source::Local => "A picture you chose".into(),
                Source::User => "A picture you pasted".into(),
                Source::Wikimedia => "From Wikimedia Commons, kept as yours".into(),
                Source::Cover => format!("The cover of {}", entry.release.as_deref().unwrap_or("one of their releases")),
            }));
            if let Some(a) = &entry.attribution {
                out.extend(credit(a, p));
            } else if let Some(url) = entry.url.clone() {
                out.push(link("picture-link", "Where it was downloaded from", url, p));
            }
            out.push(text(format!("Added {}", crate::query::property::format_date(entry.added as i64))).key("picture-added").font(SMALL).color(p.tx3));
        }
        Shown::Folder(path) => {
            out.push(line("picture-from", "From their music folder".into()));
            out.push(text(path.display().to_string()).key("picture-path").font(SMALL).color(p.tx3).wrap().selectable(true));
        }
        Shown::Online(a) => {
            out.push(line("picture-from", "Found on Wikimedia Commons. Keep it to make it theirs, or pick another.".into()));
            out.extend(credit(a, p));
        }
        Shown::Cover => {
            let from = first.map(|a| format!("The cover of {}, until you choose a picture", s.albums()[a].title));
            out.push(line("picture-from", from.unwrap_or_else(|| "No picture yet".into())));
        }
    }
    let mut actions = row().key("picture-actions").gap(8.0).flex_wrap().mt(6.0);
    if matches!(shown, Shown::Online(_)) {
        actions = actions.child(secondary_button("picture-keep", &icons::PLUS, "Keep this picture", p).on_press(|s: &mut App| {
            s.close_picture_dialog();
            s.keep_artist_picture();
        }));
    }
    actions = actions
        .child(secondary_button("picture-choose", &icons::FOLDER_OPEN, "Choose image…", p).on_press_tx(|s: &mut App, tx| {
            s.close_picture_dialog();
            s.choose_artist_picture(tx);
        }))
        .child(secondary_button("picture-wikimedia", &icons::EXTERNAL, "Choose from Wikimedia…", p).on_press(|s: &mut App| s.picture_dialog_to_picker()));
    if matches!(shown, Shown::User(_) | Shown::Online(_)) {
        actions = actions.child(secondary_button("picture-remove", &icons::TRASH, "Remove picture", p).on_press(|s: &mut App| {
            s.close_picture_dialog();
            s.remove_artist_picture();
        }));
    }
    out.push(actions);
    out
}

/// A Commons picture's author and license, and links to its page and license.
fn credit(a: &Attribution, p: &Palette) -> Vec<El> {
    let mut out = Vec::new();
    if let Some(author) = &a.author {
        out.push(text(format!("Photo: {author}")).key("picture-author").font(BODY).color(p.tx).wrap().selectable(true));
    }
    if let Some(license) = &a.license {
        out.push(match a.license_url.clone() {
            Some(url) => link("picture-license", license, url, p),
            None => text(license.clone()).key("picture-license").font(BODY).color(p.tx2),
        });
    }
    out.push(link("picture-commons", "View on Wikimedia Commons", a.page.clone(), p));
    out
}

/// A link that opens `url` in the browser.
fn link(key: &str, label: &str, url: String, p: &Palette) -> El {
    text(label.to_string())
        .key(key.to_string())
        .font(BODY)
        .color(p.accent)
        .cursor(Cursor::Pointer)
        .role("link")
        .help(url.clone())
        .on_press_tx(move |s: &mut App, tx| {
            if let Err(e) = tx.open_url(&url) {
                s.error = Some(format!("Couldn't open {url}: {e}"));
            }
        })
}

/// A tile per release of theirs, then per release they appear on, that has a cover: pressing
/// one uses its cover as their picture. Keyed `picture-cover-{album}`.
fn releases(s: &App, info: &ArtistPage, shown: &Shown, p: &Palette) -> Vec<El> {
    let current = match shown {
        Shown::User(entry) if entry.source == Source::Cover => entry.release.clone(),
        _ => None,
    };
    let appears: Vec<usize> = info.appears_on.iter().filter_map(|&i| s.album_of(i)).collect();
    let mut albums: Vec<usize> = Vec::new();
    for a in info.albums.iter().copied().chain(appears) {
        if !albums.contains(&a) {
            albums.push(a);
        }
    }
    albums
        .into_iter()
        .filter_map(|a| {
            let album = &s.albums()[a];
            let image = cover_image(s, album)?;
            let on = current.as_deref() == Some(album.title.as_str());
            let mut tile = cover("art".into(), Some(image), Some(COVER), p);
            if on {
                tile = tile.child(el().key("in-use").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).border(2.0, p.accent).inert());
            }
            let year = album.year.map(|y| format!(" ({y})")).unwrap_or_default();
            Some(
                col()
                    .key(format!("picture-cover-{a}"))
                    .w(COVER)
                    .gap(4.0)
                    .radius(3.0)
                    .hover(p.ui.alpha(0.35))
                    .cursor(Cursor::Pointer)
                    .role("button")
                    .checked(on)
                    .label(format!("Use the cover of {}", album.title))
                    .on_press(move |s: &mut App| s.use_cover_as_picture(a))
                    .child(tile)
                    .child(text(format!("{}{year}", album.title)).font(SMALL).color(p.tx2).max_lines(2).wrap()),
            )
        })
        .collect()
}

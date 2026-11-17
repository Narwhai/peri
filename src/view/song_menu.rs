//! The right-click menu on songs, albums and lists: Play Next and Play After, and a song's details.

use rune::{el, Tx};
use rune_kit::{Kit, MenuItem, context_menu};

use super::*;
use crate::app::{App, SongMenuTarget};

/// Opens the menu for `target` at `at`, when one is set, under the pointer at any `zoom`. Mounted once, from the window.
pub(super) fn song_menu(s: &App, kit: &Kit, zoom: f32, layer: &mut Vec<El>) {
    let Some((target, at)) = s.song_menu.clone() else { return };
    let Some((label, tracks)) = (match &target {
        SongMenuTarget::Track(i) => s.tracks().get(*i).map(|t| (t.title.clone(), vec![*i])),
        SongMenuTarget::Album(a) => s.albums().get(*a).map(|al| (al.title.clone(), al.tracks.clone())),
    }) else {
        return;
    };
    let next = tracks.clone();
    let after = tracks.clone();
    let mut menu = context_menu("song-menu", el().size(0.0, 0.0), Some(unzoomed(at, zoom)))
        .group_label(label)
        .item(MenuItem::new("Play Next").on_select_tx(move |s: &mut App, tx: &mut Tx| {
            s.song_menu = None;
            s.queue_play_next(next.clone(), tx);
        }))
        .item(MenuItem::new("Play After").on_select_tx(move |s: &mut App, tx: &mut Tx| {
            s.song_menu = None;
            s.queue_play_after(after.clone(), tx);
        }));
    if let SongMenuTarget::Track(i) = target {
        menu = menu.separator().item(MenuItem::new("Song details").on_select_tx(move |s: &mut App, tx: &mut Tx| {
            s.song_menu = None;
            s.show_details(i, tx);
        }));
    }
    menu.on_close(|s: &mut App| s.song_menu = None).build(kit, layer);
}

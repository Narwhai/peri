//! The settings page: the folders and files the library is made of, saved and scanned at launch.

use rune::{Cursor, El, ScrollSpec, col, icon, row, text};

use super::albums::{back_label, secondary_button};
use super::*;
use crate::palette::Palette;

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);

pub(super) fn settings_page(s: &App, p: &Palette) -> El {
    let back = row()
        .key("back")
        .h(28.0)
        .pl(6.0)
        .pr(10.0)
        .gap(4.0)
        .radius(6.0)
        .center_y()
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .label("Back")
        .help("Back (Esc)")
        .on_press(|s: &mut App| s.go_back())
        .child(icon(&icons::BACK, 16.0).color(p.tx2))
        .child(text(back_label(s)).font(TITLE).color(p.tx2));
    let mut roots = col().key("library-roots").w_full().gap(4.0);
    if s.library.is_empty() {
        roots = roots.child(
            text("No folders yet. Open or drop a folder and it's remembered for next time.").key("library-empty").font(BODY).color(p.tx3).wrap(),
        );
    }
    for (i, root) in s.library.iter().enumerate() {
        let missing = !root.exists();
        let mut line = row()
            .key(format!("library-root{i}"))
            .w_full()
            .h(40.0)
            .px(12.0)
            .gap(10.0)
            .radius(8.0)
            .center_y()
            .fill(p.ui.alpha(0.25))
            .child(icon(&icons::FOLDER_OPEN, 16.0).color(p.tx3))
            .child(text(root.display().to_string()).key(format!("library-root{i}-path")).font(BODY).color(p.tx).grow(1.0).shrink().truncate());
        if missing {
            line = line.child(text("Not found").key(format!("library-root{i}-missing")).font(SMALL).color(p.red));
        }
        roots = roots.child(line.child(icon_button_owned(format!("library-root{i}-remove"), &icons::TRASH, "Remove from library", p).on_press(move |s: &mut App| s.remove_library_root(i))));
    }
    let buttons = row()
        .gap(8.0)
        .child(secondary_button("library-add", &icons::PLUS, "Add folder…", p).on_press_tx(|s: &mut App, tx| s.add_library_folder(tx)))
        .child(secondary_button("library-rescan", &icons::REPEAT, "Rescan", p).on_press(|s: &mut App| s.rescan_library()))
        .child(secondary_button("library-forget", &icons::TRASH, "Forget library", p).on_press(|s: &mut App| s.forget_library()));
    let section = col()
        .key("settings-library")
        .w_full()
        .gap(10.0)
        .child(text("Library folders").font(HEADING).color(p.tx))
        .child(text("Scanned every time the app starts. Folders on the command line replace these for that run only.").font(BODY).color(p.tx3).wrap())
        .child(roots)
        .child(buttons);
    let body = col()
        .key("settings-page-body")
        .w_full()
        .max_w(760.0)
        .px(28.0)
        .py(16.0)
        .gap(22.0)
        .child(back)
        .child(text("Settings").key("settings-title").font(PAGE_TITLE).color(p.tx))
        .child(section);
    col().key("settings-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// A round icon button with a runtime key.
fn icon_button_owned(key: String, glyph: &'static rune::Icon, help: &str, p: &Palette) -> El {
    rune::el()
        .key(key)
        .size(28.0, 28.0)
        .radius(14.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(help.to_string())
        .help(help.to_string())
        .child(icon(glyph, 15.0).color(p.tx2))
}

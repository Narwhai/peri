//! The settings page: the folders and files the library is made of, saved and scanned at launch,
//! and the Last.fm account songs are scrobbled to.

use rune::{Cursor, El, ScrollSpec, col, icon, row, text};
use rune_kit::{Kit, TextInput};

use super::albums::{back_label, secondary_button};
use super::*;
use super::toolbar::segmented;
use crate::palette::{Appearance, Palette, Theme};

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);

pub(super) fn settings_page(s: &App, p: &Palette, kit: &Kit) -> El {
    let back = row()
        .key("back")
        .h(28.0)
        .pl(6.0)
        .pr(10.0)
        .gap(4.0)
        .radius(3.0)
        .center_y()
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .label("Back")
        .help("Back (Esc)")
        .on_press_tx(|s: &mut App, tx| s.go_back(tx))
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
            .radius(4.0)
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
        .child(appearance_section(s, p))
        .child(queue_section(s, p))
        .child(section)
        .child(lastfm_section(s, p, kit))
        .child(pictures_section(s, p, kit));
    col().key("settings-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// The theme, and whether the app is light, dark or follows the system.
fn appearance_section(s: &App, p: &Palette) -> El {
    let theme = segmented("theme", &Theme::ALL.map(|t| (t.id(), t.name())), s.theme.id(), p, |s: &mut App, id: &str| {
        if let Some(theme) = Theme::ALL.into_iter().find(|t| t.id() == id) {
            s.set_theme(theme);
        }
    });
    let mode = segmented("appearance", &Appearance::ALL.map(|a| (a.id(), a.label())), s.appearance.id(), p, |s: &mut App, id: &str| {
        if let Some(appearance) = Appearance::ALL.into_iter().find(|a| a.id() == id) {
            s.set_appearance(appearance);
        }
    });
    let field = |label: &str, control: El| row().w_full().gap(12.0).center_y().child(text(label.to_string()).font(BODY).color(p.tx2).w(110.0)).child(control);
    col()
        .key("settings-appearance")
        .w_full()
        .gap(10.0)
        .child(text("Appearance").font(HEADING).color(p.tx))
        .child(text("Mode follows the system unless set to Light or Dark.").font(BODY).color(p.tx3).wrap())
        .child(field("Theme", theme))
        .child(field("Mode", mode))
}

/// Whether artist pages look pictures up online. Off unless turned on.
fn pictures_section(s: &App, p: &Palette, kit: &Kit) -> El {
    let on = s.picture_lookups;
    let switch = rune_kit::Switch::new(on).label("Look up artist pictures on Wikimedia Commons").on_toggle(move |s: &mut App| s.set_picture_lookups(!on)).build(kit).key("picture-lookups");
    col()
        .key("settings-pictures")
        .w_full()
        .gap(10.0)
        .child(text("Artist pictures").font(HEADING).color(p.tx))
        .child(
            text("Artists show the picture you give them, else an artist.jpg from their music folder. With this on, artists without either are looked up on MusicBrainz and Wikidata, and a picture on Wikimedia Commons shows with its credit until you keep or remove it. Pictures you keep are never replaced.")
                .font(BODY)
                .color(p.tx3)
                .wrap(),
        )
        .child(switch)
}

/// What a new list does over an edited queue.
fn queue_section(s: &App, p: &Palette) -> El {
    let field = row().w_full().gap(12.0).center_y().child(text("Starting a list over an edited queue".to_string()).font(BODY).color(p.tx2).w(110.0).wrap()).child(super::queue::policy_control(s, p));
    col()
        .key("settings-queue")
        .w_full()
        .gap(10.0)
        .child(text("Queue").font(HEADING).color(p.tx))
        .child(text("Asked about when the queue has songs you added or moved. Replace queue drops the upcoming songs; Keep added songs keeps the ones you added.").font(BODY).color(p.tx3).wrap())
        .child(field)
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

/// Connecting a Last.fm account: the API account's key and secret, approval in the browser,
/// then the session that scrobbles.
fn lastfm_section(s: &App, p: &Palette, kit: &Kit) -> El {
    let lastfm = &s.lastfm;
    let mut section = col().key("settings-lastfm").w_full().gap(10.0).child(text("Last.fm").font(HEADING).color(p.tx));
    let note = |t: &str| text(t.to_string()).font(BODY).color(p.tx3).wrap();
    if let Some(session) = lastfm.session() {
        section = section
            .child(text(format!("Scrobbling as {}.", session.name)).key("lastfm-connected").font(BODY).color(p.tx).wrap())
            .child(note("Songs are scrobbled once they've played for half their length or four minutes. Failed scrobbles aren't retried."));
        let mut buttons = row().gap(8.0).child(secondary_button("lastfm-disconnect", &rune_kit::glyphs::CLOSE, "Disconnect", p).on_press(|s: &mut App| s.lastfm_disconnect()));
        if !lastfm.has_built_in() {
            buttons = buttons.child(secondary_button("lastfm-forget", &icons::TRASH, "Remove credentials", p).on_press(|s: &mut App| s.lastfm_forget()));
        }
        section = section.child(buttons);
    } else if lastfm.token.is_some() {
        let finish = match lastfm.busy {
            true => secondary_button("lastfm-finish", &icons::LINK, "Finishing…", p),
            false => secondary_button("lastfm-finish", &icons::LINK, "Finish connecting", p).on_press(|s: &mut App| s.lastfm_finish()),
        };
        section = section.child(note("Allow access on the Last.fm page that opened in your browser, then finish connecting here.").key("lastfm-approve")).child(
            row()
                .gap(8.0)
                .child(finish)
                .child(secondary_button("lastfm-reopen", &icons::EXTERNAL, "Open the page again", p).on_press_tx(|s: &mut App, tx| s.lastfm_reopen(tx)))
                .child(secondary_button("lastfm-cancel", &rune_kit::glyphs::CLOSE, "Cancel", p).on_press(|s: &mut App| s.lastfm.cancel())),
        );
    } else if lastfm.has_built_in() {
        let connect = match lastfm.busy {
            true => secondary_button("lastfm-connect", &icons::LINK, "Connecting…", p),
            false => secondary_button("lastfm-connect", &icons::LINK, "Connect Last.fm", p).on_press(|s: &mut App| s.lastfm_connect()),
        };
        section = section
            .child(note("Scrobble what you play to your Last.fm account. Connecting opens Last.fm in your browser to sign in and allow access."))
            .child(row().child(connect));
    } else {
        let field = |label: &str, input: TextInput| row().w_full().gap(12.0).center_y().child(text(label.to_string()).font(BODY).color(p.tx2).w(110.0)).child(input.build(kit).grow(1.0).shrink());
        let key = TextInput::new(&lastfm.key).label("API key").placeholder("32 hex digits").width(None).on_edit(|s: &mut App| &mut s.lastfm.key);
        let secret = TextInput::new(&lastfm.secret).label("Shared secret").placeholder("32 hex digits").width(None).on_edit(|s: &mut App| &mut s.lastfm.secret);
        let connect = match lastfm.busy {
            true => secondary_button("lastfm-connect", &icons::LINK, "Connecting…", p),
            false => secondary_button("lastfm-connect", &icons::LINK, "Connect", p).on_press(|s: &mut App| s.lastfm_connect()),
        };
        let mut buttons = row()
            .gap(8.0)
            .child(connect)
            .child(secondary_button("lastfm-create", &icons::EXTERNAL, "Get an API account…", p).on_press_tx(|s: &mut App, tx| s.lastfm_create_account(tx)));
        if lastfm.saved.credentials.is_some() {
            buttons = buttons.child(secondary_button("lastfm-forget", &icons::TRASH, "Remove credentials", p).on_press(|s: &mut App| s.lastfm_forget()));
        }
        section = section
            .child(note("This build has no Last.fm API account of its own, so scrobbling needs yours: create one, then paste its API key and shared secret here. They're saved in the preferences file, unencrypted."))
            .child(field("API key", key).key("lastfm-key"))
            .child(field("Shared secret", secret).key("lastfm-secret"))
            .child(buttons);
    }
    if let Some(status) = &lastfm.status {
        section = section.child(text(status.clone()).key("lastfm-status").font(SMALL).color(p.red).wrap());
    }
    section
}

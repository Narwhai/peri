//! The artist page: their releases or their songs (a tab), the songs they appear on, the songs
//! that credit them, and related artists. The search page shows the same sections for the artist it finds.

use rune::{Cursor, El, ScrollSpec, col, el, icon, row, text};
use rune_kit::{Kit, Placement, TextInput, popover, scrim};

use super::albums::{album_strip, back_label, cover, cover_image, play_button, secondary_button, song_cover, tinted_button};
use super::table;
use super::toolbar::{hint, segmented};
use super::*;
use crate::album::ReleaseType;
use crate::app::{ArtistTab, Panel};
use crate::artist::{ArtistPage, everything, known, page, suggest};
use crate::credits::Role;
use crate::library::Track;
use crate::palette::Palette;
use crate::relations::{LinkChoice, Relation};

const PAGE_TITLE: TextStyle = TextStyle::new(28.0, 700.0);
const SONG_ROW_H: f32 = 40.0;
/// Height of the folder tabs; the panel's top line sits at its bottom edge.
const TAB_H: f32 = 34.0;
/// The gap between folder tabs; the first sits flush with the panel's left edge.
const TAB_GAP: f32 = 2.0;
/// Corner radius of the folder tabs' top corners and the panel's other corners.
const TAB_RADIUS: f32 = 3.0;
/// Song rows' covers, as wide as the table's lead column.
const COVER_W: f32 = table::LEAD_W;

pub(super) fn artist_page(s: &App, name: &str, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let model = s.artist_model(name, s.artist_with_aliases);
    let (info, others) = (&model.0, &model.1);
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
    let mut body = col().key("artist-page-body").w_full().px(28.0).py(16.0).gap(22.0).child(back).child(head(s, info, others, Some(link_artist(s, info, p, kit, layer)), p));
    if info.is_empty() {
        body = body.child(text(format!("Nothing in the library is by or credits {}.", info.name)).key("artist-empty").font(BODY).color(p.tx3));
    }
    col().key("artist-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body.children(sections(s, info, others, p)))
}

/// Their releases or songs (a tab), the songs they appear on, their credits, then related artists.
pub(super) fn sections(s: &App, info: &ArtistPage, others: &[(Relation, ArtistPage)], p: &Palette) -> Vec<El> {
    let mut out = Vec::new();
    let pills = album_pill_rows(s, info, p);
    if !info.albums.is_empty() {
        out.push(music(s, info, &pills, p));
    }
    if !info.appears_on.is_empty() {
        out.push(songs(s, "appears-on", "Appears on", &info.appears_on, p));
    }
    // Each song once with a pill per role.
    if !info.credits.is_empty() {
        let open = !s.artist_credits_closed;
        let toggle = section_toggle("artist-credits-toggle", "Credits", info.credits.len(), open, p).on_press(move |s: &mut App| s.artist_credits_closed = open);
        // Boxed like the Releases / Songs panel, open or folded.
        let mut section = boxed(col().key("credits").w_full().gap(8.0).child(toggle), p);
        if open {
            section = section.child(credits(s, "credits", info, p));
        }
        out.push(section);
    }
    out.extend(others.iter().filter_map(|(relation, other)| involved_section(s, *relation, other, p)));
    if !info.related.is_empty() {
        out.push(related(s, info, p));
    }
    out
}

/// Wraps a section in the bordered box the Credits and Releases panels use.
pub(super) fn boxed(section: El, p: &Palette) -> El {
    section.border(1.0, p.ui).radius(TAB_RADIUS).px(16.0).py(10.0)
}

/// The Releases / Songs folder tabs over a bordered panel holding the active tab's content: the
/// release strips, or one table of every song on their albums, newest album first. Keyed `artist-music`.
fn music(s: &App, info: &ArtistPage, pills: &[(usize, Vec<table::Pill>)], p: &Palette) -> El {
    let tabs = row()
        .key("artist-tab")
        .absolute(0.0, 0.0)
        .h(TAB_H)
        .gap(TAB_GAP)
        .role("tablist")
        .label("Releases or songs")
        .focus_group()
        .child(folder_tab(ArtistTab::Releases, "releases", "Releases", s.artist_tab == ArtistTab::Releases, p))
        .child(folder_tab(ArtistTab::Songs, "songs", "Songs", s.artist_tab == ArtistTab::Songs, p));
    let content = match s.artist_tab {
        // Mixed with their aliases' releases, cards say which name each is under.
        ArtistTab::Releases => col().key("artist-releases").w_full().gap(22.0).children(release_strips(s, "artist", &info.albums, info.with_aliases, pills, p)),
        ArtistTab::Songs => {
            let list: Vec<usize> = info.albums.iter().flat_map(|&a| s.albums()[a].tracks.iter().copied()).collect();
            col().key("artist-songs-list").w_full().child(song_table(s, "artist-songs", untagged(&list), &[], &list, p))
        }
    };
    // Square top-left, where the first tab meets the panel.
    let panel = col().key("artist-music-panel").w_full().mt(TAB_H - 1.0).border(1.0, p.ui).corners([0.0, TAB_RADIUS, TAB_RADIUS, TAB_RADIUS]).padding(16.0).child(content);
    col().key("artist-music").w_full().child(panel).child(tabs)
}

/// One folder tab: its label on a top-rounded tab that opens into the panel when `on`. The
/// active tab fills with the page background and covers the panel's top line beneath it.
fn folder_tab(tab: ArtistTab, value: &'static str, label: &'static str, on: bool, p: &Palette) -> El {
    let edge = if on { p.ui } else { p.ui.alpha(0.5) };
    let mut folder = row()
        .key(value)
        .h(TAB_H)
        .px(16.0)
        .center()
        .corners([TAB_RADIUS, TAB_RADIUS, 0.0, 0.0])
        .border(1.0, edge)
        .cursor(Cursor::Pointer)
        .role("tab")
        .label(label)
        .checked(on)
        .focusable(true)
        .motion(SNAPPY)
        .on_press(move |s: &mut App| s.artist_tab = tab)
        .child(text(label).font(TITLE).color(if on { p.tx } else { p.tx2 }));
    if on {
        // Sits on the tab's bottom border line, between its side borders: the panel's top line
        // is hidden beneath the tab.
        folder = folder.fill(p.bg).child(el().key("artist-tab-seam").pin(Some(1.0), None, Some(1.0), Some(0.0)).h(1.0).fill(p.bg));
    } else {
        folder = folder.hover(p.ui.alpha(0.3));
    }
    folder
}

/// One strip per release type (Albums, EPs, Singles, Compilations) of `albums`, keeping
/// their order; keyed `{key}-albums`, `{key}-eps`…
pub(super) fn release_strips(s: &App, key: &str, albums: &[usize], artists: bool, pills: &[(usize, Vec<table::Pill>)], p: &Palette) -> Vec<El> {
    ReleaseType::ALL
        .into_iter()
        .filter_map(|kind| {
            let of_kind: Vec<usize> = albums.iter().copied().filter(|&a| s.albums()[a].release_type == kind).collect();
            let section = format!("{key}-{}", kind.plural().to_lowercase());
            (!of_kind.is_empty()).then(|| album_strip(s, &section, kind.plural(), &of_kind, artists, pills, p))
        })
        .collect()
}

/// Picture (the first album's cover), name, counts, Play. On their page `link` is the Link
/// artist button; in search results (`None`) the name and picture open the artist's page.
pub(super) fn head(s: &App, info: &ArtistPage, others: &[(Relation, ArtistPage)], link: Option<El>, p: &Palette) -> El {
    let linked = link.is_none();
    let first = info.albums.first().copied().or_else(|| info.appears_on.first().and_then(|&i| s.album_of(i)));
    let mut art = cover("artist-cover".into(), first.and_then(|a| cover_image(s, &s.albums()[a])), Some(160.0), p);
    let mut counts = Vec::new();
    for kind in ReleaseType::ALL {
        let n = info.albums.iter().filter(|&&a| s.albums()[a].release_type == kind).count();
        if n > 0 {
            counts.push(kind.count(n));
        }
    }
    if !info.songs.is_empty() {
        counts.push(plural(info.songs.len(), "song"));
    }
    if !info.featured_on.is_empty() {
        counts.push(format!("featured on {}", info.featured_on.len()));
    }
    if !info.credits.is_empty() {
        counts.push(format!("credited on {}", plural(info.credits.len(), "song")));
    }
    let mut name = text(info.name.clone()).key("artist-name").font(PAGE_TITLE).color(p.tx).wrap();
    if linked {
        let open = |n: String| move |s: &mut App, tx: &mut rune::Tx| s.show_artist(&n, tx);
        art = art.cursor(Cursor::Pointer).role("link").label(format!("Open {}", info.name)).on_press_tx(open(info.name.clone()));
        name = name.cursor(Cursor::Pointer).on_press_tx(open(info.name.clone()));
    }
    let mut details = col()
        .gap(6.0)
        .grow(1.0)
        .shrink()
        .child(text("Artist").font(SMALL).color(p.tx3))
        .child(name)
        .child(text(counts.join("  •  ")).key("artist-counts").font(BODY).color(p.tx3));
    if info.names.len() > 1 {
        details = details.child(match linked {
            true => text(format!("Also credited as {}", info.names[1..].join(", "))).key("artist-aka").font(BODY).color(p.tx3).wrap(),
            false => merged_names(&info.names, p),
        });
    }
    let mut buttons = row().gap(8.0);
    if !info.play_list(s.albums()).is_empty() {
        let (who, with_aliases) = (info.name.clone(), info.with_aliases);
        buttons = buttons.child(play_button("artist-play", "Play", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, s.tracks(), s.albums(), s.relations(), with_aliases).play_list(s.albums());
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    if !info.discography(s.albums()).is_empty() {
        let (who, with_aliases) = (info.name.clone(), info.with_aliases);
        buttons = buttons.child(tinted_button("artist-discography", &icons::PLAY, "Play Discography", p).on_press_tx(move |s: &mut App, tx| {
            let list = page(&who, s.tracks(), s.albums(), s.relations(), with_aliases).discography(s.albums());
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    // Play Everything also plays the bands and members, when they have music.
    if !linked && others.iter().any(|(_, o)| !o.all_songs(s.albums()).is_empty()) {
        let (who, with_aliases) = (info.name.clone(), info.with_aliases);
        buttons = buttons.child(tinted_button("artist-play-everything", &icons::PLAY, "Play Everything", p).on_press_tx(move |s: &mut App, tx| {
            let list = everything(&who, s.tracks(), s.albums(), s.relations(), with_aliases);
            if let Some(&first) = list.first() {
                s.play_list(list, first, tx);
            }
        }));
    }
    // Search results show the artist alone; their page can take in their aliases.
    if !linked && info.has_aliases() {
        let on = info.with_aliases;
        buttons = buttons.child(aliases_toggle(on, p).on_press(move |s: &mut App| s.artist_with_aliases = !on));
    }
    buttons = buttons.children(link);
    if linked {
        let who = info.name.clone();
        buttons = buttons.child(link_button("open-artist", "Go to artist", p).on_press_tx(move |s: &mut App, tx| s.show_artist(&who, tx)));
    }
    details = details.child(el().h(10.0)).child(buttons);
    row().gap(28.0).center_y().child(art).child(details)
}

/// Their albums' pills, one row per album with credits on it.
fn album_pill_rows(s: &App, info: &ArtistPage, p: &Palette) -> Vec<(usize, Vec<table::Pill>)> {
    info.album_credits.iter().map(|(a, roles)| (*a, album_pills(s.albums()[*a].tracks.len(), roles, p))).collect()
}

/// A collapsed section for a band the artist is in, or a member of their band: the other
/// artist's releases, appearances and credits, shown when open. Keyed `related-{name}`; none
/// for an alias, whose music the Include aliases toggle covers.
fn involved_section(s: &App, relation: Relation, other: &ArtistPage, p: &Palette) -> Option<El> {
    let key = format!("related-{}", other.name);
    let title = match relation {
        Relation::MemberOf => format!("With {}", other.name),
        Relation::Member => format!("{} (solo)", other.name),
        Relation::Alias => return None,
    };
    let count = other.all_songs(s.albums()).len();
    let open = s.artist_related_open.contains(&other.name);
    let who = other.name.clone();
    let toggle = section_toggle(&format!("{key}-toggle"), &title, count, open, p).on_press(move |s: &mut App| {
        if open {
            s.artist_related_open.retain(|n| *n != who);
        } else {
            s.artist_related_open.push(who.clone());
        }
    });
    let mut section = boxed(col().key(key.clone()).w_full().gap(8.0).child(toggle), p);
    if open {
        let pills = album_pill_rows(s, other, p);
        section = section.children(release_strips(s, &key, &other.albums, other.with_aliases, &pills, p));
        if !other.appears_on.is_empty() {
            let appears = format!("{key}-appears-on");
            section = section.child(titled(&appears, "Appears on", song_table(s, &appears, untagged(&other.appears_on), &[], &other.appears_on, p), p));
        }
        if !other.credits.is_empty() {
            section = section.child(credits(s, &format!("{key}-credits"), other, p));
        }
    }
    Some(section)
}

/// The artists linked to this one, a row of names per relation; names in the library open
/// their pages.
fn related(s: &App, info: &ArtistPage, p: &Palette) -> El {
    let mut section = boxed(col().key("artist-related").w_full().gap(10.0).child(text("Related").font(HEADING).color(p.tx)), p);
    for relation in Relation::ALL {
        let names: Vec<&String> = info.related.iter().filter(|(r, _)| *r == relation).map(|(_, n)| n).collect();
        if names.is_empty() {
            continue;
        }
        let list = names.into_iter().fold(row().gap(8.0).flex_wrap(), |list, name| {
            let who = name.clone();
            let remove = remove_button(format!("unlink-{}-{name}", relation.id()), &format!("Remove {name}"), p).on_press(move |s: &mut App| s.unlink_artist(relation, &who));
            list.child(artist_pill(name, known(name, s.tracks()), p).pr(4.0).gap(4.0).child(remove))
        });
        section = section.child(col().gap(6.0).child(text(relation.title()).font(SMALL).color(p.tx3)).child(list));
    }
    section
}

/// An artist's name as a pill, keyed `artist-pill-{name}`, opening their page when `linked`.
pub(super) fn artist_pill(name: &str, linked: bool, p: &Palette) -> El {
    let pill = row().key(format!("artist-pill-{name}")).h(30.0).px(12.0).radius(15.0).center_y().fill(p.ui.alpha(0.5)).label(name.to_string());
    if !linked {
        return pill.child(text(name.to_string()).font(TITLE).color(p.tx3));
    }
    let who = name.to_string();
    pill.hover(p.ui.alpha(0.8))
        .cursor(Cursor::Pointer)
        .role("link")
        .on_press_tx(move |s: &mut App, tx| s.show_artist(&who, tx))
        .child(text(name.to_string()).font(TITLE).color(p.tx))
}

/// A small ✕ that removes a name from a pill or list.
fn remove_button(key: String, label: &str, p: &Palette) -> El {
    el().key(key)
        .size(22.0, 22.0)
        .radius(11.0)
        .center()
        .hover(p.ui)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(label.to_string())
        .help(label.to_string())
        .child(icon(&rune_kit::glyphs::CLOSE, 12.0).color(p.tx3))
}

/// "Also credited as" and the names merged into the page after the first, each with a ✕
/// that takes it out of the merge.
fn merged_names(names: &[String], p: &Palette) -> El {
    let mut line = row().key("artist-aka").gap(6.0).center_y().flex_wrap().child(text("Also credited as").font(BODY).color(p.tx3));
    for name in &names[1..] {
        let who = name.clone();
        line = line.child(
            row()
                .key(format!("aka-{name}"))
                .h(24.0)
                .pl(10.0)
                .pr(2.0)
                .gap(2.0)
                .radius(12.0)
                .center_y()
                .fill(p.ui.alpha(0.4))
                .child(text(name.clone()).font(BODY).color(p.tx2))
                .child(remove_button(format!("unmerge-{name}"), &format!("Unmerge {name}"), p).on_press(move |s: &mut App| s.unmerge_artist(&who))),
        );
    }
    line
}

/// The Link artist button, and while open its popover: what the name becomes (same artist,
/// alias, band, member), a name field, and the library's names matching it.
fn link_artist(s: &App, info: &ArtistPage, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let open = s.panel == Some(Panel::LinkArtist);
    let trigger = secondary_button("artist-link", &icons::PLUS, "Link artist", p).expanded(open).on_press(|s: &mut App| s.open_link_artist());
    if !open {
        return trigger;
    }
    let typed = s.link_name.text.trim().to_string();
    let taken: Vec<String> = info.names.iter().chain(info.related.iter().map(|(_, n)| n)).cloned().collect();
    let found = suggest(&typed, s.tracks(), &taken, 6);
    // Enter takes the library's spelling of what's typed, else it as typed.
    let exact = found.iter().find(|n| n.eq_ignore_ascii_case(&typed)).cloned();
    let field = TextInput::new(&s.link_name)
        .placeholder("Artist name")
        .label("Artist name")
        .leading_icon(&rune_kit::glyphs::SEARCH)
        .width(None)
        .on_edit(|s: &mut App| &mut s.link_name)
        .on_submit(move |s: &mut App, tx| {
            let typed = s.link_name.text.trim().to_string();
            s.link_artist(exact.as_deref().unwrap_or(&typed));
            if s.panel.is_none() {
                tx.blur();
            }
        })
        .on_key(|s: &mut App, ev, _tx| {
            if ev.key == rune::Key::Escape {
                s.close_panel();
                return true;
            }
            false
        })
        .build(kit)
        .key("artist-link-name");
    let kinds = segmented("artist-link-kind", &LinkChoice::ALL.map(|c| (c.id(), c.label())), s.link_choice.id(), p, |s: &mut App, id: &str| {
        if let Some(c) = LinkChoice::ALL.into_iter().find(|c| c.id() == id) {
            s.link_choice = c;
        }
        // Back to the name, so picking a kind doesn't stop typing.
        s.link_name.focused = true;
    });
    let mut list = col().key("artist-link-options").gap(2.0);
    for name in &found {
        let who = name.clone();
        list = list.child(link_option(format!("link-option-{name}"), name, None, p).on_press(move |s: &mut App| s.link_artist(&who)));
    }
    if !typed.is_empty() && !found.iter().any(|n| n.eq_ignore_ascii_case(&typed)) && !taken.iter().any(|n| n.eq_ignore_ascii_case(&typed)) {
        let who = typed.clone();
        list = list.child(link_option("link-option-new".into(), &format!("Add “{typed}”"), Some("Not in the library yet"), p).on_press(move |s: &mut App| s.link_artist(&who)));
    }
    let body = col()
        .w(380.0)
        .padding(12.0)
        .gap(10.0)
        .child(text(format!("Link {} to…", info.name)).font(TITLE).color(p.tx))
        .child(kinds)
        .child(hint(s.link_choice.explain(), p))
        .child(field)
        .child(list)
        .key("artist-link-panel")
        .role("dialog")
        .label("Link artist")
        .modal(true, |s: &mut App| s.close_panel())
        .focus_return("artist-link");
    layer.push(scrim("artist-link-scrim", |s: &mut App| s.close_panel()));
    layer.push(popover::anchored(kit, popover::raised(kit, body, 10.0), "artist-link", Placement::BottomStart, 6.0));
    trigger
}

/// A pickable name in the Link artist popover, with an optional note under it.
fn link_option(key: String, label: &str, note: Option<&str>, p: &Palette) -> El {
    let mut lines = col().gap(1.0).grow(1.0).shrink().child(text(label.to_string()).font(BODY).color(p.tx).truncate());
    if let Some(note) = note {
        lines = lines.child(text(note.to_string()).font(SMALL).color(p.tx3));
    }
    row()
        .key(key)
        .min_h(30.0)
        .px(8.0)
        .py(4.0)
        .center_y()
        .radius(3.0)
        .hover(p.ui.alpha(0.35))
        .press(p.ui.alpha(0.6))
        .cursor(Cursor::Pointer)
        .role("option")
        .label(label.to_string())
        .child(lines)
}

/// The switch that adds their aliases' releases and credits to the page.
fn aliases_toggle(on: bool, p: &Palette) -> El {
    let mark = el().w(16.0).h(16.0).radius(2.0).center();
    let mark = if on { mark.fill(p.accent).child(icon(&rune_kit::glyphs::CHECK_SMALL, 14.0).color(p.on_accent)) } else { mark.border(1.5, p.tx3) };
    row()
        .key("artist-aliases")
        .h(32.0)
        .px(12.0)
        .gap(8.0)
        .radius(16.0)
        .center_y()
        .hover(p.ui.alpha(0.4))
        .cursor(Cursor::Pointer)
        .role("checkbox")
        .checked(on)
        .label("Include aliases")
        .help("Also list what they released and are credited with under their other names")
        .child(mark)
        .child(text("Include aliases").font(TITLE).color(p.tx))
}

/// An outlined button beside Play.
fn link_button(key: &'static str, label: &'static str, p: &Palette) -> El {
    row()
        .key(key)
        .h(32.0)
        .px(14.0)
        .radius(16.0)
        .center_y()
        .border(1.0, p.ui)
        .hover(p.ui.alpha(0.4))
        .cursor(Cursor::Pointer)
        .role("link")
        .label(label)
        .child(text(label).font(TITLE).color(p.tx))
}

/// A titled song table with the page columns. Double-click plays the table from that song.
pub(super) fn songs(s: &App, key: &str, title: &str, list: &[usize], p: &Palette) -> El {
    boxed(titled(key, title, song_table(s, key, untagged(list), &[], list, p), p), p)
}

fn titled(key: &str, title: &str, table: El, p: &Palette) -> El {
    col().key(key.to_string()).w_full().gap(8.0).child(text(title.to_string()).font(HEADING).color(p.tx)).child(table)
}

fn untagged(list: &[usize]) -> Vec<(usize, Vec<Option<table::Pill>>)> {
    list.iter().map(|&i| (i, Vec::new())).collect()
}

/// The Credits table: each song crediting the artist once, with a Roles column: one slot per
/// role any of the songs credits them with, so a role's pills line up down the table.
fn credits(s: &App, key: &str, info: &ArtistPage, p: &Palette) -> El {
    let used: Vec<Role> = Role::ALL.into_iter().filter(|r| info.credits.iter().any(|(_, roles)| roles.contains(r))).collect();
    let rows: Vec<(usize, Vec<Option<table::Pill>>)> = info
        .credits
        .iter()
        .map(|(i, roles)| {
            let pills = used.iter().map(|r| roles.contains(r).then(|| role_pill(&s.tracks()[*i], *r, &info.matched, p))).collect();
            (*i, pills)
        })
        .collect();
    // Each slot is as wide as its widest pill.
    let slots: Vec<f32> = (0..used.len())
        .map(|k| rows.iter().filter_map(|(_, pills)| pills[k].as_ref()).map(|pill| table::pill_width(&pill.label)).fold(0.0, f32::max))
        .collect();
    song_table(s, key, rows, &slots, &info.credited(), p)
}

/// The artist's roles on one of their albums of `songs` songs: "Producer" when they hold it
/// on every song, else "Producer · 3/10".
fn album_pills(songs: usize, roles: &[(Role, usize)], p: &Palette) -> Vec<table::Pill> {
    roles
        .iter()
        .map(|&(role, n)| {
            let label = if n == songs { role.label().to_string() } else { format!("{} · {n}/{songs}", role.label()) };
            let (fill, color) = p.role(role);
            table::Pill { label, fill, color }
        })
        .collect()
}

/// The `role` on a song of the artist with these `names`: "Producer"; a performer's says what they played:
/// "Performer · vocals".
fn role_pill(t: &Track, role: Role, names: &[String], p: &Palette) -> table::Pill {
    let played: Vec<&str> = t.credits.iter().filter(|c| c.role == role && names.iter().any(|n| c.name.eq_ignore_ascii_case(n))).filter_map(|c| c.detail.as_deref()).collect();
    let label = if played.is_empty() { role.label().to_string() } else { format!("{} · {}", role.label(), played.join(", ")) };
    let (fill, color) = p.role(role);
    table::Pill { label, fill, color }
}

/// `rows` are the songs with what fills their Roles slots; `list` is what double-click plays.
fn song_table(s: &App, key: &str, rows: Vec<(usize, Vec<Option<table::Pill>>)>, pills: &[f32], list: &[usize], p: &Palette) -> El {
    let columns = table::Columns { shown: &s.views.page_columns, widths: &s.views.page_widths, pills, covers: false };
    let mut body = col().child(table::header(&format!("{key}-header"), "", columns, 8.0, p));
    for (n, (i, pills)) in rows.into_iter().enumerate() {
        body = body.child(song_line(s, key, i, list, pills, !n.is_multiple_of(2), columns, p));
    }
    table::scrolling(format!("{key}-table"), columns, 8.0, body, p)
}

/// A section heading with its count, which opens and closes the section; the caller adds
/// the press.
fn section_toggle(key: &str, title: &str, count: usize, open: bool, p: &Palette) -> El {
    let chevron = icon(&rune_kit::glyphs::TRIANGLE_RIGHT, 12.0).color(p.tx3).icon_rotation(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 }).motion(SNAPPY);
    row()
        .key(key.to_string())
        .h(32.0)
        .pr(10.0)
        .gap(8.0)
        .center_y()
        .radius(3.0)
        .hover(p.ui.alpha(0.3))
        .cursor(Cursor::Pointer)
        .role("button")
        .expanded(open)
        .label(format!("{title}, {count}"))
        .child(el().w(16.0).center().child(chevron))
        .child(text(title).font(HEADING).color(p.tx))
        .child(text(format!("{count}")).key("count").font(NUM).color(p.tx3))
}

#[allow(clippy::too_many_arguments)]
fn song_line(s: &App, key: &str, i: usize, list: &[usize], pills: Vec<Option<table::Pill>>, stripe: bool, columns: table::Columns, p: &Palette) -> El {
    let t = &s.tracks()[i];
    let current = s.current == Some(i);
    let lead = song_cover(s, format!("{key}-cover{i}"), i, COVER_W, p);
    let list = list.to_vec();
    let look = table::RowLook { lead, cover: None, selected: s.selected == Some(i), current, stripe, height: SONG_ROW_H, px: 8.0, pills };
    table::song_row(t, i, columns, look, p)
        .key(format!("{key}-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
}

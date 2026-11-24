//! The home page: the newest albums as one strip of cards, the songs played last as a table, and
//! the Discover block's albums as a grid; below them the analytics: the library's totals as a
//! strip of tiles, the top artists, albums and songs, the people behind them, the activity
//! heatmap and the genre and decade mix.

use rune::{Cursor, El, ScrollPos, ScrollSpec, Tx, col, el, icon, row, text};

use super::albums::{CARD_GAP, small_card, song_cover};
use super::table;
use super::toolbar::segmented;
use super::*;
use crate::app::{SongMenuTarget, UnlistenedOrder};
use crate::discover::Tab;
use crate::query::property::Property;
use crate::palette::Palette;

/// The song table's columns after the title, which don't follow the views' or the pages'.
const RECENT_COLUMNS: [Property; 3] = [Property::Artist, Property::Album, Property::LastPlayed];
/// Song rows' height, as the artist page's.
const SONG_ROW_H: f32 = 40.0;
/// The recently added cards' width.
const CARD_W: f32 = 200.0;
/// The recently added strip's viewport, which scrolls sideways.
const STRIP_KEY: &str = "recent-albums-strip";

pub(super) fn home_page(s: &App, p: &Palette) -> El {
    let body = col()
        .key("home-page-body")
        .w_full()
        .px(28.0)
        .py(16.0)
        .gap(28.0)
        .child(recent_albums(s, p))
        .child(recent_songs(s, p))
        .child(discovery_block(s, p))
        .child(stats_strip(s, p))
        .child(top_block(s, p))
        .child(people_block(s, p))
        .child(activity_block(s, p))
        .child(mix_block(s, p));
    col().key("home-page").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// Top artists, albums and songs over the chosen window, side by side; they wrap on narrow
/// widths. The window switch sits above them.
fn top_block(s: &App, p: &Palette) -> El {
    let choose = segmented("top-window", &crate::top::Window::ALL.map(|w| (w.id(), w.label())), s.top_window.id(), p, |s: &mut App, id: &str| {
        if let Some(window) = crate::top::Window::ALL.into_iter().find(|w| w.id() == id) {
            s.set_top_window(window);
        }
    });
    let head = row().w_full().items_center().gap(12.0).child(text("Top").font(HEADING).color(p.tx).grow(1.0).truncate()).child(choose);
    let top = s.top_model();
    if top.songs.is_empty() {
        let line = format!("No plays in the {}.", s.top_window.label().to_lowercase());
        return col().key("top").w_full().gap(10.0).child(head).child(text(line).key("top-empty").font(BODY).color(p.tx3));
    }
    let artists = top.artists.iter().enumerate().fold(col().key("top-artists").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let name = r.item.clone();
        c.child(top_row(format!("top-artist{n}"), &r.item, r.plays, p).role("link").cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx| s.show_artist(&name, tx)))
    });
    let albums = top.albums.iter().enumerate().fold(col().key("top-albums").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let (a, title) = (r.item, s.albums()[r.item].title.clone());
        c.child(top_row(format!("top-album{n}"), &title, r.plays, p).role("link").cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx| s.show_album(a, tx)))
    });
    let songs = top.songs.iter().enumerate().fold(col().key("top-songs").grow(1.0).min_w(220.0).gap(4.0), |c, (n, r)| {
        let t = &s.tracks()[r.item];
        c.child(top_row(format!("top-song{n}"), &t.title, r.plays, p))
    });
    let columns = row().key("top-columns").w_full().gap(24.0).flex_wrap().child(artists).child(albums).child(songs);
    col().key("top").w_full().gap(10.0).child(head).child(columns)
}

/// The mix block: the genres and the decades as two bar lists side by side, wrapping on narrow
/// widths. Each says whether its shares are of the plays or of the library.
fn mix_block(s: &App, p: &Palette) -> El {
    let mixes = s.mix_model();
    let genres = bar_list("mix-genres", "Genres", &mixes.0, p);
    let decades = bar_list("mix-decades", "Decades", &mixes.1, p);
    let body = row().key("mix-columns").w_full().gap(24.0).flex_wrap().child(genres).child(decades);
    col().key("mix").w_full().gap(10.0).child(text("What it sounds like").font(HEADING).color(p.tx)).child(body)
}

/// One bar list: a heading that says whether its shares are of the plays or the library, then a
/// bar per name, its width its share of the whole. A list with no songs says so.
fn bar_list(key: &str, title: &str, mix: &crate::mix::Mix, p: &Palette) -> El {
    let source = match mix.source {
        crate::mix::Source::Plays => "of plays",
        crate::mix::Source::Library => "of the library (nothing played yet)",
    };
    let mut list = col().key(key.to_string()).grow(1.0).min_w(260.0).gap(6.0).child(text(format!("{title}, {source}")).key(format!("{key}-title")).font(SMALL).color(p.tx3));
    if mix.bars.is_empty() {
        return list.child(text(format!("No {} yet.", title.to_lowercase())).key(format!("{key}-empty")).font(BODY).color(p.tx3));
    }
    for (n, bar) in mix.bars.iter().enumerate() {
        let track = el().key(format!("{key}-track{n}")).w_full().h(8.0).radius(4.0).fill(p.ui.alpha(0.35)).child(el().key(format!("{key}-bar{n}")).w_frac(bar.share as f32).h(8.0).radius(4.0).fill(p.accent));
        list = list.child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(text(bar.name.clone()).font(SMALL).color(p.tx).min_w(90.0).truncate())
                .child(track.grow(1.0).min_w(0.0))
                .child(text(format!("{:.0}%", bar.share * 100.0)).font(SMALL).color(p.tx3)),
        );
    }
    list
}

/// The activity block: the heatmap of plays per day over the last weeks, with the current and
/// longest streak. Under a few active days it says so instead.
fn activity_block(s: &App, p: &Palette) -> El {
    let a = s.activity();
    let head = text("Activity").font(HEADING).color(p.tx);
    if !a.enough() {
        let line = "Play a few more days to see your activity here.";
        return col().key("activity").w_full().gap(10.0).child(head).child(text(line).key("activity-empty").font(BODY).color(p.tx3));
    }
    let mut grid = row().key("activity-grid").gap(3.0);
    for week in a.grid.iter() {
        let mut column = col().gap(3.0);
        for cell in week.iter() {
            let (key, level) = match cell {
                Some(c) => (format!("activity-day-{}", c.day), crate::activity::intensity(c.plays, a.busiest)),
                None => (String::new(), 0),
            };
            let fill = if level == 0 { p.ui.alpha(0.25) } else { p.accent.alpha(0.25 * f32::from(level) + 0.1) };
            let mut cellel = el().size(11.0, 11.0).radius(2.0).fill(fill);
            if !key.is_empty() {
                cellel = cellel.key(key);
            }
            column = column.child(cellel);
        }
        grid = grid.child(column);
    }
    let streaks = format!("Current streak {} day{}  •  Longest {} day{}", a.streak.current, plural(a.streak.current), a.streak.longest, plural(a.streak.longest));
    col()
        .key("activity")
        .w_full()
        .gap(10.0)
        .child(head)
        .child(grid)
        .child(text(streaks).key("activity-streaks").font(SMALL).color(p.tx3))
}

/// "s" unless `n` is 1.
fn plural(n: u32) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The people behind the music: each role's top names over the chosen window, one row per role
/// with its names as chips. Roles nobody has played on are left out.
fn people_block(s: &App, p: &Palette) -> El {
    let boards = s.people_model();
    if boards.is_empty() {
        return col().key("people").w_full().gap(10.0).child(text("The people behind your music").font(HEADING).color(p.tx)).child(text("No credited names played in the window.").key("people-empty").font(BODY).color(p.tx3));
    }
    let mut body = col().key("people-roles").w_full().gap(8.0);
    for board in boards.iter() {
        let mut chips = row().w_full().gap(6.0).flex_wrap().items_center().child(text(board.role.label().to_string()).font(SMALL).color(p.tx3).min_w(90.0));
        for (n, r) in board.names.iter().enumerate() {
            let name = r.item.clone();
            chips = chips.child(
                row()
                    .key(format!("people-{}-{n}", board.role.label().to_lowercase().replace(' ', "-")))
                    .h(24.0)
                    .px(10.0)
                    .radius(12.0)
                    .center_y()
                    .fill(p.ui.alpha(0.45))
                    .hover(p.ui.alpha(0.8))
                    .cursor(Cursor::Pointer)
                    .role("link")
                    .label(name.clone())
                    .on_press_tx(move |s: &mut App, tx| s.show_artist(&name, tx))
                    .child(text(format!("{}  {}", r.item, r.plays)).font(SMALL).color(p.tx)),
            );
        }
        body = body.child(chips);
    }
    col().key("people").w_full().gap(10.0).child(text("The people behind your music").font(HEADING).color(p.tx)).child(body)
}

/// One leaderboard row: its name and its plays.
fn top_row(key: String, name: &str, plays: u32, p: &Palette) -> El {
    row()
        .key(key)
        .w_full()
        .px(6.0)
        .py(3.0)
        .radius(4.0)
        .gap(8.0)
        .items_center()
        .hover(p.ui.alpha(0.3))
        .child(text(name.to_string()).font(BODY).color(p.tx).grow(1.0).truncate())
        .child(text(plays.to_string()).font(SMALL).color(p.tx3))
}

/// The stats strip: the library's totals as a row of tiles, each a figure over its label.
fn stats_strip(s: &App, p: &Palette) -> El {
    let t = s.totals();
    if t.songs == 0 {
        return section_empty("stats", "Library", "No songs in the library yet.", p);
    }
    let played = match t.played_share() {
        Some(share) => format!("{:.0}%", share * 100.0),
        None => "–".to_string(),
    };
    let tiles = [
        ("Songs", t.songs.to_string()),
        ("Albums", t.albums.to_string()),
        ("Artists", t.artists.to_string()),
        ("Hours", format!("{:.1}", t.hours())),
        ("Albums played", played),
        ("Streak", streak_figure(s)),
    ];
    let mut strip = row().key("stats-tiles").w_full().gap(10.0);
    for (label, value) in tiles {
        strip = strip.child(stat_tile(label, &value, p));
    }
    col().key("stats").w_full().gap(10.0).child(text("Library").font(HEADING).color(p.tx)).child(strip)
}

/// The streak tile's figure: the current run, in days.
fn streak_figure(s: &App) -> String {
    let streak = s.activity().streak;
    format!("{} day{}", streak.current, plural(streak.current))
}

/// A stats tile: its figure large, its label under it, on a faint panel.
fn stat_tile(label: &str, value: &str, p: &Palette) -> El {
    let slug = label.to_lowercase().replace(' ', "-");
    col()
        .key(format!("stat-tile-{slug}"))
        .grow(1.0)
        .min_w(0.0)
        .padding(12.0)
        .gap(2.0)
        .radius(6.0)
        .fill(p.ui.alpha(0.35))
        .child(text(value.to_string()).key(format!("stat-{slug}")).font(HEADING).color(p.tx).truncate())
        .child(text(label.to_string()).font(SMALL).color(p.tx3).truncate())
}

/// Recently Added: the newest albums as one row of cards across the page, scrolling sideways
/// from the arrow buttons at either end.
fn recent_albums(s: &App, p: &Palette) -> El {
    let albums = s.recent_albums();
    if albums.is_empty() {
        return section_empty("recent-albums", "Recently Added", "No albums in the library yet.", p);
    }
    let mut strip = row().gap(CARD_GAP).items_start();
    for &o in albums.iter() {
        strip = strip.child(small_card(s, "recent-albums", o, true, CARD_W, p));
    }
    let (at, width) = (s.home_strip.clone(), s.home_viewport.clone());
    let viewport = col()
        .key(STRIP_KEY)
        .grow(1.0)
        .min_w(0.0)
        .scroll_x()
        .on_scroll_offset(move |pos: ScrollPos| {
            at.set([pos.offset[0], pos.max[0]]);
            width.set(pos.viewport[0]);
        })
        .child(strip);
    let nav = row()
        .w_full()
        .items_center()
        .gap(6.0)
        .child(arrow_button("recent-albums-prev", &icons::BACK, "Scroll back", p).on_press_tx(|s: &mut App, tx| page_strip(s, tx, -1.0)))
        .child(viewport)
        .child(arrow_button("recent-albums-next", &icons::FORWARD, "Scroll forward", p).on_press_tx(|s: &mut App, tx| page_strip(s, tx, 1.0)));
    col().key("recent-albums").w_full().gap(10.0).child(text("Recently Added").font(HEADING).color(p.tx)).child(nav)
}

/// Scrolls the Home strip by a page of whole cards: `dir` is -1 back, 1 forward, clamped to its ends.
fn page_strip(s: &mut App, tx: &mut Tx, dir: f32) {
    let [at, max] = s.home_strip.get();
    let viewport = s.home_viewport.get();
    let pitch = CARD_W + CARD_GAP;
    let cards = ((viewport + CARD_GAP) / pitch).floor().max(1.0);
    tx.scroll_to(STRIP_KEY, (at + dir * cards * pitch).clamp(0.0, max), 0.0);
}

/// A small round icon button, at the ends of the Home strip or beside Unlistened Releases.
fn arrow_button(key: &'static str, glyph: &'static Icon, help: &str, p: &Palette) -> El {
    el()
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
        .child(icon(glyph, 16.0).color(p.tx2))
}

/// Recently Played: the songs played last, most recent at the top, as a table.
fn recent_songs(s: &App, p: &Palette) -> El {
    let list = s.recent_songs();
    if list.is_empty() {
        return section_empty("recent-songs", "Recently Played", "Nothing played yet.", p);
    }
    let columns = table::Columns { shown: &RECENT_COLUMNS, widths: &s.views.page_widths, pills: &[], covers: true };
    let mut body = col().child(table::header("recent-songs-header", "", columns, 8.0, p));
    for (n, &i) in list.iter().enumerate() {
        body = body.child(song_line(s, i, &list, !n.is_multiple_of(2), columns, p));
    }
    let table = table::scrolling("recent-songs-table".into(), columns, 8.0, body, p);
    col().key("recent-songs").w_full().gap(8.0).child(text("Recently Played").font(HEADING).color(p.tx)).child(table)
}

/// The discovery block: one heading with a switch between its tabs, over the chosen tab's albums
/// as a wrapping grid of cards. Unlistened is the albums with no plays, in the order chosen.
fn discovery_block(s: &App, p: &Palette) -> El {
    let switch = segmented("discover-tab", &Tab::ALL.map(|t| (t.id(), t.label())), s.discover.id(), p, |s: &mut App, id: &str| {
        if let Some(tab) = Tab::ALL.into_iter().find(|t| t.id() == id) {
            s.set_discover(tab);
        }
    });
    let head = row().w_full().items_center().gap(12.0).child(text("Discover").font(HEADING).color(p.tx).grow(1.0).truncate()).child(switch);
    let body = match s.discover {
        Tab::Unlistened => unlistened_albums(s, p),
        tab => discover_wall(s, tab, p),
    };
    col().key("discover").w_full().gap(10.0).child(head).child(body)
}

/// Unlistened Releases' controls and albums: the albums with no plays, as a wrapping grid of
/// cards, in the chosen order. Random has a button to re-roll its selection.
fn unlistened_albums(s: &App, p: &Palette) -> El {
    let albums = s.unlistened_albums();
    let order = segmented("unlistened-order", &UnlistenedOrder::ALL.map(|o| (o.id(), o.label())), s.unlistened_order.id(), p, |s: &mut App, id: &str| {
        if let Some(order) = UnlistenedOrder::ALL.into_iter().find(|o| o.id() == id) {
            s.set_unlistened_order(order);
        }
    });
    let mut controls = row().gap(8.0).center_y().child(order);
    if s.unlistened_order == UnlistenedOrder::Random {
        controls = controls.child(arrow_button("unlistened-reroll", &icons::SHUFFLE, "Shuffle", p).on_press(|s: &mut App| s.reroll_unlistened()));
    }
    let head = row().w_full().items_center().gap(12.0).child(text("Unlistened Releases").font(HEADING).color(p.tx).grow(1.0).truncate()).child(controls);
    let body: El = if albums.is_empty() {
        text("Every album has been played.").key("unlistened-albums-empty").font(BODY).color(p.tx3)
    } else {
        let mut wall = row().key("unlistened-albums-wall").w_full().gap(CARD_GAP).flex_wrap();
        for &o in albums.iter() {
            wall = wall.child(small_card(s, "unlistened-albums", o, true, CARD_W, p));
        }
        wall
    };
    col().key("unlistened-albums").w_full().gap(10.0).child(head).child(body)
}

/// The albums of a discovery tab other than Unlistened, as cards with a line of why each is
/// listed. The tab's empty line says why there's nothing.
fn discover_wall(s: &App, tab: Tab, p: &Palette) -> El {
    let albums = s.discover_albums(tab);
    let key = format!("discover-{}", tab.id());
    if albums.is_empty() {
        return text(discover_empty(tab)).key(format!("{key}-empty")).font(BODY).color(p.tx3);
    }
    let mut wall = row().key(format!("{key}-wall")).w_full().gap(CARD_GAP).flex_wrap();
    for &o in albums.iter() {
        wall = wall.child(small_card(s, &key, o, true, CARD_W, p));
    }
    wall
}

/// Why a discovery tab has no albums.
fn discover_empty(tab: Tab) -> String {
    match tab {
        Tab::Unlistened => "Every album has been played.".into(),
        Tab::Continue => "No album is part way through yet.".into(),
        Tab::Rediscover => format!("No album has {} plays and was last played more than {} days ago.", crate::discover::REDISCOVER_PLAYS, crate::discover::REDISCOVER_DAYS),
        Tab::New => format!("No album was added in the last {} days.", crate::discover::NEW_DAYS),
    }
}

/// A section's title over a muted line saying why it's empty.
fn section_empty(key: &str, title: &str, line: &str, p: &Palette) -> El {
    col()
        .key(key.to_string())
        .w_full()
        .gap(8.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx))
        .child(text(line.to_string()).key(format!("{key}-empty")).font(BODY).color(p.tx3))
}

/// A song row: a click selects it, a double-click plays the list from it. Its album's cover
/// leads, with the playing or paused icon over it when it's the current song.
fn song_line(s: &App, i: usize, list: &[usize], stripe: bool, columns: table::Columns, p: &Palette) -> El {
    let t = &s.tracks()[i];
    let current = s.current == Some(i);
    let cover = song_cover(s, format!("recent-cover{i}"), i, table::COVER_W, p);
    let look = table::RowLook { lead: el(), cover: Some(cover), selected: s.selected == Some(i), current, stripe, height: SONG_ROW_H, px: 8.0, pills: Vec::new() };
    let list = list.to_vec();
    table::song_row(t, i, columns, look, &[], p)
        .key(format!("recent-songs-t{i}"))
        .label(format!("{} by {}", t.title, t.artist().unwrap_or_else(|| "unknown artist".into())))
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut App| s.selected = Some(i))
        .on_double_press_tx(move |s: &mut App, tx| s.play_list(list.clone(), i, tx))
        .on_secondary_press(move |s: &mut App, at, _tx| s.song_menu = Some((SongMenuTarget::Track(i), at)))
}

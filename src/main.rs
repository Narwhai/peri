//! A desktop music player built with Rune.
//!
//! `cargo run --release -- ~/Music` opens the window and scans the folders and files given.
//! `-- --snap out.png --scene library` renders a demo library headless.

mod activity;
mod album;
mod app;
mod artist;
mod audio;
mod query;
mod covers;
mod pages;
mod collab;
mod credits;
mod discover;
mod details;
mod graph;
mod icons;
mod label;
mod lastfm;
mod library;
mod listen;
mod mix;
mod palette;
mod people;
mod pictures;
mod plays;
mod queue;
mod relations;
mod search;
mod stats;
mod top;
mod view;

#[cfg(test)]
mod album_ui_tests;
#[cfg(test)]
mod artist_ui_tests;
#[cfg(test)]
mod table_ui_tests;
#[cfg(test)]
mod toolbar_ui_tests;
#[cfg(test)]
mod graph_ui_tests;
#[cfg(test)]
mod home_ui_tests;
#[cfg(test)]
mod label_ui_tests;
#[cfg(test)]
mod picture_ui_tests;
#[cfg(test)]
mod queue_panel_ui_tests;
#[cfg(test)]
mod queue_ui_tests;

#[cfg(test)]
mod queue_menu_ui_tests;

#[cfg(test)]
mod lastfm_ui_tests;
#[cfg(test)]
mod scroll_ui_tests;
#[cfg(test)]
mod settings_ui_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod zoom_ui_tests;

use app::{App, Output, Panel};
use query::filter::{Condition, Conjunction, Group, Node, Operator};
use query::property::Property;
use query::store::Store;
use query::property::Mode;
use query::view::{Direction, GroupBy, Layout, Sort, View};
use library::Track;
use rune::AppSpec;

fn init() -> App {
    // Headless renders and scripts never open the audio device or touch saved views.
    if rune::is_headless() { App::new(Output::Silent, Store::memory()) } else { App::new(Output::Closed, Store::open()) }
}

fn setup(s: &mut App, waker: rune::Waker) {
    s.setup(waker);
}

/// A made-up library for headless scenes.
pub(crate) fn demo_tracks() -> Vec<Track> {
    type Album = (&'static str, &'static str, &'static str, u32, &'static [(&'static str, f64)]);
    let albums: [Album; 4] = [
        ("Laraaji", "Day of Radiance", "Ambient;New Age", 1980, &[("The Dance #1", 1192.0), ("The Dance #2", 488.0), ("Meditation #1", 1281.0)]),
        ("NewJeans", "Get Up", "K-Pop;Pop", 2023, &[("New Jeans", 108.0), ("Super Shy", 154.0), ("ETA", 151.0), ("Cool With You", 147.0)]),
        ("Nils Frahm", "Spaces", "Ambient;Electronic", 2013, &[("An Aborted Beginning", 192.0), ("Says", 499.0), ("Hammers", 393.0), ("Over There, It's Raining", 312.0)]),
        ("Big Thief;Laraaji", "Live Sessions", "Folk", 2024, &[("Mary", 260.0), ("Change", 281.0)]),
    ];
    let mut tracks = Vec::new();
    for (artist, album, genres, year, songs) in albums {
        for (n, (title, secs)) in songs.iter().enumerate() {
            tracks.push(Track {
                path: format!("/demo/{artist}/{album}/{n:02} {title}.flac").into(),
                title: (*title).into(),
                credit: (!artist.contains(';')).then(|| artist.to_string()),
                tagged: library::TaggedArtists { artists: if artist.contains(';') { artist.split(';').map(String::from).collect() } else { Vec::new() }, ..Default::default() },
                album_artist: Some(artist.replace(';', " & ")),
                album: Some(album.into()),
                genres: genres.split(';').map(String::from).collect(),
                year: Some(year),
                disc: Some(1),
                number: Some(n as u32 + 1),
                duration: Some(*secs),
                added: Some(1_700_000_000 + i64::from(year) * 1000),
                ..Track::default()
            });
        }
    }
    library::sort(&mut tracks);
    tracks
}

fn scene_library(s: &mut App) {
    s.set_library(demo_tracks(), false);
}

fn scene_playing(s: &mut App) {
    scene_library(s);
    let eta = s.tracks().iter().position(|t| t.title == "ETA").expect("demo has ETA");
    s.current = Some(eta);
    s.selected = Some(eta);
    s.playing = true;
    s.position = 61.0;
    s.shuffle = true;
    s.repeat = queue::Repeat::All;
}

fn scene_search(s: &mut App) {
    scene_playing(s);
    s.search = rune::TextEdit::new("laraaji");
    s.page = app::Page::Search;
}

fn scene_error(s: &mut App) {
    scene_playing(s);
    s.error = Some("Can't play broken.mp3: unrecognized format".into());
}

/// A second view grouped by genre, newest first: Laraaji shows under Ambient and New Age.
fn scene_grouped(s: &mut App) {
    scene_playing(s);
    let mut view = View::new("By genre");
    view.group_by = Some(GroupBy { property: Property::Genre, direction: Direction::Asc });
    view.sorts = vec![Sort { property: Property::Year, direction: Direction::Desc }];
    s.views.list.push(view);
    s.views.active = 1;
    s.views_changed();
}

/// A nested filter open in its popover: year ≥ 2000 and (ambient or by Laraaji).
fn scene_filter(s: &mut App) {
    scene_grouped(s);
    let cond = |property, op, value: &str| Node::Condition(Condition { property, op, value: value.into() });
    s.views.active_mut().filter = Group {
        conjunction: Conjunction::And,
        children: vec![
            cond(Property::Year, Operator::Ge, "2000"),
            Node::Group(Group {
                conjunction: Conjunction::Or,
                children: vec![cond(Property::Genre, Operator::Has, "Ambient"), cond(Property::Artist, Operator::Has, "Laraaji")],
            }),
        ],
    };
    s.views_changed();
    s.panel = Some(Panel::Filter);
}

fn scene_views(s: &mut App) {
    scene_grouped(s);
    s.panel = Some(Panel::Views);
}

fn scene_sort(s: &mut App) {
    scene_grouped(s);
    s.panel = Some(Panel::Sort);
}

/// An album view sorted by album artist, so each artist's albums sit together.
fn album_view(s: &mut App, layout: Layout) {
    scene_playing(s);
    let mut view = View::new("Albums");
    view.set_mode(Mode::Albums);
    view.layout = layout;
    view.sorts = vec![Sort { property: Property::AlbumArtist, direction: Direction::Asc }, Sort { property: Property::Year, direction: Direction::Desc }];
    s.views.list.push(view);
    s.views.active = s.views.list.len() - 1;
    s.views_changed();
}

fn scene_album_grid(s: &mut App) {
    album_view(s, Layout::Grid);
}

fn scene_album_list(s: &mut App) {
    album_view(s, Layout::List);
}

/// The demo library plus Mercy's "Calypso Gene": a collaborative album with featured guests.
fn scene_artist(s: &mut App) {
    let mut tracks = demo_tracks();
    let ids = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    tracks.push(Track {
        path: "/demo/Mercy/03 Calypso Gene.flac".into(),
        title: "Calypso Gene".into(),
        credit: Some("Armand Hammer & The Alchemist feat. Silka & Cleo Reed".into()),
        album_artist: Some("Armand Hammer & The Alchemist".into()),
        tagged: library::TaggedArtists {
            artists: ids(&["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"]),
            artist_ids: ids(&["ah", "alc", "silka", "cleo"]),
            album_artists: Vec::new(),
            album_artist_ids: ids(&["ah", "alc"]),
        },
        album: Some("Mercy".into()),
        genres: vec!["Hip Hop".into()],
        year: Some(2025),
        number: Some(3),
        duration: Some(201.0),
        credits: vec![credits::Credit::parse(credits::Role::Producer, "The Alchemist"), credits::Credit::parse(credits::Role::Performer, "Cleo Reed (vocals)")],
        ..Track::default()
    });
    s.set_library(tracks, false);
    let i = s.tracks().iter().position(|t| t.title == "Calypso Gene").expect("demo has Calypso Gene");
    s.current = Some(i);
    s.playing = true;
    s.page = app::Page::Artist("The Alchemist".into());
}

/// The Alchemist merged with another name, linked to a band and an alias.
fn scene_related(s: &mut App) {
    scene_artist(s);
    s.set_relations(relations::Relations {
        merges: vec![vec!["The Alchemist".into(), "Alan Maman".into()]],
        links: vec![
            relations::Link { from: "The Alchemist".into(), kind: relations::LinkKind::MemberOf, to: "Step Brothers".into() },
            relations::Link { from: "Armand Hammer".into(), kind: relations::LinkKind::Alias, to: "the alchemist".into() },
        ],
    });
    let i = s.tracks().iter().position(|t| t.title == "Calypso Gene").expect("demo has Calypso Gene");
    s.edit_track(i, |t| t.credits.push(credits::Credit::parse(credits::Role::Composer, "Alan Maman")));
    s.artist_with_aliases = true;
}

/// The Link artist popover open on The Alchemist's page, a name typed.
fn scene_link(s: &mut App) {
    scene_related(s);
    s.artist_with_aliases = false;
    s.open_link_artist();
    s.link_choice = relations::LinkChoice::Alias;
    s.link_name = rune::TextEdit::new("ar");
}

/// An artist credited on songs in several roles: their pills line up by role.
fn scene_credits(s: &mut App) {
    use credits::{Credit, Role};
    let mut tracks = demo_tracks();
    let roles: [(&str, &[&str]); 6] = [
        ("New Jeans", &["Composer", "Lyricist"]),
        ("Super Shy", &["Composer", "Lyricist", "Producer"]),
        ("ETA", &["Producer", "Mixer"]),
        ("Cool With You", &["Lyricist"]),
        ("Says", &["Performer (synthesizer)", "Mixer"]),
        ("Mary", &["Composer", "Producer", "Remixer"]),
    ];
    for t in &mut tracks {
        for (title, list) in roles {
            if t.title == title {
                for r in list {
                    let (role, detail) = r.split_once(' ').map_or((*r, ""), |(a, b)| (a, b));
                    let role = Role::ALL.into_iter().find(|x| x.label() == role).expect("a role");
                    t.credits.push(Credit::parse(role, &format!("Erika de Casier {detail}")));
                }
            }
        }
    }
    // Her own album: she wrote every song and produced two of the three.
    for (n, title) in ["Lucky", "Ice", "Test It"].into_iter().enumerate() {
        let mut credits = vec![Credit::parse(Role::Lyricist, "Erika de Casier")];
        if n < 2 {
            credits.push(Credit::parse(Role::Producer, "Erika de Casier"));
        }
        tracks.push(Track {
            path: format!("/demo/Erika de Casier/Still/{n:02} {title}.flac").into(),
            title: title.into(),
            credit: Some("Erika de Casier".into()),
            album_artist: Some("Erika de Casier".into()),
            album: Some("Still".into()),
            year: Some(2024),
            number: Some(n as u32 + 1),
            duration: Some(200.0),
            credits,
            ..Track::default()
        });
    }
    s.set_library(tracks, false);
    s.page = app::Page::Artist("Erika de Casier".into());
}

/// The Mercy demo on Armand Hammer and The Alchemist's collaboration page.
fn scene_collab(s: &mut App) {
    scene_artist(s);
    s.page = app::Page::Collab(vec!["Armand Hammer".into(), "The Alchemist".into()]);
}

fn scene_album_page(s: &mut App) {
    album_view(s, Layout::Grid);
    let a = s.albums().iter().position(|a| a.title == "Day of Radiance").expect("demo has Day of Radiance");
    s.page = app::Page::Album(s.albums()[a].id());
}

/// The demo library with Laraaji's and Nils Frahm's albums on one label, on its page.
fn scene_label(s: &mut App) {
    let mut tracks = demo_tracks();
    for t in &mut tracks {
        if matches!(t.album.as_deref(), Some("Day of Radiance" | "Spaces" | "Live Sessions")) {
            t.label = Some("Erased Tapes".into());
        }
    }
    s.set_library(tracks, false);
    s.page = app::Page::Label("Erased Tapes".into());
}

/// The settings page with a saved folder and one that's gone missing.
fn scene_settings(s: &mut App) {
    scene_library(s);
    s.library = vec!["/home/me/Music".into(), "/media/backup-drive/Music".into()];
    s.show_settings();
}

/// The library playing from its third song, with a queue of the whole library: one played song
/// above the current one, two songs Play Next added after it, and the queue panel open.
fn scene_queue_pinned(s: &mut App) {
    scene_queue(s);
    s.queue_pinned = true;
}

fn scene_queue(s: &mut App) {
    scene_library(s);
    let all: Vec<usize> = (0..s.tracks().len()).collect();
    let index = |title: &str| s.tracks().iter().position(|t| t.title == title);
    let added: Vec<usize> = ["ETA", "Says"].iter().filter_map(|t| index(t)).collect();
    let mut rng = queue::Rng::new(7);
    let start = all.get(2).copied().unwrap_or(0);
    let mut queue = queue::Queue::new(all, start, false, &mut rng);
    // Play Next puts the added songs right after the current one, in order.
    queue.play_next(&added);
    s.current = Some(start);
    s.selected = Some(start);
    s.queue = Some(queue);
    s.playing = false;
    s.position = 61.0;
    s.repeat = queue::Repeat::All;
    s.queue_pinned = false;
    s.queue_shown = true;
}

/// The demo library with some songs played, on the Home tab: the newest albums and the songs
/// played last. Each album's `added` is its year, so the newest is Live Sessions.
fn scene_home(s: &mut App) {
    s.set_library(demo_tracks(), false);
    for (n, title) in ["Hammers", "New Jeans", "Says", "ETA", "Mary"].into_iter().enumerate() {
        if let Some(i) = s.tracks().iter().position(|t| t.title == title) {
            // Oldest first, so the most recent is at the top.
            let _ = s.record_play(i, 1_800_000_000 + n as i64 * 60);
        }
    }
    s.show_home();
}

/// The artist and related scenes' library on the Graph tab, The Alchemist picked.
fn scene_graph(s: &mut App) {
    scene_related(s);
    s.show_graph();
    s.graph.find = rune::TextEdit::new("alchemist");
    s.graph_found();
}

fn main() {
    // `rune::run` is `App::new(spec).run()`; `App` alone takes the close hook.
    rune::App::new(AppSpec {
        scenes: &[
            ("library", scene_library),
            ("playing", scene_playing),
            ("search", scene_search),
            ("error", scene_error),
            ("grouped", scene_grouped),
            ("filter", scene_filter),
            ("views", scene_views),
            ("sort", scene_sort),
            ("albums", scene_album_grid),
            ("album-list", scene_album_list),
            ("album", scene_album_page),
            ("artist", scene_artist),
            ("credits", scene_credits),
            ("related", scene_related),
            ("link", scene_link),
            ("label", scene_label),
            ("collab", scene_collab),
            ("settings", scene_settings),
            ("graph", scene_graph),
            ("home", scene_home),
            ("queue", scene_queue),
            ("queue-pinned", scene_queue_pinned),
        ],
        min_size: Some(view::MIN_SIZE),
        setup: Some(setup),
        poll: Some(app::poll),
        on_key: Some(app::on_key),
        ..AppSpec::new("Peri", (980.0, 640.0), init, view::view)
    })
    .on_close(app::on_close)
    .run();
}

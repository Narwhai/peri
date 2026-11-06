//! A desktop music player built with Rune.
//!
//! `cargo run --release -- ~/Music` opens the window and scans the folders and files given.
//! `-- --snap out.png --scene library` renders a demo library headless.

mod album;
mod app;
mod audio;
mod base;
mod covers;
mod icons;
mod library;
mod palette;
mod queue;
mod view;

#[cfg(test)]
mod album_ui_tests;
#[cfg(test)]
mod base_ui_tests;
#[cfg(test)]
mod tests;

use app::{App, Output, Panel};
use base::filter::{Condition, Conjunction, Group, Node, Operator};
use base::property::Property;
use base::store::Store;
use base::property::Mode;
use base::view::{Direction, GroupBy, Layout, Sort, View};
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
fn demo_tracks() -> Vec<Track> {
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
                artists: artist.split(';').map(String::from).collect(),
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
    let eta = s.tracks.iter().position(|t| t.title == "ETA").expect("demo has ETA");
    s.current = Some(eta);
    s.selected = Some(eta);
    s.playing = true;
    s.position = 61.0;
    s.shuffle = true;
    s.repeat = queue::Repeat::All;
}

fn scene_search(s: &mut App) {
    scene_playing(s);
    s.search = rune::TextEdit::new("nils");
    s.refilter();
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
    s.base.views.push(view);
    s.base.active = 1;
    s.views_changed();
}

/// A nested filter open in its popover: year ≥ 2000 and (ambient or by Laraaji).
fn scene_filter(s: &mut App) {
    scene_grouped(s);
    let cond = |property, op, value: &str| Node::Condition(Condition { property, op, value: value.into() });
    s.base.active_mut().filter = Group {
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
    s.base.views.push(view);
    s.base.active = s.base.views.len() - 1;
    s.views_changed();
}

fn scene_album_grid(s: &mut App) {
    album_view(s, Layout::Grid);
}

fn scene_album_list(s: &mut App) {
    album_view(s, Layout::List);
}

fn scene_album_page(s: &mut App) {
    album_view(s, Layout::Grid);
    let a = s.albums.iter().position(|a| a.title == "Day of Radiance").expect("demo has Day of Radiance");
    s.page = app::Page::Album(s.albums[a].id());
}

fn main() {
    rune::run(AppSpec {
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
        ],
        min_size: Some((720.0, 420.0)),
        setup: Some(setup),
        poll: Some(app::poll),
        on_key: Some(app::on_key),
        ..AppSpec::new("Rune Music", (980.0, 640.0), init, view::view)
    });
}

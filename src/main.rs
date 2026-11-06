//! A desktop music player built with Rune.
//!
//! `cargo run --release -- ~/Music` opens the window and scans the folders and files given.
//! `-- --snap out.png --scene library` renders a demo library headless.

mod app;
mod audio;
mod icons;
mod library;
mod palette;
mod queue;
mod view;

#[cfg(test)]
mod tests;

use app::{App, Output};
use library::Track;
use rune::AppSpec;

fn init() -> App {
    // Headless renders and scripts never open the audio device.
    App::new(if rune::is_headless() { Output::Silent } else { Output::Closed })
}

fn setup(s: &mut App, waker: rune::Waker) {
    s.setup(waker);
}

/// A made-up library for headless scenes.
fn demo_tracks() -> Vec<Track> {
    type Album = (&'static str, &'static str, &'static [(&'static str, f64)]);
    let albums: [Album; 3] = [
        ("Laraaji", "Day of Radiance", &[("The Dance #1", 1192.0), ("The Dance #2", 488.0), ("Meditation #1", 1281.0)]),
        ("NewJeans", "Get Up", &[("New Jeans", 108.0), ("Super Shy", 154.0), ("ETA", 151.0), ("Cool With You", 147.0)]),
        ("Nils Frahm", "Spaces", &[("An Aborted Beginning", 192.0), ("Says", 499.0), ("Hammers", 393.0), ("Over There, It's Raining", 312.0)]),
    ];
    let mut tracks = Vec::new();
    for (artist, album, songs) in albums {
        for (n, (title, secs)) in songs.iter().enumerate() {
            tracks.push(Track {
                path: format!("/demo/{artist}/{album}/{n:02} {title}.flac").into(),
                title: (*title).into(),
                artist: Some(artist.into()),
                album: Some(album.into()),
                disc: Some(1),
                number: Some(n as u32 + 1),
                duration: Some(*secs),
            });
        }
    }
    tracks
}

fn scene_library(s: &mut App) {
    s.set_library(demo_tracks(), false);
}

fn scene_playing(s: &mut App) {
    scene_library(s);
    s.current = Some(5);
    s.selected = Some(5);
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

fn main() {
    rune::run(AppSpec {
        scenes: &[("library", scene_library), ("playing", scene_playing), ("search", scene_search), ("error", scene_error)],
        min_size: Some((720.0, 420.0)),
        setup: Some(setup),
        poll: Some(app::poll),
        on_key: Some(app::on_key),
        ..AppSpec::new("Rune Music", (980.0, 640.0), init, view::view)
    });
}

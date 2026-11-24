//! Pictures already in the music folders: an `artist.jpg` (or `.jpeg`, `.png`, `.webp`) in the
//! artist's folder (`Artist/Album/song.flac` → `Artist/artist.jpg`) or in an album folder that is
//! theirs alone. They're shown where they are, never copied, and only when the user gave the
//! artist no picture of their own.

use std::path::{Path, PathBuf};

use crate::album::Album;
use crate::artist::ArtistPage;
use crate::library::Track;

/// The file names taken, in order of preference.
const NAMES: [&str; 4] = ["artist.jpg", "artist.jpeg", "artist.png", "artist.webp"];

/// The picture in `dir`, by any case of [`NAMES`].
fn in_dir(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    NAMES.iter().find_map(|name| entries.iter().find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name))).cloned())
}

/// Folder names compared loosely: case, punctuation and spacing aside ("AC/DC" is "ACDC").
fn loose(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// A folder of one disc of an album ("CD1", "Disc 2"), whose album folder is its parent.
fn is_disc(dir: &Path) -> bool {
    let name = dir.file_name().and_then(|n| n.to_str()).map(loose).unwrap_or_default();
    ["cd", "disc", "disk"].iter().any(|p| name.strip_prefix(p).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
}

/// The folders that may hold the page's picture, most likely first: the artist folders their
/// albums sit in (named after one of their names), then album folders that are theirs alone.
pub fn candidates(info: &ArtistPage, tracks: &[Track], albums: &[Album]) -> Vec<PathBuf> {
    let names: Vec<String> = info.names.iter().map(|n| loose(n)).filter(|n| !n.is_empty()).collect();
    let mut artist_dirs = Vec::new();
    let mut album_dirs = Vec::new();
    for &a in &info.albums {
        let album = &albums[a];
        let Some(dir) = album.tracks.first().and_then(|&i| tracks[i].path.parent()) else { continue };
        let dir = if is_disc(dir) { dir.parent().unwrap_or(dir) } else { dir };
        if let Some(parent) = dir.parent()
            && parent.file_name().and_then(|n| n.to_str()).is_some_and(|n| names.contains(&loose(n)))
        {
            artist_dirs.push(parent.to_path_buf());
        }
        if album.album_artists.len() == 1 {
            album_dirs.push(dir.to_path_buf());
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in artist_dirs.into_iter().chain(album_dirs) {
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// The first picture in the page's [`candidates`].
pub fn find(info: &ArtistPage, tracks: &[Track], albums: &[Album]) -> Option<PathBuf> {
    candidates(info, tracks, albums).iter().find_map(|dir| in_dir(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TaggedArtists;
    use crate::relations::Relations;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn song(root: &Path, rel: &str, artist: &str, album: &str) -> Track {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        Track {
            path,
            title: rel.into(),
            credit: Some(artist.into()),
            album_artist: Some(artist.into()),
            tagged: TaggedArtists { artists: vec![artist.into()], ..TaggedArtists::default() },
            album: Some(album.into()),
            ..Track::default()
        }
    }

    fn library(tracks: Vec<Track>) -> (Vec<Track>, Vec<Album>) {
        let mut tracks = tracks;
        crate::credits::resolve(&mut tracks);
        let albums = crate::album::build(&tracks);
        (tracks, albums)
    }

    #[test]
    fn the_artist_folders_picture_wins_over_an_album_folders_and_disc_folders_count() {
        let root = Dir(std::env::temp_dir().join(format!("peri-folder-pictures-{}", std::process::id())));
        let _ = std::fs::remove_dir_all(&root.0);
        let r = &root.0;
        let (tracks, albums) = library(vec![
            song(r, "AC_DC/Highway/CD1/01 a.flac", "AC/DC", "Highway"),
            song(r, "Loose/Album/01 b.flac", "Somebody", "Album"),
            song(r, "Split/01 c.flac", "Duo A & Duo B", "Split"),
        ]);
        let page = |name: &str| crate::artist::page(name, &tracks, &albums, &Relations::default(), false);

        assert_eq!(find(&page("AC/DC"), &tracks, &albums), None);
        std::fs::write(r.join("AC_DC/Highway/Artist.PNG"), b"x").unwrap();
        assert_eq!(find(&page("AC/DC"), &tracks, &albums), Some(r.join("AC_DC/Highway/Artist.PNG")), "the album folder above the disc folder");
        std::fs::write(r.join("AC_DC/artist.jpg"), b"x").unwrap();
        assert_eq!(find(&page("AC/DC"), &tracks, &albums), Some(r.join("AC_DC/artist.jpg")), "the folder named after them comes first");

        // "Loose" isn't named after Somebody: a picture there could be anyone's.
        std::fs::write(r.join("Loose/artist.jpg"), b"x").unwrap();
        assert_eq!(find(&page("Somebody"), &tracks, &albums), None);
    }
}

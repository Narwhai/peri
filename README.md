# Rune Music

A desktop music player built with [Rune](https://github.com/anomalyco/rune). Rune draws the UI on the GPU, and [rodio](https://github.com/RustAudio/rodio) (cpal + symphonia) plays the audio. Tags and cover art come from [lofty](https://github.com/Serial-ATA/lofty-rs). It uses the Tone palette (`../gray-color-scheme/color-palette.json`) and follows the system light/dark setting.

## Run

```sh
cargo run --release -- ~/Music          # scan folders/files given on the command line
cargo run --release                     # then "Open folder…" or drop folders/files on the window
```

On Linux, building needs `libxkbcommon`, `wayland` and `alsa-lib` (PipeWire works through `pipewire-alsa`).

## Features

- Recursive folder scan on a background thread. It reads mp3, flac, ogg, wav, m4a/aac, aiff and caf files.
- Search across title, artist and album.
- A virtualised track list, so large libraries scroll smoothly.
- A now-playing bar with cover art (embedded, else `cover.jpg`/`folder.jpg` next to the file).
- Play/pause, previous/next, a seek bar, volume and mute, shuffle, and repeat (off/all/one).

## Views, filters and grouping

Views work like Obsidian's [Bases](https://help.obsidian.md/bases/views). By default a view lists every song in a flat list. Change it from the toolbar under the header:

- **Views** (top left): create, rename (double-click or ✎), duplicate, delete and switch views. Each view keeps its own filter, sorts and grouping.
- **Sort:** add several sorts and reorder them with ↑. Each sorts A→Z / 0→9 / old→new or the reverse. Songs missing a value always go last.
- **Group:** group by one property into sticky headers with counts. Click a header to collapse it. Songs with several artists, genres or composers show under each.
- **Filter:** add conditions (property, operator, value) to a group whose rule is *all*, *any* or *none* of the following are true. Groups nest up to three levels. The operators offered depend on the property type:
  - text: is, contains, starts with…
  - lists: has, does not have…
  - numbers: = ≠ < ≤ > ≥
  - dates (`YYYY-MM-DD`): before, after…
  - durations (`m:ss`): = ≠ < ≤ > ≥
  - any property: is empty / is not empty

  A condition with no value yet, or one that can't be read (a word where a year goes, shown in red), filters nothing out.

Properties: title, artist, featured artist, album artist, album, genre, year, composer, lyricist, producer, label, track and disc number, duration, format, folder and date added. Year comes from the recording date tag, else year, release date or original release date. Date added is the file's creation time.

Search narrows the current view further, and Next/Previous follow the view's order. In a grouped view, a song that appears in several groups plays once, in its first group.

Views save on every change to `prefs.json` in the app's config folder:
- Linux: `~/.config/rune-music/`
- macOS: `~/Library/Application Support/rune-music/`
- Windows: `%APPDATA%\rune-music\`

Collapsed groups are not saved.

## Albums

Each view lists either **Songs** or **Albums**; switch with the toggle in the toolbar.

- **What counts as one album:** songs with the same album artist (else track artist), album title and year, ignoring case. The same title from two years is two albums, while multi-disc and multi-folder albums merge.
- **Songs without an album tag:** they gather per artist under "Unknown album".
- **Layouts:** album views have **Grid** (covers) or **List** (a card per album with its track table).
- **Filter, sort and group:** these work on album properties: title, album artist, artists, genres, year, composers, label, track count, total duration, formats, folder and date added. For example, sort by album artist to keep each artist's albums together.
- **Search:** finds albums by title or artist, or albums that contain a matching song.
- **Album page:** click a grid card, or a list card's cover or title, to open it. It has the cover, details, Play and Shuffle, the tracks grouped by disc, and "More by" the same artist. Esc or Back returns to the list. Clicking the cover in the now-playing bar opens the playing album.
- **Playback:** a card's play button, Play album, or double-clicking a track plays that album and then continues through the next albums in the view's order.
- **Covers:** read from the first song of each album (embedded art, else `cover.jpg`/`folder.jpg`). They're shrunk to 480 px on background threads, and at most 160 are kept in memory.
- **Saved settings:** the mode and layout save with each view. Views saved before this change load as song views.

## Keys
## Artists

Every artist name is a link: in song rows, titles ("Calypso Gene (feat. Silka & Cleo Reed)"), the now-playing bar, album cards and album pages. It opens the artist's page: their albums, albums they appear on, their songs, and songs crediting them (composer, lyricist, producer, performer…). Back and Esc retrace the pages visited.

- **Who's who:** the `ARTISTS` tag (one value per act) with the MusicBrainz artist IDs names each artist. The artist tag ("A & B feat. C") and the title ("(feat. C)") say who is featured.
- **Collaborative albums:** MusicBrainz album artist IDs are matched to names anywhere in the library, so "Armand Hammer & The Alchemist" counts as an album of each.
- **Files without those tags:** the artist text is split at `feat.`/`ft.`/`featuring`. Main artists split at `&`, `,`, `x`, `and` only when every piece is a name the library already knows, so "Simon & Garfunkel" stays whole.
- **Credits:** composer, lyricist, writer, producer, arranger, conductor, performer (with instrument), engineer, mixer, remixer and DJ mixer tags. Album pages list them.

## Keys

| Key | Action |
| --- | --- |
| Space | Play / pause |
| ← / → | Seek 5 s |
| Ctrl+← / Ctrl+→ | Previous / next track |
| ↑ / ↓, Enter | Move the selection, play it |
| Ctrl+F | Search (Esc clears it, Enter plays the first match) |
| Ctrl+O | Open a folder |
| S / R / M | Shuffle / repeat / mute |
| Esc | Close a toolbar popover, or go back from an album or artist page |

Double-click a row to play it. Ctrl is Cmd on macOS.

## Develop

```sh
cargo test                                                   # unit + headless UI tests
RUNE_MUSIC_DIR=~/Music cargo test --release real_library -- --ignored --nocapture
RUNE_MUSIC_FILE=song.flac cargo test --release real_output -- --ignored   # plays muted
RUNE_MUSIC_DIR=~/Music cargo test --release real_library_groups -- --ignored --nocapture
cargo run -- --snap out.png --scene playing --theme light    # scenes: library playing search error grouped filter views sort albums album-list album artist
```

Headless runs (`--snap`, `--script`, `--tree`) never open the audio device and keep views in memory.

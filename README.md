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

Double-click a row to play it. Ctrl is Cmd on macOS.

## Develop

```sh
cargo test                                                   # unit + headless UI tests
RUNE_MUSIC_DIR=~/Music cargo test --release real_library -- --ignored --nocapture
RUNE_MUSIC_FILE=song.flac cargo test --release real_output -- --ignored   # plays muted
cargo run -- --snap out.png --scene playing --theme light    # scenes: library playing search error
```

Headless runs (`--snap`, `--script`, `--tree`) never open the audio device.

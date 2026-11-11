{
  "id": "f37cccc0",
  "title": "Optional: write play counts to file tags",
  "tags": [
    "play-counts"
  ],
  "status": "open",
  "created_at": "2026-11-11T03:17:26.939Z"
}

Revisit an opt-in setting that writes play counts into the music files through lofty. The play log stays the source of truth.

- MP3 (ID3v2): `POPM` (more common) and/or `PCNT`.
- FLAC/Ogg: `FMPS_PLAYCOUNT` (freedesktop draft, used by Amarok/Strawberry/Clementine).
- M4A: no standard field; decide or skip.
- Concerns: each play rewrites the file and changes its modified time (backups/sync), read-only libraries or unplugged drives, the risk of corrupting a file if a write fails, and other players overwriting the counts.
- Open questions: should we also read existing counts from tags on import? Should we write after every play or in batches?

//! What the song details dialog reads from a file when it opens: its audio properties, IDs,
//! lyrics and every raw tag, as the file's own tag formats hold them. Scanning skips this, so
//! the library stays quick to load.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use lofty::ape::ApeTag;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::id3::v1::Id3v1Tag;
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::iff::aiff::{AiffFile, AiffTextChunks};
use lofty::iff::wav::{RiffInfoList, WavFile};
use lofty::mp4::{AtomData, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OpusFile, SpeexFile, VorbisFile};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue};

/// A file's details beyond what the library scan keeps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    /// File size in bytes.
    pub size: Option<u64>,
    /// kbps.
    pub bitrate: Option<u32>,
    /// Hz.
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    /// Identifier names and values: MusicBrainz IDs, ISRC, barcode, catalogue number.
    pub ids: Vec<(String, String)>,
    pub lyrics: Option<String>,
    /// Every tag of every tag format in the file, as stored: (format, key, value).
    pub raw: Vec<RawTag>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawTag {
    /// The tag format, as in "Vorbis comments" or "ID3v2".
    pub format: &'static str,
    pub key: String,
    pub value: String,
}

/// The identifiers shown, in order, and their labels.
const IDS: &[(ItemKey, &str)] = &[
    (ItemKey::MusicBrainzRecordingId, "MusicBrainz recording"),
    (ItemKey::MusicBrainzTrackId, "MusicBrainz track"),
    (ItemKey::MusicBrainzReleaseId, "MusicBrainz release"),
    (ItemKey::MusicBrainzReleaseGroupId, "MusicBrainz release group"),
    (ItemKey::MusicBrainzArtistId, "MusicBrainz artist"),
    (ItemKey::MusicBrainzReleaseArtistId, "MusicBrainz album artist"),
    (ItemKey::MusicBrainzWorkId, "MusicBrainz work"),
    (ItemKey::Isrc, "ISRC"),
    (ItemKey::Barcode, "Barcode"),
    (ItemKey::CatalogNumber, "Catalogue number"),
];

/// Reads `path`'s details. An error names what couldn't be read.
pub fn read(path: &Path) -> Result<Details, String> {
    let fail = |e: &dyn std::fmt::Display| format!("Couldn't read {}: {e}", path.display());
    let tagged = Probe::open(path).and_then(|p| p.options(ParseOptions::new().read_cover_art(false)).read()).map_err(|e| fail(&e))?;
    let props = tagged.properties();
    let mut ids = Vec::new();
    let mut lyrics = None;
    for tag in tagged.tags() {
        for &(key, label) in IDS {
            for value in tag.get_strings(key) {
                let pair = (label.to_string(), value.trim().to_string());
                if !pair.1.is_empty() && !ids.contains(&pair) {
                    ids.push(pair);
                }
            }
        }
        if lyrics.is_none() {
            lyrics = tag.get_string(ItemKey::Lyrics).map(str::trim).filter(|l| !l.is_empty()).map(String::from);
        }
    }
    Ok(Details {
        size: std::fs::metadata(path).ok().map(|m| m.len()),
        bitrate: props.audio_bitrate().or(props.overall_bitrate()),
        sample_rate: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        ids,
        lyrics,
        raw: raw_tags(path, tagged.file_type()).map_err(|e| fail(&e))?,
    })
}

/// Every tag as the file stores it. Lofty's format-neutral tag drops keys it has no name for,
/// so this reads each format's own tag.
fn raw_tags(path: &Path, file_type: FileType) -> Result<Vec<RawTag>, String> {
    let mut out = Vec::new();
    let mut reader = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let options = ParseOptions::new().read_cover_art(false);
    match file_type {
        FileType::Flac => {
            let f = FlacFile::read_from(&mut reader, options).map_err(|e| e.to_string())?;
            vorbis(f.vorbis_comments(), &mut out);
            id3v2(f.id3v2(), &mut out);
        }
        FileType::Vorbis => vorbis(Some(VorbisFile::read_from(&mut reader, options).map_err(|e| e.to_string())?.vorbis_comments()), &mut out),
        FileType::Opus => vorbis(Some(OpusFile::read_from(&mut reader, options).map_err(|e| e.to_string())?.vorbis_comments()), &mut out),
        FileType::Speex => vorbis(Some(SpeexFile::read_from(&mut reader, options).map_err(|e| e.to_string())?.vorbis_comments()), &mut out),
        FileType::Mpeg => {
            let f = MpegFile::read_from(&mut reader, options).map_err(|e| e.to_string())?;
            id3v2(f.id3v2(), &mut out);
            ape(f.ape(), &mut out);
            id3v1(f.id3v1(), &mut out);
        }
        FileType::Mp4 => ilst(Mp4File::read_from(&mut reader, options).map_err(|e| e.to_string())?.ilst(), &mut out),
        FileType::Wav => {
            let f = WavFile::read_from(&mut reader, options).map_err(|e| e.to_string())?;
            riff(f.riff_info(), &mut out);
            id3v2(f.id3v2(), &mut out);
        }
        FileType::Aiff => {
            let f = AiffFile::read_from(&mut reader, options).map_err(|e| e.to_string())?;
            aiff(f.text_chunks(), &mut out);
            id3v2(f.id3v2(), &mut out);
        }
        // Formats the player doesn't scan for.
        other => return Err(format!("raw tags of {other:?} files aren't read")),
    }
    Ok(out)
}

fn push(out: &mut Vec<RawTag>, format: &'static str, key: impl Into<String>, value: impl Into<String>) {
    out.push(RawTag { format, key: key.into(), value: value.into() });
}

fn vorbis(tag: Option<&VorbisComments>, out: &mut Vec<RawTag>) {
    let Some(tag) = tag else { return };
    if !tag.vendor().is_empty() {
        push(out, "Vorbis comments", "(vendor)", tag.vendor());
    }
    for (k, v) in tag.items() {
        push(out, "Vorbis comments", k, v);
    }
}

fn id3v2(tag: Option<&Id3v2Tag>, out: &mut Vec<RawTag>) {
    const F: &str = "ID3v2";
    let Some(tag) = tag else { return };
    for frame in tag {
        let id = frame.id().as_str().to_string();
        match frame {
            Frame::Text(f) => push(out, F, id, f.value.replace('\0', "; ")),
            Frame::UserText(f) => push(out, F, format!("TXXX:{}", f.description), f.content.replace('\0', "; ")),
            Frame::Comment(f) => push(out, F, format!("COMM:{}", f.description), f.content.to_string()),
            Frame::UnsynchronizedText(f) => push(out, F, format!("USLT:{}", f.description), f.content.to_string()),
            Frame::Url(f) => push(out, F, id, f.url()),
            Frame::UserUrl(f) => push(out, F, format!("WXXX:{}", f.description), f.content.to_string()),
            Frame::UniqueFileIdentifier(f) => push(out, F, format!("UFID:{}", f.owner), String::from_utf8_lossy(&f.identifier).into_owned()),
            Frame::KeyValue(f) => push(out, F, id, f.key_value_pairs.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("; ")),
            Frame::Timestamp(f) => push(out, F, id, f.timestamp.to_string()),
            Frame::Picture(_) => push(out, F, id, "(picture)"),
            _ => push(out, F, id, "(binary data)"),
        }
    }
}

fn id3v1(tag: Option<&Id3v1Tag>, out: &mut Vec<RawTag>) {
    const F: &str = "ID3v1";
    let Some(tag) = tag else { return };
    let fields = [
        ("Title", tag.title.clone()),
        ("Artist", tag.artist.clone()),
        ("Album", tag.album.clone()),
        ("Year", tag.year.map(|y| y.to_string())),
        ("Comment", tag.comment.clone()),
        ("Track", tag.track_number.map(|n| n.to_string())),
        ("Genre", tag.genre.map(|g| lofty::id3::v1::GENRES.get(usize::from(g)).map_or_else(|| g.to_string(), |s| s.to_string()))),
    ];
    for (k, v) in fields {
        if let Some(v) = v {
            push(out, F, k, v);
        }
    }
}

fn ape(tag: Option<&ApeTag>, out: &mut Vec<RawTag>) {
    let Some(tag) = tag else { return };
    for item in tag {
        let value = match item.value() {
            ItemValue::Text(t) | ItemValue::Locator(t) => t.clone(),
            ItemValue::Binary(_) => "(binary data)".to_string(),
        };
        push(out, "APE", item.key(), value);
    }
}

fn ilst(tag: Option<&Ilst>, out: &mut Vec<RawTag>) {
    let Some(tag) = tag else { return };
    for atom in tag {
        let values: Vec<String> = atom
            .data()
            .map(|d| match d {
                AtomData::UTF8(s) | AtomData::UTF16(s) => s.clone(),
                AtomData::SignedInteger(n) => n.to_string(),
                AtomData::UnsignedInteger(n) => n.to_string(),
                AtomData::Bool(b) => b.to_string(),
                AtomData::Picture(_) => "(picture)".to_string(),
                AtomData::Unknown { data, .. } => format!("({} bytes)", data.len()),
            })
            .collect();
        push(out, "MP4", atom.ident().to_string(), values.join("; "));
    }
}

fn riff(tag: Option<&RiffInfoList>, out: &mut Vec<RawTag>) {
    let Some(tag) = tag else { return };
    for (k, v) in tag {
        push(out, "RIFF INFO", k.clone(), v.clone());
    }
}

fn aiff(tag: Option<&AiffTextChunks>, out: &mut Vec<RawTag>) {
    const F: &str = "AIFF text";
    let Some(tag) = tag else { return };
    for (k, v) in [("NAME", &tag.name), ("AUTH", &tag.author), ("(c) ", &tag.copyright)] {
        if let Some(v) = v {
            push(out, F, k, v.clone());
        }
    }
    for a in tag.annotations.iter().flatten() {
        push(out, F, "ANNO", a.clone());
    }
    for c in tag.comments.iter().flatten() {
        push(out, F, "COMT", c.text.clone());
    }
}

/// "34.2 MB", "812 KB".
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

/// "Stereo", "Mono", "6 channels".
pub fn format_channels(n: u8) -> String {
    match n {
        1 => "Mono".to_string(),
        2 => "Stereo".to_string(),
        n => format!("{n} channels"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_channels_read_plainly() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(812_000), "812 KB");
        assert_eq!(format_size(34_200_000), "34.2 MB");
        assert_eq!(format_channels(2), "Stereo");
        assert_eq!(format_channels(6), "6 channels");
    }

    #[test]
    fn unreadable_files_say_why() {
        let e = read(Path::new("/nonexistent/song.flac")).expect_err("no such file");
        assert!(e.starts_with("Couldn't read /nonexistent/song.flac"), "{e}");
    }

    /// `RUNE_MUSIC_FILE=song.flac cargo test real_details -- --ignored --nocapture`
    #[test]
    #[ignore = "needs RUNE_MUSIC_FILE"]
    fn real_details() {
        let path = std::env::var("RUNE_MUSIC_FILE").expect("set RUNE_MUSIC_FILE to an audio file");
        let d = read(Path::new(&path)).expect("readable");
        println!("{:?} kbps, {:?} Hz, {:?} bit, {:?} ch, {:?} bytes", d.bitrate, d.sample_rate, d.bit_depth, d.channels, d.size);
        for (k, v) in &d.ids {
            println!("  id {k}: {v}");
        }
        for t in &d.raw {
            println!("  {} {} = {}", t.format, t.key, t.value.chars().take(80).collect::<String>());
        }
        assert!(!d.raw.is_empty(), "a tagged file has raw tags");
    }
}

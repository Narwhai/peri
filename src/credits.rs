//! Who a song is by and who made it. Tags give the names: `ARTISTS` (one per act), MusicBrainz
//! artist IDs, `COMPOSER`, `PRODUCER` and the other credit tags. The artist text ("A & B feat. C")
//! and the title ("Song (feat. C)") say which artists are featured. Files without the name lists
//! fall back to splitting that text, trusting `&`, `,`, `x` and the like between main artists only
//! when every piece is a name the library already knows.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use lofty::tag::ItemKey;

use crate::library::Track;

/// Words that start a featured list: "feat. C", "(ft. C)", "featuring C". Longest first.
const FEAT: &[&str] = &["featuring", "feat.", "feat", "ft.", "ft"];

/// What joins names in artist text. Where two start at the same place the longer wins.
const JOINS: &[&str] = &[", and ", ", & ", ", ", " & ", " and ", " x ", " X ", " × ", " + ", " / ", "; "];

/// What someone did on a song, after the tag that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Composer,
    Lyricist,
    Writer,
    Producer,
    Arranger,
    Conductor,
    Performer,
    Engineer,
    Mixer,
    Remixer,
    DjMixer,
}

impl Role {
    /// Every role, in the order credits list them.
    pub const ALL: [Role; 11] = [
        Role::Composer,
        Role::Lyricist,
        Role::Writer,
        Role::Producer,
        Role::Arranger,
        Role::Conductor,
        Role::Performer,
        Role::Engineer,
        Role::Mixer,
        Role::Remixer,
        Role::DjMixer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Role::Composer => "Composer",
            Role::Lyricist => "Lyricist",
            Role::Writer => "Writer",
            Role::Producer => "Producer",
            Role::Arranger => "Arranger",
            Role::Conductor => "Conductor",
            Role::Performer => "Performer",
            Role::Engineer => "Engineer",
            Role::Mixer => "Mixer",
            Role::Remixer => "Remixer",
            Role::DjMixer => "DJ mixer",
        }
    }

    /// The tag that holds the role.
    pub fn key(self) -> ItemKey {
        match self {
            Role::Composer => ItemKey::Composer,
            Role::Lyricist => ItemKey::Lyricist,
            Role::Writer => ItemKey::Writer,
            Role::Producer => ItemKey::Producer,
            Role::Arranger => ItemKey::Arranger,
            Role::Conductor => ItemKey::Conductor,
            Role::Performer => ItemKey::Performer,
            Role::Engineer => ItemKey::Engineer,
            Role::Mixer => ItemKey::MixEngineer,
            Role::Remixer => ItemKey::Remixer,
            Role::DjMixer => ItemKey::MixDj,
        }
    }
}

/// One person's credit on a song.
#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub role: Role,
    pub name: String,
    /// What exactly they did, from a performer's "Elly (keyboard)": "keyboard".
    pub detail: Option<String>,
}

impl Credit {
    /// Reads a tag value. Performers carry their instrument in brackets after the name, as
    /// MusicBrainz Picard writes them.
    pub fn parse(role: Role, value: &str) -> Credit {
        let value = value.trim();
        let bracketed = (role == Role::Performer && value.ends_with(')'))
            .then(|| value.rfind(" ("))
            .flatten()
            .map(|at| (value[..at].trim(), value[at + 2..value.len() - 1].trim()))
            .filter(|(name, detail)| !name.is_empty() && !detail.is_empty());
        match bracketed {
            Some((name, detail)) => Credit { role, name: name.to_string(), detail: Some(detail.to_string()) },
            None => Credit { role, name: value.to_string(), detail: None },
        }
    }
}

/// The names credited with `role`, in order, without repeats (ignoring case).
pub fn names(credits: &[Credit], role: Role) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in credits.iter().filter(|c| c.role == role) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&c.name)) {
            out.push(c.name.clone());
        }
    }
    out
}

/// Each name credited with `role` once, followed by what they did when the tags say:
/// `("Elly", "keyboard, sampler")`.
pub fn people(credits: &[Credit], role: Role) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for c in credits.iter().filter(|c| c.role == role) {
        let at = match out.iter().position(|(n, _)| n.eq_ignore_ascii_case(&c.name)) {
            Some(at) => at,
            None => {
                out.push((c.name.clone(), Vec::new()));
                out.len() - 1
            }
        };
        if let Some(d) = &c.detail
            && !out[at].1.iter().any(|x| x.eq_ignore_ascii_case(d))
        {
            out[at].1.push(d.clone());
        }
    }
    out
}

/// Where a featured list sits in `text`: the bytes of the whole "(feat. C)" or "feat. C" part,
/// and of its names "C". `None` when the text features nobody.
pub fn feat(text: &str) -> Option<(Range<usize>, Range<usize>)> {
    let bytes = text.as_bytes();
    for i in 1..bytes.len() {
        // The word follows a space or an opening bracket, so `i` is a character boundary.
        let close = match bytes[i - 1] {
            b'(' => Some(b')'),
            b'[' => Some(b']'),
            b' ' => None,
            _ => continue,
        };
        let Some(word) = FEAT.iter().find(|w| bytes[i..].len() >= w.len() && bytes[i..i + w.len()].eq_ignore_ascii_case(w.as_bytes())) else {
            continue;
        };
        let after = i + word.len();
        if bytes.get(after) != Some(&b' ') {
            continue;
        }
        let names_start = after + (text.len() - after - text[after..].trim_start().len());
        let (part, names_end) = match close {
            Some(close) => {
                let end = bytes[names_start..].iter().position(|&b| b == close).map_or(text.len(), |k| names_start + k);
                (i - 1..(end + 1).min(text.len()), end)
            }
            None => (i..text.len(), text.len()),
        };
        let names_end = names_start + text[names_start..names_end].trim_end().len();
        if names_end > names_start {
            return Some((part, names_start..names_end));
        }
    }
    None
}

/// The bytes of each of `names` in `text`, as whole words and ignoring case, in text order.
/// Longer names are placed first and names never overlap, so "The Alchemist" wins over "Alchemist".
pub fn find_names<'a>(text: &str, names: &'a [String]) -> Vec<(Range<usize>, &'a str)> {
    let mut order: Vec<&String> = names.iter().filter(|n| !n.trim().is_empty()).collect();
    order.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let mut found: Vec<(Range<usize>, &str)> = Vec::new();
    for name in order {
        if let Some(r) = find_word(text, name.trim(), &found) {
            found.push((r, name.as_str()));
        }
    }
    found.sort_by_key(|(r, _)| r.start);
    found
}

/// Whether `name` appears in `text` as whole words, ignoring case.
pub fn contains_name(text: &str, name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && find_word(text, name, &[]).is_some()
}

fn find_word(text: &str, name: &str, taken: &[(Range<usize>, &str)]) -> Option<Range<usize>> {
    let (hay, needle) = (text.as_bytes(), name.as_bytes());
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    text.char_indices().map(|(i, _)| i).find_map(|i| {
        let end = i + needle.len();
        let hit = end <= hay.len()
            && text.is_char_boundary(end)
            && hay[i..end].eq_ignore_ascii_case(needle)
            && !word(text[..i].chars().next_back())
            && !word(text[end..].chars().next())
            && !taken.iter().any(|(r, _)| r.start < end && i < r.end);
        hit.then_some(i..end)
    })
}

/// The joiners ("&", ",", "x"…) standing alone between consecutive `found` names (as
/// [`find_names`] gives them) in `text`, as byte ranges without their spaces.
pub fn joiners(text: &str, found: &[(Range<usize>, &str)]) -> Vec<Range<usize>> {
    found
        .windows(2)
        .filter_map(|pair| {
            let between = pair[0].0.end..pair[1].0.start;
            let gap = text.get(between.clone())?;
            let word = gap.trim();
            let joins = !word.is_empty() && JOINS.iter().any(|j| j.trim() == word);
            joins.then(|| {
                let start = between.start + (gap.len() - gap.trim_start().len());
                start..start + word.len()
            })
        })
        .collect()
}

/// Pieces of `text` between the [`JOINS`], trimmed, empty ones dropped.
fn split_joined(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let next = JOINS.iter().filter_map(|j| rest.find(j).map(|at| (at, j.len()))).min_by_key(|&(at, len)| (at, std::cmp::Reverse(len)));
        match next {
            Some((at, len)) => {
                out.push(rest[..at].trim());
                rest = &rest[at + len..];
            }
            None => {
                out.push(rest.trim());
                break;
            }
        }
    }
    out.retain(|p| !p.is_empty());
    out
}

/// Main artists from text: split only when every piece is a known name, since band names
/// hold `&` and `and` too ("Simon & Garfunkel").
fn split_main(text: &str, known: &HashSet<String>) -> Vec<String> {
    let text = text.trim();
    let pieces = split_joined(text);
    if pieces.len() > 1 && !known.contains(&text.to_lowercase()) && pieces.iter().all(|p| known.contains(&p.to_lowercase())) {
        return pieces.into_iter().map(String::from).collect();
    }
    if text.is_empty() { Vec::new() } else { vec![text.to_string()] }
}

/// Featured artists from text: split at every join unless the whole is a known name.
fn split_featured(text: &str, known: &HashSet<String>) -> Vec<String> {
    let text = text.trim();
    if known.contains(&text.to_lowercase()) {
        return vec![text.to_string()];
    }
    split_joined(text).into_iter().map(String::from).collect()
}

/// The (MusicBrainz ID, name) pairs a song's tags line up: its artists with their IDs when the
/// counts match, and likewise its album artists.
pub fn id_pairs(t: &Track) -> Vec<(&str, &str)> {
    let g = &t.tagged;
    let mut out: Vec<(&str, &str)> = Vec::new();
    if g.artists.len() == g.artist_ids.len() {
        out.extend(g.artist_ids.iter().map(String::as_str).zip(g.artists.iter().map(String::as_str)));
    }
    if !g.album_artists.is_empty() && g.album_artists.len() == g.album_artist_ids.len() {
        out.extend(g.album_artist_ids.iter().map(String::as_str).zip(g.album_artists.iter().map(String::as_str)));
    } else if let (true, [id], Some(name)) = (g.album_artists.is_empty(), g.album_artist_ids.as_slice(), t.album_artist.as_deref()) {
        out.push((id.as_str(), name));
    }
    out
}

/// Fills every song's `artists`, `featured` and `album_artists` from its tags, using what the
/// whole library knows: names paired with MusicBrainz IDs, and names that stand alone somewhere.
pub fn resolve(tracks: &mut [Track]) {
    let mut ids: HashMap<String, String> = HashMap::new();
    for t in tracks.iter() {
        for (id, name) in id_pairs(t) {
            ids.entry(id.to_string()).or_insert_with(|| name.to_string());
        }
    }
    let mut known: HashSet<String> = ids.values().map(|n| n.to_lowercase()).collect();
    for t in tracks.iter() {
        known.extend(t.tagged.artists.iter().chain(&t.tagged.album_artists).map(|n| n.to_lowercase()));
        let alone = |text: &str| feat(text).is_none() && split_joined(text).len() == 1;
        known.extend(t.credit.iter().chain(&t.album_artist).filter(|n| alone(n)).map(|n| n.trim().to_lowercase()));
    }
    for t in tracks.iter_mut() {
        let (artists, featured, album_artists) = resolve_track(t, &ids, &known);
        t.artists = artists;
        t.featured = featured;
        t.album_artists = album_artists;
    }
}

/// A song's artists (main then featured), its featured ones, and its album artists.
fn resolve_track(t: &Track, ids: &HashMap<String, String>, known: &HashSet<String>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let title_feat = feat(&t.title).map(|(_, names)| &t.title[names]);
    let credit = t.credit.as_deref().map(|c| match feat(c) {
        Some((part, names)) => (c[..part.start].trim(), Some(&c[names])),
        None => (c.trim(), None),
    });
    let feat_texts: Vec<&str> = credit.and_then(|(_, f)| f).into_iter().chain(title_feat).collect();
    let tagged = &t.tagged.artists;
    let (main, mut featured): (Vec<String>, Vec<String>) = if tagged.is_empty() {
        let main = credit.map_or_else(Vec::new, |(m, _)| split_main(m, known));
        (main, feat_texts.iter().flat_map(|f| split_featured(f, known)).collect())
    } else {
        let (featured, main) = tagged.iter().cloned().partition(|a| feat_texts.iter().any(|f| contains_name(f, a)));
        (main, featured)
    };
    // Featured names the artist list misses, as when only the title says "(feat. C)": what's
    // left of the list once the tagged names are taken out, so a tagged "A & B" stays whole.
    if !tagged.is_empty() {
        for f in &feat_texts {
            let mut rest = f.to_string();
            for (r, _) in find_names(f, tagged).into_iter().rev() {
                rest.replace_range(r, ", ");
            }
            featured.extend(split_featured(&rest, known).into_iter().filter(|p| p.chars().any(char::is_alphanumeric)).map(|p| p.trim_matches(|c: char| c == '&' || c == ',' || c.is_whitespace()).to_string()));
        }
    }
    let mut artists: Vec<String> = Vec::new();
    for name in main.iter().chain(&featured) {
        if !artists.iter().any(|a| a.eq_ignore_ascii_case(name)) {
            artists.push(name.clone());
        }
    }
    featured.retain(|f| !main.iter().any(|m| m.eq_ignore_ascii_case(f)));
    let mut seen: Vec<String> = Vec::new();
    featured.retain(|f| {
        let new = !seen.iter().any(|s| s.eq_ignore_ascii_case(f));
        seen.push(f.clone());
        new
    });

    let g = &t.tagged;
    let by_id = || g.album_artist_ids.iter().map(|id| ids.get(id).cloned()).collect::<Option<Vec<String>>>().filter(|n| !n.is_empty());
    let album_artists = if !g.album_artists.is_empty() {
        g.album_artists.clone()
    } else if let Some(names) = by_id() {
        names
    } else if let Some(aa) = t.album_artist.as_deref().map(str::trim) {
        let same_as_main = credit.is_some_and(|(m, _)| m.eq_ignore_ascii_case(aa)) || main.join(", ").eq_ignore_ascii_case(aa);
        if same_as_main && !main.is_empty() { main.clone() } else { split_main(aa, known) }
    } else {
        Vec::new()
    };
    (artists, featured, album_artists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TaggedArtists;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn song(title: &str, credit: Option<&str>, album_artist: Option<&str>, tagged: TaggedArtists) -> Track {
        Track {
            path: format!("/m/{title}.flac").into(),
            title: title.into(),
            credit: credit.map(Into::into),
            album_artist: album_artist.map(Into::into),
            tagged,
            ..Track::default()
        }
    }

    /// The real tags of Armand Hammer & The Alchemist's "Calypso Gene".
    fn calypso() -> Track {
        song(
            "Calypso Gene",
            Some("Armand Hammer & The Alchemist feat. Silka & Cleo Reed"),
            Some("Armand Hammer & The Alchemist"),
            TaggedArtists {
                artists: strings(&["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"]),
                artist_ids: strings(&["id-ah", "id-alc", "id-silka", "id-cleo"]),
                album_artists: Vec::new(),
                album_artist_ids: strings(&["id-ah", "id-alc"]),
            },
        )
    }

    #[test]
    fn tagged_artists_split_into_main_and_featured_and_album_artists_resolve_by_id() {
        let mut tracks = vec![calypso()];
        resolve(&mut tracks);
        let t = &tracks[0];
        assert_eq!(t.artists, ["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"]);
        assert_eq!(t.featured, ["Silka", "Cleo Reed"]);
        assert_eq!(t.album_artists, ["Armand Hammer", "The Alchemist"]);
    }

    #[test]
    fn album_artist_ids_resolve_through_other_songs_of_the_library() {
        // This song's own artist tags don't name the album artists, another song's do.
        let mut other = song("Solo", Some("Laraaji"), Some("Laraaji"), TaggedArtists {
            artists: strings(&["Laraaji"]),
            artist_ids: strings(&["id-lar"]),
            ..TaggedArtists::default()
        });
        other.path = "/m/solo.flac".into();
        let guest = song("Guest", Some("Someone"), Some("Laraaji & Friends"), TaggedArtists {
            artists: strings(&["Someone"]),
            artist_ids: strings(&["id-some"]),
            album_artists: Vec::new(),
            album_artist_ids: strings(&["id-lar", "id-some"]),
        });
        let mut tracks = vec![other, guest];
        resolve(&mut tracks);
        assert_eq!(tracks[1].album_artists, ["Laraaji", "Someone"]);
    }

    #[test]
    fn untagged_songs_split_text_trusting_joins_between_known_names_only() {
        let mut tracks = vec![
            // Makes "Armand Hammer" and "The Alchemist" known names.
            song("a", Some("Armand Hammer"), None, TaggedArtists::default()),
            song("b", Some("The Alchemist"), None, TaggedArtists::default()),
            song("Calypso Gene", Some("Armand Hammer & The Alchemist feat. Silka & Cleo Reed"), Some("Armand Hammer & The Alchemist"), TaggedArtists::default()),
            song("Mrs. Robinson", Some("Simon & Garfunkel"), Some("Simon & Garfunkel"), TaggedArtists::default()),
        ];
        resolve(&mut tracks);
        assert_eq!(tracks[2].artists, ["Armand Hammer", "The Alchemist", "Silka", "Cleo Reed"]);
        assert_eq!(tracks[2].featured, ["Silka", "Cleo Reed"]);
        assert_eq!(tracks[2].album_artists, ["Armand Hammer", "The Alchemist"]);
        assert_eq!(tracks[3].artists, ["Simon & Garfunkel"], "unknown pieces keep a band name whole");
        assert_eq!(tracks[3].album_artists, ["Simon & Garfunkel"]);
    }

    #[test]
    fn featured_artists_come_from_the_title_too() {
        let mut tracks = vec![
            song("Same Old Jeans (feat. tobi lou)", Some("NewJeans"), None, TaggedArtists::default()),
            song("Mary [ft. Laraaji]", None, None, TaggedArtists { artists: strings(&["Big Thief"]), ..TaggedArtists::default() }),
        ];
        resolve(&mut tracks);
        assert_eq!((tracks[0].artists.as_slice(), tracks[0].featured.as_slice()), (&strings(&["NewJeans", "tobi lou"])[..], &strings(&["tobi lou"])[..]));
        assert_eq!(tracks[1].artists, ["Big Thief", "Laraaji"], "a title's guest missing from the artist list is added");
        assert_eq!(tracks[1].featured, ["Laraaji"]);
    }

    #[test]
    fn a_tagged_act_named_with_a_join_stays_whole() {
        // Pink Siifu's "8)": ARTISTS lists "Aaliyah & Azul" as one act.
        let tagged = TaggedArtists { artists: strings(&["Pink Siifu", "Jack Davey", "Ss.Sylver", "Aaliyah & Azul"]), ..TaggedArtists::default() };
        let mut tracks = vec![song("8) (feat. Jack Davey, Ss.Sylver & Aaliyah & Azul)", Some("Pink Siifu"), Some("Pink Siifu"), tagged)];
        resolve(&mut tracks);
        assert_eq!(tracks[0].artists, ["Pink Siifu", "Jack Davey", "Ss.Sylver", "Aaliyah & Azul"]);
        assert_eq!(tracks[0].featured, ["Jack Davey", "Ss.Sylver", "Aaliyah & Azul"]);
    }

    #[test]
    fn several_artist_values_without_a_credit_are_all_main() {
        let mut tracks = vec![song("Mary", None, Some("Big Thief & Laraaji"), TaggedArtists { artists: strings(&["Big Thief", "Laraaji"]), ..TaggedArtists::default() })];
        resolve(&mut tracks);
        assert_eq!(tracks[0].artists, ["Big Thief", "Laraaji"]);
        assert!(tracks[0].featured.is_empty());
        assert_eq!(tracks[0].album_artists, ["Big Thief", "Laraaji"], "the album artist splits between known names");
    }

    #[test]
    fn feat_finds_bracketed_and_bare_lists() {
        let part = |t: &'static str| feat(t).map(|(p, n)| (&t[p], &t[n]));
        assert_eq!(part("Calypso Gene (feat. Silka & Cleo Reed)"), Some(("(feat. Silka & Cleo Reed)", "Silka & Cleo Reed")));
        assert_eq!(part("A & B feat. C"), Some(("feat. C", "C")));
        assert_eq!(part("Song [FT. X] (Remix)"), Some(("[FT. X]", "X")));
        assert_eq!(part("A featuring B, C"), Some(("featuring B, C", "B, C")));
        assert_eq!(part("Daft Punk"), None);
        assert_eq!(part("Left Feather"), None, "only whole words");
        assert_eq!(part("Song (feat. )"), None, "an empty list features nobody");
    }

    #[test]
    fn names_are_found_as_whole_words_without_overlapping() {
        let names = strings(&["Alchemist", "The Alchemist", "Silka", "Ann"]);
        let text = "Armand Hammer & The Alchemist feat. Silka & Joanna";
        let found: Vec<(&str, &str)> = find_names(text, &names).into_iter().map(|(r, n)| (&text[r], n)).collect();
        assert_eq!(found, [("The Alchemist", "The Alchemist"), ("Silka", "Silka")], "\"Ann\" isn't a word of Joanna");
        assert!(contains_name("Silka & Cleo Reed", "cleo reed"));
        assert!(!contains_name("Silkas", "Silka"));
        let korean = strings(&["뉴진스"]);
        assert_eq!(find_names("뉴진스 x tobi lou", &korean).len(), 1);
    }

    #[test]
    fn joins_split_lists_with_a_final_and() {
        assert_eq!(split_joined("A, B, and C"), ["A", "B", "C"]);
        assert_eq!(split_joined("NewJeans x tobi lou"), ["NewJeans", "tobi lou"]);
        assert_eq!(split_joined("Solo"), ["Solo"]);
    }

    #[test]
    fn performers_carry_their_instrument() {
        assert_eq!(Credit::parse(Role::Performer, "Elly (keyboard)"), Credit { role: Role::Performer, name: "Elly".into(), detail: Some("keyboard".into()) });
        assert_eq!(Credit::parse(Role::Producer, "Zack Sekoff (Remix)").detail, None, "only performers have details");
        let credits = vec![
            Credit::parse(Role::Performer, "Elly (keyboard)"),
            Credit::parse(Role::Performer, "Elly (sampler)"),
            Credit::parse(Role::Performer, "AKAI SOLO (vocals)"),
            Credit::parse(Role::Producer, "The Alchemist"),
        ];
        assert_eq!(people(&credits, Role::Performer), [("Elly".to_string(), strings(&["keyboard", "sampler"])), ("AKAI SOLO".to_string(), strings(&["vocals"]))]);
        assert_eq!(names(&credits, Role::Producer), ["The Alchemist"]);
    }
}

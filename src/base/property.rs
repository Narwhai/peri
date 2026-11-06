//! Song and album attributes a view can filter, sort and group on.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::library::Track;

/// What a view lists: songs, or albums built from them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Songs,
    Albums,
}

/// A row a view can filter, sort and group: a song or an album.
pub trait Record {
    fn value(&self, p: Property) -> Value;
    /// Whether every word of `query` (lowercased) appears in the record's searchable text.
    fn matches(&self, query: &str) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Genre,
    Year,
    Composer,
    Label,
    Track,
    Disc,
    Duration,
    Format,
    Folder,
    Added,
    /// Albums only: how many songs.
    Tracks,
}

/// How a property's values compare, which decides the operators a filter offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One piece of text.
    Text,
    /// Several pieces of text (artists, genres); a song groups under each.
    List,
    Number,
    /// Seconds, shown as m:ss.
    Duration,
    /// Seconds since the Unix epoch, shown as a date.
    Date,
}

/// A song's value for a property. Empty text and lists count as no value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Text(String),
    List(Vec<String>),
    Number(f64),
}

impl Property {
    pub const ALL: [Property; 14] = [
        Property::Title,
        Property::Artist,
        Property::AlbumArtist,
        Property::Album,
        Property::Genre,
        Property::Year,
        Property::Composer,
        Property::Label,
        Property::Track,
        Property::Disc,
        Property::Duration,
        Property::Format,
        Property::Folder,
        Property::Added,
    ];

    /// The properties a mode offers. Albums drop the per-song ones and add a track count;
    /// "Title" is the album's title there.
    pub fn for_mode(mode: Mode) -> &'static [Property] {
        const ALBUM: [Property; 12] = [
            Property::Title,
            Property::AlbumArtist,
            Property::Artist,
            Property::Genre,
            Property::Year,
            Property::Composer,
            Property::Label,
            Property::Tracks,
            Property::Duration,
            Property::Format,
            Property::Folder,
            Property::Added,
        ];
        match mode {
            Mode::Songs => &Property::ALL,
            Mode::Albums => &ALBUM,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Property::Title => "Title",
            Property::Artist => "Artist",
            Property::AlbumArtist => "Album artist",
            Property::Album => "Album",
            Property::Genre => "Genre",
            Property::Year => "Year",
            Property::Composer => "Composer",
            Property::Label => "Label",
            Property::Track => "Track number",
            Property::Disc => "Disc number",
            Property::Duration => "Duration",
            Property::Format => "Format",
            Property::Folder => "Folder",
            Property::Added => "Date added",
            Property::Tracks => "Track count",
        }
    }

    pub fn from_label(label: &str) -> Option<Property> {
        Property::ALL.into_iter().chain([Property::Tracks]).find(|p| p.label() == label)
    }

    pub fn kind(self) -> Kind {
        match self {
            Property::Title | Property::AlbumArtist | Property::Album | Property::Label | Property::Format | Property::Folder => Kind::Text,
            Property::Artist | Property::Genre | Property::Composer => Kind::List,
            Property::Year | Property::Track | Property::Disc | Property::Tracks => Kind::Number,
            Property::Duration => Kind::Duration,
            Property::Added => Kind::Date,
        }
    }

    pub fn value(self, r: &impl Record) -> Value {
        r.value(self)
    }

    /// A song's value.
    pub(crate) fn track_value(self, t: &Track) -> Value {
        match self {
            Property::Title => Value::Text(t.title.clone()),
            Property::Artist => list(&t.artists),
            Property::AlbumArtist => text(t.album_artist.as_deref()),
            Property::Album => text(t.album.as_deref()),
            Property::Genre => list(&t.genres),
            Property::Year => number(t.year.map(f64::from)),
            Property::Composer => list(&t.composers),
            Property::Label => text(t.label.as_deref()),
            Property::Track => number(t.number.map(f64::from)),
            Property::Disc => number(t.disc.map(f64::from)),
            Property::Duration => number(t.duration),
            Property::Format => text(t.path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref()),
            Property::Folder => text(t.path.parent().and_then(|p| p.to_str())),
            Property::Added => number(t.added.map(|s| s as f64)),
            Property::Tracks => Value::None,
        }
    }

    /// The values a record groups under: one per list item, `None` alone when it has none.
    pub fn group_keys(self, r: &impl Record) -> Vec<Value> {
        match r.value(self) {
            Value::List(items) => items.into_iter().map(Value::Text).collect(),
            Value::Number(n) if self.kind() == Kind::Date => vec![Value::Number(day_start(n))],
            // Exact durations would make a group per song; group by the minute instead.
            Value::Number(n) if self.kind() == Kind::Duration => vec![Value::Number((n / 60.0).floor() * 60.0)],
            v => vec![v],
        }
    }

    /// How a group reads in its header: a minute of durations reads as a range.
    pub fn group_title(self, v: &Value) -> String {
        match v {
            Value::Number(n) if self.kind() == Kind::Duration => {
                let m = (*n / 60.0) as u64;
                format!("{m}:00 – {m}:59")
            }
            v => self.display(v),
        }
    }

    /// How a value reads in a group header or filter summary.
    pub fn display(self, v: &Value) -> String {
        match v {
            Value::None => format!("No {}", self.label().to_lowercase()),
            Value::Text(s) => s.clone(),
            Value::List(items) => items.join(", "),
            Value::Number(n) => match self.kind() {
                Kind::Duration => crate::view::clock(*n),
                Kind::Date => format_date(*n as i64),
                _ => format_number(*n),
            },
        }
    }
}

pub(crate) fn text(s: Option<&str>) -> Value {
    s.filter(|s| !s.is_empty()).map_or(Value::None, |s| Value::Text(s.to_string()))
}

pub(crate) fn list(v: &[String]) -> Value {
    if v.is_empty() { Value::None } else { Value::List(v.to_vec()) }
}

pub(crate) fn number(n: Option<f64>) -> Value {
    n.map_or(Value::None, Value::Number)
}

impl Record for Track {
    fn value(&self, p: Property) -> Value {
        p.track_value(self)
    }

    fn matches(&self, query: &str) -> bool {
        Track::matches(self, query)
    }
}

impl Value {
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// Ascending order: numbers by size, text ignoring case, and no value last either way
    /// (the caller keeps it last when reversing).
    pub fn compare(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::None, Value::None) => Ordering::Equal,
            (Value::None, _) => Ordering::Greater,
            (_, Value::None) => Ordering::Less,
            (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
            (a, b) => natural(&a.sort_text(), &b.sort_text()),
        }
    }

    fn sort_text(&self) -> String {
        match self {
            Value::None => String::new(),
            Value::Text(s) => s.to_lowercase(),
            Value::List(items) => items.join(", ").to_lowercase(),
            Value::Number(n) => format_number(*n),
        }
    }
}

/// Text order with runs of digits compared as numbers, so "Disc 2" sorts before "Disc 10".
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n:.2}") }
}

const DAY: f64 = 86_400.0;

fn day_start(secs: f64) -> f64 {
    (secs / DAY).floor() * DAY
}

/// `YYYY-MM-DD` (UTC) for seconds since the Unix epoch.
pub fn format_date(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Seconds since the Unix epoch at the start of a `YYYY-MM-DD` (UTC) date.
pub fn parse_date(s: &str) -> Option<i64> {
    let mut parts = s.trim().splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// Howard Hinnant's days-from-civil.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip_through_days() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_700_000_000), "2023-11-14");
        assert_eq!(parse_date("2023-11-14"), Some(1_699_920_000));
        assert_eq!(parse_date("2024-02-29").map(format_date).as_deref(), Some("2024-02-29"));
        assert_eq!(parse_date("2024-13-01"), None);
        assert_eq!(parse_date("soon"), None);
    }

    #[test]
    fn text_compares_naturally_and_ignores_case() {
        let t = |s: &str| Value::Text(s.into());
        assert_eq!(t("Disc 2").compare(&t("disc 10")), Ordering::Less);
        assert_eq!(t("abba").compare(&t("ABBA")), Ordering::Equal);
        assert_eq!(t("b").compare(&Value::None), Ordering::Less, "no value sorts last");
    }

    #[test]
    fn every_property_has_a_unique_label() {
        for p in Property::ALL.into_iter().chain([Property::Tracks]) {
            assert_eq!(Property::from_label(p.label()), Some(p));
        }
    }
}

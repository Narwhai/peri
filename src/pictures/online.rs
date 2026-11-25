//! Looking an artist's picture up on Wikimedia Commons, only when the user turns it on in
//! Settings: the artist's MusicBrainz entry names their Wikidata item, whose image (P18) is a
//! Commons file, which says who made it and under what license. A picture found is shown with
//! that attribution; it's the user's to keep (copied in, never replaced) or leave.
//!
//! Asked to, it also gathers [`candidates`] to choose from: every image Wikidata gives the
//! artist, the Commons images MusicBrainz links to, and photos from their Commons category.
//! Fanart.tv and other sources aren't used.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Fetch;

const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";
const WIKIDATA: &str = "https://www.wikidata.org/wiki/Special:EntityData";
const COMMONS: &str = "https://commons.wikimedia.org/w/api.php";
/// The width Commons is asked to scale the picture to.
const WIDTH: u32 = 1000;
/// The width of the thumbnails the picker shows.
pub const THUMB_WIDTH: u32 = 320;
/// The most photos taken from an artist's Commons category.
pub const CATEGORY_MAX: usize = 12;
/// Category files asked for, of which those that look like photos of them are kept.
const CATEGORY_ASK: usize = 40;
/// Category photos smaller than this on either side aren't offered.
const CATEGORY_MIN_PX: u64 = 300;
/// Only the metadata the attribution needs.
const METADATA: &str = "iiextmetadatafilter=Artist%7CCredit%7CLicenseShortName%7CLicenseUrl";

/// Who made a Commons picture and under what license, as the license asks to be shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    /// The author as Commons credits them, as plain text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// "CC BY-SA 4.0", "Public domain".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_url: Option<String>,
    /// The file's page on Commons.
    pub page: String,
}

impl Attribution {
    /// "Photo: Jane Doe · CC BY-SA 4.0 · Wikimedia Commons".
    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(author) = &self.author {
            parts.push(format!("Photo: {author}"));
        }
        if let Some(license) = &self.license {
            parts.push(license.clone());
        }
        parts.push("Wikimedia Commons".into());
        parts.join(" · ")
    }
}

/// A picture found online: its bytes as downloaded, and its attribution.
#[derive(Clone, Debug)]
pub struct Found {
    pub bytes: Vec<u8>,
    pub url: String,
    pub attribution: Attribution,
}

/// Percent-encodes a query value or path part.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn json(fetch: &Fetch, url: &str) -> Result<Value, String> {
    let bytes = fetch(url)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Couldn't read the answer from {}: {e}", host(url)))
}

fn host(url: &str) -> &str {
    url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or(url)
}

/// Whether `id` looks like a MusicBrainz ID, so it's safe in a URL.
fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn artist_url(mbid: &str) -> String {
    format!("{MUSICBRAINZ}/artist/{mbid}?inc=url-rels&fmt=json")
}

pub fn search_url(name: &str) -> String {
    format!("{MUSICBRAINZ}/artist/?query={}&limit=5&fmt=json", encode(&format!("artist:\"{}\"", name.replace('"', ""))))
}

pub fn wikidata_url(qid: &str) -> String {
    format!("{WIKIDATA}/{qid}.json")
}

pub fn commons_url(file: &str) -> String {
    format!("{COMMONS}?action=query&format=json&prop=imageinfo&iiprop=url%7Cextmetadata&iiurlwidth={WIDTH}&titles={}", encode(&format!("File:{file}")))
}

/// Image info for several files at once, at thumbnail size.
pub fn files_url(files: &[String]) -> String {
    let titles: Vec<String> = files.iter().map(|f| format!("File:{f}")).collect();
    format!("{COMMONS}?action=query&format=json&prop=imageinfo&iiprop=url%7Cextmetadata%7Cmime%7Csize&{METADATA}&iiurlwidth={THUMB_WIDTH}&titles={}", encode(&titles.join("|")))
}

/// The files in a Commons category, with their image info at thumbnail size.
pub fn category_url(category: &str) -> String {
    format!(
        "{COMMONS}?action=query&format=json&generator=categorymembers&gcmtype=file&gcmlimit={CATEGORY_ASK}&gcmtitle={}&prop=imageinfo&iiprop=url%7Cextmetadata%7Cmime%7Csize&{METADATA}&iiurlwidth={THUMB_WIDTH}",
        encode(&format!("Category:{category}"))
    )
}

/// Where a picker candidate came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Wikidata's image of them: the first is their main image.
    Wikidata,
    /// A Commons image their MusicBrainz entry links to.
    MusicBrainz,
    /// A photo in their Commons category.
    Category,
}

/// A picture the user may choose: its Commons file, a thumbnail to show, and its credit.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The file's name on Commons, without "File:".
    pub file: String,
    pub thumb: String,
    pub attribution: Attribution,
    pub origin: Origin,
}

/// Commons' spelling of a file name: spaces for underscores, the first letter capitalised.
fn normal(file: &str) -> String {
    let file = file.trim().trim_start_matches("File:").replace('_', " ");
    let mut chars = file.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Decodes a URL's percent escapes.
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The Commons files a MusicBrainz artist's image links name.
pub fn parse_artist_images(v: &Value) -> Vec<String> {
    let Some(relations) = v["relations"].as_array() else { return Vec::new() };
    relations
        .iter()
        .filter(|r| r["type"].as_str() == Some("image"))
        .filter_map(|r| r["url"]["resource"].as_str())
        .filter_map(|url| url.split_once("commons.wikimedia.org/wiki/File:").map(|(_, f)| normal(&decode(f))))
        .collect()
}

/// Every usable image (P18) of a Wikidata item, the preferred ones first, and its Commons
/// category (P373).
pub fn parse_wikidata_all(v: &Value, qid: &str) -> (Vec<String>, Option<String>) {
    let claims = &v["entities"][qid]["claims"];
    let value = |c: &Value| c["mainsnak"]["datavalue"]["value"].as_str().map(normal);
    let mut images: Vec<(bool, String)> = claims["P18"]
        .as_array()
        .map(|cs| cs.iter().filter(|c| c["rank"].as_str() != Some("deprecated")).filter_map(|c| value(c).map(|f| (c["rank"].as_str() != Some("preferred"), f))).collect())
        .unwrap_or_default();
    images.sort_by_key(|(later, _)| *later);
    let category = claims["P373"].as_array().and_then(|cs| cs.first()).and_then(|c| c["mainsnak"]["datavalue"]["value"].as_str()).map(str::to_string);
    (images.into_iter().map(|(_, f)| f).collect(), category)
}

/// Each file's thumbnail and credit from Commons' image info, by its file name.
fn parse_pages(v: &Value) -> Vec<(String, String, Attribution, &Value)> {
    let Some(pages) = v["query"]["pages"].as_object() else { return Vec::new() };
    pages
        .values()
        .filter_map(|page| {
            let file = normal(page["title"].as_str()?);
            let info = page["imageinfo"].as_array()?.first()?;
            let thumb = info["thumburl"].as_str().or(info["url"].as_str())?.to_string();
            thumb.starts_with("https://").then(|| (file, thumb, attribution(info), info))
        })
        .collect()
}

/// A category file that looks like a photo worth offering: a photo format, not tiny, not a strip.
fn photo_like(info: &Value) -> bool {
    let mime = info["mime"].as_str().unwrap_or_default();
    let (w, h) = (info["width"].as_u64().unwrap_or(0), info["height"].as_u64().unwrap_or(0));
    matches!(mime, "image/jpeg" | "image/png" | "image/webp") && w >= CATEGORY_MIN_PX && h >= CATEGORY_MIN_PX && w.max(h) * 2 <= w.min(h) * 5
}

/// The MBID of the one artist a MusicBrainz search finds by exactly `names`; none when it's
/// ambiguous (two artists by that name) or no match is exact.
pub fn parse_search(v: &Value, names: &[String]) -> Option<String> {
    let exact: Vec<&Value> = v["artists"]
        .as_array()?
        .iter()
        .filter(|a| a["score"].as_u64() == Some(100))
        .filter(|a| a["name"].as_str().is_some_and(|n| names.iter().any(|m| m.trim().eq_ignore_ascii_case(n.trim()))))
        .collect();
    match exact.as_slice() {
        [one] => one["id"].as_str().filter(|id| is_mbid(id)).map(str::to_string),
        _ => None,
    }
}

/// The Wikidata item a MusicBrainz artist links to ("Q123").
pub fn parse_artist(v: &Value) -> Option<String> {
    v["relations"].as_array()?.iter().filter(|r| r["type"].as_str() == Some("wikidata")).find_map(|r| {
        let url = r["url"]["resource"].as_str()?;
        let qid = url.rsplit('/').next()?;
        (qid.starts_with('Q') && qid.len() > 1 && qid[1..].chars().all(|c| c.is_ascii_digit())).then(|| qid.to_string())
    })
}

/// The Commons file name of a Wikidata item's image (P18), the preferred one first.
pub fn parse_wikidata(v: &Value, qid: &str) -> Option<String> {
    let claims = v["entities"][qid]["claims"]["P18"].as_array()?;
    let preferred = claims.iter().find(|c| c["rank"].as_str() == Some("preferred"));
    let usable = claims.iter().find(|c| c["rank"].as_str() != Some("deprecated"));
    preferred.or(usable)?["mainsnak"]["datavalue"]["value"].as_str().map(str::to_string)
}

/// The scaled picture's URL and its attribution, from Commons' image info.
pub fn parse_commons(v: &Value) -> Option<(String, Attribution)> {
    let page = v["query"]["pages"].as_object()?.values().next()?;
    let info = page["imageinfo"].as_array()?.first()?;
    let url = info["thumburl"].as_str().or(info["url"].as_str())?.to_string();
    url.starts_with("https://").then(|| (url, attribution(info)))
}

/// Who made a file and its license, from its image info.
fn attribution(info: &Value) -> Attribution {
    let meta = &info["extmetadata"];
    let field = |k: &str| meta[k]["value"].as_str().map(strip_html).filter(|s| !s.is_empty());
    Attribution {
        author: field("Artist").or_else(|| field("Credit")),
        license: field("LicenseShortName"),
        license_url: field("LicenseUrl"),
        page: info["descriptionurl"].as_str().unwrap_or("https://commons.wikimedia.org").to_string(),
    }
}

/// Commons' HTML fields as plain text: tags dropped, entities decoded, spaces collapsed.
fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Looks the artist with these names (and MusicBrainz ID, when their tags have one) up:
/// MusicBrainz, then Wikidata, then Commons, then the picture. `Ok(None)` when any step finds
/// nothing. Several requests, so it runs off the UI thread.
pub fn lookup(names: &[String], mbid: Option<&str>, fetch: &Fetch) -> Result<Option<Found>, String> {
    let Some(artist) = musicbrainz_artist(names, mbid, fetch)? else { return Ok(None) };
    let Some(qid) = parse_artist(&artist) else { return Ok(None) };
    let Some(file) = parse_wikidata(&json(fetch, &wikidata_url(&qid))?, &qid) else { return Ok(None) };
    file_picture(&file, fetch).map(Some)
}

/// The artist's MusicBrainz entry, by their ID or else a search for their names. None when
/// the search can't tell who they are.
fn musicbrainz_artist(names: &[String], mbid: Option<&str>, fetch: &Fetch) -> Result<Option<Value>, String> {
    let mbid = match mbid.filter(|id| is_mbid(id)) {
        Some(id) => id.to_string(),
        None => {
            let Some(name) = names.first() else { return Ok(None) };
            let found = json(fetch, &search_url(name));
            // MusicBrainz asks for at most one request a second, from all lookups together.
            pace();
            match parse_search(&found?, names) {
                Some(id) => id,
                None => return Ok(None),
            }
        }
    };
    let artist = json(fetch, &artist_url(&mbid));
    pace();
    artist.map(Some)
}

/// A Commons file at the size pictures are kept, with its credit: the picture a user picked.
pub fn file_picture(file: &str, fetch: &Fetch) -> Result<Found, String> {
    let (url, attribution) = parse_commons(&json(fetch, &commons_url(file))?).ok_or_else(|| format!("Wikimedia Commons has no picture {file}"))?;
    let bytes = fetch(&url)?;
    Ok(Found { bytes, url, attribution })
}

/// The pictures of the artist to choose from: Wikidata's images (the main one first), the
/// Commons images MusicBrainz links to, then up to [`CATEGORY_MAX`] photos from their Commons
/// category, each once. Empty when they can't be told apart from others of their name.
pub fn candidates(names: &[String], mbid: Option<&str>, fetch: &Fetch) -> Result<Vec<Candidate>, String> {
    let Some(artist) = musicbrainz_artist(names, mbid, fetch)? else { return Ok(Vec::new()) };
    let (images, category) = match parse_artist(&artist) {
        Some(qid) => parse_wikidata_all(&json(fetch, &wikidata_url(&qid))?, &qid),
        None => (Vec::new(), None),
    };
    let mut named: Vec<(String, Origin)> = images.into_iter().map(|f| (f, Origin::Wikidata)).collect();
    for file in parse_artist_images(&artist) {
        if !named.iter().any(|(f, _)| *f == file) {
            named.push((file, Origin::MusicBrainz));
        }
    }
    let mut out = Vec::new();
    if !named.is_empty() {
        let files: Vec<String> = named.iter().map(|(f, _)| f.clone()).collect();
        let v = json(fetch, &files_url(&files))?;
        let pages = parse_pages(&v);
        for (file, origin) in &named {
            if let Some((_, thumb, attribution, _)) = pages.iter().find(|(f, ..)| f == file) {
                out.push(Candidate { file: file.clone(), thumb: thumb.clone(), attribution: attribution.clone(), origin: *origin });
            }
        }
    }
    if let Some(category) = category {
        let v = json(fetch, &category_url(&category))?;
        let mut photos: Vec<Candidate> = parse_pages(&v)
            .into_iter()
            .filter(|(file, _, _, info)| photo_like(info) && !out.iter().any(|c| c.file == *file))
            .map(|(file, thumb, attribution, _)| Candidate { file, thumb, attribution, origin: Origin::Category })
            .collect();
        photos.sort_by(|a, b| a.file.cmp(&b.file));
        out.extend(photos.into_iter().take(CATEGORY_MAX));
    }
    Ok(out)
}

#[cfg(not(test))]
fn pace() {
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

#[cfg(test)]
fn pace() {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Arc;

    pub const MBID: &str = "0c751690-c784-4a4f-b1e4-c1de27d47581";

    pub fn musicbrainz() -> String {
        r#"{"id":"0c751690-c784-4a4f-b1e4-c1de27d47581","name":"Somebody","relations":[
            {"type":"discogs","url":{"resource":"https://www.discogs.com/artist/1"}},
            {"type":"image","url":{"resource":"https://commons.wikimedia.org/wiki/File:Somebody_%26_band.jpg"}},
            {"type":"image","url":{"resource":"https://commons.wikimedia.org/wiki/File:Somebody_live_2019.jpg"}},
            {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#
            .into()
    }

    pub fn wikidata() -> String {
        r#"{"entities":{"Q42":{"claims":{"P18":[
            {"rank":"deprecated","mainsnak":{"datavalue":{"value":"Old.jpg"}}},
            {"rank":"normal","mainsnak":{"datavalue":{"value":"Somebody live 2019.jpg"}}}],
            "P373":[{"rank":"normal","mainsnak":{"datavalue":{"value":"Somebody (musician)"}}}]}}}}"#
            .into()
    }

    pub fn commons() -> String {
        r#"{"query":{"pages":{"-1":{"imageinfo":[{
            "thumburl":"https://upload.wikimedia.org/thumb/somebody.jpg/1000px-somebody.jpg",
            "url":"https://upload.wikimedia.org/somebody.jpg",
            "descriptionurl":"https://commons.wikimedia.org/wiki/File:Somebody_live_2019.jpg",
            "extmetadata":{
                "Artist":{"value":"<a href=\"//commons.wikimedia.org/wiki/User:Jane\">Jane  Doe</a> &amp; co"},
                "LicenseShortName":{"value":"CC BY-SA 4.0"},
                "LicenseUrl":{"value":"https://creativecommons.org/licenses/by-sa/4.0"}}}]}}}}"#
            .into()
    }

    fn page(file: &str, mime: &str, w: u32, h: u32) -> String {
        let slug = file.replace(' ', "_");
        // Credits can be long unbroken names, as a derivative work's often is.
        let author = match file {
            "Somebody concert 01.jpg" => "Somebody_Coachella_2012_derivative_work_by_a_very_long_username_and_more".to_string(),
            _ => format!("Author of {file}"),
        };
        format!(
            r#""{slug}":{{"title":"File:{file}","imageinfo":[{{"thumburl":"https://upload.wikimedia.org/thumb/{slug}/320px-{slug}","url":"https://upload.wikimedia.org/{slug}",
            "descriptionurl":"https://commons.wikimedia.org/wiki/File:{slug}","mime":"{mime}","width":{w},"height":{h},
            "extmetadata":{{"Artist":{{"value":"{author}"}},"LicenseShortName":{{"value":"CC BY 4.0"}}}}}}]}}"#
        )
    }

    /// Image info for the files Wikidata and MusicBrainz name.
    pub fn files() -> String {
        format!(r#"{{"query":{{"pages":{{{},{}}}}}}}"#, page("Somebody & band.jpg", "image/jpeg", 800, 600), page("Somebody live 2019.jpg", "image/jpeg", 1200, 800))
    }

    /// Their category: photos, a poster's PDF, a tiny icon, a panorama and Wikidata's own image again.
    pub fn category() -> String {
        let mut pages: Vec<String> = (1..=14).map(|n| page(&format!("Somebody concert {n:02}.jpg"), "image/jpeg", 1024, 768)).collect();
        pages.push(page("Somebody poster.pdf", "application/pdf", 1000, 1400));
        pages.push(page("Somebody icon.png", "image/png", 64, 64));
        pages.push(page("Somebody panorama.jpg", "image/jpeg", 4000, 800));
        pages.push(page("Somebody live 2019.jpg", "image/jpeg", 1200, 800));
        format!(r#"{{"query":{{"pages":{{{}}}}}}}"#, pages.join(","))
    }

    /// A stand-in for the network that answers the lookup's requests for Somebody with
    /// `image`, and records them.
    pub(crate) fn fake(image: Vec<u8>, asked: Arc<std::sync::Mutex<Vec<String>>>) -> Fetch {
        Arc::new(move |url: &str| {
            asked.lock().unwrap().push(url.to_string());
            if url == search_url("Somebody") {
                Ok(format!(r#"{{"artists":[{{"id":"{MBID}","name":"Somebody","score":100}},{{"id":"x","name":"Somebody Else","score":80}}]}}"#).into_bytes())
            } else if url == artist_url(MBID) {
                Ok(musicbrainz().into_bytes())
            } else if url == wikidata_url("Q42") {
                Ok(wikidata().into_bytes())
            } else if url.starts_with(COMMONS) && url.contains("iiurlwidth=1000") {
                // Any one file at full size: the picture's info.
                Ok(commons().into_bytes())
            } else if url == files_url(&["Somebody live 2019.jpg".into(), "Somebody & band.jpg".into()]) {
                Ok(files().into_bytes())
            } else if url == category_url("Somebody (musician)") {
                Ok(category().into_bytes())
            } else if url.starts_with("https://upload.wikimedia.org/") {
                Ok(image.clone())
            } else {
                Err(format!("no answer for {url}"))
            }
        })
    }

    #[test]
    fn each_step_reads_what_the_next_needs() {
        let v = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        assert_eq!(parse_artist(&v(&musicbrainz())).as_deref(), Some("Q42"));
        assert_eq!(parse_wikidata(&v(&wikidata()), "Q42").as_deref(), Some("Somebody live 2019.jpg"), "a deprecated image is skipped");
        let (url, attribution) = parse_commons(&v(&commons())).unwrap();
        assert!(url.contains("1000px"));
        assert_eq!(attribution.author.as_deref(), Some("Jane Doe & co"));
        assert_eq!(attribution.line(), "Photo: Jane Doe & co · CC BY-SA 4.0 · Wikimedia Commons");
        assert_eq!(commons_url("A b.jpg"), "https://commons.wikimedia.org/w/api.php?action=query&format=json&prop=imageinfo&iiprop=url%7Cextmetadata&iiurlwidth=1000&titles=File%3AA%20b.jpg");
    }

    #[test]
    fn a_search_takes_only_one_exact_match() {
        let names = vec!["Somebody".to_string()];
        let one = serde_json::json!({"artists":[{"id":MBID,"name":"somebody","score":100},{"id":MBID,"name":"Somebody Else","score":100}]});
        assert_eq!(parse_search(&one, &names).as_deref(), Some(MBID));
        let two = serde_json::json!({"artists":[{"id":MBID,"name":"Somebody","score":100},{"id":MBID,"name":"Somebody","score":100}]});
        assert_eq!(parse_search(&two, &names), None, "two artists of that name: can't tell which");
        let near = serde_json::json!({"artists":[{"id":MBID,"name":"Somebody","score":90}]});
        assert_eq!(parse_search(&near, &names), None);
    }

    #[test]
    fn lookup_follows_musicbrainz_to_wikidata_to_commons() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let fetch = fake(b"image".to_vec(), asked.clone());
        let names = vec!["Somebody".to_string()];
        let found = lookup(&names, None, &fetch).unwrap().expect("found");
        assert_eq!(found.bytes, b"image");
        assert_eq!(asked.lock().unwrap().len(), 5, "search, artist, item, file, picture");
        asked.lock().unwrap().clear();
        lookup(&names, Some(MBID), &fetch).unwrap().expect("found by ID");
        assert_eq!(asked.lock().unwrap().len(), 4, "a tagged ID needs no search");
        assert!(lookup(&["Nobody".to_string()], None, &fetch).is_err(), "a failed request is an error");
    }

    #[test]
    fn candidates_are_wikidatas_images_then_musicbrainzs_then_category_photos() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let fetch = fake(b"image".to_vec(), asked.clone());
        let found = candidates(&["Somebody".to_string()], Some(MBID), &fetch).unwrap();
        let files: Vec<(&str, Origin)> = found.iter().map(|c| (c.file.as_str(), c.origin)).collect();
        assert_eq!(files[0], ("Somebody live 2019.jpg", Origin::Wikidata), "the main image first");
        assert_eq!(files[1], ("Somebody & band.jpg", Origin::MusicBrainz), "a MusicBrainz link Wikidata lacks; the one it shares isn't repeated");
        let category: Vec<&str> = files[2..].iter().map(|(f, o)| {
            assert_eq!(*o, Origin::Category);
            *f
        }).collect();
        assert_eq!(category.len(), CATEGORY_MAX, "at most a dozen from the category");
        assert_eq!(category[0], "Somebody concert 01.jpg");
        assert!(!category.iter().any(|f| f.contains("poster") || f.contains("icon") || f.contains("panorama") || f.contains("live")), "{category:?}");
        assert_eq!(found[3].attribution.line(), "Photo: Author of Somebody concert 02.jpg · CC BY 4.0 · Wikimedia Commons");
        assert!(found.iter().all(|c| c.thumb.contains("320px")));
        assert_eq!(asked.lock().unwrap().len(), 4, "artist, item, the named files at once, the category");
    }

    #[test]
    fn file_names_are_read_as_commons_spells_them() {
        assert_eq!(normal("somebody_live.jpg"), "Somebody live.jpg");
        assert_eq!(decode("A%20%26%20B%C3%A9.jpg"), "A & Bé.jpg");
        let none = serde_json::json!({"relations":[{"type":"image","url":{"resource":"https://example.com/x.jpg"}}]});
        assert!(parse_artist_images(&none).is_empty(), "only Commons images");
    }
}

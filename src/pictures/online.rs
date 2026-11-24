//! Looking an artist's picture up on Wikimedia Commons, only when the user turns it on in
//! Settings: the artist's MusicBrainz entry names their Wikidata item, whose image (P18) is a
//! Commons file, which says who made it and under what license. A picture found is shown with
//! that attribution; it's the user's to keep (copied in, never replaced) or leave.
//! Fanart.tv and other sources aren't used.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Fetch;

const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";
const WIKIDATA: &str = "https://www.wikidata.org/wiki/Special:EntityData";
const COMMONS: &str = "https://commons.wikimedia.org/w/api.php";
/// The width Commons is asked to scale the picture to.
const WIDTH: u32 = 1000;

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
    let meta = &info["extmetadata"];
    let field = |k: &str| meta[k]["value"].as_str().map(strip_html).filter(|s| !s.is_empty());
    let attribution = Attribution {
        author: field("Artist").or_else(|| field("Credit")),
        license: field("LicenseShortName"),
        license_url: field("LicenseUrl"),
        page: info["descriptionurl"].as_str().unwrap_or("https://commons.wikimedia.org").to_string(),
    };
    url.starts_with("https://").then_some((url, attribution))
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
    let Some(qid) = parse_artist(&artist?) else { return Ok(None) };
    let Some(file) = parse_wikidata(&json(fetch, &wikidata_url(&qid))?, &qid) else { return Ok(None) };
    let Some((url, attribution)) = parse_commons(&json(fetch, &commons_url(&file))?) else { return Ok(None) };
    let bytes = fetch(&url)?;
    Ok(Some(Found { bytes, url, attribution }))
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
            {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#
            .into()
    }

    pub fn wikidata() -> String {
        r#"{"entities":{"Q42":{"claims":{"P18":[
            {"rank":"deprecated","mainsnak":{"datavalue":{"value":"Old.jpg"}}},
            {"rank":"normal","mainsnak":{"datavalue":{"value":"Somebody live 2019.jpg"}}}]}}}}"#
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
            } else if url == commons_url("Somebody live 2019.jpg") {
                Ok(commons().into_bytes())
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
}

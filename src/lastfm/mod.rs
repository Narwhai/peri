//! Last.fm scrobbling: signed API calls (https://www.last.fm/api/scrobbling) made on a worker
//! thread, so playback never waits on the network.

mod md5;
pub mod scrobbling;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Where users approve the app; the token goes in the query.
const AUTH: &str = "https://www.last.fm/api/auth/";
/// Where developers (and users of builds without a built-in key) create an API account.
pub const CREATE_ACCOUNT: &str = "https://www.last.fm/api/account/create";
/// Last.fm's "Invalid session key": the user revoked access, so they must connect again.
pub const INVALID_SESSION: u32 = 9;
const TIMEOUT: Duration = Duration::from_secs(20);

/// The API account the app signs requests with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: String,
    pub secret: String,
}

// A key without its secret (or the reverse) can't sign anything: refuse to build with half.
const _: () = assert!(
    option_env!("RUNE_LASTFM_API_KEY").is_some() == option_env!("RUNE_LASTFM_SECRET").is_some(),
    "set both RUNE_LASTFM_API_KEY and RUNE_LASTFM_SECRET, or neither"
);

/// The app's own API account, read from `RUNE_LASTFM_API_KEY` and `RUNE_LASTFM_SECRET` at
/// build time so it's never in the repository. Builds without them ask users for their own.
pub fn built_in() -> Option<Credentials> {
    match (option_env!("RUNE_LASTFM_API_KEY"), option_env!("RUNE_LASTFM_SECRET")) {
        (Some(api_key), Some(secret)) if !api_key.trim().is_empty() && !secret.trim().is_empty() => {
            Some(Credentials { api_key: api_key.trim().to_string(), secret: secret.trim().to_string() })
        }
        _ => None,
    }
}

/// A user's approval: the key signs their scrobbles and doesn't expire unless revoked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub key: String,
    /// Their Last.fm user name.
    pub name: String,
    /// The API key the session was granted to: it's only valid with that key's secret.
    pub api_key: String,
}

/// What `prefs.json` keeps under `lastfm`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// The user's own API account, for builds without one built in.
    pub credentials: Option<Credentials>,
    /// None until the user connects, and again after they disconnect.
    pub session: Option<Session>,
}

/// A song as Last.fm wants it. Only `artist` and `track` are required.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    /// Only when it differs from `artist`.
    pub album_artist: Option<String>,
    pub number: Option<u32>,
    /// Whole seconds.
    pub duration: Option<u32>,
    /// The MusicBrainz recording ID.
    pub mbid: Option<String>,
}

impl Song {
    fn params(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![("artist", self.artist.clone()), ("track", self.track.clone())];
        out.extend(self.album.clone().map(|v| ("album", v)));
        out.extend(self.album_artist.clone().map(|v| ("albumArtist", v)));
        out.extend(self.number.map(|v| ("trackNumber", v.to_string())));
        out.extend(self.duration.map(|v| ("duration", v.to_string())));
        out.extend(self.mbid.clone().map(|v| ("mbid", v)));
        out
    }
}

/// One API call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// `auth.getToken`: a token for the user to approve in the browser.
    Token,
    /// `auth.getSession`: the session for an approved token.
    Session { token: String },
    /// `track.updateNowPlaying`.
    NowPlaying { session: String, song: Song },
    /// `track.scrobble`; `started` is when the song began, in seconds since the Unix epoch.
    Scrobble { session: String, song: Song, started: i64 },
}

impl Call {
    fn method(&self) -> &'static str {
        match self {
            Call::Token => "auth.getToken",
            Call::Session { .. } => "auth.getSession",
            Call::NowPlaying { .. } => "track.updateNowPlaying",
            Call::Scrobble { .. } => "track.scrobble",
        }
    }

    /// The POST form: the call's parameters, the API key and session, and their signature.
    pub fn form(&self, credentials: &Credentials) -> Vec<(&'static str, String)> {
        let mut params = vec![("method", self.method().to_string()), ("api_key", credentials.api_key.clone())];
        match self {
            Call::Token => {}
            Call::Session { token } => params.push(("token", token.clone())),
            Call::NowPlaying { session, song } => {
                params.push(("sk", session.clone()));
                params.extend(song.params());
            }
            Call::Scrobble { session, song, started } => {
                params.push(("sk", session.clone()));
                params.extend(song.params());
                params.push(("timestamp", started.to_string()));
            }
        }
        let signature = sign(&params, &credentials.secret);
        params.push(("api_sig", signature));
        // Not signed: it only picks the response's format.
        params.push(("format", "json".to_string()));
        params
    }
}

/// The `api_sig` of `params`: the MD5 of every name and value, sorted by name, then the secret.
pub fn sign(params: &[(&str, String)], secret: &str) -> String {
    let mut sorted: Vec<&(&str, String)> = params.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut text = String::new();
    for (name, value) in sorted {
        text.push_str(name);
        text.push_str(value);
    }
    text.push_str(secret);
    md5::hex(text.as_bytes())
}

/// The page where the user approves `token` for the API account `api_key`.
pub fn auth_url(api_key: &str, token: &str) -> String {
    format!("{AUTH}?api_key={}&token={}", encode(api_key), encode(token))
}

/// Percent-encodes everything but unreserved characters.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A failed call: Last.fm's error code when it answered with one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: Option<u32>,
    pub message: String,
}

impl Error {
    fn other(message: impl Into<String>) -> Error {
        Error { code: None, message: message.into() }
    }
}

/// What a call came back with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Token(String),
    /// The session key and the user's name.
    Session { key: String, name: String },
    NowPlaying,
    /// Accepted, or ignored with Last.fm's reason (as for a timestamp too far in the past).
    Scrobbled { ignored: Option<String> },
}

/// Reads Last.fm's JSON answer to `call`.
pub fn parse(call: &Call, body: &str) -> Result<Answer, Error> {
    let json: Value = serde_json::from_str(body).map_err(|e| Error::other(format!("Last.fm sent something unreadable: {e}")))?;
    if let Some(code) = json.get("error") {
        let message = json.get("message").and_then(Value::as_str).unwrap_or("no message").to_string();
        let code = number(code).and_then(|c| u32::try_from(c).ok());
        return Err(Error { code, message });
    }
    let missing = |what: &str| Error::other(format!("Last.fm's answer had no {what}"));
    match call {
        Call::Token => json.get("token").and_then(Value::as_str).map(|t| Answer::Token(t.to_string())).ok_or_else(|| missing("token")),
        Call::Session { .. } => {
            let session = json.get("session").ok_or_else(|| missing("session"))?;
            let field = |name: &str| session.get(name).and_then(Value::as_str).map(String::from).ok_or_else(|| missing(&format!("session {name}")));
            Ok(Answer::Session { key: field("key")?, name: field("name")? })
        }
        Call::NowPlaying { .. } => json.get("nowplaying").map(|_| Answer::NowPlaying).ok_or_else(|| missing("now playing")),
        Call::Scrobble { .. } => {
            let scrobbles = json.get("scrobbles").ok_or_else(|| missing("scrobbles"))?;
            let ignored = scrobbles.get("@attr").and_then(|a| a.get("ignored")).and_then(number).ok_or_else(|| missing("ignored count"))?;
            if ignored == 0 {
                return Ok(Answer::Scrobbled { ignored: None });
            }
            // One scrobble comes back as an object, a batch as a list.
            let scrobble = scrobbles.get("scrobble").map(|s| s.get(0).unwrap_or(s));
            let reason = scrobble
                .and_then(|s| s.get("ignoredMessage"))
                .and_then(|m| m.get("#text"))
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .unwrap_or("no reason given");
            Ok(Answer::Scrobbled { ignored: Some(reason.to_string()) })
        }
    }
}

/// A JSON number, or a number in a string: Last.fm sends both.
fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

pub struct Job {
    pub credentials: Credentials,
    pub call: Call,
}

pub struct Reply {
    pub call: Call,
    pub result: Result<Answer, Error>,
}

/// Sends calls to a worker thread and takes its replies.
pub struct Client {
    jobs: Sender<Job>,
    replies: Receiver<Reply>,
}

impl Client {
    /// A worker that makes each call over HTTPS, calling `wake` after each reply. It ends when
    /// the client is dropped.
    pub fn spawn(wake: impl Fn() + Send + 'static) -> Client {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, replies) = channel();
        std::thread::Builder::new()
            .name("lastfm".into())
            .spawn(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .https_only(true)
                    .http_status_as_error(false)
                    .timeout_global(Some(TIMEOUT))
                    .user_agent(concat!("rune-music/", env!("CARGO_PKG_VERSION")))
                    .build()
                    .into();
                for job in inbox {
                    let result = send(&agent, &job);
                    if outbox.send(Reply { call: job.call, result }).is_err() {
                        return;
                    }
                    wake();
                }
            })
            .expect("spawn the Last.fm thread");
        Client { jobs, replies }
    }

    /// A client whose calls go nowhere: tests read them from the first receiver and answer
    /// through the sender.
    #[cfg(test)]
    pub fn fake() -> (Client, Receiver<Job>, Sender<Reply>) {
        let (jobs, inbox) = channel();
        let (outbox, replies) = channel();
        (Client { jobs, replies }, inbox, outbox)
    }

    pub fn call(&self, credentials: &Credentials, call: Call) -> Result<(), String> {
        self.jobs.send(Job { credentials: credentials.clone(), call }).map_err(|_| "The Last.fm thread stopped".to_string())
    }

    pub fn replies(&self) -> impl Iterator<Item = Reply> + '_ {
        self.replies.try_iter()
    }
}

fn send(agent: &ureq::Agent, job: &Job) -> Result<Answer, Error> {
    let form = job.call.form(&job.credentials);
    let mut response = agent.post(API).send_form(form).map_err(|e| Error::other(format!("Couldn't reach Last.fm: {e}")))?;
    let body = response.body_mut().read_to_string().map_err(|e| Error::other(format!("Couldn't read Last.fm's answer: {e}")))?;
    parse(&job.call, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Credentials {
        Credentials { api_key: "key".into(), secret: "secret".into() }
    }

    fn song() -> Song {
        Song {
            artist: "Björk".into(),
            track: "Jóga".into(),
            album: Some("Homogenic".into()),
            album_artist: None,
            number: Some(3),
            duration: Some(305),
            mbid: None,
        }
    }

    #[test]
    fn signature_sorts_names_and_appends_the_secret() {
        let params = [("token", "t0k".to_string()), ("method", "auth.getSession".to_string()), ("api_key", "key".to_string())];
        // printf 'api_keykeymethodauth.getSessiontokent0ksecret' | md5sum
        assert_eq!(sign(&params, "secret"), "a136306e95df5aa947f33506a362dd47");
    }

    #[test]
    fn form_is_signed_without_format() {
        let call = Call::Scrobble { session: "sk1".into(), song: song(), started: 1_700_000_000 };
        let form = call.form(&credentials());
        let get = |name: &str| form.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str());
        assert_eq!(get("method"), Some("track.scrobble"));
        assert_eq!(get("sk"), Some("sk1"));
        assert_eq!(get("timestamp"), Some("1700000000"));
        assert_eq!(get("trackNumber"), Some("3"));
        assert_eq!(get("albumArtist"), None);
        assert_eq!(get("format"), Some("json"));
        let signed: Vec<(&str, String)> = form.iter().filter(|(n, _)| *n != "api_sig" && *n != "format").cloned().collect();
        assert_eq!(get("api_sig"), Some(sign(&signed, "secret").as_str()));
    }

    #[test]
    fn auth_url_encodes_its_query() {
        assert_eq!(auth_url("ab c", "t&1"), "https://www.last.fm/api/auth/?api_key=ab%20c&token=t%261");
    }

    #[test]
    fn parses_answers() {
        let session = parse(&Call::Session { token: "t".into() }, r#"{"session":{"name":"rj","key":"d580d57f","subscriber":0}}"#);
        assert_eq!(session, Ok(Answer::Session { key: "d580d57f".into(), name: "rj".into() }));
        assert_eq!(parse(&Call::Token, r#"{"token":"cf45fe5a"}"#), Ok(Answer::Token("cf45fe5a".into())));
        let playing = Call::NowPlaying { session: "s".into(), song: song() };
        assert_eq!(parse(&playing, r##"{"nowplaying":{"artist":{"corrected":"0","#text":"Björk"}}}"##), Ok(Answer::NowPlaying));
    }

    #[test]
    fn parses_errors() {
        let err = parse(&Call::Token, r#"{"error":9,"message":"Invalid session key - Please re-authenticate"}"#);
        assert_eq!(err, Err(Error { code: Some(INVALID_SESSION), message: "Invalid session key - Please re-authenticate".into() }));
        assert!(matches!(parse(&Call::Token, "<html>"), Err(Error { code: None, .. })));
        assert!(matches!(parse(&Call::Token, "{}"), Err(Error { code: None, .. })));
    }

    #[test]
    fn parses_accepted_and_ignored_scrobbles() {
        let call = Call::Scrobble { session: "s".into(), song: song(), started: 1 };
        let accepted = r##"{"scrobbles":{"@attr":{"accepted":1,"ignored":0},"scrobble":{"ignoredMessage":{"code":"0","#text":""}}}}"##;
        assert_eq!(parse(&call, accepted), Ok(Answer::Scrobbled { ignored: None }));
        let ignored = r##"{"scrobbles":{"@attr":{"accepted":"0","ignored":"1"},"scrobble":{"ignoredMessage":{"code":"3","#text":"Timestamp too old"}}}}"##;
        assert_eq!(parse(&call, ignored), Ok(Answer::Scrobbled { ignored: Some("Timestamp too old".into()) }));
        let listed = r##"{"scrobbles":{"@attr":{"accepted":0,"ignored":1},"scrobble":[{"ignoredMessage":{"code":"1","#text":"Artist ignored"}}]}}"##;
        assert_eq!(parse(&call, listed), Ok(Answer::Scrobbled { ignored: Some("Artist ignored".into()) }));
    }
}

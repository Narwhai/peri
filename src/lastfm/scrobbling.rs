//! The app's side of Last.fm: connecting an account, and following what plays to send
//! "now playing" and the scrobble once a play counts. Failed scrobbles are dropped.

use rune::TextEdit;

use super::{Answer, Call, Client, Credentials, Error, INVALID_SESSION, Saved, Session, Song, auth_url, should_scrobble};
use crate::library::Track;

/// Last.fm's "This token has not been authorized".
const UNAUTHORIZED_TOKEN: u32 = 14;
/// Last.fm's "This token has expired".
const EXPIRED_TOKEN: u32 = 15;

/// What the app does with a reply.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    /// Open the approval page in the browser.
    OpenUrl(String),
    /// Show a now-playing or scrobble failure.
    Error(String),
}

/// The song playing now, and how long it has really played: seeking doesn't count.
struct Play {
    song: Song,
    duration: Option<f64>,
    /// When it started, in seconds since the Unix epoch.
    started: i64,
    played: f64,
    /// The last position seen while playing, and the clock then.
    last: Option<(f64, f64)>,
    scrobbled: bool,
}

pub struct Scrobbling {
    /// The user's own API account (when the build has none) and their session; saved in the
    /// preferences.
    pub saved: Saved,
    /// The app's API account, when the build has one: it's used instead of the user's.
    built_in: Option<Credentials>,
    /// None in headless runs and tests, which never reach the network.
    client: Option<Client>,
    /// The token the user is approving in the browser.
    pub token: Option<String>,
    /// Whether a connect call is on its way.
    pub busy: bool,
    /// The connect flow's latest news for the settings page.
    pub status: Option<String>,
    pub key: TextEdit,
    pub secret: TextEdit,
    play: Option<Play>,
}

impl Scrobbling {
    /// `built_in` is the build's API account ([`super::built_in`]). A saved session granted to
    /// another API key than the one in use is dropped: Last.fm would refuse it.
    pub fn new(saved: Option<Saved>, built_in: Option<Credentials>) -> Scrobbling {
        let mut saved = saved.unwrap_or(Saved { credentials: None, session: None });
        let (key, secret) = saved.credentials.as_ref().map_or((String::new(), String::new()), |c| (c.api_key.clone(), c.secret.clone()));
        let api_key = built_in.as_ref().or(saved.credentials.as_ref()).map(|c| c.api_key.clone());
        if saved.session.as_ref().is_some_and(|s| Some(&s.api_key) != api_key.as_ref()) {
            saved.session = None;
        }
        Scrobbling { saved, built_in, client: None, token: None, busy: false, status: None, key: TextEdit::new(key), secret: TextEdit::new(secret).secure(), play: None }
    }

    /// Whether the build has its own API account, so users only connect.
    pub fn has_built_in(&self) -> bool {
        self.built_in.is_some()
    }

    /// The API account requests are signed with: the build's, else the user's.
    pub fn credentials(&self) -> Option<&Credentials> {
        self.built_in.as_ref().or(self.saved.credentials.as_ref())
    }

    pub fn set_client(&mut self, client: Client) {
        self.client = Some(client);
    }

    /// The connected account's session.
    pub fn session(&self) -> Option<&Session> {
        self.saved.session.as_ref()
    }

    fn call(&mut self, call: Call) -> Result<(), String> {
        let Some(credentials) = self.credentials() else { return Err("Enter Last.fm API credentials first".into()) };
        let Some(client) = &self.client else { return Err("Last.fm is off in headless runs".into()) };
        client.call(credentials, call)
    }

    /// "Connect": keeps the typed credentials (in builds without their own) and asks for a
    /// token to approve.
    pub fn connect(&mut self) {
        if self.built_in.is_none() {
            let api_key = self.key.text.trim().to_string();
            let secret = self.secret.text.trim().to_string();
            if api_key.is_empty() || secret.is_empty() {
                self.status = Some("Enter the API key and the shared secret first.".into());
                return;
            }
            self.saved.credentials = Some(Credentials { api_key, secret });
        }
        self.saved.session = None;
        self.token = None;
        self.request(Call::Token);
    }

    /// "Finish connecting": trades the approved token for a session.
    pub fn finish(&mut self) {
        let Some(token) = self.token.clone() else { return };
        self.request(Call::Session { token });
    }

    /// "Open the page again", while a token waits for approval.
    pub fn approval_url(&self) -> Option<String> {
        let credentials = self.credentials()?;
        self.token.as_ref().map(|t| auth_url(&credentials.api_key, t))
    }

    pub fn cancel(&mut self) {
        self.token = None;
        self.busy = false;
        self.status = None;
    }

    /// "Disconnect": forgets the session, keeping the API credentials.
    pub fn disconnect(&mut self) {
        self.saved.session = None;
        self.token = None;
        self.status = None;
    }

    /// Forgets the account and the user's API credentials.
    pub fn forget(&mut self) {
        self.saved = Saved { credentials: None, session: None };
        self.token = None;
        self.busy = false;
        self.status = None;
        self.play = None;
        self.key.replace_text("");
        self.secret.replace_text("");
    }

    fn request(&mut self, call: Call) {
        match self.call(call) {
            Ok(()) => {
                self.busy = true;
                self.status = None;
            }
            Err(e) => {
                self.busy = false;
                self.status = Some(e);
            }
        }
    }

    /// `track` started playing: tell Last.fm, and follow it for the scrobble.
    pub fn started(&mut self, track: &Track) -> Result<(), String> {
        self.play = None;
        let Some(session) = self.session().map(|s| s.key.clone()) else { return Ok(()) };
        if self.client.is_none() {
            return Ok(());
        }
        // Last.fm needs an artist; untagged files can't be scrobbled.
        let Some(song) = song_of(track) else { return Ok(()) };
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Can't scrobble: the clock is before 1970 ({e})"))
            .and_then(|d| i64::try_from(d.as_secs()).map_err(|e| format!("Can't scrobble: the clock is out of range ({e})")))?;
        self.play = Some(Play { song: song.clone(), duration: track.duration, started, played: 0.0, last: None, scrobbled: false });
        self.call(Call::NowPlaying { session, song })
    }

    pub fn stopped(&mut self) {
        self.play = None;
    }

    /// Every frame: counts the time played since the last tick (when the position moved no
    /// further than the clock did, so seeks don't count), and scrobbles once the play counts.
    pub fn tick(&mut self, now: f64, position: f64, playing: bool) -> Result<(), String> {
        let Some(play) = &mut self.play else { return Ok(()) };
        if !playing {
            play.last = None;
            return Ok(());
        }
        if let Some((p0, t0)) = play.last {
            let moved = position - p0;
            if moved > 0.0 && moved <= (now - t0) + 0.5 {
                play.played += moved;
            }
        }
        play.last = Some((position, now));
        if play.scrobbled || !should_scrobble(play.duration, play.played) {
            return Ok(());
        }
        play.scrobbled = true;
        let (song, started) = (play.song.clone(), play.started);
        let Some(session) = self.session().map(|s| s.key.clone()) else { return Ok(()) };
        self.call(Call::Scrobble { session, song, started })
    }

    /// Takes the worker's replies; the bool is whether anything shown changed.
    pub fn drain(&mut self) -> (bool, Vec<Effect>) {
        let Some(client) = &self.client else { return (false, Vec::new()) };
        let replies: Vec<_> = client.replies().collect();
        let mut effects = Vec::new();
        for reply in &replies {
            self.answered(&reply.call, reply.result.clone(), &mut effects);
        }
        (!replies.is_empty(), effects)
    }

    fn answered(&mut self, call: &Call, result: Result<Answer, Error>, effects: &mut Vec<Effect>) {
        match (call, result) {
            (Call::Token, Ok(Answer::Token(token))) => {
                self.busy = false;
                match self.credentials().map(|c| auth_url(&c.api_key, &token)) {
                    Some(url) => {
                        self.token = Some(token);
                        effects.push(Effect::OpenUrl(url));
                    }
                    None => self.status = Some("The Last.fm credentials were removed while connecting.".into()),
                }
            }
            (Call::Session { .. }, Ok(Answer::Session { key, name })) => {
                self.busy = false;
                self.token = None;
                self.status = None;
                match self.credentials().map(|c| c.api_key.clone()) {
                    Some(api_key) => self.saved.session = Some(Session { key, name, api_key }),
                    None => self.status = Some("The Last.fm credentials were removed while connecting.".into()),
                }
            }
            (Call::Token | Call::Session { .. }, Err(e)) => {
                self.busy = false;
                self.status = Some(match e.code {
                    Some(UNAUTHORIZED_TOKEN) => "Last.fm hasn't been approved yet. Allow access on the page that opened, then finish again.".into(),
                    Some(EXPIRED_TOKEN) => {
                        self.token = None;
                        "The approval expired. Connect again.".into()
                    }
                    _ => format!("Couldn't connect to Last.fm: {}", e.message),
                });
            }
            (Call::NowPlaying { song, .. } | Call::Scrobble { song, .. }, Err(e)) => {
                let what = if matches!(call, Call::Scrobble { .. }) { "scrobble" } else { "send now playing for" };
                if e.code == Some(INVALID_SESSION) {
                    self.disconnect();
                    self.play = None;
                    self.status = Some("Last.fm no longer accepts this connection. Connect again.".into());
                }
                effects.push(Effect::Error(format!("Last.fm: couldn't {what} \u{201c}{}\u{201d}: {}", song.track, e.message)));
            }
            (Call::Scrobble { song, .. }, Ok(Answer::Scrobbled { ignored: Some(reason) })) => {
                effects.push(Effect::Error(format!("Last.fm ignored \u{201c}{}\u{201d}: {reason}", song.track)));
            }
            (Call::NowPlaying { .. }, Ok(Answer::NowPlaying)) | (Call::Scrobble { .. }, Ok(Answer::Scrobbled { ignored: None })) => {}
            (_, Ok(answer)) => effects.push(Effect::Error(format!("Last.fm answered {answer:?} to {call:?}"))),
        }
    }

    /// Answers the call the fake client sent, as the worker would.
    #[cfg(test)]
    pub fn answer(&mut self, call: &Call, result: Result<Answer, Error>) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.answered(call, result, &mut effects);
        effects
    }
}

/// The song Last.fm hears about: the artist tag as written; None without an artist.
pub fn song_of(track: &Track) -> Option<Song> {
    let artist = track.artist()?;
    let album_artist = track.album_artist.clone().filter(|a| *a != artist);
    Some(Song {
        artist,
        track: track.title.clone(),
        album: track.album.clone(),
        album_artist,
        number: track.number,
        duration: track.duration.map(|d| d.round() as u32),
        mbid: track.recording_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> (Scrobbling, std::sync::mpsc::Receiver<super::super::Job>) {
        let saved = Saved { credentials: Some(credentials("key")), session: Some(session("sk", "key")) };
        let mut s = Scrobbling::new(Some(saved), None);
        let (client, jobs, _replies) = Client::fake();
        s.set_client(client);
        (s, jobs)
    }

    fn credentials(api_key: &str) -> Credentials {
        Credentials { api_key: api_key.into(), secret: "secret".into() }
    }

    fn session(key: &str, api_key: &str) -> Session {
        Session { key: key.into(), name: "rj".into(), api_key: api_key.into() }
    }

    fn track(duration: f64) -> Track {
        Track {
            title: "Jóga".into(),
            credit: Some("Björk".into()),
            album_artist: Some("Björk".into()),
            album: Some("Homogenic".into()),
            number: Some(3),
            duration: Some(duration),
            recording_id: Some("mbid-1".into()),
            ..Track::default()
        }
    }

    fn calls(jobs: &std::sync::mpsc::Receiver<super::super::Job>) -> Vec<Call> {
        jobs.try_iter().map(|j| j.call).collect()
    }

    /// Plays from `from` to `to` seconds, a second per tick, starting at clock `now`.
    fn play(s: &mut Scrobbling, now: &mut f64, from: f64, to: f64) {
        let mut p = from;
        while p <= to {
            s.tick(*now, p, true).unwrap();
            p += 1.0;
            *now += 1.0;
        }
    }

    #[test]
    fn the_song_is_the_tags_as_written() {
        let song = song_of(&track(305.0)).unwrap();
        assert_eq!(song.artist, "Björk");
        assert_eq!(song.album_artist, None, "the same as the artist, so left out");
        assert_eq!(song.duration, Some(305));
        assert_eq!(song.mbid.as_deref(), Some("mbid-1"));
        let duet = Track { credit: Some("A & B feat. C".into()), album_artist: Some("A & B".into()), ..track(200.0) };
        let song = song_of(&duet).unwrap();
        assert_eq!(song.artist, "A & B feat. C");
        assert_eq!(song.album_artist.as_deref(), Some("A & B"));
        assert_eq!(song_of(&Track { credit: None, ..track(200.0) }), None, "no artist, no scrobble");
    }

    #[test]
    fn now_playing_then_one_scrobble_at_half_the_song() {
        let (mut s, jobs) = connected();
        s.started(&track(100.0)).unwrap();
        assert!(matches!(calls(&jobs).as_slice(), [Call::NowPlaying { session, .. }] if session == "sk"));
        let mut now = 0.0;
        play(&mut s, &mut now, 0.0, 49.0);
        assert!(calls(&jobs).is_empty(), "49 seconds of 100 isn't half");
        play(&mut s, &mut now, 50.0, 90.0);
        assert!(matches!(calls(&jobs).as_slice(), [Call::Scrobble { session, song, .. }] if session == "sk" && song.track == "Jóga"), "scrobbled once");
    }

    #[test]
    fn seeking_ahead_doesnt_count_as_listening() {
        let (mut s, jobs) = connected();
        s.started(&track(100.0)).unwrap();
        calls(&jobs);
        let mut now = 0.0;
        play(&mut s, &mut now, 0.0, 10.0);
        play(&mut s, &mut now, 80.0, 99.0);
        assert!(calls(&jobs).is_empty(), "30 seconds heard of 100");
    }

    #[test]
    fn paused_time_doesnt_count_and_short_songs_never_scrobble() {
        let (mut s, jobs) = connected();
        s.started(&track(100.0)).unwrap();
        calls(&jobs);
        for t in 0..200 {
            s.tick(f64::from(t), 5.0, false).unwrap();
        }
        assert!(calls(&jobs).is_empty());
        s.started(&track(30.0)).unwrap();
        calls(&jobs);
        let mut now = 0.0;
        play(&mut s, &mut now, 0.0, 30.0);
        assert!(calls(&jobs).is_empty(), "30 seconds is too short to scrobble");
    }

    #[test]
    fn nothing_is_sent_when_not_connected() {
        let (mut s, jobs) = connected();
        s.disconnect();
        s.started(&track(100.0)).unwrap();
        let mut now = 0.0;
        play(&mut s, &mut now, 0.0, 100.0);
        assert!(calls(&jobs).is_empty());
    }

    #[test]
    fn a_revoked_session_disconnects() {
        let (mut s, jobs) = connected();
        s.started(&track(100.0)).unwrap();
        let call = calls(&jobs).remove(0);
        let effects = s.answer(&call, Err(Error { code: Some(INVALID_SESSION), message: "Invalid session key".into() }));
        assert_eq!(s.session(), None);
        assert!(s.saved.credentials.is_some(), "the API credentials stay");
        assert!(matches!(effects.as_slice(), [Effect::Error(e)] if e.contains("Invalid session key")));
    }

    #[test]
    fn failed_and_ignored_scrobbles_are_reported_and_dropped() {
        let (mut s, jobs) = connected();
        let call = Call::Scrobble { session: "sk".into(), song: song_of(&track(100.0)).unwrap(), started: 1 };
        let effects = s.answer(&call, Err(Error { code: Some(11), message: "Service Offline".into() }));
        assert!(matches!(effects.as_slice(), [Effect::Error(e)] if e.contains("Service Offline")));
        let effects = s.answer(&call, Ok(Answer::Scrobbled { ignored: Some("Timestamp too old".into()) }));
        assert!(matches!(effects.as_slice(), [Effect::Error(e)] if e.contains("Timestamp too old")));
        assert!(calls(&jobs).is_empty(), "nothing is retried");
        assert!(s.session().is_some());
    }

    #[test]
    fn a_built_in_account_is_used_without_asking_for_one() {
        let mut s = Scrobbling::new(None, Some(credentials("app")));
        let (client, jobs, _replies) = Client::fake();
        s.set_client(client);
        assert!(s.has_built_in());
        s.connect();
        let job = jobs.try_recv().unwrap();
        assert_eq!((job.call, job.credentials), (Call::Token, credentials("app")));
        let effects = s.answer(&Call::Token, Ok(Answer::Token("tok".into())));
        assert_eq!(effects, [Effect::OpenUrl("https://www.last.fm/api/auth/?api_key=app&token=tok".into())]);
        s.answer(&Call::Session { token: "tok".into() }, Ok(Answer::Session { key: "sk".into(), name: "rj".into() }));
        assert_eq!(s.session(), Some(&session("sk", "app")));
        assert_eq!(s.saved.credentials, None, "the built-in account isn't saved");
    }

    #[test]
    fn the_built_in_account_wins_over_saved_credentials() {
        let saved = Saved { credentials: Some(credentials("mine")), session: Some(session("sk", "app")) };
        let s = Scrobbling::new(Some(saved), Some(credentials("app")));
        assert_eq!(s.credentials(), Some(&credentials("app")));
        assert_eq!(s.session(), Some(&session("sk", "app")), "granted to the built-in key, so it stays");
    }

    #[test]
    fn a_session_granted_to_another_key_is_dropped() {
        // Connected with their own key, then upgraded to a build with one built in.
        let saved = Saved { credentials: Some(credentials("mine")), session: Some(session("sk", "mine")) };
        assert_eq!(Scrobbling::new(Some(saved.clone()), Some(credentials("app"))).session(), None);
        // Connected with a built-in key, then on a build without one.
        let saved = Saved { credentials: Some(credentials("mine")), session: Some(session("sk", "app")) };
        assert_eq!(Scrobbling::new(Some(saved.clone()), None).session(), None);
        // The built-in key changed between releases.
        assert_eq!(Scrobbling::new(Some(saved), Some(credentials("app2"))).session(), None);
    }

    #[test]
    fn an_unapproved_token_can_be_finished_again() {
        let (mut s, jobs) = connected();
        s.disconnect();
        s.key.replace_text("");
        s.connect();
        assert!(s.status.is_some(), "the fields are empty: {:?}", s.status);
        s.key.replace_text("key");
        s.secret.replace_text("secret");
        s.connect();
        assert_eq!(calls(&jobs), [Call::Token]);
        let effects = s.answer(&Call::Token, Ok(Answer::Token("tok".into())));
        assert_eq!(effects, [Effect::OpenUrl("https://www.last.fm/api/auth/?api_key=key&token=tok".into())]);
        s.finish();
        let finish = Call::Session { token: "tok".into() };
        assert_eq!(calls(&jobs), std::slice::from_ref(&finish));
        s.answer(&finish, Err(Error { code: Some(UNAUTHORIZED_TOKEN), message: "Unauthorized Token".into() }));
        assert_eq!(s.token.as_deref(), Some("tok"), "still waiting for approval");
        assert!(s.status.as_deref().is_some_and(|m| m.contains("hasn't been approved")));
        s.answer(&finish, Ok(Answer::Session { key: "sk2".into(), name: "rj".into() }));
        assert_eq!(s.session(), Some(&session("sk2", "key")));
        assert_eq!(s.token, None);
        assert_eq!(s.status, None);
    }
}

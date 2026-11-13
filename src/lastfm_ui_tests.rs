//! Last.fm driven headlessly through a fake client: connecting from the settings page, and
//! what playback sends. Nothing reaches the network.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output};
use crate::query::store::Store;
use crate::lastfm::{Answer, Call, Client, Credentials, Error, INVALID_SESSION, Job, Reply, Saved, Session};
use crate::view::view;

struct Prefs(PathBuf);

impl Prefs {
    fn new(name: &str) -> Prefs {
        let dir = std::env::temp_dir().join(format!("peri-lastfm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Prefs(dir)
    }

    fn path(&self) -> PathBuf {
        self.0.join("prefs.json")
    }
}

impl Drop for Prefs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fake {
    rt: Runtime<App>,
    jobs: Receiver<Job>,
    replies: Sender<Reply>,
}

impl Fake {
    /// Without a built-in API account, as a build without the environment variables.
    fn new(prefs: &Path) -> Fake {
        Fake::with(prefs, None)
    }

    fn with(prefs: &Path, built_in: Option<Credentials>) -> Fake {
        let mut app = App::with_lastfm(Output::Silent, Store::at(prefs), built_in);
        app.set_waker(Arc::new(|| {}));
        let (client, jobs, replies) = Client::fake();
        app.lastfm.set_client(client);
        let mut rt = Runtime::new(app, view, (980.0, 900.0), Env::new(true, 1.0));
        rt.on_key = Some(crate::app::on_key);
        rt.poll = Some(crate::app::poll);
        rt.frame(0.0);
        Fake { rt, jobs, replies }
    }

    fn settle(&mut self) {
        for _ in 0..30 {
            self.rt.frame(1.0 / 60.0);
        }
    }

    fn click(&mut self, key: &str) {
        let (x, y) = self.rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
        self.rt.pointer_down(x, y);
        self.rt.pointer_up(x, y);
        self.settle();
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.rt.key(KeyEvent { key: Key::Text(c.to_string()), mods: Mods::default() });
        }
        self.settle();
    }

    fn calls(&self) -> Vec<Call> {
        self.jobs.try_iter().map(|j| j.call).collect()
    }

    fn reply(&mut self, call: Call, result: Result<Answer, Error>) {
        self.replies.send(Reply { call, result }).unwrap();
        self.settle();
    }

    fn text_of(&self, key: &str) -> String {
        self.rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
    }
}

fn credentials(api_key: &str) -> Credentials {
    Credentials { api_key: api_key.into(), secret: "secret".into() }
}

fn session(api_key: &str) -> Session {
    Session { key: "sk".into(), name: "rj".into(), api_key: api_key.into() }
}

fn saved(session: Option<Session>) -> Saved {
    Saved { credentials: Some(credentials("key")), session }
}

#[test]
fn connecting_from_settings_saves_the_session() {
    let prefs = Prefs::new("connect");
    let mut f = Fake::new(&prefs.path());
    f.click("settings");
    f.rt.update(|s, _| s.lastfm.key.focused = true);
    f.type_text("key");
    f.rt.update(|s, _| s.lastfm.secret.focused = true);
    f.type_text("secret");
    f.click("lastfm-connect");
    assert_eq!(f.calls(), [Call::Token]);
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(saved(None))), "the credentials are saved at once");

    f.reply(Call::Token, Ok(Answer::Token("tok".into())));
    assert_eq!(f.rt.take_url_requests(), ["https://www.last.fm/api/auth/?api_key=key&token=tok"]);
    assert!(f.rt.find("lastfm-approve").is_some());
    f.click("lastfm-reopen");
    assert_eq!(f.rt.take_url_requests().len(), 1);

    f.click("lastfm-finish");
    let finish = Call::Session { token: "tok".into() };
    assert_eq!(f.calls(), std::slice::from_ref(&finish));
    f.reply(finish, Ok(Answer::Session { key: "sk".into(), name: "rj".into() }));
    assert!(f.text_of("lastfm-connected").contains("rj"));
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(saved(Some(session("key"))))));

    f.click("lastfm-disconnect");
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(saved(None))));
    assert!(f.rt.find("lastfm-connect").is_some());
    f.click("lastfm-forget");
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(None));
    assert_eq!(f.rt.state.lastfm.key.text, "");
}

#[test]
fn connect_failures_show_on_the_settings_page() {
    let prefs = Prefs::new("fail");
    let mut f = Fake::new(&prefs.path());
    f.click("settings");
    f.click("lastfm-connect");
    assert!(f.calls().is_empty(), "no credentials, no call");
    assert!(f.text_of("lastfm-status").contains("API key"));
    f.rt.update(|s, _| {
        s.lastfm.key.replace_text("bad");
        s.lastfm.secret.replace_text("secret");
    });
    f.click("lastfm-connect");
    f.reply(Call::Token, Err(Error { code: Some(10), message: "Invalid API key".into() }));
    assert!(f.text_of("lastfm-status").contains("Invalid API key"));
    assert!(f.rt.take_url_requests().is_empty());
}

#[test]
fn playing_sends_now_playing_and_the_scrobble_once() {
    let prefs = Prefs::new("play");
    Store::at(prefs.path()).save_lastfm(&saved(Some(session("key")))).unwrap();
    let mut f = Fake::new(&prefs.path());
    f.rt.update(|s, _| crate::scene_library(s));
    let eta = f.rt.state.tracks.iter().position(|t| t.title == "ETA").unwrap();
    f.rt.update(|s, tx| s.play_index(eta, tx));
    f.rt.frame(0.0);
    assert!(matches!(f.calls().as_slice(), [Call::NowPlaying { session, song }] if session == "sk" && song.track == "ETA" && song.artist == "NewJeans"));
    // ETA lasts 151 seconds: the scrobble goes out after 75.5 of them.
    // The position follows the clock, as the engine's does, in the runtime's 0.2 s ticks.
    for step in 1..=400 {
        f.rt.update(|s, _| s.position = f64::from(step) * 0.2);
        f.rt.frame(0.2);
        if step == 350 {
            assert!(f.calls().is_empty(), "70 seconds isn't half of 151");
        }
    }
    let calls = f.calls();
    assert!(matches!(calls.as_slice(), [Call::Scrobble { song, .. }] if song.track == "ETA"), "{calls:?}");
    f.reply(calls[0].clone(), Err(Error { code: Some(INVALID_SESSION), message: "Invalid session key".into() }));
    assert!(f.rt.state.error.as_deref().is_some_and(|e| e.contains("Invalid session key")));
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(saved(None))), "a revoked session is forgotten");
}

#[test]
fn headless_runs_have_no_client_and_play_quietly() {
    let prefs = Prefs::new("headless");
    Store::at(prefs.path()).save_lastfm(&saved(Some(session("key")))).unwrap();
    let mut app = App::with_lastfm(Output::Silent, Store::at(prefs.path()), None);
    app.setup_with(Arc::new(|| {}), Vec::new());
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    rt.update(|s, _| crate::scene_library(s));
    rt.update(|s, tx| s.play_index(0, tx));
    rt.frame(1.0);
    assert_eq!(rt.state.error, None);
}

#[test]
fn a_build_with_its_own_api_account_only_asks_to_connect() {
    let prefs = Prefs::new("built-in");
    let mut f = Fake::with(&prefs.path(), Some(credentials("app")));
    f.click("settings");
    assert!(f.rt.find("lastfm-key").is_none(), "no API key field");
    assert!(f.rt.find("lastfm-create").is_none());
    f.click("lastfm-connect");
    let job = f.jobs.try_recv().unwrap();
    assert_eq!((job.call, job.credentials), (Call::Token, credentials("app")));
    f.reply(Call::Token, Ok(Answer::Token("tok".into())));
    assert_eq!(f.rt.take_url_requests(), ["https://www.last.fm/api/auth/?api_key=app&token=tok"]);
    f.click("lastfm-finish");
    f.calls();
    f.reply(Call::Session { token: "tok".into() }, Ok(Answer::Session { key: "sk".into(), name: "rj".into() }));
    assert!(f.text_of("lastfm-connected").contains("rj"));
    assert!(f.rt.find("lastfm-forget").is_none(), "no credentials of the user's to remove");
    let on_disk = Saved { credentials: None, session: Some(session("app")) };
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(on_disk)), "the app's key and secret aren't saved");
    f.click("lastfm-disconnect");
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(None));
}

#[test]
fn a_session_for_another_api_key_is_forgotten_at_launch() {
    let prefs = Prefs::new("stale");
    Store::at(prefs.path()).save_lastfm(&saved(Some(session("key")))).unwrap();
    let f = Fake::with(&prefs.path(), Some(credentials("app")));
    assert_eq!(f.rt.state.lastfm.session(), None);
    assert_eq!(Store::at(prefs.path()).load_lastfm(), Ok(Some(saved(None))), "the user's own credentials stay");
}

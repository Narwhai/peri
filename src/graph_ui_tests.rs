//! The Graph tab driven headlessly: switching tabs, finding and opening an artist, hiding link
//! kinds, and the camera surviving tab switches and restarts.

use rune::{Env, Runtime, WheelEvent};

use crate::app::{App, Output, Page};
use crate::base::store::Store;
use crate::credits::{Credit, Role};
use crate::graph::EdgeKind;
use crate::graph::state::Hover;
use crate::library::Track;
use crate::relations::{Link, LinkKind, Relations};
use crate::view::view;

fn song(n: u32, artists: &[&str], featured: &[&str], credits: Vec<Credit>) -> Track {
    Track {
        path: format!("/g/{n:02}.flac").into(),
        title: format!("Song {n}"),
        tagged: crate::library::TaggedArtists { artists: artists.iter().map(|s| s.to_string()).collect(), ..Default::default() },
        credit: Some(match artists.iter().partition::<Vec<&&str>, _>(|a| !featured.contains(a)) {
            (main, guests) if guests.is_empty() => main.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" & "),
            (main, guests) => format!("{} feat. {}", main.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" & "), guests.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" & ")),
        }),
        album: Some("Album".into()),
        credits,
        ..Track::default()
    }
}

fn library() -> Vec<Track> {
    vec![
        song(1, &["Armand Hammer", "The Alchemist"], &[], vec![Credit::parse(Role::Producer, "The Alchemist")]),
        song(2, &["Armand Hammer", "Silka"], &["Silka"], vec![Credit::parse(Role::Producer, "Kenny Segal")]),
        song(3, &["billy woods"], &[], vec![Credit::parse(Role::Producer, "Kenny Segal")]),
    ]
}

fn runtime(store: Store) -> Runtime<App> {
    let mut app = App::new(Output::Silent, store);
    app.relations = Relations { merges: Vec::new(), links: vec![Link { from: "billy woods".into(), kind: LinkKind::MemberOf, to: "Armand Hammer".into() }] };
    app.set_library(library(), false);
    let mut rt = Runtime::new(app, view, (1200.0, 800.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}")).rect
}

/// Where node `name` shows in the window.
fn node_on_screen(rt: &Runtime<App>, name: &str) -> (f32, f32) {
    let g = &rt.state.graph;
    let i = g.model.find(name).expect("a node");
    let [x, y] = g.screen(g.cam(), g.world[i]);
    let canvas = rect(rt, "graph-canvas");
    (canvas[0] + x, canvas[1] + y)
}

#[test]
fn the_graph_tab_shows_every_artist_and_library_returns() {
    let mut rt = runtime(Store::memory());
    assert!(rt.find("graph-canvas").is_none());
    click(&mut rt, "graph-tab");
    assert_eq!(rt.state.page, Page::Graph);
    let g = &rt.state.graph;
    assert_eq!(g.model.nodes.len(), 5, "{:?}", g.model.nodes);
    assert_eq!(g.world.len(), 5, "laid out");
    assert!(rt.find("graph-canvas").is_some());
    assert!(rt.find("toolbar").is_none(), "the views' toolbar belongs to the library");
    for kind in [EdgeKind::MemberOf, EdgeKind::Featured, EdgeKind::With, EdgeKind::Credit(Role::Producer)] {
        assert!(rt.find(&format!("kind-{}", kind.id())).is_some(), "the legend lists {}", kind.label());
    }
    assert!(rt.find("kind-alias").is_none(), "only kinds the graph has");
    click(&mut rt, "home");
    assert_eq!(rt.state.page, Page::Library);
}

#[test]
fn clicking_an_artist_opens_their_page_and_back_returns_to_the_graph() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    rt.pointer_move(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.graph.hovered_node(), rt.state.graph.model.find("Kenny Segal"));
    assert!(rt.find("graph-tip").is_some(), "the hovered name shows");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Artist("Kenny Segal".into()));
    assert!(rt.state.in_graph(), "the Graph tab stays selected on pages opened from it");
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.page, Page::Graph);
}

#[test]
fn hovering_a_link_names_it_and_lights_its_artists() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let (ax, ay) = node_on_screen(&rt, "Kenny Segal");
    let (bx, by) = node_on_screen(&rt, "billy woods");
    // Halfway along the link, clear of both dots.
    let (x, y) = ((ax + bx) / 2.0, (ay + by) / 2.0);
    rt.pointer_move(x, y);
    settle(&mut rt);
    let g = &rt.state.graph;
    let (k, b) = (g.model.find("Kenny Segal").expect("node"), g.model.find("billy woods").expect("node"));
    match &g.hover {
        Some(Hover::Link { a, b: other, edges }) => {
            assert_eq!((*a.min(other), *a.max(other)), (k.min(b), k.max(b)), "the pair the link joins");
            assert_eq!(edges.iter().map(|&e| g.model.edges[e].kind).collect::<Vec<_>>(), [EdgeKind::Credit(Role::Producer)]);
        }
        other => panic!("hovering the link: {other:?}"),
    }
    assert!(rt.find("graph-link-tip").is_some(), "the link's tooltip shows");
    let lines: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).filter(|t| t.contains("produced")).collect();
    assert_eq!(lines, ["Kenny Segal produced 1 song by billy woods"]);
    assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("lit-"))), "the link and its artists are lit");
    // Off the link, nothing is.
    let canvas = rect(&rt, "graph-canvas");
    rt.pointer_move(canvas[0] + 2.0, canvas[1] + 2.0);
    settle(&mut rt);
    assert_eq!(rt.state.graph.hover, None);
    assert!(rt.find("graph-link-tip").is_none());
}

#[test]
fn finding_an_artist_picks_and_centres_them() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    rt.update(|s, _| {
        s.graph.find = rune::TextEdit::new("segal");
        s.graph_found();
    });
    settle(&mut rt);
    let i = rt.state.graph.model.find("Kenny Segal");
    assert_eq!(rt.state.graph.selected, i);
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    let canvas = rect(&rt, "graph-canvas");
    assert!((x - (canvas[0] + canvas[2] / 2.0)).abs() < 1.0 && (y - (canvas[1] + canvas[3] / 2.0)).abs() < 1.0, "centred: {x},{y} in {canvas:?}");
    assert!(rt.find(&format!("n{}", i.expect("node"))).is_some(), "the picked artist is named");
}

#[test]
fn hiding_a_kind_of_link_and_the_camera_save_and_come_back() {
    let dir = std::env::temp_dir().join(format!("rune-music-graph-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let prefs = dir.join("prefs.json");
    {
        let mut rt = runtime(Store::at(&prefs));
        click(&mut rt, "graph-tab");
        click(&mut rt, "kind-producer");
        assert_ne!(rt.state.graph.hidden & EdgeKind::Credit(Role::Producer).bit(), 0);
        let canvas = rect(&rt, "graph-canvas");
        rt.wheel_event(WheelEvent::new(-40.0, 25.0).at(canvas[0] + 100.0, canvas[1] + 100.0).precise(true));
        settle(&mut rt);
        let cam = rt.state.graph.camera.expect("the camera moved");
        // Leaving and coming back keeps it.
        click(&mut rt, "home");
        click(&mut rt, "graph-tab");
        assert_eq!(rt.state.graph.camera, Some(cam));
        rt.update(|s, _| s.save_graph());
        assert!(dir.join(crate::graph::state::CACHE_FILE).exists(), "the layout is cached beside the preferences");
    }
    let rt = runtime(Store::at(&prefs));
    assert!(rt.state.graph.camera.is_some(), "the camera came back");
    assert_ne!(rt.state.graph.hidden & EdgeKind::Credit(Role::Producer).bit(), 0, "producer links stay hidden");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_cached_layout_is_reused_until_the_graph_changes() {
    let dir = std::env::temp_dir().join(format!("rune-music-graph-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let prefs = dir.join("prefs.json");
    let first = {
        let mut rt = runtime(Store::at(&prefs));
        click(&mut rt, "graph-tab");
        rt.state.graph.world.clone()
    };
    let cache = dir.join(crate::graph::state::CACHE_FILE);
    let written = std::fs::metadata(&cache).and_then(|m| m.modified()).expect("cached");
    let mut rt = runtime(Store::at(&prefs));
    click(&mut rt, "graph-tab");
    assert_eq!(rt.state.graph.world, first, "the same graph lays out from the cache");
    assert_eq!(std::fs::metadata(&cache).and_then(|m| m.modified()).expect("cached"), written, "and doesn't write it again");
    // A new artist: the old ones start where they were.
    rt.update(|s, _| {
        let mut tracks = library();
        tracks.push(song(4, &["Elucid"], &[], vec![Credit::parse(Role::Producer, "Kenny Segal")]));
        s.set_library(tracks, false);
    });
    settle(&mut rt);
    assert_eq!(rt.state.graph.model.nodes.len(), 6, "Elucid joined");
    assert_eq!(rt.state.graph.world.len(), 6, "laid out again");
    std::fs::remove_dir_all(&dir).ok();
}

/// `cargo test --release graph_frames_at_scale -- --ignored --nocapture`: frame times while
/// panning a 10,000-artist graph.
#[test]
#[ignore]
fn graph_frames_at_scale() {
    let mut tracks = Vec::new();
    for n in 0..60_000_u32 {
        let a = (n * 7919) % 10_000;
        let b = (a + 1 + n % 150) % 10_000;
        let c = (a + 3 + n % 90) % 10_000;
        let name = |i: u32| format!("Artist {i}");
        let mut t = song(n, &[&name(a)], &[], vec![Credit::parse(Role::ALL[(n % 11) as usize], &name(c))]);
        t.path = format!("/big/{n}.flac").into();
        if n % 3 == 0 {
            t.artists.push(name(b));
        }
        tracks.push(t);
    }
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_library(tracks, false);
    let mut rt = Runtime::new(app, view, (1400.0, 900.0), Env::new(true, 1.0));
    settle(&mut rt);
    let t = std::time::Instant::now();
    click(&mut rt, "graph-tab");
    println!("{} artists, {} edges: opened and laid out in {:.2?}", rt.state.graph.model.nodes.len(), rt.state.graph.model.edges.len(), t.elapsed());
    let canvas = rect(&rt, "graph-canvas");
    let mut times = Vec::new();
    for k in 0..120 {
        let t = std::time::Instant::now();
        rt.wheel_event(WheelEvent::new(if k % 40 < 20 { -12.0 } else { 12.0 }, 7.0).at(canvas[0] + 300.0, canvas[1] + 300.0).precise(true));
        rt.frame(1.0 / 60.0);
        times.push(t.elapsed());
    }
    times.sort();
    println!("pan frames: p50 {:.2?}, p95 {:.2?}, max {:.2?}", times[60], times[114], times[119]);
    let mut times = Vec::new();
    for k in 0..60 {
        let t = std::time::Instant::now();
        rt.wheel_event(WheelEvent::new(0.0, if k < 30 { 20.0 } else { -20.0 }).at(canvas[0] + 500.0, canvas[1] + 400.0).mods(rune::Mods { ctrl: true, ..Default::default() }).precise(true));
        rt.frame(1.0 / 60.0);
        times.push(t.elapsed());
    }
    times.sort();
    println!("zoom frames: p50 {:.2?}, p95 {:.2?}, max {:.2?}", times[30], times[57], times[59]);
    rt.update(|s, _| s.graph.reset_camera());
    settle(&mut rt);
    let mut times = Vec::new();
    let mut links = 0;
    for k in 0..120 {
        let t = std::time::Instant::now();
        links += usize::from(matches!(rt.state.graph.hover, Some(Hover::Link { .. })));
        rt.pointer_move(canvas[0] + canvas[2] * (0.3 + k as f32 * 0.0033), canvas[1] + canvas[3] * (0.3 + (k % 30) as f32 * 0.013));
        rt.frame(1.0 / 60.0);
        times.push(t.elapsed());
    }
    times.sort();
    println!("hover frames ({links} on links): p50 {:.2?}, p95 {:.2?}, max {:.2?}", times[60], times[114], times[119]);
}

/// `RUNE_MUSIC_DIR=~/Music cargo test --release real_library_graph -- --ignored --nocapture`:
/// the real library's graph, snapped to `$RUNE_MUSIC_SNAP` if set.
#[test]
#[ignore]
fn real_library_graph() {
    let dir = std::env::var("RUNE_MUSIC_DIR").expect("set RUNE_MUSIC_DIR to a music folder");
    let rx = crate::library::scan(vec![dir.into()], || {});
    let tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    let mut app = App::new(Output::Silent, Store::memory());
    app.relations = Store::open().load_relations().unwrap_or_default();
    app.set_library(tracks, false);
    let t = std::time::Instant::now();
    app.show_graph();
    let g = &app.graph;
    println!("{} artists, {} edges, laid out in {:.2?}", g.model.nodes.len(), g.model.edges.len(), t.elapsed());
    for kind in EdgeKind::ALL {
        let n = g.model.edges.iter().filter(|e| e.kind == kind).count();
        if n > 0 {
            println!("  {}: {n}", kind.label());
        }
    }
    let mut top: Vec<&crate::graph::Node> = g.model.nodes.iter().collect();
    top.sort_by_key(|n| std::cmp::Reverse(n.degree));
    println!("most linked: {:?}", top.iter().take(8).map(|n| (&n.name, n.degree)).collect::<Vec<_>>());
}

/// Prints where, in window points, the link between two artists of the real library shows,
/// for snapshot scripts: `RUNE_MUSIC_DIR=… RUNE_GRAPH_LINK="A|B" cargo test --release real_link_midpoint -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_link_midpoint() {
    let dir = std::env::var("RUNE_MUSIC_DIR").expect("set RUNE_MUSIC_DIR to a music folder");
    let pair = std::env::var("RUNE_GRAPH_LINK").expect("set RUNE_GRAPH_LINK to \"A|B\"");
    let (a, b) = pair.split_once('|').expect("A|B");
    let rx = crate::library::scan(vec![dir.into()], || {});
    let tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    let mut app = App::new(Output::Silent, Store::memory());
    app.relations = Store::open().load_relations().unwrap_or_default();
    app.set_library(tracks, false);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    settle(&mut rt);
    click(&mut rt, "graph-tab");
    let (ax, ay) = node_on_screen(&rt, a);
    let (bx, by) = node_on_screen(&rt, b);
    println!("{a} at {ax},{ay}; {b} at {bx},{by}; midpoint {},{}", (ax + bx) / 2.0, (ay + by) / 2.0);
}

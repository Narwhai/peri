//! The Graph tab driven headlessly: switching tabs, finding and opening an artist, hiding link
//! kinds, and the camera surviving tab switches and restarts.

use rune::{Env, Runtime, WheelEvent};

use crate::app::{App, Output, Page};
use crate::query::store::Store;
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
    app.set_relations(Relations { merges: Vec::new(), links: vec![Link { from: "billy woods".into(), kind: LinkKind::MemberOf, to: "Armand Hammer".into() }] });
    app.set_library(library(), false);
    let mut rt = Runtime::new(app, view, (1200.0, 800.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    // The drag simulation steps in poll, as in the app.
    rt.poll = Some(crate::app::poll);
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
    click(&mut rt, "library-tab");
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
    let dir = std::env::temp_dir().join(format!("peri-graph-{}", std::process::id()));
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
        click(&mut rt, "library-tab");
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
    let dir = std::env::temp_dir().join(format!("peri-graph-cache-{}", std::process::id()));
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
    // Drag the most linked artist about.
    rt.poll = Some(crate::app::poll);
    let hub = (0..rt.state.graph.model.nodes.len()).max_by_key(|&i| rt.state.graph.model.nodes[i].degree).expect("artists");
    let g = &rt.state.graph;
    let [x, y] = g.screen(g.cam(), g.world[hub]);
    let (x, y) = (canvas[0] + x, canvas[1] + y);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    let mut times = Vec::new();
    for k in 1..=120 {
        let t = std::time::Instant::now();
        let a = k as f32 * 0.1;
        rt.pointer_move(x + 60.0 * a.sin(), y + 40.0 * (a * 0.7).cos());
        rt.frame(1.0 / 60.0);
        times.push(t.elapsed());
    }
    let moving = rt.state.graph.drag.as_ref().map_or(0, |d| d.moving.len());
    rt.pointer_up(x, y);
    let mut settle_frames = 0;
    while rt.state.graph.drag.is_some() && settle_frames < 600 {
        let t = std::time::Instant::now();
        rt.frame(1.0 / 60.0);
        times.push(t.elapsed());
        settle_frames += 1;
    }
    times.sort();
    let n = times.len();
    println!("drag frames ({moving} moving, {settle_frames} to settle): p50 {:.2?}, p95 {:.2?}, max {:.2?}", times[n / 2], times[n * 95 / 100], times[n - 1]);
}

/// `PERI_MUSIC_DIR=~/Music cargo test --release real_library_graph -- --ignored --nocapture`:
/// the real library's graph, snapped to `$PERI_MUSIC_SNAP` if set.
#[test]
#[ignore]
fn real_library_graph() {
    let dir = std::env::var("PERI_MUSIC_DIR").expect("set PERI_MUSIC_DIR to a music folder");
    let rx = crate::library::scan(vec![dir.into()], || {});
    let tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_relations(Store::open().load_relations().unwrap_or_default());
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
/// for snapshot scripts: `PERI_MUSIC_DIR=… RUNE_GRAPH_LINK="A|B" cargo test --release real_link_midpoint -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_link_midpoint() {
    let dir = std::env::var("PERI_MUSIC_DIR").expect("set PERI_MUSIC_DIR to a music folder");
    let pair = std::env::var("RUNE_GRAPH_LINK").expect("set RUNE_GRAPH_LINK to \"A|B\"");
    let (a, b) = pair.split_once('|').expect("A|B");
    let rx = crate::library::scan(vec![dir.into()], || {});
    let tracks = loop {
        if let crate::library::ScanMsg::Done { tracks, .. } = rx.recv().expect("the scan finishes") {
            break tracks;
        }
    };
    let mut app = App::new(Output::Silent, Store::memory());
    app.set_relations(Store::open().load_relations().unwrap_or_default());
    app.set_library(tracks, false);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    settle(&mut rt);
    click(&mut rt, "graph-tab");
    let (ax, ay) = node_on_screen(&rt, a);
    let (bx, by) = node_on_screen(&rt, b);
    println!("{a} at {ax},{ay}; {b} at {bx},{by}; midpoint {},{}", (ax + bx) / 2.0, (ay + by) / 2.0);
}

/// Drags artist `name` by (`dx`, `dy`) window points and lets go, then lets the others settle.
fn drag_artist(rt: &mut Runtime<App>, name: &str, dx: f32, dy: f32) {
    let (x, y) = node_on_screen(rt, name);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    for k in 1..=9 {
        let t = k as f32 / 10.0;
        rt.pointer_move(x + dx * t, y + dy * t);
        rt.frame(1.0 / 60.0);
    }
    // The last move and the release come together, before the simulation steps again.
    rt.pointer_move(x + dx, y + dy);
    rt.pointer_up(x + dx, y + dy);
    for _ in 0..600 {
        rt.frame(1.0 / 60.0);
        if rt.state.graph.drag.is_none() {
            break;
        }
    }
    settle(rt);
}

fn world_of(rt: &Runtime<App>, name: &str) -> [f32; 2] {
    let g = &rt.state.graph;
    g.world[g.model.find(name).expect("a node")]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[test]
fn dragging_an_artist_moves_it_pulls_its_neighbours_and_pins_it() {
    let mut rt = runtime(Store::memory());
    // Lord Sko is three links from Kenny Segal: Kenny Segal, Armand Hammer, The Alchemist.
    rt.update(|s, _| {
        let mut tracks = library();
        tracks.push(song(5, &["Lord Sko"], &[], vec![Credit::parse(Role::Producer, "The Alchemist")]));
        s.set_library(tracks, false);
    });
    settle(&mut rt);
    click(&mut rt, "graph-tab");
    let zoom = rt.state.graph.cam().zoom;
    let (kenny, woods, far) = (world_of(&rt, "Kenny Segal"), world_of(&rt, "billy woods"), world_of(&rt, "Lord Sko"));
    drag_artist(&mut rt, "Kenny Segal", 120.0, 80.0);
    let g = &rt.state.graph;
    assert!(g.drag.is_none(), "the drop settled");
    assert_eq!(rt.state.page, Page::Graph, "a drag doesn't open the artist");
    let moved = world_of(&rt, "Kenny Segal");
    assert!((moved[0] - kenny[0] - 120.0 / zoom).abs() < 1.0 && (moved[1] - kenny[1] - 80.0 / zoom).abs() < 1.0, "the artist went with the pointer: {kenny:?} to {moved:?}");
    let i = g.model.find("Kenny Segal").expect("node");
    assert!(g.pinned[i], "dropped means pinned");
    assert_eq!(g.pinned.iter().filter(|p| **p).count(), 1, "only the dragged artist is pinned");
    let pulled = world_of(&rt, "billy woods");
    assert!(dist(pulled, moved) < dist(woods, moved), "a linked artist followed: {woods:?} to {pulled:?}");
    assert_eq!(world_of(&rt, "Lord Sko"), far, "artists more than two links away hold still");
    // The moved artist is found where it now is.
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    rt.pointer_move(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.graph.hovered_node(), Some(i));
}

#[test]
fn a_cancelled_drag_puts_everything_back() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let before = rt.state.graph.world.clone();
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    rt.pointer_down(x, y);
    for k in 1..=5 {
        rt.pointer_move(x + 20.0 * k as f32, y);
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.graph.drag.is_some());
    rt.update(|s, _| s.graph.cancel_drag());
    rt.pointer_up(x + 100.0, y);
    settle(&mut rt);
    assert_eq!(rt.state.graph.world, before);
    assert!(rt.state.graph.pinned.iter().all(|p| !p), "nothing was pinned");
}

#[test]
fn pins_survive_a_restart_and_a_new_layout_and_reset_layout_clears_them() {
    let dir = std::env::temp_dir().join(format!("peri-graph-pins-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let prefs = dir.join("prefs.json");
    let dropped = {
        let mut rt = runtime(Store::at(&prefs));
        click(&mut rt, "graph-tab");
        drag_artist(&mut rt, "Kenny Segal", 150.0, -60.0);
        rt.state.graph.layout_raw("Kenny Segal").expect("placed")
    };
    let mut rt = runtime(Store::at(&prefs));
    click(&mut rt, "graph-tab");
    let i = rt.state.graph.model.find("Kenny Segal").expect("node");
    assert!(rt.state.graph.pinned[i], "the pin came back");
    assert_eq!(rt.state.graph.layout_raw("Kenny Segal"), Some(dropped), "where it was dropped");
    // New music lays the graph out again: the pinned artist stays.
    rt.update(|s, _| {
        let mut tracks = library();
        tracks.push(song(4, &["Elucid"], &[], vec![Credit::parse(Role::Producer, "Kenny Segal")]));
        s.set_library(tracks, false);
    });
    settle(&mut rt);
    assert_eq!(rt.state.graph.model.nodes.len(), 6);
    assert_eq!(rt.state.graph.layout_raw("Kenny Segal"), Some(dropped), "a new layout leaves it be");
    assert!(rt.state.graph.pinned[rt.state.graph.model.find("Kenny Segal").expect("node")]);
    click(&mut rt, "graph-reset");
    assert!(rt.state.graph.pinned.iter().all(|p| !p), "Reset layout unpins everyone");
    assert_ne!(rt.state.graph.layout_raw("Kenny Segal"), Some(dropped), "and lays out from scratch");
    let rt = runtime(Store::at(&prefs));
    let mut rt = rt;
    click(&mut rt, "graph-tab");
    assert!(rt.state.graph.pinned.iter().all(|p| !p), "the reset was saved");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn right_clicking_a_pinned_artist_unpins_it() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    drag_artist(&mut rt, "Kenny Segal", 100.0, 40.0);
    let i = rt.state.graph.model.find("Kenny Segal").expect("node");
    assert!(rt.state.graph.pinned[i]);
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.graph.menu.map(|(n, _)| n), Some(i));
    // The menu's items: Open artist, then Unpin.
    let (ux, uy) = rt.find("graph-menu-item2").expect("Unpin");
    rt.pointer_down(ux, uy);
    rt.pointer_up(ux, uy);
    settle(&mut rt);
    assert!(!rt.state.graph.pinned[i], "unpinned");
    assert!(rt.state.graph.menu.is_none(), "the menu closed");
}

/// Presses and drags from window point (`x`, `y`) by (`dx`, `dy`), letting go without waiting
/// for anything to settle.
fn quick_drag(rt: &mut Runtime<App>, (x, y): (f32, f32), dx: f32, dy: f32) {
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    for k in 1..=6 {
        let t = k as f32 / 6.0;
        rt.pointer_move(x + dx * t, y + dy * t);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x + dx, y + dy);
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_new_press_while_a_dropped_artist_settles_leaves_it_where_it_was_dropped() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let at = node_on_screen(&rt, "Kenny Segal");
    quick_drag(&mut rt, at, 120.0, 80.0);
    assert!(rt.state.graph.drag.is_some(), "its neighbours are still settling");
    let dropped = world_of(&rt, "Kenny Segal");
    // Straight away, pan from empty space: the dropped artist doesn't follow the pointer.
    let canvas = rect(&rt, "graph-canvas");
    quick_drag(&mut rt, (canvas[0] + 20.0, canvas[1] + 20.0), 90.0, 40.0);
    assert_eq!(world_of(&rt, "Kenny Segal"), dropped, "panning doesn't move the dropped artist");
    // And straight away again, drag another artist: that one moves, not the dropped one.
    let woods = world_of(&rt, "The Alchemist");
    let at = node_on_screen(&rt, "The Alchemist");
    quick_drag(&mut rt, at, -100.0, 60.0);
    assert_ne!(world_of(&rt, "The Alchemist"), woods, "the artist pressed moves");
    // Moving the pointer with no button held moves nothing.
    let before = rt.state.graph.world.clone();
    let (x, y) = node_on_screen(&rt, "The Alchemist");
    for k in 1..=6 {
        rt.pointer_move(x + 15.0 * k as f32, y + 10.0 * k as f32);
        rt.frame(1.0 / 60.0);
    }
    let g = &rt.state.graph;
    for name in ["Kenny Segal", "The Alchemist"] {
        let i = g.model.find(name).expect("node");
        assert_eq!(g.world[i], before[i], "{name} stays put while the pointer merely moves");
    }
    for _ in 0..600 {
        rt.frame(1.0 / 60.0);
    }
    let g = &rt.state.graph;
    assert!(g.drag.is_none());
    for name in ["Kenny Segal", "The Alchemist"] {
        assert!(g.pinned[g.model.find(name).expect("node")], "{name} is pinned where it was dropped");
    }
    assert_eq!(world_of(&rt, "Kenny Segal"), dropped);
}

/// How far, at most, the moving tiles draw from where the simulation puts them (in window
/// points), and how many frames draw one fading, over a drag and its settling.
#[test]
#[ignore]
fn settling_draws_where_the_simulation_is() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let (x, y) = node_on_screen(&rt, "Kenny Segal");
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    let (mut worst, mut fades) = (0.0_f32, 0);
    let mut check = |rt: &Runtime<App>| {
        let canvas = rect(rt, "graph-canvas");
        let g = &rt.state.graph;
        let cam = g.cam();
        for e in rt.elements() {
            if !e.key.as_deref().is_some_and(|k| k.starts_with("moving-")) {
                continue;
            }
            // Where the tile's laid-out corner shows under the camera.
            let [sx, sy] = g.screen(cam, [e.target[0] - canvas[0], e.target[1] - canvas[1]]);
            let lag = (e.rect[0] - canvas[0] - sx).abs().max((e.rect[1] - canvas[1] - sy).abs());
            worst = worst.max(lag);
            fades += usize::from(e.opacity < 0.99);
        }
    };
    for k in 1..=20 {
        rt.pointer_move(x + 8.0 * k as f32, y + 5.0 * k as f32);
        rt.frame(1.0 / 60.0);
        check(&rt);
    }
    rt.pointer_up(x + 160.0, y + 100.0);
    for _ in 0..240 {
        rt.frame(1.0 / 60.0);
        check(&rt);
    }
    println!("tiles drawn off the simulation: worst {worst:.2} pt; fading tile-frames: {fades}");
}

#[test]
fn the_window_keeps_drawing_while_artists_settle_and_rests_after() {
    let mut rt = runtime(Store::memory());
    click(&mut rt, "graph-tab");
    let at = node_on_screen(&rt, "Kenny Segal");
    quick_drag(&mut rt, at, 120.0, 80.0);
    assert!(rt.state.graph.drag.is_some(), "settling");
    // A frame that says it's animating asks the host for the next display frame: the
    // simulation steps at the display's rate, not a 60 Hz timer's.
    let mut frames = 0;
    while rt.state.graph.drag.is_some() && frames < 600 {
        let asked = rt.frame(1.0 / 144.0);
        // The frame the artists settle in has nothing more to draw.
        assert!(asked || rt.state.graph.drag.is_none(), "frame {frames} of the settling asked for the next");
        frames += 1;
    }
    assert!(rt.state.graph.drag.is_none(), "settled");
    settle(&mut rt);
    assert!(!rt.frame(1.0 / 144.0), "once settled, the window rests");
}


fn has(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn the_active_header_tab_stands_on_the_header_edge_and_flares_beside_its_neighbours() {
    let mut rt = runtime(Store::memory());
    let bottom = |r: [f32; 4]| r[1] + r[3];
    let right_edge = |r: [f32; 4]| r[0] + r[2];
    let header = bottom(rect(&rt, "header"));
    let tabs = ["home-tab", "library-tab", "graph-tab"];
    let check = |rt: &Runtime<App>, on: &str| {
        for t in tabs {
            assert_eq!(bottom(rect(rt, t)), header, "{t} stands on the header's bottom edge");
        }
        let tab = rect(rt, on);
        let (left, right) = (rect(rt, &format!("{on}-flare-left")), rect(rt, &format!("{on}-flare-right")));
        for flare in [left, right] {
            assert_eq!(bottom(flare), header, "{on}'s flares stand on the header's bottom edge");
            assert!(flare[3] > 1.0, "a flare is taller than the header's line");
        }
        // Each flare overlaps only the tab's 1-point side border and reaches out beside it.
        assert!(left[0] < tab[0] && right_edge(left) <= tab[0] + 1.0, "the left flare is outside {on}");
        assert!(right[0] >= right_edge(tab) - 1.0 && right_edge(right) > right_edge(tab), "the right flare is outside {on}");
        let apart = |a: [f32; 4], b: [f32; 4]| right_edge(a) <= b[0] || a[0] >= right_edge(b);
        for off in tabs.into_iter().filter(|&t| t != on) {
            assert!(!has(rt, &format!("{off}-flare-left")) && !has(rt, &format!("{off}-flare-right")), "only the active tab flares");
            assert!(!has(rt, &format!("{off}-seam")), "no seam on {off}");
            // Inactive tabs are bare labels whose hover fill stays inside them, clear of the flares.
            let hover = rect(rt, &format!("{off}-hover"));
            assert!(apart(hover, left) && apart(hover, right), "{off}'s hover fill stays clear of {on}'s flares");
        }
        assert!(!has(rt, &format!("{on}-hover")), "the active tab has no hover fill");
    };
    // Library is active: no two inactive tabs are neighbours, so no dividers.
    check(&rt, "library-tab");
    assert!(!has(&rt, "home-tab-divider") && !has(&rt, "library-tab-divider"), "no divider beside the active tab");
    click(&mut rt, "graph-tab");
    check(&rt, "graph-tab");
    // Home and Library are inactive neighbours: one divider in the gap between them.
    let divider = rect(&rt, "home-tab-divider");
    let (home, library) = (rect(&rt, "home-tab"), rect(&rt, "library-tab"));
    assert!(divider[0] >= right_edge(home) - 1.0 && right_edge(divider) <= library[0] + 1.0, "the divider sits between Home and Library");
    assert!(!has(&rt, "library-tab-divider"), "no divider beside the active Graph tab");
}

#[test]
fn switching_header_tabs_leaves_their_icons_and_labels_in_place() {
    let mut rt = runtime(Store::memory());
    // Every icon and label in the tabs, visible or fading out, by path and rect.
    let glyphs = |rt: &Runtime<App>| -> Vec<(String, [f32; 4], bool)> {
        rt.elements().into_iter()
            .filter(|e| e.path.contains("/tabs/") && matches!(e.role, "icon" | "text"))
            .map(|e| (e.path, e.rect, e.exiting))
            .collect()
    };
    let rest = glyphs(&rt);
    assert_eq!(rest.len(), 6, "an icon and a label in each of three tabs: {rest:?}");
    for target in ["graph-tab", "home-tab", "library-tab"] {
        let (x, y) = rt.find(target).unwrap_or_else(|| panic!("no element {target}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        // Each frame of the switch, not only once it settles.
        for frame in 0..30 {
            rt.frame(1.0 / 60.0);
            for (path, rect, exiting) in glyphs(&rt) {
                assert!(!exiting, "switching to {target}, frame {frame}: {path} fades out instead of staying");
                let home = rest.iter().find(|g| g.0 == path).unwrap_or_else(|| panic!("switching to {target}, frame {frame}: {path} is new"));
                assert_eq!(rect, home.1, "switching to {target}, frame {frame}: {path} stays put");
            }
        }
    }
}

// Pictures

fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A music folder under the temp folder, removed on drop.
struct Music(std::path::PathBuf);

impl Music {
    fn new(name: &str) -> Music {
        let dir = std::env::temp_dir().join(format!("peri-graph-pictures-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Music(dir)
    }

    /// `n`'s song on `artist`'s album `album`, in `Artist/Album/`.
    fn song(&self, n: u32, artist: &str, album: &str, credits: Vec<Credit>) -> Track {
        let dir = self.0.join(artist).join(album);
        std::fs::create_dir_all(&dir).unwrap();
        Track { path: dir.join(format!("{n:02}.flac")), album_artist: Some(artist.into()), album: Some(album.into()), ..song(n, &[artist], &[], credits) }
    }

    fn image(&self, rel: &str, rgba: [u8; 4]) -> std::path::PathBuf {
        let path = self.0.join(rel);
        std::fs::write(&path, png(64, 64, rgba)).unwrap();
        path
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Armand Hammer has an `artist.jpg` in their folder, billy woods only an album cover, Kenny
/// Segal a picture of the user's, and Silka (featured, with no album) nothing at all. Lookups
/// are on, answered by `fetch`, which records what it's asked.
fn pictured_runtime(music: &Music, asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Runtime<App> {
    let tracks = vec![
        music.song(1, "Armand Hammer", "Haram", vec![Credit::parse(Role::Producer, "The Alchemist")]),
        music.song(2, "billy woods", "Hiding Places", vec![Credit::parse(Role::Producer, "Kenny Segal")]),
        Track { path: music.0.join("loose.flac"), ..song(3, &["Armand Hammer", "Silka"], &["Silka"], vec![Credit::parse(Role::Producer, "Kenny Segal")]) },
    ];
    music.image("Armand Hammer/artist.jpg", [200, 30, 60, 255]);
    music.image("billy woods/Hiding Places/cover.jpg", [30, 200, 60, 255]);
    let mut app = App::new(Output::Silent, Store::memory());
    app.pictures.set_fetch(std::sync::Arc::new(move |url: &str| {
        asked.lock().unwrap().push(url.to_string());
        Err("offline".into())
    }));
    app.picture_lookups = true;
    app.set_relations(Relations { merges: Vec::new(), links: vec![Link { from: "billy woods".into(), kind: LinkKind::MemberOf, to: "Armand Hammer".into() }] });
    app.set_library(tracks, false);
    app.setup_with(std::sync::Arc::new(|| {}), Vec::new());
    let target = crate::pictures::Target { names: vec!["Kenny Segal".into()], mbid: None };
    app.pictures.import(target, crate::pictures::Input::Bytes(png(64, 64, [30, 60, 200, 255])));
    assert_eq!(app.pictures.finish(), (true, None));
    let mut rt = Runtime::new(app, view, (1200.0, 800.0), Env::new(true, 1.0));
    rt.poll = Some(crate::app::poll);
    settle(&mut rt);
    click(&mut rt, "graph-tab");
    rt
}

/// Zooms the camera to `zoom` about the canvas's middle.
fn zoom_to(rt: &mut Runtime<App>, zoom: f32) {
    rt.update(move |s, _| {
        let cam = s.graph.cam();
        let [w, h] = s.graph.viewport.expect("laid out");
        s.graph.zoom_about([w / 2.0, h / 2.0], zoom / cam.zoom);
        s.graph.drawn_region = s.graph.region().0;
    });
    settle(rt);
}

fn picture_key(rt: &Runtime<App>, name: &str) -> String {
    format!("pic-{}", rt.state.graph.model.find(name).expect("a node"))
}

#[test]
fn nodes_show_their_artists_picture_else_an_album_cover_else_their_dot() {
    let music = Music::new("show");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut rt = pictured_runtime(&music, asked.clone());
    zoom_to(&mut rt, 2.0);
    for name in ["Armand Hammer", "billy woods", "Kenny Segal"] {
        let key = picture_key(&rt, name);
        let el = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap_or_else(|| panic!("{name} shows a picture"));
        // Drawn over the dot, at its size, cropped round.
        let (x, y) = node_on_screen(&rt, name);
        let r = crate::graph::state::radius(rt.state.graph.model.nodes[rt.state.graph.model.find(name).unwrap()].degree) * rt.state.graph.cam().zoom;
        let [ex, ey, ew, eh] = el.rect;
        assert!((ex + ew / 2.0 - x).abs() < 1.0 && (ey + eh / 2.0 - y).abs() < 1.0, "{name}'s picture sits on its dot: {:?} at {x},{y}", el.rect);
        assert!((ew - 2.0 * r).abs() < 1.0 && (eh - ew).abs() < 0.5, "as wide as the dot: {ew} for radius {r}");
        // Its corner radius is in world points, under the camera's scale.
        assert!((el.radius * el.scale - ew / 2.0).abs() < 1.0, "round: radius {} × {} of {ew}", el.radius, el.scale);
    }
    assert!(!has(&rt, &picture_key(&rt, "Silka")), "no picture or cover: the plain dot");
    assert!(!has(&rt, &picture_key(&rt, "The Alchemist")));
    // Which source each shows: the folder's, the cover, the user's.
    let source = |rt: &Runtime<App>, name: &str| {
        let info = rt.state.picture_page(name);
        rt.state.graph_picture(&info).map(|i| i.and_then(|i| i.wait().ok()))
    };
    assert_eq!(source(&rt, "Armand Hammer"), Some(Some([64, 64])), "their artist.jpg");
    assert_eq!(source(&rt, "Kenny Segal"), Some(Some([64, 64])), "the user's picture");
    assert_eq!(source(&rt, "billy woods"), None, "no picture of their own: the album cover shows");
    // Opening the graph and drawing pictures never looks one up, with lookups on.
    assert!(asked.lock().unwrap().is_empty(), "the graph asked for {:?}", asked.lock().unwrap());
}

#[test]
fn pictures_show_only_for_nodes_drawn_big_enough_and_not_faded_by_a_pick() {
    let music = Music::new("zoom");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut rt = pictured_runtime(&music, asked);
    zoom_to(&mut rt, 0.2);
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("pic-"))), "zoomed out, dots only");
    zoom_to(&mut rt, 2.0);
    let woods = picture_key(&rt, "billy woods");
    assert!(has(&rt, &woods));
    // Picking Silka fades billy woods, who isn't linked to them: their picture goes.
    rt.update(|s, _| {
        s.graph.find = rune::TextEdit::new("silka");
        s.graph_found();
    });
    settle(&mut rt);
    zoom_to(&mut rt, 2.0);
    assert!(!has(&rt, &woods), "faded artists show their dot");
    assert!(has(&rt, &picture_key(&rt, "Armand Hammer")), "Silka's neighbours keep theirs");
}

#[test]
fn a_picture_set_on_the_artist_page_shows_in_the_graph() {
    let music = Music::new("set");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut rt = pictured_runtime(&music, asked);
    zoom_to(&mut rt, 2.0);
    let alchemist = picture_key(&rt, "The Alchemist");
    assert!(!has(&rt, &alchemist));
    rt.update(|s, _| {
        let target = crate::pictures::Target { names: vec!["The Alchemist".into()], mbid: None };
        s.pictures.import(target, crate::pictures::Input::Bytes(png(32, 32, [9, 9, 9, 255])));
        s.finish_pictures();
    });
    settle(&mut rt);
    assert!(has(&rt, &alchemist), "the memoized pictures rebuild when one is set");
}

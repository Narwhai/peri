use super::*;
use crate::credits::Credit;
use crate::relations::{Link, LinkKind, Relations};

fn song(artists: &[&str], featured: &[&str], credits: Vec<Credit>) -> Track {
    Track {
        artists: artists.iter().map(|s| s.to_string()).collect(),
        featured: featured.iter().map(|s| s.to_string()).collect(),
        credits,
        ..Track::default()
    }
}

fn edge(m: &Model, from: &str, to: &str, kind: EdgeKind) -> Option<u32> {
    let (a, b) = (m.find(from)? as u32, m.find(to)? as u32);
    m.edges.iter().find(|e| e.kind == kind && ((e.from == a && e.to == b) || (kind.undirected() && e.from == b && e.to == a))).map(|e| e.count)
}

#[test]
fn songs_link_main_artists_featured_guests_and_credits() {
    let tracks = vec![
        song(&["Armand Hammer", "The Alchemist", "Silka"], &["Silka"], vec![Credit::parse(Role::Producer, "The Alchemist"), Credit::parse(Role::Performer, "Cleo Reed (vocals)")]),
        song(&["armand hammer", "the alchemist"], &[], vec![Credit::parse(Role::Producer, "Kenny Segal")]),
    ];
    let m = Model::build(&tracks, &Relations::default());
    assert_eq!(m.nodes.len(), 5, "names match ignoring case: {:?}", m.nodes);
    assert_eq!(edge(&m, "The Alchemist", "Armand Hammer", EdgeKind::With), Some(2), "either order, counted per song");
    assert_eq!(edge(&m, "Silka", "Armand Hammer", EdgeKind::Featured), Some(1));
    assert_eq!(edge(&m, "Armand Hammer", "Silka", EdgeKind::Featured), None, "featuring reads one way");
    assert_eq!(edge(&m, "Silka", "The Alchemist", EdgeKind::With), None, "a guest isn't a main artist");
    assert_eq!(edge(&m, "Kenny Segal", "Armand Hammer", EdgeKind::Credit(Role::Producer)), Some(1), "produced for");
    assert_eq!(edge(&m, "Cleo Reed", "The Alchemist", EdgeKind::Credit(Role::Performer)), Some(1), "the instrument isn't part of the name");
    assert_eq!(edge(&m, "The Alchemist", "The Alchemist", EdgeKind::Credit(Role::Producer)), None, "producing their own song links nothing");
    assert_eq!(m.nodes[m.find("the alchemist").expect("node")].degree, 4);
}

#[test]
fn merges_and_links_become_edges_between_separate_nodes() {
    let relations = Relations {
        merges: vec![vec!["Earl Sweatshirt".into(), "Thebe Kgositsile".into(), "randomblackdude".into()]],
        links: vec![
            Link { from: "MF DOOM".into(), kind: LinkKind::Alias, to: "Viktor Vaughn".into() },
            Link { from: "Thom Yorke".into(), kind: LinkKind::MemberOf, to: "Radiohead".into() },
        ],
    };
    let m = Model::build(&[song(&["Earl Sweatshirt"], &[], Vec::new())], &relations);
    assert!(m.find("Thebe Kgositsile").is_some() && m.find("Earl Sweatshirt") != m.find("Thebe Kgositsile"), "merged names stay separate nodes");
    assert_eq!(edge(&m, "Earl Sweatshirt", "Thebe Kgositsile", EdgeKind::Merge), Some(1));
    assert_eq!(edge(&m, "randomblackdude", "Earl Sweatshirt", EdgeKind::Merge), Some(1));
    assert_eq!(edge(&m, "Viktor Vaughn", "MF DOOM", EdgeKind::Alias), Some(1), "names not in the library still show");
    assert_eq!(edge(&m, "Thom Yorke", "Radiohead", EdgeKind::MemberOf), Some(1));
    assert_eq!(edge(&m, "Radiohead", "Thom Yorke", EdgeKind::MemberOf), None, "membership reads one way");
}

#[test]
fn the_fingerprint_follows_names_and_links_not_order_or_counts() {
    let a = song(&["A", "B"], &[], Vec::new());
    let c = song(&["C"], &[], vec![Credit::parse(Role::Producer, "A")]);
    let base = Model::build(&[a.clone(), c.clone()], &Relations::default()).fingerprint();
    assert_eq!(Model::build(&[c.clone(), a.clone()], &Relations::default()).fingerprint(), base, "library order");
    assert_eq!(Model::build(&[a.clone(), c.clone(), a.clone()], &Relations::default()).fingerprint(), base, "another song by the same artists");
    assert_eq!(Model::build(&[song(&["a", "b"], &[], Vec::new()), c.clone()], &Relations::default()).fingerprint(), base, "case");
    let new_artist = song(&["D"], &[], Vec::new());
    assert_ne!(Model::build(&[a.clone(), c.clone(), new_artist], &Relations::default()).fingerprint(), base, "a new artist");
    let new_link = Relations { merges: vec![vec!["B".into(), "C".into()]], links: Vec::new() };
    assert_ne!(Model::build(&[a, c], &new_link).fingerprint(), base, "a new link");
}

#[test]
fn search_prefers_the_exact_name_then_prefixes_of_the_most_linked() {
    let tracks = vec![song(&["Madlib", "MF DOOM"], &[], Vec::new()), song(&["Mad Professor"], &[], Vec::new()), song(&["Madvillain"], &[], Vec::new())];
    let m = Model::build(&tracks, &Relations::default());
    let name = |q: &str| m.search(q).map(|i| m.nodes[i].name.clone());
    assert_eq!(name("madvillain").as_deref(), Some("Madvillain"));
    assert_eq!(name("mad").as_deref(), Some("Madlib"), "linked artists first");
    assert_eq!(name("doom").as_deref(), Some("MF DOOM"));
    assert_eq!(name("  "), None);
}

#[test]
fn kinds_round_trip_through_their_ids() {
    for k in EdgeKind::ALL {
        assert_eq!(EdgeKind::from_id(k.id()), Some(k));
    }
    let bits: u32 = EdgeKind::ALL.iter().fold(0, |b, k| b | k.bit());
    assert_eq!(bits.count_ones() as usize, EdgeKind::ALL.len(), "every kind has its own bit");
}

/// A made-up library of `artists` artists, each song by one or two of them with a few credits,
/// linked in clusters as real scenes are.
fn synthetic(artists: usize, songs: usize) -> Vec<Track> {
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = |n: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n as u64) as usize
    };
    let name = |i: usize| format!("Artist {i}");
    (0..songs)
        .map(|_| {
            let a = next(artists);
            // Collaborators come from nearby: clusters.
            let near = |next: &mut dyn FnMut(usize) -> usize| (a + next(200)) % artists;
            let mut main = vec![name(a)];
            if next(4) == 0 {
                main.push(name(near(&mut next)));
            }
            let featured: Vec<String> = if next(5) == 0 { vec![name(near(&mut next))] } else { Vec::new() };
            let credits = (0..next(3)).map(|k| Credit::parse(Role::ALL[k * 3 % Role::ALL.len()], &name(near(&mut next)))).collect();
            Track { artists: main.iter().chain(&featured).cloned().collect(), featured, credits, ..Track::default() }
        })
        .collect()
}

/// `cargo test --release graph_at_scale -- --ignored --nocapture`: how long building and laying
/// out a large library's graph takes.
#[test]
#[ignore]
fn graph_at_scale() {
    for (artists, songs) in [(2_000, 20_000), (10_000, 100_000), (30_000, 300_000)] {
        let tracks = synthetic(artists, songs);
        let t = std::time::Instant::now();
        let m = Model::build(&tracks, &Relations::default());
        let built = t.elapsed();
        let links = m.links();
        let start: Vec<[f32; 2]> = m.nodes.iter().map(|n| layout::seed(&n.name, m.nodes.len())).collect();
        let steps = layout::cold_steps(m.nodes.len());
        let t = std::time::Instant::now();
        let pos = layout::run(m.nodes.len(), &links, start, &vec![false; m.nodes.len()], steps, &std::sync::atomic::AtomicBool::new(false), |_, _| {});
        let laid = t.elapsed();
        assert!(pos.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        println!("{} artists, {} edges ({} pairs): built in {built:.2?}, {steps} steps in {laid:.2?} ({:.2?}/step)", m.nodes.len(), m.edges.len(), links.len(), laid / steps as u32);
    }
}

#[test]
fn a_link_reads_as_a_sentence_in_its_direction() {
    assert_eq!(EdgeKind::Credit(Role::Producer).sentence("Kenny Segal", "billy woods", 2), "Kenny Segal produced 2 songs by billy woods");
    assert_eq!(EdgeKind::Credit(Role::Performer).sentence("Cleo Reed", "Armand Hammer", 1), "Cleo Reed performed on 1 song by Armand Hammer");
    assert_eq!(EdgeKind::Featured.sentence("Silka", "Armand Hammer", 1), "Silka is featured on 1 song by Armand Hammer");
    assert_eq!(EdgeKind::With.sentence("A", "B", 3), "A and B made 3 songs together");
    assert_eq!(EdgeKind::MemberOf.sentence("Thom Yorke", "Radiohead", 1), "Thom Yorke is a member of Radiohead");
    assert_eq!(EdgeKind::Alias.sentence("MF DOOM", "Viktor Vaughn", 1), "MF DOOM also releases as Viktor Vaughn");
}

/// How unevenly a settling artist moves from frame to frame at `hz`, its frames arriving up to
/// `jitter` of an interval early or late: the worst change in speed between two frames, as a
/// share of the speed it settles from. Smooth motion changes speed a little each frame.
fn settling_jerk(hz: f64, jitter: f64) -> f32 {
    use crate::graph::state::Graph;
    let tracks = vec![song(&["Hub"], &[], vec![Credit::parse(Role::Producer, "A"), Credit::parse(Role::Producer, "B")]), song(&["A"], &[], Vec::new())];
    let mut g = Graph::with(None, 0);
    g.refresh(Model::build(&tracks, &Relations::default()), None, None).expect("laid out");
    let (hub, a) = (g.model.find("Hub").expect("hub"), g.model.find("A").expect("a"));
    let start = g.world[hub];
    g.start_drag(hub, start);
    let mut now = 0.0;
    let interval = 1.0 / hz;
    // Frames that come early and late in turn, as a busy machine's do.
    let wobble = |k: usize| interval * (1.0 + jitter * [1.0, -1.0, 0.3][k % 3]);
    for k in 0..(0.2 * hz) as usize {
        now += wobble(k);
        g.drag_to([start[0] + 200.0 * (now / 0.2) as f32, start[1]]);
        g.tick(now).expect("steps");
    }
    g.drop_drag([start[0] + 200.0, start[1]]);
    let (mut last, mut speeds) = (g.world[a], Vec::new());
    for k in 0..(1.5 * hz) as usize {
        let dt = wobble(k + 1);
        now += dt;
        if g.drag.is_none() {
            break;
        }
        g.tick(now).expect("steps");
        let p = g.world[a];
        speeds.push((((p[0] - last[0]).powi(2) + (p[1] - last[1]).powi(2)).sqrt() / dt as f32, dt));
        last = p;
    }
    let top = speeds.iter().map(|s| s.0).fold(0.0_f32, f32::max).max(1e-3);
    speeds.windows(2).map(|w| (w[1].0 - w[0].0).abs() / top).fold(0.0, f32::max)
}

#[test]
fn settling_moves_evenly_whatever_the_frame_rate() {
    for (hz, jitter) in [(60.0, 0.0), (60.0, 0.25), (144.0, 0.0), (144.0, 0.25), (90.0, 0.1)] {
        let jerk = settling_jerk(hz, jitter);
        println!("{hz} Hz, ±{:.0}% timing: worst speed change between frames {:.0}% of top speed", jitter * 100.0, jerk * 100.0);
        assert!(jerk < 0.15, "{hz} Hz ±{jitter}: a frame's speed jumps by {:.0}% of the top speed", jerk * 100.0);
    }
}

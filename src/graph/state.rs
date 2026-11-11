//! The graph page's state: the model, its layout (run on a worker thread, cached on disk), the
//! camera, and what's hovered or picked.
//!
//! **The cache:** `graph-layout.json` beside the preferences keeps the last layout's positions by
//! artist name, with the fingerprint of the graph they were laid out for ([`Model::fingerprint`]:
//! the names and which pairs are linked). When the graph still has that fingerprint the layout is
//! used as is. When it changed (music added, an artist linked), the known artists start where
//! they were, new ones next to the artists they're linked to, and a shorter layout settles them.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use rune::{Live, PointerPress, TextEdit, Tween, Waker};
use serde::{Deserialize, Serialize};

use super::layout;
use super::{EdgeKind, Model};

pub const CACHE_FILE: &str = "graph-layout.json";
pub const ZOOM_MIN: f32 = 0.02;
pub const ZOOM_MAX: f32 = 6.0;
/// How long after the camera last moved it's saved.
pub const SAVE_AFTER: Duration = Duration::from_millis(600);
/// How often a running layout shows its progress.
const PROGRESS_EVERY: Duration = Duration::from_millis(80);
/// The hit-test grid's cell, in world points.
const CELL: f32 = 64.0;
/// The link hit-test grid's cell, in world points.
const EDGE_CELL: f32 = 96.0;
/// How near, in points on screen, the pointer has to be to a link to hover it.
const EDGE_REACH: f32 = 5.0;
/// Dots whose edges are within this many points on screen of the pointer's nearest are too
/// close to tell apart: the most linked is picked.
const PICK_TIE: f32 = 3.0;
/// At most this many artists move with a dragged one: those within [`DRAG_HOPS`] links of it,
/// nearest first. The rest hold still, so a drag costs the same in any library.
const DRAG_MOVING: usize = 400;
const DRAG_HOPS: usize = 2;
/// The springs along links: stiffness (per second squared, per point the link is bent out of
/// its shape), and damping (per second).
const SPRING: f32 = 80.0;
const DAMPING: f32 = 12.0;
/// The simulation's step, in seconds; frames run as many as they need.
const SIM_STEP: f32 = 1.0 / 120.0;
/// After the drop, the artists have settled once none moves faster than this (points per
/// second), or after [`SETTLE_MAX`] seconds.
const SETTLED_SPEED: f32 = 3.0;
const SETTLE_MAX: f64 = 4.0;
/// A running simulation asks for the next frame. (On a surface no display link paces, Rune
/// takes this as about a frame at 60 Hz; [`Drag::pacer`] keeps it at the display's rate.)
pub const SIM_FRAME: Duration = Duration::ZERO;
/// How long a drag's pacer runs at most: far longer than anyone holds a drag.
const PACER_SPAN: Duration = Duration::from_secs(3600);

/// What the pointer is over on the canvas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Hover {
    Node(usize),
    /// The links drawn between artists `a` and `b`: every kind shown between them, by index
    /// into the model's edges, in the legend's order.
    Link { a: usize, b: usize, edges: Vec<usize> },
}

/// A node's radius in world points, from how many artists it's linked to.
pub fn radius(degree: u32) -> f32 {
    (3.0 + (degree as f32).sqrt() * 1.6).min(20.0)
}

/// Where the canvas looks: drawn = centre + (world − centre) · zoom + pan, with `centre` the
/// middle of the world element (the live scale turns about it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

/// What `prefs.json` keeps under `graph`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    /// `None` until the canvas is panned or zoomed: the drawing fits the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<Camera>,
    /// The link kinds not drawn, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
}

/// The cache file's shape.
#[derive(Serialize, Deserialize)]
struct Cached {
    /// [`Model::fingerprint`] in hex.
    fingerprint: String,
    /// Lowercased names, one per position.
    names: Vec<String>,
    /// Layout positions, before [`layout::Transform::world`].
    positions: Vec<[f32; 2]>,
    /// The artists dragged into place, lowercased: layouts leave them there. Caches written
    /// before pinning have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pinned: Vec<String>,
}

enum Msg {
    Progress { positions: Vec<[f32; 2]>, step: usize, steps: usize },
    /// The positions, and the artists pinned in them (lowercased).
    Done(Vec<[f32; 2]>, HashSet<String>),
    Error(String),
}

/// An artist being dragged, and those moving with it on springs.
pub struct Drag {
    /// The dragged artist.
    pub node: usize,
    /// Which artists move, by node: their index into `slots`, else `NO_SLOT`.
    slot: Vec<u32>,
    /// The moving artists, the dragged one first.
    pub moving: Vec<u32>,
    /// Their speeds, in world points per second.
    speed: Vec<[f32; 2]>,
    /// Where the simulation has them, after its last step and the one before: what's drawn
    /// lies between the two, by how far the clock has got into the next step. Steps are always
    /// [`SIM_STEP`] long, so the motion is the same however unevenly frames come.
    sim: Vec<[f32; 2]>,
    prev: Vec<[f32; 2]>,
    /// Time the clock has moved that no step has taken yet, in seconds.
    carry: f32,
    /// The fastest artist's speed after the last step.
    fastest: f32,
    /// Where each started, to put back if the drag is cancelled.
    origin: Vec<[f32; 2]>,
    /// The links touching a moving artist: both ends, and the offset from one to the other they
    /// rest at (as drawn when the drag began). Springs that keep direction as well as length
    /// carry a cluster along in its shape, where length alone would let it swing and fold.
    springs: Vec<(u32, u32, [f32; 2])>,
    /// How many springs pull each moving artist, which it divides their pull by.
    weight: Vec<f32>,
    /// The model's edges touching a moving artist, which draw with them.
    pub edges: Vec<u32>,
    /// Where the pointer holds the dragged artist, in world points, while it's held.
    target: Option<[f32; 2]>,
    /// The pointer's offset from the artist's centre where it grabbed it.
    grab: [f32; 2],
    /// The clock when the simulation last stepped, and when the artist was dropped.
    last: Option<f64>,
    dropped: Option<f64>,
    /// Nothing moves (the pointer is still, or everything settled): no frames needed.
    asleep: bool,
    /// Bumped every step, for drawing what moves.
    pub tick: u64,
    /// Bumped per drag, for drawing what doesn't.
    pub id: u64,
    /// A track that's always running, bound to an invisible element while the simulation
    /// moves. Rune draws every display frame while a live track runs, and polls (stepping the
    /// simulation) each frame it draws, so this paces the simulation at the display's rate,
    /// 144 Hz on a 144 Hz display, where timer wakes alone are held to 60 Hz on Linux and
    /// Windows. Removing the element stops it.
    pub pacer: Tween<f32>,
}

const NO_SLOT: u32 = u32::MAX;

struct Job {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct Graph {
    pub model: Model,
    /// The library or relations changed since `model` was built.
    stale: bool,
    /// Each node's place in the layout's own units, kept for warm starts.
    raw: Vec<[f32; 2]>,
    /// Each node's place, in world points; empty until a layout arrives.
    pub world: Vec<[f32; 2]>,
    /// The world's size in points.
    pub size: [f32; 2],
    /// Bumped whenever `model` or `world` change, for memoized drawing.
    pub version: u64,
    grid: HashMap<(i32, i32), Vec<u32>>,
    job: Option<Job>,
    /// While a layout runs: (steps done, steps).
    pub progress: Option<(usize, usize)>,
    /// `None` fits the drawing to the canvas.
    pub camera: Option<Camera>,
    pub live_pan: Live<[f32; 2]>,
    pub live_zoom: Live<f32>,
    /// The canvas's size, as last laid out.
    pub viewport: Option<[f32; 2]>,
    /// What the pointer is over, and where it was (canvas points) when that changed.
    pub hover: Option<Hover>,
    pub hover_at: [f32; 2],
    /// Each cell's links, for hovering them; built on first use after a layout.
    edge_grid: Option<HashMap<(i32, i32), Vec<u32>>>,
    /// The artist the find field picked.
    pub selected: Option<usize>,
    /// [`EdgeKind::bit`]s of the kinds not drawn.
    pub hidden: u32,
    pub find: TextEdit,
    /// The camera when a drag started.
    pub drag_from: Camera,
    /// When the camera last moved, unsaved.
    pub moved_at: Option<Instant>,
    /// The region the view last drew names for.
    pub drawn_region: Region,
    /// How layout positions map to the world, from the last layout.
    transform: layout::Transform,
    /// Where the artists are drawn, with room around: what Fit shows. (Dragged artists can
    /// leave the world's laid-out size.)
    bounds: [f32; 4],
    /// The artists pinned where they were dropped, lowercased; `None` until the first layout
    /// reads them from the cache.
    pins: Option<HashSet<String>>,
    /// Whether each node is pinned, from `pins`.
    pub pinned: Vec<bool>,
    /// The layout cache's file, when it has one.
    cache: Option<PathBuf>,
    /// A press on an artist that may become a drag.
    pub press: Option<usize>,
    pub drag: Option<Drag>,
    drags: u64,
    /// The artist whose menu is open, and where (in window points).
    pub menu: Option<(usize, PointerPress)>,
}

impl Graph {
    pub fn new(saved: Saved) -> Result<Graph, String> {
        let mut hidden = 0;
        for id in &saved.hidden {
            match EdgeKind::from_id(id) {
                Some(kind) => hidden |= kind.bit(),
                None => return Err(format!("Couldn't read \"graph\" in the preferences: no link kind \"{id}\"")),
            }
        }
        Ok(Graph::with(saved.camera, hidden))
    }

    pub fn with(camera: Option<Camera>, hidden: u32) -> Graph {
        let start = camera.unwrap_or(Camera { pan: [0.0, 0.0], zoom: 1.0 });
        Graph {
            model: Model::default(),
            stale: true,
            raw: Vec::new(),
            world: Vec::new(),
            size: [0.0, 0.0],
            version: 0,
            grid: HashMap::new(),
            job: None,
            progress: None,
            camera,
            live_pan: Live::new(start.pan),
            live_zoom: Live::new(start.zoom),
            viewport: None,
            hover: None,
            hover_at: [0.0, 0.0],
            edge_grid: None,
            selected: None,
            hidden,
            find: TextEdit::new(""),
            drag_from: start,
            moved_at: None,
            drawn_region: Region::default(),
            transform: layout::Transform { scale: 1.0, origin: [0.0, 0.0], size: [0.0, 0.0] },
            bounds: [0.0, 0.0, 0.0, 0.0],
            pins: None,
            pinned: Vec::new(),
            cache: None,
            press: None,
            drag: None,
            drags: 0,
            menu: None,
        }
    }

    pub fn saved(&self) -> Saved {
        Saved { camera: self.camera, hidden: EdgeKind::ALL.into_iter().filter(|k| self.hidden & k.bit() != 0).map(|k| k.id().to_string()).collect() }
    }

    /// The library or the relations changed: the graph is rebuilt next time it shows.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Takes a freshly built `model`, keeping what's picked by name, and lays it out: from the
    /// cache at `cache` when it's still right, else on a worker thread (`waker` set) or here.
    /// Pinned artists stay where they are.
    pub fn refresh(&mut self, model: Model, cache: Option<PathBuf>, waker: Option<Waker>) -> Result<(), String> {
        self.relayout(model, cache, waker, false)
    }

    /// Reset layout: unpins every artist and lays the graph out from scratch.
    pub fn reset_layout(&mut self, cache: Option<PathBuf>, waker: Option<Waker>) -> Result<(), String> {
        let model = std::mem::take(&mut self.model);
        self.pins = Some(HashSet::new());
        self.camera = None;
        self.moved_at = Some(Instant::now());
        self.relayout(model, cache, waker, true)
    }

    fn relayout(&mut self, model: Model, cache: Option<PathBuf>, waker: Option<Waker>, cold: bool) -> Result<(), String> {
        self.cancel_drag();
        let selected = self.selected.and_then(|i| self.model.nodes.get(i)).map(|n| n.name.clone());
        let previous: HashMap<String, [f32; 2]> = if cold { HashMap::new() } else { self.model.nodes.iter().zip(&self.raw).map(|(n, p)| (n.name.to_lowercase(), *p)).collect() };
        self.stale = false;
        self.cache = cache.clone();
        self.menu = None;
        self.press = None;
        self.model = model;
        self.selected = selected.and_then(|n| self.model.find(&n));
        self.hover = None;
        self.edge_grid = None;
        self.job = None;
        self.progress = None;
        let names: Vec<String> = self.model.nodes.iter().map(|n| n.name.to_lowercase()).collect();
        let links = self.model.links();
        let fingerprint = self.model.fingerprint();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let start = Start { names, links, fingerprint, cache, previous, pins: self.pins.clone(), cold };
        self.raw.clear();
        self.world.clear();
        self.grid.clear();
        self.version += 1;
        match waker {
            Some(waker) => {
                let stop = cancel.clone();
                std::thread::Builder::new()
                    .name("graph-layout".into())
                    .spawn(move || compute(start, &stop, &tx, &|| waker()))
                    .map_err(|e| format!("Couldn't start the graph layout: {e}"))?;
            }
            None => compute(start, &cancel, &tx, &|| {}),
        }
        self.job = Some(Job { rx, cancel });
        match self.drain() {
            (_, Some(e)) => Err(e),
            (_, None) => Ok(()),
        }
    }

    /// Takes the worker's news; true when what shows changed. Errors come back to show.
    pub fn drain(&mut self) -> (bool, Option<String>) {
        let Some(job) = &self.job else { return (false, None) };
        let mut latest = None;
        let mut done = false;
        let mut error = None;
        for msg in job.rx.try_iter() {
            match msg {
                Msg::Progress { positions, step, steps } => {
                    latest = Some(positions);
                    self.progress = Some((step, steps));
                }
                Msg::Done(positions, pins) => {
                    latest = Some(positions);
                    self.pins = Some(pins);
                    done = true;
                }
                Msg::Error(e) => error = Some(e),
            }
        }
        if done {
            self.job = None;
            self.progress = None;
        }
        let changed = latest.is_some() || done;
        if let Some(raw) = latest {
            self.place(&raw);
        }
        (changed, error)
    }

    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    fn place(&mut self, raw: &[[f32; 2]]) {
        if raw.len() != self.model.nodes.len() {
            return;
        }
        self.transform = layout::Transform::fit(raw, &self.model.links());
        self.raw = raw.to_vec();
        self.world = raw.iter().map(|&p| self.transform.world(p)).collect();
        self.size = self.transform.size;
        self.pinned = self.model.nodes.iter().map(|n| self.pins.as_ref().is_some_and(|p| p.contains(&n.name.to_lowercase()))).collect();
        self.placed();
        self.sync_live();
    }

    /// The artists moved (a layout, or a drop): what finds them and what Fit shows follow.
    fn placed(&mut self) {
        self.grid.clear();
        self.edge_grid = None;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (i, &[x, y]) in self.world.iter().enumerate() {
            self.grid.entry(cell(x, y)).or_default().push(i as u32);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        let m = layout::MARGIN;
        self.bounds = if self.world.is_empty() { [0.0, 0.0, self.size[0], self.size[1]] } else { [x0 - m, y0 - m, x1 + m, y1 + m] };
        self.version += 1;
    }

    /// Where artist `name` sits in the layout's own units, which the cache keeps.
    #[cfg(test)]
    pub fn layout_raw(&self, name: &str) -> Option<[f32; 2]> {
        self.model.find(name).and_then(|i| self.raw.get(i).copied())
    }

    // Dragging

    /// Starts dragging artist `i`, grabbed at world point `at`: it and the artists within
    /// [`DRAG_HOPS`] links of it (but not other pinned ones, which hold still) move with it.
    pub fn start_drag(&mut self, i: usize, at: [f32; 2]) {
        let n = self.model.nodes.len();
        if i >= n || self.world.len() != n {
            return;
        }
        self.cancel_drag();
        self.hover = None;
        self.menu = None;
        // A camera fitting the drawing would refit when the drop changes its bounds, moving
        // every artist out from under the pointer: hold it where it is. (Not saved: the user
        // didn't move it.)
        if self.camera.is_none() {
            self.camera = Some(self.fit());
        }
        let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (a, b) in self.model.links() {
            neighbours[a as usize].push(b);
            neighbours[b as usize].push(a);
        }
        let mut slot = vec![NO_SLOT; n];
        let mut moving = vec![i as u32];
        slot[i] = 0;
        let mut queue = VecDeque::from([(i as u32, 0)]);
        'search: while let Some((at, hops)) = queue.pop_front() {
            if hops >= DRAG_HOPS {
                continue;
            }
            for &j in &neighbours[at as usize] {
                if slot[j as usize] != NO_SLOT || self.pinned.get(j as usize).copied().unwrap_or(false) {
                    continue;
                }
                if moving.len() >= DRAG_MOVING {
                    break 'search;
                }
                slot[j as usize] = moving.len() as u32;
                moving.push(j);
                queue.push_back((j, hops + 1));
            }
        }
        let springs: Vec<(u32, u32, [f32; 2])> = self
            .model
            .links()
            .into_iter()
            .filter(|&(a, b)| slot[a as usize] != NO_SLOT || slot[b as usize] != NO_SLOT)
            .map(|(a, b)| {
                let (pa, pb) = (self.world[a as usize], self.world[b as usize]);
                (a, b, [pb[0] - pa[0], pb[1] - pa[1]])
            })
            .collect();
        let mut weight = vec![0.0_f32; moving.len()];
        for &(a, b, _) in &springs {
            for end in [a, b] {
                if let Some(w) = weight.get_mut(slot[end as usize] as usize) {
                    *w += 1.0;
                }
            }
        }
        let edges = (0..self.model.edges.len() as u32).filter(|&e| {
            let edge = &self.model.edges[e as usize];
            slot[edge.from as usize] != NO_SLOT || slot[edge.to as usize] != NO_SLOT
        }).collect();
        let origin: Vec<[f32; 2]> = moving.iter().map(|&j| self.world[j as usize]).collect();
        let p = self.world[i];
        self.drags += 1;
        self.drag = Some(Drag {
            node: i,
            speed: vec![[0.0, 0.0]; moving.len()],
            sim: origin.clone(),
            prev: origin.clone(),
            carry: 0.0,
            fastest: 0.0,
            weight: weight.into_iter().map(|w| w.max(1.0)).collect(),
            origin,
            slot,
            moving,
            springs,
            edges,
            target: Some(p),
            grab: [p[0] - at[0], p[1] - at[1]],
            last: None,
            dropped: None,
            asleep: false,
            tick: 0,
            id: self.drags,
            pacer: Tween::linear(0.0, 0.0, PACER_SPAN),
        });
    }

    /// Whether the pointer is holding an artist: a drag that hasn't been let go. Once dropped,
    /// a drag only settles, and no pointer moves it.
    pub fn holding(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.target.is_some())
    }

    /// The pointer, holding the dragged artist, moved to world point `at`. Nothing, once
    /// dropped.
    pub fn drag_to(&mut self, at: [f32; 2]) {
        if let Some(d) = self.drag.as_mut().filter(|d| d.target.is_some()) {
            d.target = Some([at[0] + d.grab[0], at[1] + d.grab[1]]);
            d.asleep = false;
        }
    }

    /// A new press: a dropped drag still settling ends now, pinned where it was dropped and
    /// its neighbours where they've got to, so the press starts from a still graph.
    pub fn finish_drag(&mut self) -> Result<(), String> {
        if self.drag.as_ref().is_some_and(|d| d.target.is_none()) {
            return self.settle();
        }
        Ok(())
    }

    /// The pointer let go at world point `at`: the artist stays there, pinned, while the others
    /// settle around it. (The simulation may not have stepped since the pointer last moved, so
    /// the artist is put under it here.)
    pub fn drop_drag(&mut self, at: [f32; 2]) {
        self.drag_to(at);
        if let Some(d) = self.drag.as_mut().filter(|d| d.target.is_some()) {
            if let (Some(t), Some(p)) = (d.target, self.world.get_mut(d.node)) {
                *p = t;
                d.sim[0] = t;
                d.prev[0] = t;
            }
            d.target = None;
            d.asleep = false;
        }
    }

    /// Puts every moving artist back where the drag found it.
    pub fn cancel_drag(&mut self) {
        let Some(d) = self.drag.take() else { return };
        for (k, &j) in d.moving.iter().enumerate() {
            if let Some(p) = self.world.get_mut(j as usize) {
                *p = d.origin[k];
            }
        }
        self.version += 1;
    }

    /// Whether a drag needs frames: something is moving.
    pub fn animating(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| !d.asleep)
    }

    /// Steps the simulation to clock `now` (seconds); once the dropped artists settle, pins the
    /// dragged one and saves. True when anything moved.
    ///
    /// The simulation runs in fixed steps of [`SIM_STEP`], however long frames take, carrying
    /// over the time a frame leaves; what's drawn is interpolated between the last two steps.
    /// Stepping a frame's time rounded to whole steps instead makes motion speed up and slow
    /// down with the frame times, which shows as judder when frames come unevenly.
    pub fn tick(&mut self, now: f64) -> Result<bool, String> {
        let Some(d) = &mut self.drag else { return Ok(false) };
        if d.asleep {
            return Ok(false);
        }
        // Never so much time that a stall replays a second of motion in one frame.
        let elapsed = d.last.map_or(SIM_STEP, |last| ((now - last) as f32).clamp(0.0, 0.1));
        d.last = Some(now);
        if d.target.is_none() && d.dropped.is_none() {
            d.dropped = Some(now);
        }
        d.carry += elapsed;
        let damp = (-DAMPING * SIM_STEP).exp();
        let mut force = vec![[0.0_f32; 2]; d.moving.len()];
        while d.carry >= SIM_STEP {
            d.carry -= SIM_STEP;
            d.prev.clone_from(&d.sim);
            if let Some(t) = d.target {
                d.sim[0] = t;
            }
            force.iter_mut().for_each(|f| *f = [0.0, 0.0]);
            let at = |i: u32, sim: &[[f32; 2]]| match d.slot[i as usize] {
                NO_SLOT => self.world[i as usize],
                k => sim[k as usize],
            };
            for &(a, b, rest) in &d.springs {
                let (pa, pb) = (at(a, &d.sim), at(b, &d.sim));
                // How far the link is bent from its resting offset, pulling both ends back.
                let bend = [SPRING * (pb[0] - pa[0] - rest[0]), SPRING * (pb[1] - pa[1] - rest[1])];
                if let Some(f) = force.get_mut(d.slot[a as usize] as usize) {
                    f[0] += bend[0];
                    f[1] += bend[1];
                }
                if let Some(f) = force.get_mut(d.slot[b as usize] as usize) {
                    f[0] -= bend[0];
                    f[1] -= bend[1];
                }
            }
            d.fastest = 0.0;
            // The dragged artist (slot 0) goes where the pointer is, then stays where dropped.
            for (k, ((p, v), f)) in d.sim.iter_mut().zip(d.speed.iter_mut()).zip(&force).enumerate().skip(1) {
                v[0] = (v[0] + f[0] / d.weight[k] * SIM_STEP) * damp;
                v[1] = (v[1] + f[1] / d.weight[k] * SIM_STEP) * damp;
                p[0] += v[0] * SIM_STEP;
                p[1] += v[1] * SIM_STEP;
                d.fastest = d.fastest.max((v[0] * v[0] + v[1] * v[1]).sqrt());
            }
        }
        // Drawn between the last two steps, by how far into the next the clock is. The held
        // artist is drawn under the pointer, without the step's lag.
        let t = d.carry / SIM_STEP;
        for (k, &j) in d.moving.iter().enumerate() {
            let (a, b) = (d.prev[k], d.sim[k]);
            self.world[j as usize] = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        }
        if let Some(target) = d.target {
            self.world[d.node] = target;
        }
        d.tick += 1;
        if d.fastest < SETTLED_SPEED {
            match d.dropped {
                Some(_) => return self.settle().map(|()| true),
                // Held still: sleep until the pointer moves again.
                None => d.asleep = d.target.is_some_and(|t| d.sim[0] == t),
            }
        } else if d.dropped.is_some_and(|at| now - at > SETTLE_MAX) {
            return self.settle().map(|()| true);
        }
        Ok(true)
    }

    /// Ends the drag: the dragged artist is pinned where it was dropped, everything that
    /// moved keeps its place, and the layout is saved.
    fn settle(&mut self) -> Result<(), String> {
        let Some(d) = self.drag.take() else { return Ok(()) };
        // Where the simulation stopped, not between its last two steps.
        for (k, &j) in d.moving.iter().enumerate() {
            let j = j as usize;
            self.world[j] = d.sim[k];
            self.raw[j] = self.transform.raw(self.world[j]);
        }
        let name = self.model.nodes[d.node].name.to_lowercase();
        self.pins.get_or_insert_with(HashSet::new).insert(name);
        self.pinned[d.node] = true;
        self.placed();
        self.save()
    }

    /// Unpins artist `i`: the next layout may move it.
    pub fn unpin(&mut self, i: usize) -> Result<(), String> {
        self.menu = None;
        let Some(node) = self.model.nodes.get(i) else { return Ok(()) };
        if let Some(pins) = &mut self.pins {
            pins.remove(&node.name.to_lowercase());
        }
        if let Some(p) = self.pinned.get_mut(i) {
            *p = false;
        }
        self.version += 1;
        self.save()
    }

    /// Writes the layout as it shows, with its pins, to the cache.
    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.cache else { return Ok(()) };
        let names: Vec<String> = self.model.nodes.iter().map(|n| n.name.to_lowercase()).collect();
        let mut pinned: Vec<String> = names.iter().zip(&self.pinned).filter(|(_, p)| **p).map(|(n, _)| n.clone()).collect();
        pinned.sort_unstable();
        let cached = Cached { fingerprint: format!("{:016x}", self.model.fingerprint()), names, positions: self.raw.clone(), pinned };
        write(path, &cached).map_err(|e| format!("Couldn't save the graph layout to {}: {e}", path.display()))
    }

    /// The camera in use: the user's, else the one fitting the drawing to the canvas.
    pub fn cam(&self) -> Camera {
        self.camera.unwrap_or_else(|| self.fit())
    }

    /// The camera showing the whole drawing, centred.
    pub fn fit(&self) -> Camera {
        let Some([w, h]) = self.viewport else { return Camera { pan: [0.0, 0.0], zoom: 1.0 } };
        let [x0, y0, x1, y1] = self.bounds;
        let (sw, sh) = (x1 - x0, y1 - y0);
        if sw <= 0.0 || sh <= 0.0 {
            return Camera { pan: [0.0, 0.0], zoom: 1.0 };
        }
        let zoom = (w / sw).min(h / sh).clamp(ZOOM_MIN, 1.5);
        let c = self.centre();
        let m = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        Camera { pan: [w / 2.0 - c[0] - (m[0] - c[0]) * zoom, h / 2.0 - c[1] - (m[1] - c[1]) * zoom], zoom }
    }

    /// The world element's middle, which the live scale turns about.
    pub fn centre(&self) -> [f32; 2] {
        [self.size[0] / 2.0, self.size[1] / 2.0]
    }

    /// Where world point `p` shows on the canvas under `cam`.
    pub fn screen(&self, cam: Camera, p: [f32; 2]) -> [f32; 2] {
        let c = self.centre();
        [c[0] + (p[0] - c[0]) * cam.zoom + cam.pan[0], c[1] + (p[1] - c[1]) * cam.zoom + cam.pan[1]]
    }

    /// The world point under canvas point `q`.
    pub fn world_at(&self, cam: Camera, q: [f32; 2]) -> [f32; 2] {
        let c = self.centre();
        [c[0] + (q[0] - c[0] - cam.pan[0]) / cam.zoom, c[1] + (q[1] - c[1] - cam.pan[1]) / cam.zoom]
    }

    /// Moves the camera (the user did), noting it for saving.
    pub fn set_camera(&mut self, cam: Camera) {
        let zoom = cam.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        if !zoom.is_finite() || !cam.pan[0].is_finite() || !cam.pan[1].is_finite() {
            return;
        }
        self.camera = Some(Camera { pan: cam.pan, zoom });
        self.moved_at = Some(Instant::now());
        self.sync_live();
    }

    /// Zooms by `factor` keeping canvas point `q` where it is.
    pub fn zoom_about(&mut self, q: [f32; 2], factor: f32) {
        let cam = self.cam();
        let p = self.world_at(cam, q);
        let zoom = (cam.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let c = self.centre();
        self.set_camera(Camera { pan: [q[0] - c[0] - (p[0] - c[0]) * zoom, q[1] - c[1] - (p[1] - c[1]) * zoom], zoom });
    }

    /// Centres node `i`, zooming in to at least `min_zoom`.
    pub fn centre_on(&mut self, i: usize, min_zoom: f32) {
        let (Some(&p), Some([w, h])) = (self.world.get(i), self.viewport) else { return };
        let zoom = self.cam().zoom.max(min_zoom);
        let c = self.centre();
        self.set_camera(Camera { pan: [w / 2.0 - c[0] - (p[0] - c[0]) * zoom, h / 2.0 - c[1] - (p[1] - c[1]) * zoom], zoom });
    }

    /// Back to fitting the drawing.
    pub fn reset_camera(&mut self) {
        self.camera = None;
        self.moved_at = Some(Instant::now());
        self.sync_live();
    }

    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.viewport = Some(size);
        self.sync_live();
    }

    fn sync_live(&self) {
        let cam = self.cam();
        self.live_pan.set(cam.pan);
        self.live_zoom.set(cam.zoom);
    }

    /// The node under canvas point `q`, if any: the dot (plus a few points of slack) nearest it,
    /// or the most linked of those too close together on screen to tell apart.
    pub fn node_at(&self, q: [f32; 2]) -> Option<usize> {
        let cam = self.cam();
        let [x, y] = self.world_at(cam, q);
        let slack = 4.0 / cam.zoom;
        // Zoomed out, a dot draws at least a point wide, which can be wider than its radius.
        let widest = 20.0_f32.max(1.0 / cam.zoom);
        let reach = ((widest + slack) / CELL).ceil() as i32;
        let (cx, cy) = cell(x, y);
        // Every dot whose edge is within the slack, by how far outside its edge the point is
        // (negative inside it).
        let mut near: Vec<(f32, usize)> = Vec::new();
        let mut consider = |i: usize| {
            let [px, py] = self.world[i];
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            let r = radius(self.model.nodes[i].degree).max(1.0 / cam.zoom);
            if d <= r + slack {
                near.push((d - r, i));
            }
        };
        // The grid files artists where the last layout or drop left them: those a drag is
        // moving are looked at where they are now instead.
        let moving = |i: u32| self.drag.as_ref().is_some_and(|d| d.slot.get(i as usize).is_some_and(|s| *s != NO_SLOT));
        for gx in cx - reach..=cx + reach {
            for gy in cy - reach..=cy + reach {
                for &i in self.grid.get(&(gx, gy)).map_or(&[][..], Vec::as_slice) {
                    if !moving(i) {
                        consider(i as usize);
                    }
                }
            }
        }
        if let Some(d) = &self.drag {
            d.moving.iter().for_each(|&i| consider(i as usize));
        }
        // Dots this close on screen can't be told apart, as when a hub's neighbours crowd its
        // dot zoomed out: the most linked of them wins, else the nearest.
        let closest = near.iter().map(|&(off, _)| off).fold(f32::MAX, f32::min);
        let tie = PICK_TIE / cam.zoom;
        near.iter()
            .filter(|&&(off, _)| off <= closest + tie)
            .max_by(|a, b| self.model.nodes[a.1].degree.cmp(&self.model.nodes[b.1].degree).then(b.0.total_cmp(&a.0)))
            .map(|&(_, i)| i)
    }

    /// What's under canvas point `q`: an artist's dot, else the links nearest it.
    pub fn hit(&mut self, q: [f32; 2]) -> Option<Hover> {
        match self.node_at(q) {
            Some(i) => Some(Hover::Node(i)),
            None => self.link_at(q),
        }
    }

    /// The artist hovered, if it's an artist.
    pub fn hovered_node(&self) -> Option<usize> {
        match self.hover {
            Some(Hover::Node(i)) => Some(i),
            Some(Hover::Link { .. }) | None => None,
        }
    }

    /// Whether `edge` draws as a line one can see: its kind isn't hidden, and with an artist
    /// picked, it touches them (the rest draw faded).
    fn shown(&self, edge: &super::Edge) -> bool {
        self.hidden & edge.kind.bit() == 0 && self.selected.is_none_or(|f| edge.from as usize == f || edge.to as usize == f)
    }

    /// The links within [`EDGE_REACH`] points of canvas point `q`: the pair of artists of the
    /// nearest, and every shown link between them.
    pub fn link_at(&mut self, q: [f32; 2]) -> Option<Hover> {
        // While artists move, the links grid is out of date.
        if self.world.len() != self.model.nodes.len() || self.drag.is_some() {
            return None;
        }
        let cam = self.cam();
        let [x, y] = self.world_at(cam, q);
        let reach = EDGE_REACH / cam.zoom + 1.0;
        if self.edge_grid.is_none() {
            self.edge_grid = Some(self.build_edge_grid());
        }
        let grid = self.edge_grid.as_ref()?;
        // Each link is filed under the cells its points fall in, sampled every half cell, so one
        // more cell around covers the ones it only clips.
        let cells = (reach / EDGE_CELL).ceil() as i32 + 1;
        let (cx, cy) = edge_cell(x, y);
        let mut near: Vec<(f32, u32)> = Vec::new();
        for gx in cx - cells..=cx + cells {
            for gy in cy - cells..=cy + cells {
                for &e in grid.get(&(gx, gy)).map_or(&[][..], Vec::as_slice) {
                    let edge = &self.model.edges[e as usize];
                    if !self.shown(edge) {
                        continue;
                    }
                    let d = to_segment([x, y], self.world[edge.from as usize], self.world[edge.to as usize]);
                    if d <= reach {
                        near.push((d, e));
                    }
                }
            }
        }
        let &(_, nearest) = near.iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
        let pair = |e: u32| {
            let edge = &self.model.edges[e as usize];
            (edge.from.min(edge.to), edge.from.max(edge.to))
        };
        let (a, b) = pair(nearest);
        let mut edges: Vec<usize> = near.iter().filter(|&&(_, e)| pair(e) == (a, b)).map(|&(_, e)| e as usize).collect();
        edges.sort_unstable_by_key(|&e| (EdgeKind::ALL.iter().position(|k| *k == self.model.edges[e].kind), e));
        edges.dedup();
        Some(Hover::Link { a: a as usize, b: b as usize, edges })
    }

    fn build_edge_grid(&self) -> HashMap<(i32, i32), Vec<u32>> {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, e) in self.model.edges.iter().enumerate() {
            let (a, b) = (self.world[e.from as usize], self.world[e.to as usize]);
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let steps = (len / (EDGE_CELL / 2.0)).ceil().max(1.0) as usize;
            let mut last = None;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let c = edge_cell(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                // A straight line never comes back to a cell it left.
                if last != Some(c) {
                    grid.entry(c).or_default().push(i as u32);
                    last = Some(c);
                }
            }
        }
        grid
    }

    /// The world rectangle (x0, y0, x1, y1) the view draws labels and edges for. It moves in
    /// steps of half the visible area and changes zoom in steps of √2, so panning and zooming
    /// rebuild the view only now and then: see [`Graph::region`].
    pub fn region(&self) -> (Region, [f32; 4]) {
        let cam = self.cam();
        let everything = (Region::default(), [f32::MIN, f32::MIN, f32::MAX, f32::MAX]);
        let Some([w, h]) = self.viewport.filter(|[w, h]| *w > 0.0 && *h > 0.0) else { return everything };
        let level = (cam.zoom.log2() * 2.0).round() as i32;
        let zoom = 2.0_f32.powf(level as f32 / 2.0);
        let (vw, vh) = (w / zoom, h / zoom);
        let mid = self.world_at(cam, [w / 2.0, h / 2.0]);
        let (sx, sy) = (vw / 2.0, vh / 2.0);
        let (fx, fy) = ((mid[0] / sx).floor(), (mid[1] / sy).floor());
        if !fx.is_finite() || !fy.is_finite() || fx.abs() > 1e6 || fy.abs() > 1e6 {
            return everything;
        }
        let (tx, ty) = (fx as i32, fy as i32);
        let rect = [tx as f32 * sx - vw, ty as f32 * sy - vh, (tx + 1) as f32 * sx + vw, (ty + 1) as f32 * sy + vh];
        (Region { level, tx, ty }, rect)
    }
}

/// Which stretch of the world the view last drew, coarsely: while it's the same, a pan or zoom
/// only moves the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Region {
    /// The zoom, in half powers of two.
    pub level: i32,
    pub tx: i32,
    pub ty: i32,
}

impl Region {
    /// The zoom this region draws labels for.
    pub fn zoom(self) -> f32 {
        2.0_f32.powf(self.level as f32 / 2.0)
    }
}

fn edge_cell(x: f32, y: f32) -> (i32, i32) {
    ((x / EDGE_CELL).floor() as i32, (y / EDGE_CELL).floor() as i32)
}

/// How far point `p` is from the segment from `a` to `b`.
fn to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

fn cell(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (y / CELL).floor() as i32)
}

struct Start {
    names: Vec<String>,
    links: Vec<(u32, u32)>,
    fingerprint: u64,
    cache: Option<PathBuf>,
    /// Where each name sat in the layout shown before, by lowercased name.
    previous: HashMap<String, [f32; 2]>,
    /// The pinned artists, lowercased; `None` takes the cache's.
    pins: Option<HashSet<String>>,
    /// Lay out from scratch, ignoring where anything was.
    cold: bool,
}

/// Lays the graph out, reporting through `tx` and calling `wake` after each message.
fn compute(start: Start, cancel: &AtomicBool, tx: &Sender<Msg>, wake: &dyn Fn()) {
    let send = |msg: Msg| {
        let _ = tx.send(msg);
        wake();
    };
    let n = start.names.len();
    let fingerprint = format!("{:016x}", start.fingerprint);
    let cached = start.cache.as_ref().and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice::<Cached>(&bytes).ok()).filter(|c| c.names.len() == c.positions.len());
    let pins: HashSet<String> = match (&start.pins, &cached) {
        (Some(pins), _) => pins.clone(),
        (None, Some(c)) => c.pinned.iter().cloned().collect(),
        (None, None) => HashSet::new(),
    };
    let known: HashMap<String, [f32; 2]> = match &cached {
        Some(_) if start.cold => HashMap::new(),
        Some(c) if c.fingerprint == fingerprint && c.names == start.names => {
            send(Msg::Done(c.positions.clone(), pins));
            return;
        }
        Some(c) => c.names.iter().cloned().zip(c.positions.iter().copied()).collect(),
        None => HashMap::new(),
    };
    // What showed before beats the cache, which may be older.
    let mut placed: Vec<Option<[f32; 2]>> = start.names.iter().map(|name| known.get(name).copied()).collect();
    if !start.previous.is_empty() {
        placed = start.names.iter().map(|name| start.previous.get(name).copied().or_else(|| known.get(name).copied())).collect();
    }
    // A pinned artist stays where it was; one with no place yet is laid out as any other.
    let fixed: Vec<bool> = start.names.iter().zip(&placed).map(|(name, p)| p.is_some() && pins.contains(name)).collect();
    let pins: HashSet<String> = start.names.iter().zip(&fixed).filter(|(_, f)| **f).map(|(n, _)| n.clone()).collect();
    let warm = placed.iter().filter(|p| p.is_some()).count() * 2 > n;
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(a, b) in &start.links {
        neighbours[a as usize].push(b);
        neighbours[b as usize].push(a);
    }
    let positions: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            if let Some(p) = placed[i] {
                return p;
            }
            // Next to the artists it's linked to, when they're placed.
            let around: Vec<[f32; 2]> = neighbours[i].iter().filter_map(|&j| placed[j as usize]).collect();
            let jitter = layout::seed(&start.names[i], 1);
            match around.len() {
                0 => layout::seed(&start.names[i], n),
                k => {
                    let (sx, sy) = around.iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
                    [sx / k as f32 + jitter[0], sy / k as f32 + jitter[1]]
                }
            }
        })
        .collect();
    let steps = if warm { layout::WARM_STEPS } else { layout::cold_steps(n) };
    let mut last = Instant::now();
    let result = layout::run(n, &start.links, positions, &fixed, steps, cancel, |step, pos| {
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            send(Msg::Progress { positions: pos.to_vec(), step, steps });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if let Some(path) = &start.cache {
        let mut pinned: Vec<String> = pins.iter().cloned().collect();
        pinned.sort_unstable();
        let cached = Cached { fingerprint, names: start.names.clone(), positions: result.clone(), pinned };
        if let Err(e) = write(path, &cached) {
            send(Msg::Error(format!("Couldn't save the graph layout to {}: {e}", path.display())));
        }
    }
    send(Msg::Done(result, pins));
}

fn write(path: &std::path::Path, cached: &Cached) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(cached).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

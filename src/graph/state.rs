//! The graph page's state: the model, its layout (run on a worker thread, cached on disk), the
//! camera, and what's hovered or picked.
//!
//! **The cache:** `graph-layout.json` beside the preferences keeps the last layout's positions by
//! artist name, with the fingerprint of the graph they were laid out for ([`Model::fingerprint`]:
//! the names and which pairs are linked). When the graph still has that fingerprint the layout is
//! used as is. When it changed (music added, an artist linked), the known artists start where
//! they were, new ones next to the artists they're linked to, and a shorter layout settles them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use rune::{Live, TextEdit, Waker};
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
    /// Layout positions, before [`layout::to_world`].
    positions: Vec<[f32; 2]>,
}

enum Msg {
    Progress { positions: Vec<[f32; 2]>, step: usize, steps: usize },
    Done(Vec<[f32; 2]>),
    Error(String),
}

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
    pub fn refresh(&mut self, model: Model, cache: Option<PathBuf>, waker: Option<Waker>) -> Result<(), String> {
        let selected = self.selected.and_then(|i| self.model.nodes.get(i)).map(|n| n.name.clone());
        let previous: HashMap<String, [f32; 2]> = self.model.nodes.iter().zip(&self.raw).map(|(n, p)| (n.name.to_lowercase(), *p)).collect();
        self.stale = false;
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
        let start = Start { names, links, fingerprint, cache, previous };
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
                Msg::Done(positions) => {
                    latest = Some(positions);
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
        let (world, size) = layout::to_world(raw, &self.model.links());
        self.raw = raw.to_vec();
        self.world = world;
        self.size = size;
        self.grid.clear();
        self.edge_grid = None;
        for (i, &[x, y]) in self.world.iter().enumerate() {
            self.grid.entry(cell(x, y)).or_default().push(i as u32);
        }
        self.version += 1;
        self.sync_live();
    }

    /// The camera in use: the user's, else the one fitting the drawing to the canvas.
    pub fn cam(&self) -> Camera {
        self.camera.unwrap_or_else(|| self.fit())
    }

    /// The camera showing the whole drawing, centred.
    pub fn fit(&self) -> Camera {
        let Some([w, h]) = self.viewport else { return Camera { pan: [0.0, 0.0], zoom: 1.0 } };
        let [sw, sh] = self.size;
        if sw <= 0.0 || sh <= 0.0 {
            return Camera { pan: [0.0, 0.0], zoom: 1.0 };
        }
        let zoom = (w / sw).min(h / sh).clamp(ZOOM_MIN, 1.5);
        let c = self.centre();
        Camera { pan: [w / 2.0 - c[0], h / 2.0 - c[1]], zoom }
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

    /// The node under canvas point `q`, if any: the nearest whose dot (plus a few points of
    /// slack) holds it.
    pub fn node_at(&self, q: [f32; 2]) -> Option<usize> {
        let cam = self.cam();
        let [x, y] = self.world_at(cam, q);
        let slack = 4.0 / cam.zoom;
        let reach = ((20.0 + slack) / CELL).ceil() as i32;
        let (cx, cy) = cell(x, y);
        let mut best: Option<(f32, usize)> = None;
        for gx in cx - reach..=cx + reach {
            for gy in cy - reach..=cy + reach {
                for &i in self.grid.get(&(gx, gy)).map_or(&[][..], Vec::as_slice) {
                    let i = i as usize;
                    let [px, py] = self.world[i];
                    let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                    if d <= radius(self.model.nodes[i].degree) + slack && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
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
        if self.world.len() != self.model.nodes.len() {
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
        let Some([w, h]) = self.viewport else { return (Region::default(), [f32::MIN, f32::MIN, f32::MAX, f32::MAX]) };
        let level = (cam.zoom.log2() * 2.0).round() as i32;
        let zoom = 2.0_f32.powf(level as f32 / 2.0);
        let (vw, vh) = (w / zoom, h / zoom);
        let mid = self.world_at(cam, [w / 2.0, h / 2.0]);
        let (sx, sy) = (vw / 2.0, vh / 2.0);
        let (tx, ty) = ((mid[0] / sx).floor() as i32, (mid[1] / sy).floor() as i32);
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
    let known: HashMap<String, [f32; 2]> = match &cached {
        Some(c) if c.fingerprint == fingerprint && c.names == start.names => {
            send(Msg::Done(c.positions.clone()));
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
    let result = layout::run(n, &start.links, positions, steps, cancel, |step, pos| {
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            send(Msg::Progress { positions: pos.to_vec(), step, steps });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if let Some(path) = &start.cache {
        let cached = Cached { fingerprint, names: start.names.clone(), positions: result.clone() };
        if let Err(e) = write(path, &cached) {
            send(Msg::Error(format!("Couldn't save the graph layout to {}: {e}", path.display())));
        }
    }
    send(Msg::Done(result));
}

fn write(path: &std::path::Path, cached: &Cached) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(cached).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

//! A force-directed layout: ForceAtlas2 (Jacomy et al., 2014) with its adaptive speed, and a
//! Barnes–Hut quadtree so repulsion costs O(n log n) a step rather than O(n²).
//!
//! - **Repulsion:** every pair of nodes pushes apart by `REPULSION · mᵢ · mⱼ / d`, where a
//!   node's mass is its link count plus one, so hubs claim room around them.
//! - **Attraction:** linked nodes pull together by their distance.
//! - **Gravity:** every node is pulled to the origin by `GRAVITY · m · d` (ForceAtlas2's
//!   "strong gravity"), so lone artists and small groups stay close instead of ringing the
//!   drawing far out.
//!
//! Positions are in the layout's own units; [`to_world`] scales them for drawing.

use std::sync::atomic::{AtomicBool, Ordering};

const REPULSION: f32 = 10.0;
const GRAVITY: f32 = 0.05;
/// Cells this much smaller than their distance stand in for their nodes.
const THETA: f32 = 1.2;
/// The quadtree stops splitting here; nodes in one leaf push each other directly.
const MAX_DEPTH: u32 = 24;
/// Below this many nodes a step runs on one thread.
const PARALLEL_FROM: usize = 2000;

/// The steps a layout from scratch takes, by node count: more for small graphs, which are cheap.
pub fn cold_steps(n: usize) -> usize {
    match n {
        0..=2000 => 1200,
        2001..=10000 => 800,
        _ => 500,
    }
}

/// The steps a layout started from a cached one takes.
pub const WARM_STEPS: usize = 300;

/// Lays out `n` nodes joined by `links` from `start`, for `steps` steps or until `cancel` is set.
/// `progress` hears the step count and positions now and then.
pub fn run(n: usize, links: &[(u32, u32)], start: Vec<[f32; 2]>, steps: usize, cancel: &AtomicBool, mut progress: impl FnMut(usize, &[[f32; 2]])) -> Vec<[f32; 2]> {
    let mut pos = start;
    if n == 0 || pos.len() != n {
        return pos;
    }
    let mut mass = vec![1.0_f32; n];
    for &(a, b) in links {
        mass[a as usize] += 1.0;
        mass[b as usize] += 1.0;
    }
    let mut speed = Speed::new();
    let mut force = vec![[0.0_f32; 2]; n];
    let mut old = vec![[0.0_f32; 2]; n];
    let threads = std::thread::available_parallelism().map_or(1, |t| t.get()).clamp(1, 8);
    for step in 0..steps {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        std::mem::swap(&mut force, &mut old);
        let tree = Tree::build(&pos, &mass);
        repel(&tree, &pos, &mass, &mut force, threads);
        for (i, f) in force.iter_mut().enumerate() {
            let [x, y] = pos[i];
            let g = GRAVITY * mass[i];
            f[0] -= x * g;
            f[1] -= y * g;
        }
        for &(a, b) in links {
            let (a, b) = (a as usize, b as usize);
            let dx = pos[a][0] - pos[b][0];
            let dy = pos[a][1] - pos[b][1];
            force[a][0] -= dx;
            force[a][1] -= dy;
            force[b][0] += dx;
            force[b][1] += dy;
        }
        speed.apply(&mut pos, &force, &old, &mass, step == 0);
        if step % 10 == 9 {
            progress(step + 1, &pos);
        }
    }
    pos
}

/// Each node's force set to the repulsion of all the others.
fn repel(tree: &Tree, pos: &[[f32; 2]], mass: &[f32], force: &mut [[f32; 2]], threads: usize) {
    if pos.len() < PARALLEL_FROM || threads == 1 {
        for (i, f) in force.iter_mut().enumerate() {
            *f = tree.repulsion(i, pos, mass);
        }
        return;
    }
    let chunk = pos.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for (c, part) in force.chunks_mut(chunk).enumerate() {
            scope.spawn(move || {
                for (k, f) in part.iter_mut().enumerate() {
                    *f = tree.repulsion(c * chunk + k, pos, mass);
                }
            });
        }
    });
}

/// ForceAtlas2's adaptive speed: fast while nodes travel steadily, slow while they swing.
struct Speed {
    speed: f32,
    efficiency: f32,
}

impl Speed {
    fn new() -> Speed {
        Speed { speed: 1.0, efficiency: 1.0 }
    }

    fn apply(&mut self, pos: &mut [[f32; 2]], force: &[[f32; 2]], old: &[[f32; 2]], mass: &[f32], first: bool) {
        let n = pos.len() as f32;
        let (mut swinging, mut traction) = (0.0_f32, 0.0_f32);
        let swing = |i: usize| if first { 0.0 } else { len(force[i][0] - old[i][0], force[i][1] - old[i][1]) };
        for i in 0..pos.len() {
            swinging += mass[i] * swing(i);
            traction += mass[i] * 0.5 * len(force[i][0] + old[i][0], force[i][1] + old[i][1]);
        }
        if !first && swinging > 0.0 && traction > 0.0 {
            let estimated = 0.05 * n.sqrt();
            let mut jitter = (estimated * traction / (n * n)).clamp(0.1_f32.sqrt(), 10.0);
            if swinging / traction > 2.0 {
                if self.efficiency > 0.05 {
                    self.efficiency *= 0.5;
                }
                jitter = jitter.max(1.0);
            }
            let target = jitter * self.efficiency * traction / swinging;
            if swinging > jitter * traction {
                if self.efficiency > 0.05 {
                    self.efficiency *= 0.7;
                }
            } else if self.speed < 1000.0 {
                self.efficiency *= 1.3;
            }
            self.speed += (target - self.speed).min(0.5 * self.speed);
        }
        for i in 0..pos.len() {
            let factor = self.speed / (1.0 + (self.speed * mass[i] * swing(i)).sqrt());
            let [fx, fy] = force[i];
            // A node never jumps further than this in one step, whatever the forces.
            let step = len(fx, fy) * factor;
            let k = if step > 10.0 { factor * 10.0 / step } else { factor };
            if k.is_finite() {
                pos[i][0] += fx * k;
                pos[i][1] += fy * k;
            }
        }
    }
}

fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// A Barnes–Hut quadtree over the nodes, each cell knowing its total mass and centre of mass.
struct Tree {
    cells: Vec<Cell>,
    /// Node indices, each leaf's a contiguous run.
    order: Vec<u32>,
}

struct Cell {
    mass: f32,
    centre: [f32; 2],
    size: f32,
    /// The first of four children (some may be `NONE`), or `NONE` for a leaf.
    children: [u32; 4],
    /// A leaf's run of `order`.
    first: u32,
    count: u32,
}

const NONE: u32 = u32::MAX;

impl Tree {
    fn build(pos: &[[f32; 2]], mass: &[f32]) -> Tree {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &[x, y] in pos {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let size = (x1 - x0).max(y1 - y0).max(1e-3);
        let mut tree = Tree { cells: Vec::with_capacity(pos.len() * 2), order: (0..pos.len() as u32).collect() };
        let len = tree.order.len();
        tree.split(0, len, [x0, y0], size, 0, pos, mass);
        tree
    }

    /// Adds the cell over `order[start..end]`, inside the square at `corner`; its index.
    #[allow(clippy::too_many_arguments)]
    fn split(&mut self, start: usize, end: usize, corner: [f32; 2], size: f32, depth: u32, pos: &[[f32; 2]], mass: &[f32]) -> u32 {
        let (mut m, mut cx, mut cy) = (0.0_f32, 0.0_f32, 0.0_f32);
        for &i in &self.order[start..end] {
            let i = i as usize;
            m += mass[i];
            cx += pos[i][0] * mass[i];
            cy += pos[i][1] * mass[i];
        }
        let at = self.cells.len() as u32;
        self.cells.push(Cell { mass: m, centre: [cx / m, cy / m], size, children: [NONE; 4], first: start as u32, count: (end - start) as u32 });
        if end - start <= 1 || depth >= MAX_DEPTH {
            return at;
        }
        let half = size / 2.0;
        let mid = [corner[0] + half, corner[1] + half];
        let quadrant = |i: u32| usize::from(pos[i as usize][0] >= mid[0]) + 2 * usize::from(pos[i as usize][1] >= mid[1]);
        self.order[start..end].sort_unstable_by_key(|&i| quadrant(i));
        let mut from = start;
        let mut children = [NONE; 4];
        for (q, child) in children.iter_mut().enumerate() {
            let to = from + self.order[from..end].iter().take_while(|&&i| quadrant(i) == q).count();
            if to > from {
                let c = [corner[0] + half * (q % 2) as f32, corner[1] + half * (q / 2) as f32];
                *child = self.split(from, to, c, half, depth + 1, pos, mass);
            }
            from = to;
        }
        self.cells[at as usize].children = children;
        at
    }

    /// The repulsion on node `i` from every other node.
    fn repulsion(&self, i: usize, pos: &[[f32; 2]], mass: &[f32]) -> [f32; 2] {
        let [x, y] = pos[i];
        let mi = mass[i] * REPULSION;
        let mut f = [0.0_f32; 2];
        let mut push = |dx: f32, dy: f32, m: f32| {
            let d2 = dx * dx + dy * dy;
            if d2 > 1e-8 {
                // m·mᵢ / d along the unit vector: over d².
                let k = mi * m / d2;
                f[0] += dx * k;
                f[1] += dy * k;
            }
        };
        let mut stack = vec![0_u32];
        while let Some(c) = stack.pop() {
            let cell = &self.cells[c as usize];
            let leaf = cell.children == [NONE; 4];
            if leaf {
                for &j in &self.order[cell.first as usize..(cell.first + cell.count) as usize] {
                    let j = j as usize;
                    if j != i {
                        push(x - pos[j][0], y - pos[j][1], mass[j]);
                    }
                }
                continue;
            }
            let (dx, dy) = (x - cell.centre[0], y - cell.centre[1]);
            let d = len(dx, dy);
            if d > 0.0 && cell.size / d < THETA {
                push(dx, dy, cell.mass);
            } else {
                stack.extend(cell.children.iter().copied().filter(|&ch| ch != NONE));
            }
        }
        f
    }
}

/// How far apart, in points, linked nodes typically sit once drawn.
const LINK_LENGTH: f32 = 90.0;
/// Room around the drawing.
pub const MARGIN: f32 = 200.0;

/// `raw` positions scaled so a typical link is [`LINK_LENGTH`] long, moved so the drawing starts
/// at [`MARGIN`]; and the drawing's size with its margins.
pub fn to_world(raw: &[[f32; 2]], links: &[(u32, u32)]) -> (Vec<[f32; 2]>, [f32; 2]) {
    if raw.is_empty() {
        return (Vec::new(), [2.0 * MARGIN, 2.0 * MARGIN]);
    }
    let mut lengths: Vec<f32> = links.iter().map(|&(a, b)| len(raw[a as usize][0] - raw[b as usize][0], raw[a as usize][1] - raw[b as usize][1])).filter(|d| *d > 0.0).collect();
    let scale = if lengths.is_empty() {
        1.0
    } else {
        let mid = lengths.len() / 2;
        let (_, median, _) = lengths.select_nth_unstable_by(mid, f32::total_cmp);
        LINK_LENGTH / *median
    };
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &[x, y] in raw {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let world = raw.iter().map(|&[x, y]| [(x - x0) * scale + MARGIN, (y - y0) * scale + MARGIN]).collect();
    (world, [(x1 - x0) * scale + 2.0 * MARGIN, (y1 - y0) * scale + 2.0 * MARGIN])
}

/// A starting point for node `name`: spread over a square that grows with the node count, the
/// same for the same name every time.
pub fn seed(name: &str, n: usize) -> [f32; 2] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let side = (n as f32).sqrt() * 10.0 + 10.0;
    let unit = |bits: u64| (bits & 0xffff) as f32 / 65535.0 - 0.5;
    [unit(h) * side, unit(h >> 20) * side]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(p: &[[f32; 2]], a: usize, b: usize) -> f32 {
        len(p[a][0] - p[b][0], p[a][1] - p[b][1])
    }

    #[test]
    fn linked_nodes_end_closer_than_unlinked_ones() {
        // Two triangles joined by nothing.
        let links = [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)];
        let start: Vec<[f32; 2]> = (0..6).map(|i| seed(&format!("n{i}"), 6)).collect();
        let pos = run(6, &links, start, 600, &AtomicBool::new(false), |_, _| {});
        let within = dist(&pos, 0, 1).max(dist(&pos, 3, 4));
        let across = dist(&pos, 0, 3).min(dist(&pos, 1, 4)).min(dist(&pos, 2, 5));
        assert!(within < across, "links pull together: within {within}, across {across}");
        assert!(pos.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn barnes_hut_matches_the_exact_repulsion_closely() {
        let n = 400;
        let pos: Vec<[f32; 2]> = (0..n).map(|i| seed(&format!("artist {i}"), n)).collect();
        let mass: Vec<f32> = (0..n).map(|i| 1.0 + (i % 5) as f32).collect();
        let tree = Tree::build(&pos, &mass);
        let mut worst = 0.0_f32;
        for i in (0..n).step_by(37) {
            let approx = tree.repulsion(i, &pos, &mass);
            let mut exact = [0.0_f32; 2];
            for j in (0..n).filter(|&j| j != i) {
                let (dx, dy) = (pos[i][0] - pos[j][0], pos[i][1] - pos[j][1]);
                let k = REPULSION * mass[i] * mass[j] / (dx * dx + dy * dy);
                exact[0] += dx * k;
                exact[1] += dy * k;
            }
            worst = worst.max(len(approx[0] - exact[0], approx[1] - exact[1]) / len(exact[0], exact[1]));
        }
        assert!(worst < 0.25, "relative error {worst}");
    }

    #[test]
    fn nodes_on_one_spot_stay_finite() {
        let start = vec![[0.0, 0.0]; 50];
        let pos = run(50, &[(0, 1)], start, 50, &AtomicBool::new(false), |_, _| {});
        assert!(pos.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn world_positions_fit_inside_the_drawing() {
        let raw = [[-5.0, 2.0], [5.0, 2.0], [0.0, 12.0]];
        let (world, size) = to_world(&raw, &[(0, 1)]);
        assert!((world[1][0] - world[0][0] - LINK_LENGTH).abs() < 1e-3, "the median link is LINK_LENGTH long");
        for p in &world {
            assert!(p[0] >= MARGIN - 1e-3 && p[0] <= size[0] - MARGIN + 1e-3);
            assert!(p[1] >= MARGIN - 1e-3 && p[1] <= size[1] - MARGIN + 1e-3);
        }
    }
}

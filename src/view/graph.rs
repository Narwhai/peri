//! The Graph tab: every artist as a dot, joined by coloured lines to those they've worked with.
//!
//! **Fast at any size:** the world (edges and dots) is one canvas per link kind plus one for the
//! dots, under a camera (`live_offset` and `live_scale`), so panning and zooming draw without
//! running the view. Names show only for the artists in view that are big enough at the zoom,
//! from a coarse [`Region`] that changes in steps as the camera moves: only then does the view
//! run again, and the shapes are memoized on the graph's version so even then they're reused.

use rune::{Color, Cursor, Drag, El, LineCap, MagnifyEvent, Phase, PointerButton, Shape, Transition, Tx, WheelEvent, canvas, col, el, memo, row, text};
use rune_kit::{Kit, MenuItem, TextInput, context_menu};

use super::*;
use crate::app::App;
use crate::graph::EdgeKind;
use crate::graph::state::{Graph, Hover, radius};
use crate::palette::Palette;

/// Names show for at most this many artists at once.
const MAX_LABELS: usize = 160;
/// A name shows once its dot is drawn this many points wide.
const LABEL_FROM: f32 = 7.0;
/// The most linked artists in view are named however small they're drawn.
const ALWAYS_LABEL: usize = 12;
/// Wheel steps zoom by this per point: a mouse wheel's 40-point notch is about 10%.
const WHEEL_ZOOM: f32 = 0.0025;

/// A link kind's colour.
pub fn edge_color(kind: EdgeKind, p: &Palette) -> Color {
    let (dark, light) = match kind {
        EdgeKind::Merge => (0xf4f4f5, 0x18181b),
        EdgeKind::Alias => (0x22d3ee, 0x0891b2),
        EdgeKind::MemberOf => (0xfacc15, 0xca8a04),
        EdgeKind::FormerMemberOf => (0xfef08a, 0xa16207),
        EdgeKind::Featured => (0x60a5fa, 0x2563eb),
        EdgeKind::With => (0x94a3b8, 0x64748b),
        EdgeKind::Credit(r) => return p.role(r).1,
    };
    rgb(if p.dark { dark } else { light })
}

fn rgb(hex: u32) -> Color {
    use rune::color::srgb_decode;
    let channel = |shift: u32| srgb_decode(((hex >> shift) & 0xff) as f32 / 255.0);
    Color { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }
}

/// How wide a kind's lines draw: the relations the user records stand out.
fn edge_width(kind: EdgeKind) -> f32 {
    match kind {
        EdgeKind::Merge | EdgeKind::Alias | EdgeKind::MemberOf | EdgeKind::FormerMemberOf => 2.0,
        EdgeKind::Featured | EdgeKind::With | EdgeKind::Credit(_) => 1.0,
    }
}

pub(super) fn graph_page(s: &App, p: &Palette, kit: &Kit, zoom: f32, layer: &mut Vec<El>) -> El {
    let g = &s.graph;
    let body = if s.tracks().is_empty() && s.scan.is_none() {
        empty("No artists yet", "Open a folder to see how its artists are linked.", p)
    } else if g.model.nodes.is_empty() {
        empty(if s.scan.is_some() { "Reading your music…" } else { "No artists" }, "Artists show once songs with artist tags are in the library.", p)
    } else {
        canvas_area(g, p, kit, zoom, layer)
    };
    col().key("graph-page").w_full().grow(1.0).min_h(0.0).child(graph_toolbar(s, p, kit)).child(body)
}

fn empty(title: &str, detail: &str, p: &Palette) -> El {
    col()
        .key("graph-empty")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(6.0)
        .child(text(title.to_string()).font(HEADING).color(p.tx2))
        .child(text(detail.to_string()).font(BODY).color(p.tx3))
}

/// Find, counts and the layout's progress, then the legend's toggles.
fn graph_toolbar(s: &App, p: &Palette, kit: &Kit) -> El {
    let g = &s.graph;
    let find = TextInput::new(&g.find)
        .placeholder("Find an artist")
        .label("Find an artist")
        .leading_icon(&rune_kit::glyphs::SEARCH)
        .width(Some(200.0))
        .clear_button()
        .on_edit(|s: &mut App| &mut s.graph.find)
        .on_change(|s: &mut App, _tx| s.graph_found())
        .on_action(|s: &mut App, _tx| {
            s.graph.find.clear();
            s.graph.selected = None;
        })
        .on_key(|s: &mut App, ev, tx| match ev.key {
            rune::Key::Enter => {
                if let Some(name) = s.graph.selected.and_then(|i| s.graph.model.nodes.get(i)).map(|n| n.name.clone()) {
                    s.show_artist(&name, tx);
                }
                true
            }
            rune::Key::Escape => {
                s.graph.find.clear();
                s.graph.selected = None;
                tx.blur();
                true
            }
            _ => false,
        })
        .build(kit)
        .key("graph-find")
        .radius(2.0);
    let shown_edges = g.model.edges.iter().filter(|e| g.hidden & e.kind.bit() == 0).count();
    let status = match g.progress {
        Some((step, steps)) => format!("Arranging… {}%", step * 100 / steps.max(1)),
        None if g.running() => "Arranging…".to_string(),
        None => format!("{} · {}", plural(g.model.nodes.len(), "artist"), plural(shown_edges, "link")),
    };
    let present: u32 = g.model.edges.iter().fold(0, |bits, e| bits | e.kind.bit());
    let legend = EdgeKind::ALL.into_iter().filter(|k| present & k.bit() != 0).fold(row().key("graph-legend").gap(4.0).flex_wrap().grow(1.0).shrink().min_w(0.0), |r, kind| r.child(legend_chip(kind, g.hidden & kind.bit() == 0, p)));
    col()
        .key("graph-toolbar")
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(8.0)
        .fill(p.bg)
        .border_bottom(p)
        .child(
            row()
                .w_full()
                .gap(12.0)
                .center_y()
                .child(find)
                .child(text(status).key("graph-status").font(SMALL).color(p.tx3).grow(1.0).shrink().truncate())
                .child(pill_button("graph-fit", "Fit", p).help("Show every artist").on_press(|s: &mut App| s.graph.reset_camera()))
                .child(
                    pill_button("graph-reset", "Reset layout", p)
                        .help(if g.pinned.iter().any(|p| *p) { "Unpin every artist and arrange the graph from scratch" } else { "Arrange the graph from scratch" })
                        .on_press(|s: &mut App| s.reset_graph_layout()),
                ),
        )
        .child(legend)
}

fn legend_chip(kind: EdgeKind, on: bool, p: &Palette) -> El {
    let color = edge_color(kind, p);
    // A checkbox in the link's own colour: filled and checked when shown, outlined when hidden.
    let mark = el().key(format!("kind-{}-mark", kind.id())).size(14.0, 14.0).radius(3.0).center();
    let mark = if on { mark.fill(color).child(icon(&rune_kit::glyphs::CHECK_SMALL, 12.0).color(p.bg)) } else { mark.border(1.5, color.alpha(0.6)) };
    row()
        .key(format!("kind-{}", kind.id()))
        .h(24.0)
        .px(6.0)
        .gap(6.0)
        .radius(4.0)
        .center_y()
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .role("checkbox")
        .checked(on)
        .label(kind.label())
        .help(if on { format!("Hide {} links", kind.label().to_lowercase()) } else { format!("Show {} links", kind.label().to_lowercase()) })
        .on_press(move |s: &mut App| s.toggle_graph_kind(kind))
        .child(mark)
        .child(text(kind.label()).font(SMALL).color(if on { p.tx } else { p.tx3 }))
}

/// The canvas: the world under the camera, and what's hovered over it.
fn canvas_area(g: &Graph, p: &Palette, kit: &Kit, zoom: f32, layer: &mut Vec<El>) -> El {
    let (region, rect) = g.region();
    let p = *p;
    // While an artist is dragged, what moves draws on its own layer, every frame; the rest
    // draws once for the drag, without what moves.
    let drag = g.drag.as_ref().map(|d| d.id);
    let shapes = memo("graph-shapes", (g.version, g.hidden, g.selected, region.level, drag, p.dark), || shapes(g, region.zoom(), &p));
    let moving = g.drag.as_ref().map(|d| memo("graph-moving", (d.tick, g.hidden, g.selected, region.level, p.dark), || moving(g, region.zoom(), &p)));
    let names = memo("graph-names", (g.version, g.selected, region, drag.map(|_| g.drag.as_ref().map_or(0, |d| d.tick)), p.dark), || names(g, region.zoom(), rect, &p));
    let [w, h] = g.size;
    let lit = memo("graph-hover", (g.version, g.hover.clone(), region.level, p.dark), || highlight(g, region.zoom(), &p));
    let world = el()
        .key("graph-world")
        .pin(Some(0.0), Some(0.0), None, None)
        .size(w.max(1.0), h.max(1.0))
        .live_offset(&g.live_pan)
        .live_scale(&g.live_zoom)
        .inert()
        .child(shapes)
        .children(moving)
        .children(pacer(g))
        .child(lit)
        .child(names);
    let tip = match &g.hover {
        Some(Hover::Node(i)) => hover_tip(g, *i, &p),
        Some(Hover::Link { edges, .. }) => link_tip(g, edges, &p),
        None => None,
    };
    let canvas = el().key("graph-canvas")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(p.bg)
        .clip_soft(0.0)
        .cursor(match (&g.drag, g.hovered_node()) {
            (Some(_), _) => Cursor::Grabbing,
            (None, Some(_)) => Cursor::Pointer,
            (None, None) => Cursor::Grab,
        })
        .role("application")
        .label("Artist graph")
        .on_layout(|s: &mut App, r| s.graph.set_viewport([r[2], r[3]]))
        .child(world)
        .children(tip)
        .on_wheel(|s: &mut App, e: WheelEvent, tx| {
            let cam = s.graph.cam();
            if e.mods.cmd || e.mods.ctrl || !e.precise {
                // A notched mouse wheel zooms, as on maps; a trackpad pans.
                s.graph.zoom_about([e.x, e.y], (e.dy * WHEEL_ZOOM).exp());
            } else {
                s.graph.set_camera(crate::graph::state::Camera { pan: [cam.pan[0] + e.dx, cam.pan[1] + e.dy], zoom: cam.zoom });
            }
            moved(s, tx);
            true
        })
        .on_magnify(|s: &mut App, e: MagnifyEvent, tx| {
            s.graph.zoom_about([e.x, e.y], 1.0 + e.delta);
            moved(s, tx);
            true
        })
        .on_pointer_move_local(|s: &mut App, e, tx| {
            if s.graph.holding() {
                tx.live_only();
                return;
            }
            let hit = s.graph.hit([e.x, e.y]);
            if hit == s.graph.hover {
                tx.live_only();
                return;
            }
            s.graph.hover = hit;
            s.graph.hover_at = [e.x, e.y];
        })
        .on_hover_change(|s: &mut App, over, _tx| {
            if !over {
                s.graph.hover = None;
            }
        })
        .pointer_buttons([PointerButton::Middle])
        .on_secondary_press_local(|s: &mut App, e, _tx| {
            s.graph.hover = None;
            s.graph.menu = s.graph.node_at([e.x, e.y]).map(|i| (i, e.window()));
        })
        .on_drag(|s: &mut App, d: &mut Drag, tx: &mut Tx| {
            // The primary button on an artist moves it; on empty space, or with the middle
            // button, it pans.
            let artist = tx.button() == PointerButton::Primary;
            match d.phase {
                Phase::Down => {
                    if let Err(e) = s.graph.finish_drag() {
                        s.error = Some(e);
                    }
                    s.graph.drag_from = s.graph.cam();
                    s.graph.press = if artist { s.graph.node_at([d.start_x, d.start_y]) } else { None };
                    return;
                }
                Phase::Move => {
                    let far = d.dx.abs() >= 4.0 || d.dy.abs() >= 4.0;
                    if let Some(i) = s.graph.press.filter(|_| far && !s.graph.holding()) {
                        let at = s.graph.world_at(s.graph.drag_from, [d.start_x, d.start_y]);
                        s.graph.start_drag(i, at);
                        tx.wake_after(crate::graph::state::SIM_FRAME);
                        return;
                    }
                    if s.graph.holding() {
                        let at = s.graph.world_at(s.graph.cam(), [d.x, d.y]);
                        s.graph.drag_to(at);
                        tx.wake_after(crate::graph::state::SIM_FRAME);
                        tx.live_only();
                        return;
                    }
                    if s.graph.press.is_some() {
                        return;
                    }
                    let from = s.graph.drag_from;
                    s.graph.set_camera(crate::graph::state::Camera { pan: [from.pan[0] + d.dx, from.pan[1] + d.dy], zoom: from.zoom });
                }
                Phase::Up => {
                    let press = s.graph.press.take();
                    if s.graph.holding() {
                        if d.cancelled() {
                            s.graph.cancel_drag();
                        } else {
                            let at = s.graph.world_at(s.graph.cam(), [d.x, d.y]);
                            s.graph.drop_drag(at);
                            tx.wake_after(crate::graph::state::SIM_FRAME);
                        }
                        return;
                    }
                    if d.cancelled() {
                        s.graph.set_camera(s.graph.drag_from);
                    } else if let Some(name) = press.filter(|_| d.dx.abs() < 4.0 && d.dy.abs() < 4.0).and_then(|i| s.graph.model.nodes.get(i)).map(|n| n.name.clone()) {
                        // A click, not a drag: open the artist.
                        s.show_artist(&name, tx);
                        return;
                    }
                }
            }
            moved(s, tx);
        });
    match g.menu {
        Some((i, at)) => {
            let name = g.model.nodes.get(i).map_or_else(String::new, |n| n.name.clone());
            let pinned = g.pinned.get(i).copied().unwrap_or(false);
            let open = name.clone();
            // The menu's trigger is an empty point: wrapping the canvas in it would take the
            // canvas out of the column's flex layout.
            let menu = context_menu("graph-menu", el().size(0.0, 0.0), Some(unzoomed(at, zoom)))
                .group_label(name)
                .item(MenuItem::new("Open artist").on_select_tx(move |s: &mut App, tx| {
                    s.graph.menu = None;
                    s.show_artist(&open, tx);
                }))
                .item(
                    MenuItem::new("Unpin")
                        .description(if pinned { "Layouts may move them" } else { "Drag to pin" })
                        .disabled(!pinned)
                        .on_select(move |s: &mut App| s.unpin_graph_artist(i)),
                )
                .on_close(|s: &mut App| s.graph.menu = None)
                .build(kit, layer);
            canvas.child(menu.pin(Some(0.0), Some(0.0), None, None))
        }
        None => canvas,
    }
}

/// The camera moved: the view only needs to run when the drawn region changes, or to drop a
/// tooltip the move left behind.
fn moved(s: &mut App, tx: &mut Tx) {
    let region = s.graph.region().0;
    let hovering = s.graph.hover.take().is_some();
    if region == s.graph.drawn_region && !hovering {
        tx.live_only();
    } else {
        s.graph.drawn_region = region;
    }
    tx.wake_after(crate::graph::state::SAVE_AFTER);
}

/// The hovered artist's name and link count, by their dot.
fn hover_tip(g: &Graph, i: usize, p: &Palette) -> Option<El> {
    let node = g.model.nodes.get(i)?;
    let cam = g.cam();
    let [x, y] = g.screen(cam, *g.world.get(i)?);
    let r = radius(node.degree) * cam.zoom;
    Some(
        col()
            .key("graph-tip")
            .pin(Some(x + r + 6.0), Some(y - 12.0), None, None)
            .px(8.0)
            .py(4.0)
            .radius(3.0)
            .fill(p.bg2)
            .border(1.0, p.ui)
            .shadow(4.0, 12.0, Color::BLACK.alpha(0.2))
            .inert()
            .child(text(node.name.clone()).font(TITLE).color(p.tx))
            .child(text(plural(node.degree as usize, "linked artist")).font(SMALL).color(p.tx3)),
    )
}

/// What a hovered link says, by the pointer: a line per kind of link between the two artists,
/// each in its colour.
fn link_tip(g: &Graph, edges: &[usize], p: &Palette) -> Option<El> {
    const MAX_LINES: usize = 6;
    let [x, y] = g.hover_at;
    // Below and right of the pointer, else left of it, else against the canvas's right edge:
    // wherever it fits, by its width estimated from its longest line.
    let longest = edges.iter().take(MAX_LINES).filter_map(|&e| {
        let edge = g.model.edges.get(e)?;
        let (from, to) = (g.model.nodes.get(edge.from as usize)?, g.model.nodes.get(edge.to as usize)?);
        Some(edge.kind.sentence(&from.name, &to.name, edge.count).chars().count())
    });
    let width = 84.0 + 8.0 + longest.max().unwrap_or(0) as f32 * 7.2 + 18.0;
    let vw = g.viewport.map_or(f32::MAX, |[w, _]| w);
    let left = if x + 14.0 + width <= vw - 8.0 {
        x + 14.0
    } else if x - 14.0 - width >= 8.0 {
        x - 14.0 - width
    } else {
        (vw - 8.0 - width).max(8.0)
    };
    let mut tip = col()
        .key("graph-link-tip")
        .pin(Some(left), Some(y + 14.0), None, None)
        .px(8.0)
        .py(6.0)
        .gap(4.0)
        .radius(3.0)
        .fill(p.bg2)
        .border(1.0, p.ui)
        .shadow(4.0, 12.0, Color::BLACK.alpha(0.2))
        .inert();
    for &e in edges.iter().take(MAX_LINES) {
        let edge = g.model.edges.get(e)?;
        let (from, to) = (g.model.nodes.get(edge.from as usize)?, g.model.nodes.get(edge.to as usize)?);
        tip = tip.child(
            row()
                .key(format!("link-{e}"))
                .gap(8.0)
                .center_y()
                .child(row().w(84.0).gap(6.0).center_y().child(el().size(12.0, 3.0).radius(1.5).fill(edge_color(edge.kind, p))).child(text(edge.kind.label()).font(SMALL).color(p.tx3)))
                .child(text(edge.kind.sentence(&from.name, &to.name, edge.count)).font(BODY).color(p.tx)),
        );
    }
    if edges.len() > MAX_LINES {
        tip = tip.child(text(format!("and {} more", edges.len() - MAX_LINES)).font(SMALL).color(p.tx3));
    }
    Some(tip)
}

/// The hovered link drawn over the rest, wider and in full colour, with both its artists'
/// dots lit and ringed.
fn highlight(g: &Graph, zoom: f32, p: &Palette) -> El {
    let [w, h] = g.size;
    let world = el().key("graph-lit").pin(Some(0.0), Some(0.0), None, None).size(w.max(1.0), h.max(1.0)).instant_changes();
    let Some(Hover::Link { a, b, edges }) = &g.hover else { return world };
    if g.world.len() != g.model.nodes.len() {
        return world;
    }
    let mut tiles = Tiles::new(w, h);
    let (pa, pb) = (g.world[*a], g.world[*b]);
    // Several kinds between the same two artists sit side by side, each its own colour.
    let width = 3.0 / zoom;
    let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let normal = [-dy / len, dx / len];
    for (k, &e) in edges.iter().enumerate() {
        let Some(edge) = g.model.edges.get(e) else { continue };
        let shift = (k as f32 - (edges.len() - 1) as f32 / 2.0) * width;
        let off = |q: [f32; 2]| [q[0] + normal[0] * shift, q[1] + normal[1] * shift];
        tiles.line(off(pa), off(pb), width, edge_color(edge.kind, p));
    }
    for &i in [a, b] {
        let r = radius(g.model.nodes[i].degree).max(1.0 / zoom);
        tiles.dot(g.world[i], r, p.tx);
        tiles.ring(g.world[i], r + 4.0 / zoom, 2.0 / zoom, p.tx);
    }
    tiles.flush(world, "lit")
}

/// The picked artist and those linked to it; empty with nothing picked.
fn near(g: &Graph) -> Vec<bool> {
    match g.selected {
        Some(f) => {
            let mut near = vec![false; g.model.nodes.len()];
            near[f] = true;
            for e in &g.model.edges {
                if e.from as usize == f {
                    near[e.to as usize] = true;
                }
                if e.to as usize == f {
                    near[e.from as usize] = true;
                }
            }
            near
        }
        None => Vec::new(),
    }
}

/// The edges and dots, in world points, drawn at `zoom`: lines and dots never get thinner than
/// a hairline on screen.
fn shapes(g: &Graph, zoom: f32, p: &Palette) -> El {
    let [w, h] = g.size;
    // A drop hands what moved back to this layer: it shows there at once, not animated in.
    let mut world = el().key("graph-shapes").pin(Some(0.0), Some(0.0), None, None).size(w.max(1.0), h.max(1.0)).instant_changes();
    if g.world.len() != g.model.nodes.len() {
        return world;
    }
    let focus = g.selected;
    let near = near(g);
    let dim = |i: u32| focus.is_some() && !near[i as usize];
    // What a drag moves draws on its own layer.
    let moves = |i: u32| g.drag.as_ref().is_some_and(|d| d.moving.contains(&i));
    let moving: Vec<bool> = (0..g.model.nodes.len() as u32).map(moves).collect();
    // Edges and dots draw in tiles, each sized to what it holds, so a zoomed-in camera skips
    // the tiles out of view.
    let mut tiles = Tiles::new(w, h);
    // Edges: one shape per kind, most numerous first so the rarer ones draw on top. Edges with
    // a focus draw faded unless they touch it.
    let mut kinds: Vec<(EdgeKind, usize)> = EdgeKind::ALL.into_iter().filter(|k| g.hidden & k.bit() == 0).map(|k| (k, g.model.edges.iter().filter(|e| e.kind == k).count())).filter(|(_, n)| *n > 0).collect();
    kinds.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (kind, count) in kinds {
        let color = edge_color(kind, p);
        let (faded, bright) = (color.alpha(0.06), color.alpha(if focus.is_some() { 0.9 } else { edge_alpha(count) }));
        for e in g.model.edges.iter().filter(|e| e.kind == kind && !moving[e.from as usize] && !moving[e.to as usize]) {
            let (a, b) = (g.world[e.from as usize], g.world[e.to as usize]);
            let touches = focus.is_none() || focus == Some(e.from as usize) || focus == Some(e.to as usize);
            let color = if dim(e.from) || dim(e.to) || !touches { faded } else { bright };
            tiles.line(a, b, edge_width(kind).max(0.9 / zoom), color);
        }
        // Each kind draws over the more numerous ones.
        world = tiles.flush(world, kind.id());
    }
    for (i, _) in g.model.nodes.iter().enumerate().filter(|&(i, _)| !moving[i]) {
        dot(&mut tiles, g, i, zoom, dim(i as u32), p);
    }
    tiles.flush(world, "dots")
}

/// How opaque a kind's lines draw, by how many there are: many lines fade so they don't swamp.
fn edge_alpha(count: usize) -> f32 {
    if count > 20000 {
        0.25
    } else if count > 3000 {
        0.4
    } else {
        0.6
    }
}

/// Artist `i`'s dot: ringed when picked, with a pin when pinned.
fn dot(tiles: &mut Tiles, g: &Graph, i: usize, zoom: f32, dim: bool, p: &Palette) {
    let r = radius(g.model.nodes[i].degree).max(1.0 / zoom);
    let c = g.world[i];
    let focus = g.selected;
    let color = if dim { p.ui2.alpha(0.25) } else if focus == Some(i) { p.accent } else { p.tx2 };
    tiles.dot(c, r, color);
    if focus == Some(i) {
        // A 2-point ring 3 points clear of the dot, on screen at any zoom.
        tiles.ring(c, r + 4.0 / zoom, 2.0 / zoom, p.accent);
    }
    if g.pinned.get(i).copied().unwrap_or(false) {
        // A pin's head just off the dot's top right, 2 points across on screen, outlined in the
        // background so it reads over lines and dots.
        let head = 2.0 / zoom;
        let k = std::f32::consts::FRAC_1_SQRT_2 * (r + head);
        let at = [c[0] + k, c[1] - k];
        tiles.dot(at, head + 1.0 / zoom, if dim { p.bg.alpha(0.4) } else { p.bg });
        tiles.dot(at, head, if dim { p.accent.alpha(0.3) } else { p.accent });
    }
}

/// While the drag simulation moves, an invisible element bound to its running pacer track,
/// which keeps the window drawing (and the simulation stepping) at the display's rate.
fn pacer(g: &Graph) -> Option<El> {
    let d = g.drag.as_ref().filter(|_| g.animating())?;
    Some(el().key("graph-pacer").pin(Some(0.0), Some(0.0), None, None).size(1.0, 1.0).live_opacity(&d.pacer).transition(Transition::none()))
}

/// What a drag moves: the moving artists and their links, as they are now.
fn moving(g: &Graph, zoom: f32, p: &Palette) -> El {
    let [w, h] = g.size;
    // The simulation is the animation: tiles land where each frame puts them, rather than
    // springing there (stretching their lines) or fading in.
    // And the layer goes the frame the drag ends, as the static layer takes what it held:
    // fading out, both would draw for a moment.
    let mut world = el()
        .key("graph-moving")
        .pin(Some(0.0), Some(0.0), None, None)
        .size(w.max(1.0), h.max(1.0))
        .instant_changes()
        .transition(Transition::none());
    let Some(d) = &g.drag else { return world };
    let focus = g.selected;
    let near = near(g);
    let dim = |i: u32| focus.is_some() && !near[i as usize];
    let mut tiles = Tiles::new(w, h);
    let mut counts = [0_usize; EdgeKind::ALL.len()];
    for e in &g.model.edges {
        if let Some(k) = EdgeKind::ALL.iter().position(|k| *k == e.kind) {
            counts[k] += 1;
        }
    }
    for &e in &d.edges {
        let Some(edge) = g.model.edges.get(e as usize) else { continue };
        if g.hidden & edge.kind.bit() != 0 {
            continue;
        }
        let touches = focus.is_none() || focus == Some(edge.from as usize) || focus == Some(edge.to as usize);
        let bright = if focus.is_some() { 0.9 } else { edge_alpha(EdgeKind::ALL.iter().position(|k| *k == edge.kind).map_or(0, |k| counts[k])) };
        let color = edge_color(edge.kind, p).alpha(if dim(edge.from) || dim(edge.to) || !touches { 0.06 } else { bright });
        tiles.line(g.world[edge.from as usize], g.world[edge.to as usize], edge_width(edge.kind).max(0.9 / zoom), color);
    }
    world = tiles.flush(world, "moving-links");
    for &i in &d.moving {
        dot(&mut tiles, g, i as usize, zoom, dim(i), p);
    }
    tiles.flush(world, "moving-dots")
}

/// The names of the artists in `rect` worth naming at `zoom`: the most linked, the big enough
/// dots, and the picked artist's neighbours.
///
/// Rune rasterizes text only at scales from 1/16 to 4, and stretches it beyond, so names drawn
/// small in world points and scaled up by the camera would blur when zoomed in. Instead they're
/// laid out at `zoom` (world points times `zoom`, at their on-screen size) in a layer scaled by
/// `1 / zoom`: under the camera they draw at between 0.84 and 1.19 times their size, as `zoom`
/// is the camera's rounded to a half power of two, and rasterize crisp.
fn names(g: &Graph, zoom: f32, rect: [f32; 4], p: &Palette) -> El {
    let [w, h] = g.size;
    let mut world = el().key("graph-names").pin(Some(0.0), Some(0.0), None, None).size((w * zoom).max(1.0), (h * zoom).max(1.0)).scale(1.0 / zoom).origin(0.0, 0.0)
        // A new zoom level lands at once: springing the scale would stretch the names.
        .instant_changes();
    if g.world.len() != g.model.nodes.len() {
        return world;
    }
    let focus = g.selected;
    let near = near(g);
    let in_rect = |[x, y]: [f32; 2]| x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3];
    let mut labelled: Vec<usize> = (0..g.model.nodes.len()).filter(|&i| in_rect(g.world[i]) && focus.is_none_or(|_| near[i])).collect();
    labelled.sort_by_key(|&i| std::cmp::Reverse(g.model.nodes[i].degree));
    let big = |i: usize| radius(g.model.nodes[i].degree) * 2.0 * zoom >= LABEL_FROM;
    let font = TextStyle::new(11.5, 500.0);
    // The most linked claim their room first; a name that would overlap one already placed
    // waits for a closer zoom. The picked artist's name always shows.
    let mut taken: Vec<[f32; 4]> = Vec::new();
    let mut shown = 0;
    for (rank, &i) in labelled.iter().enumerate() {
        if shown >= MAX_LABELS {
            break;
        }
        if focus.is_none() && rank >= ALWAYS_LABEL && !big(i) {
            continue;
        }
        let node = &g.model.nodes[i];
        let [x, y] = g.world[i];
        // In the layer's points: world points times `zoom`.
        let r = radius(node.degree).max(1.0 / zoom) * zoom;
        // A name's box, estimated from its length (measuring every name costs more).
        let (left, top) = (x * zoom + r + 3.0, y * zoom - 7.0);
        let b = [left, top, left + node.name.chars().count() as f32 * 6.6, top + 15.0];
        if focus != Some(i) && taken.iter().any(|t| b[0] < t[2] && t[0] < b[2] && b[1] < t[3] && t[1] < b[3]) {
            continue;
        }
        taken.push(b);
        shown += 1;
        world = world.child(
            text(node.name.clone())
                .key(format!("n{i}"))
                .font(font)
                .color(if focus == Some(i) { p.tx } else { p.tx2 })
                .pin(Some(left), Some(top), None, None)
                // A backdrop keeps the name readable over the lines it sits on.
                .px(2.0)
                .radius(2.0)
                .fill(p.bg.alpha(0.7)),
        );
    }
    world
}

/// Tiles per side of the world.
const TILES: usize = 8;

/// Lines and dots gathered by the tile their first point falls in, drawn as one canvas per tile
/// sized to what it holds, so the renderer skips the tiles out of view.
struct Tiles {
    cell: [f32; 2],
    tiles: Vec<(Vec<Mark>, [f32; 4])>,
}

enum Mark {
    Line([f32; 2], [f32; 2], f32, Color),
    Dot([f32; 2], f32, Color),
    /// Centre, radius, stroke width.
    Ring([f32; 2], f32, f32, Color),
}

impl Tiles {
    fn new(w: f32, h: f32) -> Tiles {
        Tiles { cell: [(w / TILES as f32).max(1.0), (h / TILES as f32).max(1.0)], tiles: (0..TILES * TILES).map(|_| (Vec::new(), EMPTY)).collect() }
    }

    fn add(&mut self, [x, y]: [f32; 2], mark: Mark) {
        let tx = ((x / self.cell[0]).max(0.0) as usize).min(TILES - 1);
        let ty = ((y / self.cell[1]).max(0.0) as usize).min(TILES - 1);
        let (marks, bounds) = &mut self.tiles[ty * TILES + tx];
        match mark {
            Mark::Line(a, b, w, _) => {
                grow(bounds, a, w);
                grow(bounds, b, w);
            }
            Mark::Dot(c, r, _) => grow(bounds, c, r + 1.0),
            Mark::Ring(c, r, w, _) => grow(bounds, c, r + w),
        }
        marks.push(mark);
    }

    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: Color) {
        self.add(a, Mark::Line(a, b, width, color));
    }

    fn dot(&mut self, c: [f32; 2], r: f32, color: Color) {
        self.add(c, Mark::Dot(c, r, color));
    }

    fn ring(&mut self, c: [f32; 2], r: f32, width: f32, color: Color) {
        self.add(c, Mark::Ring(c, r, width, color));
    }

    /// Adds the tiles holding anything to `world` as canvases, and empties them.
    fn flush(&mut self, mut world: El, id: &str) -> El {
        for (t, (marks, b)) in self.tiles.iter_mut().enumerate() {
            if marks.is_empty() {
                continue;
            }
            let [x0, y0, x1, y1] = std::mem::replace(b, EMPTY);
            let at = |[x, y]: [f32; 2]| [x - x0, y - y0];
            let shape = marks.drain(..).fold(Shape::new(), |shape, mark| match mark {
                Mark::Line(a, b, w, color) => shape.line(at(a), at(b), w, LineCap::Butt, color),
                // A round-capped line of no length is a disc the shader draws exactly, round at
                // any zoom, as one instance where a polygon takes a triangle per side.
                Mark::Dot(c, r, color) => shape.line(at(c), at(c), 2.0 * r, LineCap::Round, color),
                Mark::Ring(c, r, w, color) => shape.stroke_with(circle(at(c), r), true, w, LineCap::Round, color),
            });
            // A tile comes and goes as what it holds moves across tiles: it shows and leaves at
            // once, never fading.
            world = world.child(
                canvas(shape)
                    .key(format!("{id}-{t}"))
                    .pin(Some(x0), Some(y0), None, None)
                    .size((x1 - x0).max(1.0), (y1 - y0).max(1.0))
                    .transition(Transition::none()),
            );
        }
        world
    }
}

const EMPTY: [f32; 4] = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];

fn grow(b: &mut [f32; 4], [x, y]: [f32; 2], pad: f32) {
    b[0] = b[0].min(x - pad);
    b[1] = b[1].min(y - pad);
    b[2] = b[2].max(x + pad);
    b[3] = b[3].max(y + pad);
}

fn circle([cx, cy]: [f32; 2], r: f32) -> Vec<[f32; 2]> {
    // Only the picked artist's ring is a polygon: enough sides that it reads as round up close.
    (0..64)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 64.0;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

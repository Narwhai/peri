//! The artist graph: every artist in the library as a node, joined to the artists they've
//! worked with. Links come from the tags (featured artists, artists sharing a song, credits
//! such as "produced for") and from the relations the user records (merges, aliases, bands).
//!
//! Names are nodes as written, matched ignoring case: merged names stay separate nodes joined by
//! a [`EdgeKind::Merge`] edge. [`layout`] places them; [`state`] runs it off the UI thread and
//! caches the result beside the preferences.

pub mod layout;
pub mod state;

use std::collections::HashMap;

use crate::credits::Role;
use crate::library::Track;
use crate::relations::{LinkKind, Relations};

/// How two artists are linked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Two names of one artist (the user's merges).
    Merge,
    /// Another name an artist releases under.
    Alias,
    /// `from` is a member of the band `to`.
    MemberOf,
    /// `from` is featured on a song by `to`.
    Featured,
    /// Main artists of the same song ("A & B").
    With,
    /// `from` is credited with this role on a song by `to`: "produced for".
    Credit(Role),
}

impl EdgeKind {
    /// Every kind, in the order the legend lists them.
    pub const ALL: [EdgeKind; 16] = [
        EdgeKind::Merge,
        EdgeKind::Alias,
        EdgeKind::MemberOf,
        EdgeKind::Featured,
        EdgeKind::With,
        EdgeKind::Credit(Role::Composer),
        EdgeKind::Credit(Role::Lyricist),
        EdgeKind::Credit(Role::Writer),
        EdgeKind::Credit(Role::Producer),
        EdgeKind::Credit(Role::Arranger),
        EdgeKind::Credit(Role::Conductor),
        EdgeKind::Credit(Role::Performer),
        EdgeKind::Credit(Role::Engineer),
        EdgeKind::Credit(Role::Mixer),
        EdgeKind::Credit(Role::Remixer),
        EdgeKind::Credit(Role::DjMixer),
    ];

    /// A stable id, for keys and the saved preferences.
    pub fn id(self) -> &'static str {
        match self {
            EdgeKind::Merge => "merge",
            EdgeKind::Alias => "alias",
            EdgeKind::MemberOf => "member-of",
            EdgeKind::Featured => "featured",
            EdgeKind::With => "with",
            EdgeKind::Credit(Role::Composer) => "composer",
            EdgeKind::Credit(Role::Lyricist) => "lyricist",
            EdgeKind::Credit(Role::Writer) => "writer",
            EdgeKind::Credit(Role::Producer) => "producer",
            EdgeKind::Credit(Role::Arranger) => "arranger",
            EdgeKind::Credit(Role::Conductor) => "conductor",
            EdgeKind::Credit(Role::Performer) => "performer",
            EdgeKind::Credit(Role::Engineer) => "engineer",
            EdgeKind::Credit(Role::Mixer) => "mixer",
            EdgeKind::Credit(Role::Remixer) => "remixer",
            EdgeKind::Credit(Role::DjMixer) => "dj-mixer",
        }
    }

    pub fn from_id(id: &str) -> Option<EdgeKind> {
        EdgeKind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The legend's name for it.
    pub fn label(self) -> &'static str {
        match self {
            EdgeKind::Merge => "Same artist",
            EdgeKind::Alias => "Alias",
            EdgeKind::MemberOf => "Member of",
            EdgeKind::Featured => "Featured",
            EdgeKind::With => "Together",
            EdgeKind::Credit(r) => r.label(),
        }
    }

    /// What a link of this kind from `from` to `to` says, `count` being how many songs (or
    /// relations) say so: "Kenny Segal produced 2 songs by billy woods".
    pub fn sentence(self, from: &str, to: &str, count: u32) -> String {
        let songs = if count == 1 { "1 song".to_string() } else { format!("{count} songs") };
        let did = match self {
            EdgeKind::Merge => return format!("{from} and {to} are the same artist"),
            EdgeKind::Alias => return format!("{from} also releases as {to}"),
            EdgeKind::MemberOf => return format!("{from} is a member of {to}"),
            EdgeKind::Featured => return format!("{from} is featured on {songs} by {to}"),
            EdgeKind::With => return format!("{from} and {to} made {songs} together"),
            EdgeKind::Credit(Role::Composer) => "composed",
            EdgeKind::Credit(Role::Lyricist) => "wrote the lyrics of",
            EdgeKind::Credit(Role::Writer) => "wrote",
            EdgeKind::Credit(Role::Producer) => "produced",
            EdgeKind::Credit(Role::Arranger) => "arranged",
            EdgeKind::Credit(Role::Conductor) => "conducted",
            EdgeKind::Credit(Role::Performer) => "performed on",
            EdgeKind::Credit(Role::Engineer) => "engineered",
            EdgeKind::Credit(Role::Mixer) => "mixed",
            EdgeKind::Credit(Role::Remixer) => "remixed",
            EdgeKind::Credit(Role::DjMixer) => "DJ-mixed",
        };
        format!("{from} {did} {songs} by {to}")
    }

    /// Whether `from` and `to` read the same either way round.
    fn undirected(self) -> bool {
        matches!(self, EdgeKind::Merge | EdgeKind::Alias | EdgeKind::With)
    }

    /// Its bit in a set of kinds.
    pub fn bit(self) -> u32 {
        let at = EdgeKind::ALL.iter().position(|k| *k == self).unwrap_or(0);
        1 << at
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The name as first seen.
    pub name: String,
    /// How many other artists it's linked to, in any way.
    pub degree: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub kind: EdgeKind,
    /// How many songs (or relations) say so.
    pub count: u32,
}

/// The artists and their links. Node and edge order follows the library's, so the same
/// library always builds the same graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Lowercased, trimmed name to node.
    index: HashMap<String, u32>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

#[derive(Default)]
struct Builder {
    model: Model,
    edges: HashMap<(u32, u32, EdgeKind), usize>,
}

impl Builder {
    fn node(&mut self, name: &str) -> Option<u32> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let k = key(name);
        if let Some(&n) = self.model.index.get(&k) {
            return Some(n);
        }
        let n = self.model.nodes.len() as u32;
        self.model.nodes.push(Node { name: name.to_string(), degree: 0 });
        self.model.index.insert(k, n);
        Some(n)
    }

    fn edge(&mut self, from: &str, to: &str, kind: EdgeKind) {
        let (Some(a), Some(b)) = (self.node(from), self.node(to)) else { return };
        if a == b {
            return;
        }
        let (a, b) = if kind.undirected() && b < a { (b, a) } else { (a, b) };
        match self.edges.get(&(a, b, kind)) {
            Some(&at) => self.model.edges[at].count += 1,
            None => {
                self.edges.insert((a, b, kind), self.model.edges.len());
                self.model.edges.push(Edge { from: a, to: b, kind, count: 1 });
            }
        }
    }
}

impl Model {
    /// The graph of `tracks` and the user's `relations`.
    pub fn build(tracks: &[Track], relations: &Relations) -> Model {
        let mut b = Builder::default();
        for t in tracks {
            let main: Vec<&String> = t.artists.iter().filter(|a| !t.featured.contains(a)).collect();
            for a in t.artists.iter().chain(&t.album_artists) {
                b.node(a);
            }
            for (i, a) in main.iter().enumerate() {
                for other in &main[i + 1..] {
                    b.edge(a, other, EdgeKind::With);
                }
            }
            for f in &t.featured {
                for m in &main {
                    b.edge(f, m, EdgeKind::Featured);
                }
            }
            for c in &t.credits {
                // Credited on their own song: no link.
                for m in &main {
                    b.edge(&c.name, m, EdgeKind::Credit(c.role));
                }
                if main.is_empty() {
                    b.node(&c.name);
                }
            }
        }
        for merge in &relations.merges {
            if let Some((first, rest)) = merge.split_first() {
                for other in rest {
                    b.edge(first, other, EdgeKind::Merge);
                }
            }
        }
        for link in &relations.links {
            let kind = match link.kind {
                LinkKind::Alias => EdgeKind::Alias,
                LinkKind::MemberOf => EdgeKind::MemberOf,
            };
            b.edge(&link.from, &link.to, kind);
        }
        let mut model = b.model;
        let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); model.nodes.len()];
        for e in &model.edges {
            neighbours[e.from as usize].push(e.to);
            neighbours[e.to as usize].push(e.from);
        }
        for (node, mut list) in model.nodes.iter_mut().zip(neighbours) {
            list.sort_unstable();
            list.dedup();
            node.degree = list.len() as u32;
        }
        model
    }

    /// The node named `name`, ignoring case.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.index.get(&key(name)).map(|&n| n as usize)
    }

    /// The best match for a typed `query`: the name itself, else one starting with it, else one
    /// containing it, the most linked first.
    pub fn search(&self, query: &str) -> Option<usize> {
        let q = key(query);
        if q.is_empty() {
            return None;
        }
        if let Some(n) = self.find(&q) {
            return Some(n);
        }
        let best = |pred: &dyn Fn(&str) -> bool| {
            self.nodes.iter().enumerate().filter(|(_, n)| pred(&key(&n.name))).max_by_key(|(i, n)| (n.degree, std::cmp::Reverse(*i))).map(|(i, _)| i)
        };
        best(&|n| n.starts_with(&q)).or_else(|| best(&|n| n.contains(&q)))
    }

    /// The pairs of nodes linked in any way, each once: what the layout pulls together.
    pub fn links(&self) -> Vec<(u32, u32)> {
        let mut pairs: Vec<(u32, u32)> = self.edges.iter().map(|e| (e.from.min(e.to), e.from.max(e.to))).collect();
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }

    /// Identifies what the layout depends on (the names and which pairs are linked), so a cached
    /// layout is reused until those change. Stable across runs and builds (FNV-1a).
    pub fn fingerprint(&self) -> u64 {
        let mut h = Fnv::new();
        let mut names: Vec<String> = self.nodes.iter().map(|n| key(&n.name)).collect();
        names.sort_unstable();
        for n in &names {
            h.write(n.as_bytes());
            h.write(&[0]);
        }
        let mut pairs: Vec<(&str, &str)> = self
            .links()
            .into_iter()
            .map(|(a, b)| {
                let (a, b) = (self.nodes[a as usize].name.as_str(), self.nodes[b as usize].name.as_str());
                if key(a) <= key(b) { (a, b) } else { (b, a) }
            })
            .collect();
        pairs.sort_unstable_by_key(|(a, b)| (key(a), key(b)));
        for (a, b) in pairs {
            h.write(key(a).as_bytes());
            h.write(&[1]);
            h.write(key(b).as_bytes());
            h.write(&[2]);
        }
        h.0
    }
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests;

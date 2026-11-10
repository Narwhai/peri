//! How artists relate, as the user records them under `artists` in `prefs.json`: names that
//! are one artist (merged onto one page), and links between artists (aliases, band members).
//! Names match ignoring case and surrounding spaces.
//!
//! ```json
//! "artists": {
//!   "merges": [["Earl Sweatshirt", "Thebe Kgositsile"]],
//!   "links": [
//!     { "from": "MF DOOM", "kind": "alias", "to": "Viktor Vaughn" },
//!     { "from": "Thom Yorke", "kind": "member_of", "to": "Radiohead" }
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relations {
    /// Each list is one artist under several names: their pages show as one.
    #[serde(default)]
    pub merges: Vec<Vec<String>>,
    #[serde(default)]
    pub links: Vec<Link>,
}

/// Two artists that keep their own pages, each listing the other as related.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub kind: LinkKind,
    pub to: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// The same person releasing as someone else, either way round.
    Alias,
    /// `from` is a member of the band `to`.
    MemberOf,
}

/// How a related artist relates to the one whose page lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// Another name they release under.
    Alias,
    /// A band they're in.
    MemberOf,
    /// A member of their band.
    Member,
}

impl Relation {
    pub const ALL: [Relation; 3] = [Relation::Alias, Relation::MemberOf, Relation::Member];

    /// The heading their names list under on an artist page.
    pub fn title(self) -> &'static str {
        match self {
            Relation::Alias => "Also releases as",
            Relation::MemberOf => "Member of",
            Relation::Member => "Members",
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn among(names: &[String], name: &str) -> bool {
    names.iter().any(|n| same(n, name))
}

impl Relations {
    /// Every name `name` is merged with, `name` first, through merges that share a name;
    /// just `name` when it's in none.
    pub fn merged(&self, name: &str) -> Vec<String> {
        let mut names = vec![name.trim().to_string()];
        loop {
            let before = names.len();
            for merge in &self.merges {
                if !merge.iter().any(|n| among(&names, n)) {
                    continue;
                }
                for n in merge {
                    if !among(&names, n) {
                        names.push(n.trim().to_string());
                    }
                }
            }
            if names.len() == before {
                return names;
            }
        }
    }

    /// Every name of the artist `name` and of the artists they release as, through aliases of
    /// aliases and the merges of each, `name`'s own merged names first.
    pub fn with_aliases(&self, name: &str) -> Vec<String> {
        let mut names = self.merged(name);
        let mut at = 0;
        while at < names.len() {
            for link in self.links.iter().filter(|l| l.kind == LinkKind::Alias) {
                let other = if same(&link.from, &names[at]) {
                    &link.to
                } else if same(&link.to, &names[at]) {
                    &link.from
                } else {
                    continue;
                };
                for n in self.merged(other) {
                    if !among(&names, &n) {
                        names.push(n);
                    }
                }
            }
            at += 1;
        }
        names
    }

    /// The artists linked to any of `names` (one artist's merged names), each once, in the
    /// order the links list them; none of `names` themselves.
    pub fn related(&self, names: &[String]) -> Vec<(Relation, String)> {
        let mut out: Vec<(Relation, String)> = Vec::new();
        for link in &self.links {
            let (from, to) = (among(names, &link.from), among(names, &link.to));
            let found = match (link.kind, from, to) {
                (_, true, true) | (_, false, false) => None,
                (LinkKind::Alias, true, false) => Some((Relation::Alias, &link.to)),
                (LinkKind::Alias, false, true) => Some((Relation::Alias, &link.from)),
                (LinkKind::MemberOf, true, false) => Some((Relation::MemberOf, &link.to)),
                (LinkKind::MemberOf, false, true) => Some((Relation::Member, &link.from)),
            };
            if let Some((relation, name)) = found
                && !out.iter().any(|(r, n)| *r == relation && same(n, name))
            {
                out.push((relation, name.trim().to_string()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn link(from: &str, kind: LinkKind, to: &str) -> Link {
        Link { from: from.into(), kind, to: to.into() }
    }

    #[test]
    fn a_name_merges_with_every_name_of_its_artist_even_across_merges() {
        let r = Relations { merges: vec![strings(&["Earl Sweatshirt", "Thebe Kgositsile"]), strings(&["thebe kgositsile", "randomblackdude"])], links: Vec::new() };
        assert_eq!(r.merged(" Thebe Kgositsile "), ["Thebe Kgositsile", "Earl Sweatshirt", "randomblackdude"], "the asked name first, then the rest");
        assert_eq!(r.merged("Somebody"), ["Somebody"]);
    }

    #[test]
    fn links_relate_both_ways_and_membership_reads_by_side() {
        let r = Relations {
            merges: vec![strings(&["MF DOOM", "Daniel Dumile"])],
            links: vec![
                link("MF DOOM", LinkKind::Alias, "Viktor Vaughn"),
                link("daniel dumile", LinkKind::MemberOf, "KMD"),
                link("Viktor Vaughn", LinkKind::Alias, "mf doom"),
                link("MF DOOM", LinkKind::Alias, "Daniel Dumile"),
            ],
        };
        assert_eq!(
            r.related(&r.merged("MF DOOM")),
            [(Relation::Alias, "Viktor Vaughn".to_string()), (Relation::MemberOf, "KMD".to_string())],
            "a link from any merged name counts once, and none between their own names"
        );
        assert_eq!(r.related(&r.merged("Viktor Vaughn")), [(Relation::Alias, "MF DOOM".to_string())]);
        assert_eq!(r.related(&r.merged("KMD")), [(Relation::Member, "daniel dumile".to_string())]);
    }

    #[test]
    fn aliases_gather_through_aliases_of_aliases_and_their_merges_but_not_bands() {
        let r = Relations {
            merges: vec![strings(&["Viktor Vaughn", "Victor Vaughn"])],
            links: vec![link("MF DOOM", LinkKind::Alias, "Viktor Vaughn"), link("victor vaughn", LinkKind::Alias, "King Geedorah"), link("MF DOOM", LinkKind::MemberOf, "KMD")],
        };
        assert_eq!(r.with_aliases("mf doom"), ["mf doom", "Viktor Vaughn", "Victor Vaughn", "King Geedorah"]);
        assert_eq!(r.with_aliases("King Geedorah")[0], "King Geedorah");
        assert_eq!(r.with_aliases("KMD"), ["KMD"]);
    }

    #[test]
    fn reads_the_prefs_shape_and_rejects_typos() {
        let json = r#"{ "merges": [["A", "B"]], "links": [{ "from": "A", "kind": "member_of", "to": "Band" }] }"#;
        let r: Relations = serde_json::from_str(json).expect("valid");
        assert_eq!(r.links, [link("A", LinkKind::MemberOf, "Band")]);
        assert!(serde_json::from_str::<Relations>(r#"{ "links": [{ "from": "A", "kind": "member", "to": "B" }] }"#).is_err());
        assert!(serde_json::from_str::<Relations>(r#"{ "merge": [] }"#).is_err());
    }
}

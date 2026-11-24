//! How artists relate, as the user records them under `artists` in `prefs.json`: names that
//! are one artist (merged onto one page), and links between artists (aliases, band members).
//! Names match ignoring case and surrounding spaces.
//!
//! ```json
//! "artists": {
//!   "merges": [["Earl Sweatshirt", "Thebe Kgositsile"]],
//!   "links": [
//!     { "from": "MF DOOM", "kind": "alias", "to": "Viktor Vaughn" },
//!     { "from": "Thom Yorke", "kind": "member_of", "to": "Radiohead" },
//!     { "from": "Ryan Walchonski", "kind": "former_member_of", "to": "feeble little horse" }
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
    /// `from` used to be a member of the band `to`.
    FormerMemberOf,
}

/// What the Link artist popover adds between the open artist and another name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkChoice {
    /// The other name is the same artist: merge it into their page.
    Merge,
    /// The other name is another name they release as.
    Alias,
    /// They're a member of the other name's band.
    MemberOf,
    /// They used to be a member of the other name's band.
    FormerMemberOf,
    /// The other name is a member of their band.
    HasMember,
    /// The other name used to be a member of their band.
    HadMember,
}

impl LinkChoice {
    pub const ALL: [LinkChoice; 6] = [LinkChoice::Merge, LinkChoice::Alias, LinkChoice::MemberOf, LinkChoice::FormerMemberOf, LinkChoice::HasMember, LinkChoice::HadMember];

    pub fn label(self) -> &'static str {
        match self {
            LinkChoice::Merge => "Same artist",
            LinkChoice::Alias => "Alias",
            LinkChoice::MemberOf => "Member of",
            LinkChoice::FormerMemberOf => "Former member of",
            LinkChoice::HasMember => "Has member",
            LinkChoice::HadMember => "Had member",
        }
    }

    /// A stable id, for keys.
    pub fn id(self) -> &'static str {
        match self {
            LinkChoice::Merge => "merge",
            LinkChoice::Alias => "alias",
            LinkChoice::MemberOf => "member-of",
            LinkChoice::FormerMemberOf => "former-member-of",
            LinkChoice::HasMember => "has-member",
            LinkChoice::HadMember => "had-member",
        }
    }

    /// What adding a name does, for the popover.
    pub fn explain(self) -> &'static str {
        match self {
            LinkChoice::Merge => "One page for both names: everything credited to either shows together.",
            LinkChoice::Alias => "Another name they release as. Each keeps its own page and lists the other.",
            LinkChoice::MemberOf => "A band they're a member of.",
            LinkChoice::FormerMemberOf => "A band they used to be in.",
            LinkChoice::HasMember => "A member of their band.",
            LinkChoice::HadMember => "A former member of their band.",
        }
    }
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
    /// A band they used to be in.
    FormerMemberOf,
    /// A former member of their band.
    FormerMember,
}

impl Relation {
    pub const ALL: [Relation; 5] = [Relation::Alias, Relation::MemberOf, Relation::FormerMemberOf, Relation::Member, Relation::FormerMember];

    /// A stable id, for keys.
    pub fn id(self) -> &'static str {
        match self {
            Relation::Alias => "alias",
            Relation::MemberOf => "member-of",
            Relation::Member => "member",
            Relation::FormerMemberOf => "former-member-of",
            Relation::FormerMember => "former-member",
        }
    }

    /// The heading their names list under on an artist page.
    pub fn title(self) -> &'static str {
        match self {
            Relation::Alias => "Also releases as",
            Relation::MemberOf => "Member of",
            Relation::Member => "Members",
            Relation::FormerMemberOf => "Former member of",
            Relation::FormerMember => "Former members",
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
                (LinkKind::FormerMemberOf, true, false) => Some((Relation::FormerMemberOf, &link.to)),
                (LinkKind::FormerMemberOf, false, true) => Some((Relation::FormerMember, &link.from)),
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

impl Relations {
    /// The artists in a band with `name` (any of their merged names) or members of one: one
    /// membership step out, each once, in link order. With `with_aliases`, starts from every
    /// name they release as. Never their own names, nor artists merged with them, and never
    /// the bands or members of those artists.
    pub fn involved(&self, name: &str, with_aliases: bool) -> Vec<(Relation, String)> {
        let starts = if with_aliases { self.with_aliases(name) } else { self.merged(name) };
        let mut out: Vec<(Relation, String)> = Vec::new();
        for (relation, other) in self.related(&starts) {
            if relation == Relation::Alias {
                continue;
            }
            let group = self.merged(&other);
            let is_ours = group.iter().any(|n| among(&starts, n));
            let seen = out.iter().any(|(_, n)| among(&group, n));
            if !is_ours && !seen {
                out.push((relation, other));
            }
        }
        out
    }

    /// Records `choice` between the artist `name` (any of their merged names) and `other`.
    /// Errs when `other` is blank, already one of their names, or already related that way.
    pub fn add(&mut self, name: &str, choice: LinkChoice, other: &str) -> Result<(), String> {
        let other = other.trim();
        if other.is_empty() {
            return Err("Type an artist's name to link".into());
        }
        let names = self.merged(name);
        if among(&names, other) {
            return Err(format!("{other} is already one of this artist's names"));
        }
        let relation = match choice {
            LinkChoice::Merge => None,
            LinkChoice::Alias => Some(Relation::Alias),
            LinkChoice::MemberOf => Some(Relation::MemberOf),
            LinkChoice::FormerMemberOf => Some(Relation::FormerMemberOf),
            LinkChoice::HasMember => Some(Relation::Member),
            LinkChoice::HadMember => Some(Relation::FormerMember),
        };
        if let Some(relation) = relation
            && self.related(&names).iter().any(|(r, n)| *r == relation && same(n, other))
        {
            return Err(format!("{other} is already linked as {}", choice.label().to_lowercase()));
        }
        let name = name.trim().to_string();
        let other = other.to_string();
        match choice {
            LinkChoice::Merge => match self.merges.iter_mut().find(|m| among(m, &name)) {
                Some(merge) => merge.push(other),
                None => self.merges.push(vec![name, other]),
            },
            LinkChoice::Alias => self.links.push(Link { from: name, kind: LinkKind::Alias, to: other }),
            LinkChoice::MemberOf => self.add_membership(&names, name, other, true, true),
            LinkChoice::FormerMemberOf => self.add_membership(&names, name, other, false, true),
            LinkChoice::HasMember => self.add_membership(&names, name, other, true, false),
            LinkChoice::HadMember => self.add_membership(&names, name, other, false, false),
        }
        Ok(())
    }

    /// Adds a current (`current`) or former membership between the artist and `other`, the artist
    /// the member when `artist_is_member`, else the band. Replaces the other kind the same way round.
    fn add_membership(&mut self, names: &[String], name: String, other: String, current: bool, artist_is_member: bool) {
        let (kind, replaced) = if current { (LinkKind::MemberOf, LinkKind::FormerMemberOf) } else { (LinkKind::FormerMemberOf, LinkKind::MemberOf) };
        self.links.retain(|l| {
            let hit = if artist_is_member { among(names, &l.from) && same(&l.to, &other) } else { among(names, &l.to) && same(&l.from, &other) };
            !(l.kind == replaced && hit)
        });
        let (from, to) = if artist_is_member { (name, other) } else { (other, name) };
        self.links.push(Link { from, kind, to });
    }

    /// Removes every link that makes `other` related to the artist with these merged `names`
    /// as `relation`.
    pub fn unlink(&mut self, names: &[String], relation: Relation, other: &str) {
        self.links.retain(|l| {
            let ours = |a: &str, b: &str| among(names, a) && same(b, other);
            let hit = match (relation, l.kind) {
                (Relation::Alias, LinkKind::Alias) => ours(&l.from, &l.to) || ours(&l.to, &l.from),
                (Relation::MemberOf, LinkKind::MemberOf) => ours(&l.from, &l.to),
                (Relation::Member, LinkKind::MemberOf) => ours(&l.to, &l.from),
                (Relation::FormerMemberOf, LinkKind::FormerMemberOf) => ours(&l.from, &l.to),
                (Relation::FormerMember, LinkKind::FormerMemberOf) => ours(&l.to, &l.from),
                _ => false,
            };
            !hit
        });
    }

    /// Takes `name` out of every merge, dropping merges left with one name.
    pub fn unmerge(&mut self, name: &str) {
        for merge in &mut self.merges {
            merge.retain(|n| !same(n, name));
        }
        self.merges.retain(|m| m.len() > 1);
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
    fn adding_merges_into_an_existing_merge_and_links_each_way() {
        let mut r = Relations::default();
        r.add("Earl Sweatshirt", LinkChoice::Merge, " Thebe Kgositsile ").expect("merge");
        r.add("thebe kgositsile", LinkChoice::Merge, "randomblackdude").expect("joins their merge");
        assert_eq!(r.merges, [strings(&["Earl Sweatshirt", "Thebe Kgositsile", "randomblackdude"])]);
        r.add("Earl Sweatshirt", LinkChoice::HasMember, "Nobody").expect("member");
        r.add("Earl Sweatshirt", LinkChoice::MemberOf, "Odd Future").expect("band");
        r.add("Earl Sweatshirt", LinkChoice::Alias, "Other Name").expect("alias");
        assert_eq!(r.links, [link("Nobody", LinkKind::MemberOf, "Earl Sweatshirt"), link("Earl Sweatshirt", LinkKind::MemberOf, "Odd Future"), link("Earl Sweatshirt", LinkKind::Alias, "Other Name")]);
        assert!(r.add("Earl Sweatshirt", LinkChoice::Alias, "  ").is_err());
        assert!(r.add("Earl Sweatshirt", LinkChoice::Alias, "RANDOMBLACKDUDE").is_err(), "already their name");
        assert!(r.add("randomblackdude", LinkChoice::Alias, "other name").is_err(), "already linked, from any of their names");
        assert!(r.add("Odd Future", LinkChoice::HasMember, "earl sweatshirt").is_err(), "the same link from the other side");
        assert_eq!(r.links.len(), 3);
    }

    #[test]
    fn unlinking_and_unmerging_remove_only_that_name() {
        let mut r = Relations {
            merges: vec![strings(&["A", "B", "C"]), strings(&["X", "y"])],
            links: vec![link("B", LinkKind::Alias, "Z"), link("z", LinkKind::Alias, "c"), link("A", LinkKind::MemberOf, "Band"), link("Kid", LinkKind::MemberOf, "A"), link("Q", LinkKind::Alias, "Z")],
        };
        let names = r.merged("A");
        r.unlink(&names, Relation::Alias, "Z");
        r.unlink(&names, Relation::Member, "kid");
        assert_eq!(r.links, [link("A", LinkKind::MemberOf, "Band"), link("Q", LinkKind::Alias, "Z")]);
        r.unlink(&names, Relation::Member, "Band");
        assert_eq!(r.links.len(), 2, "Band is a band they're in, not a member");
        r.unmerge("b");
        r.unmerge("Y");
        assert_eq!(r.merges, [strings(&["A", "C"])]);
    }

    #[test]
    fn reads_the_prefs_shape_and_rejects_typos() {
        let json = r#"{ "merges": [["A", "B"]], "links": [{ "from": "A", "kind": "member_of", "to": "Band" }] }"#;
        let r: Relations = serde_json::from_str(json).expect("valid");
        assert_eq!(r.links, [link("A", LinkKind::MemberOf, "Band")]);
        assert!(serde_json::from_str::<Relations>(r#"{ "links": [{ "from": "A", "kind": "member", "to": "B" }] }"#).is_err());
        assert!(serde_json::from_str::<Relations>(r#"{ "merge": [] }"#).is_err());
    }

    #[test]
    fn involved_is_one_membership_step_out_each_way() {
        let r = Relations {
            merges: vec![strings(&["Radiohead", "RH"])],
            links: vec![
                link("Thom Yorke", LinkKind::MemberOf, "Radiohead"),
                link("Thom Yorke", LinkKind::MemberOf, "rh"),
                link("Jonny Greenwood", LinkKind::MemberOf, "Radiohead"),
                link("Jonny Greenwood", LinkKind::MemberOf, "Other Band"),
            ],
        };
        assert_eq!(r.involved("Thom Yorke", false), [(Relation::MemberOf, "Radiohead".to_string())], "a band once, though linked under both merged names");
        assert_eq!(
            r.involved("Radiohead", false),
            [(Relation::Member, "Thom Yorke".to_string()), (Relation::Member, "Jonny Greenwood".to_string())],
            "members, not the other bands of their members"
        );
        assert!(r.involved("Other Band", false).iter().all(|(_, n)| n == "Jonny Greenwood"));
        assert!(r.involved("Nobody", false).is_empty());
    }

    #[test]
    fn involved_follows_aliases_only_when_asked_and_skips_their_own_names() {
        let r = Relations {
            merges: Vec::new(),
            links: vec![
                link("MF DOOM", LinkKind::Alias, "Viktor Vaughn"),
                link("Viktor Vaughn", LinkKind::MemberOf, "X"),
                link("Viktor Vaughn", LinkKind::MemberOf, "MF DOOM"),
            ],
        };
        assert_eq!(r.involved("MF DOOM", false), [(Relation::Member, "Viktor Vaughn".to_string())], "Viktor is a member of MF DOOM; X is only Viktor's band");
        assert_eq!(r.involved("MF DOOM", true), [(Relation::MemberOf, "X".to_string())], "with aliases, Viktor is one of their names, so their band is in and Viktor isn't");
    }

    #[test]
    fn former_membership_reads_from_either_side_and_the_prefs_shape() {
        let r: Relations = serde_json::from_str(r#"{ "links": [{ "from": "Ryan Walchonski", "kind": "former_member_of", "to": "feeble little horse" }] }"#).expect("valid");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse")]);
        assert_eq!(r.related(&r.merged("Ryan Walchonski")), [(Relation::FormerMemberOf, "feeble little horse".to_string())]);
        assert_eq!(r.related(&r.merged("feeble little horse")), [(Relation::FormerMember, "Ryan Walchonski".to_string())]);
    }

    #[test]
    fn adding_former_and_had_members_records_them_in_their_direction() {
        let mut r = Relations::default();
        r.add("Ryan Walchonski", LinkChoice::FormerMemberOf, "feeble little horse").expect("former band");
        r.add("Aunt Katrina", LinkChoice::HadMember, "Someone").expect("former member");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse"), link("Someone", LinkKind::FormerMemberOf, "Aunt Katrina")]);
        assert!(r.add("ryan walchonski", LinkChoice::FormerMemberOf, "Feeble Little Horse").is_err(), "already linked that way");
    }

    #[test]
    fn switching_between_current_and_former_membership_replaces_the_link_in_that_direction() {
        let mut r = Relations::default();
        r.add("Ryan Walchonski", LinkChoice::MemberOf, "feeble little horse").expect("member");
        r.add("Ryan Walchonski", LinkChoice::FormerMemberOf, "feeble little horse").expect("switched to former");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse")]);
        r.add("Ryan Walchonski", LinkChoice::MemberOf, "feeble little horse").expect("switched back to current");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::MemberOf, "feeble little horse")]);
        r.add("feeble little horse", LinkChoice::HadMember, "Ryan Walchonski").expect("from the band's side");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse")], "the same direction, so replaced");
    }

    #[test]
    fn a_membership_the_other_way_round_is_not_replaced() {
        let mut r = Relations { merges: Vec::new(), links: vec![link("feeble little horse", LinkKind::MemberOf, "Ryan Walchonski")] };
        r.add("Ryan Walchonski", LinkChoice::FormerMemberOf, "feeble little horse").expect("the reverse direction is a different link");
        assert_eq!(r.links, [link("feeble little horse", LinkKind::MemberOf, "Ryan Walchonski"), link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse")]);
    }

    #[test]
    fn unlinking_former_memberships_removes_them_from_either_side() {
        let mut r = Relations { merges: Vec::new(), links: vec![link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse"), link("Ryan Walchonski", LinkKind::MemberOf, "Aunt Katrina")] };
        r.unlink(&r.merged("feeble little horse"), Relation::FormerMember, "ryan walchonski");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::MemberOf, "Aunt Katrina")]);
        r.unlink(&r.merged("Ryan Walchonski"), Relation::FormerMemberOf, "Aunt Katrina");
        assert_eq!(r.links.len(), 1, "a current band isn't a former one");
        r.unlink(&r.merged("Ryan Walchonski"), Relation::FormerMemberOf, "feeble little horse");
        assert_eq!(r.links, [link("Ryan Walchonski", LinkKind::MemberOf, "Aunt Katrina")], "the band's member link stays");
    }

    #[test]
    fn involved_includes_former_bands_and_former_members() {
        let r = Relations { merges: Vec::new(), links: vec![link("Ryan Walchonski", LinkKind::FormerMemberOf, "feeble little horse")] };
        assert_eq!(r.involved("Ryan Walchonski", false), [(Relation::FormerMemberOf, "feeble little horse".to_string())]);
        assert_eq!(r.involved("feeble little horse", false), [(Relation::FormerMember, "Ryan Walchonski".to_string())]);
    }
}

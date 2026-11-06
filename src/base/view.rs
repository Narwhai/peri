//! A view's settings, and evaluating one over the library: filter, sort, then group.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use serde::{Deserialize, Serialize};

use super::filter::Group;
use super::property::{Property, Value};
use crate::library::Track;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Asc,
    Desc,
}

impl Direction {
    pub fn flip(self) -> Direction {
        match self {
            Direction::Asc => Direction::Desc,
            Direction::Desc => Direction::Asc,
        }
    }

    fn apply(self, ord: Ordering) -> Ordering {
        match self {
            Direction::Asc => ord,
            Direction::Desc => ord.reverse(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub property: Property,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupBy {
    pub property: Property,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    #[serde(default)]
    pub filter: Group,
    /// Applied in order; ties fall back to library order.
    #[serde(default)]
    pub sorts: Vec<Sort>,
    #[serde(default)]
    pub group_by: Option<GroupBy>,
}

impl View {
    pub fn new(name: impl Into<String>) -> View {
        View { name: name.into(), filter: Group::default(), sorts: Vec::new(), group_by: None }
    }
}

/// One section of a grouped view.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// `None` for the songs that have no value for the grouped property.
    pub value: Value,
    pub title: String,
    /// Indices into the library, sorted.
    pub tracks: Vec<usize>,
}

/// A view evaluated over the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evaluated {
    /// Every matching song, once, in play order: sorted, and by section when grouped.
    pub order: Vec<usize>,
    /// Empty when the view isn't grouped.
    pub sections: Vec<Section>,
}

/// Applies `view` and the search `query` (lowercased) to `tracks`, which are in library order.
pub fn evaluate(view: &View, tracks: &[Track], query: &str) -> Evaluated {
    let mut matching: Vec<usize> = (0..tracks.len()).filter(|&i| tracks[i].matches(query) && view.filter.matches(&tracks[i])).collect();
    if !view.sorts.is_empty() {
        let keys: Vec<Vec<Value>> = matching.iter().map(|&i| view.sorts.iter().map(|s| s.property.value(&tracks[i])).collect()).collect();
        let mut order: Vec<usize> = (0..matching.len()).collect();
        // Stable, so ties keep library order.
        order.sort_by(|&a, &b| {
            view.sorts.iter().enumerate().fold(Ordering::Equal, |ord, (k, sort)| {
                ord.then_with(|| compare_with_none_last(&keys[a][k], &keys[b][k], sort.direction))
            })
        });
        matching = order.into_iter().map(|j| matching[j]).collect();
    }
    let Some(group_by) = view.group_by else {
        return Evaluated { order: matching, sections: Vec::new() };
    };
    let mut sections: Vec<Section> = Vec::new();
    let mut index: HashMap<GroupKey, usize> = HashMap::new();
    for &i in &matching {
        for value in group_by.property.group_keys(&tracks[i]) {
            match index.entry(GroupKey::of(&value)) {
                Entry::Occupied(at) => sections[*at.get()].tracks.push(i),
                Entry::Vacant(slot) => {
                    slot.insert(sections.len());
                    sections.push(Section { title: group_by.property.group_title(&value), value, tracks: vec![i] });
                }
            }
        }
    }
    sections.sort_by(|a, b| compare_with_none_last(&a.value, &b.value, group_by.direction));
    // A song in several groups plays once, in its first group.
    let mut seen = vec![false; tracks.len()];
    let order = sections.iter().flat_map(|s| s.tracks.iter().copied()).filter(|&i| !std::mem::replace(&mut seen[i], true)).collect();
    Evaluated { order, sections }
}

/// What makes two values one group: text ignoring case, numbers by their bits.
#[derive(PartialEq, Eq, Hash)]
enum GroupKey {
    None,
    Text(String),
    Number(u64),
}

impl GroupKey {
    fn of(v: &Value) -> GroupKey {
        match v {
            Value::None => GroupKey::None,
            Value::Text(s) => GroupKey::Text(s.to_lowercase()),
            Value::List(items) => GroupKey::Text(items.join("\u{1f}").to_lowercase()),
            Value::Number(n) => GroupKey::Number(n.to_bits()),
        }
    }
}

/// Sorts by `direction`, with songs missing the value last either way.
fn compare_with_none_last(a: &Value, b: &Value, direction: Direction) -> Ordering {
    match (a.is_none(), b.is_none()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => direction.apply(a.compare(b)),
    }
}

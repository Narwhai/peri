//! Filters: a group's conjunction (all, any or none of the following are true) over
//! conditions and nested groups.

use serde::{Deserialize, Serialize};

use super::property::{Kind, Property, Record, Value, parse_date};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conjunction {
    /// All the following are true.
    #[default]
    And,
    /// Any of the following are true.
    Or,
    /// None of the following are true.
    Not,
}

impl Conjunction {
    pub const ALL: [Conjunction; 3] = [Conjunction::And, Conjunction::Or, Conjunction::Not];

    pub fn label(self) -> &'static str {
        match self {
            Conjunction::And => "All the following are true",
            Conjunction::Or => "Any of the following are true",
            Conjunction::Not => "None of the following are true",
        }
    }

    pub fn from_label(label: &str) -> Option<Conjunction> {
        Conjunction::ALL.into_iter().find(|c| c.label() == label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Is,
    IsNot,
    Contains,
    DoesNotContain,
    StartsWith,
    EndsWith,
    /// For lists: one of the items is the value.
    Has,
    /// For lists: none of the items is the value.
    HasNot,
    Lt,
    Le,
    Gt,
    Ge,
    IsEmpty,
    IsNotEmpty,
}

impl Operator {
    /// The operators a property's kind offers, the first being the default.
    pub fn for_kind(kind: Kind) -> &'static [Operator] {
        use Operator::*;
        match kind {
            Kind::Text => &[Is, IsNot, Contains, DoesNotContain, StartsWith, EndsWith, IsEmpty, IsNotEmpty],
            Kind::List => &[Has, HasNot, Contains, DoesNotContain, IsEmpty, IsNotEmpty],
            Kind::Number | Kind::Duration | Kind::Date => &[Is, IsNot, Lt, Le, Gt, Ge, IsEmpty, IsNotEmpty],
        }
    }

    pub fn label(self, kind: Kind) -> &'static str {
        use Operator::*;
        match (self, kind) {
            (Lt, Kind::Date) => "is before",
            (Le, Kind::Date) => "is on or before",
            (Gt, Kind::Date) => "is after",
            (Ge, Kind::Date) => "is on or after",
            (Is, _) => "is",
            (IsNot, _) => "is not",
            (Contains, _) => "contains",
            (DoesNotContain, _) => "does not contain",
            (StartsWith, _) => "starts with",
            (EndsWith, _) => "ends with",
            (Has, _) => "has",
            (HasNot, _) => "does not have",
            (Lt, _) => "<",
            (Le, _) => "≤",
            (Gt, _) => ">",
            (Ge, _) => "≥",
            (IsEmpty, _) => "is empty",
            (IsNotEmpty, _) => "is not empty",
        }
    }

    pub fn from_label(kind: Kind, label: &str) -> Option<Operator> {
        Operator::for_kind(kind).iter().copied().find(|o| o.label(kind) == label)
    }

    /// Whether the operator compares against a value the user types.
    pub fn takes_value(self) -> bool {
        !matches!(self, Operator::IsEmpty | Operator::IsNotEmpty)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub property: Property,
    pub op: Operator,
    /// As typed: text, a number, `m:ss` for durations or `YYYY-MM-DD` for dates.
    pub value: String,
}

impl Condition {
    pub fn new(property: Property) -> Condition {
        Condition { property, op: Operator::for_kind(property.kind())[0], value: String::new() }
    }

    /// Switches the property, keeping the operator when the new kind offers it.
    pub fn set_property(&mut self, property: Property) {
        let ops = Operator::for_kind(property.kind());
        if !ops.contains(&self.op) {
            self.op = ops[0];
        }
        self.property = property;
    }

    /// Whether the condition is complete enough to filter by: a condition still waiting for
    /// its value matches everything.
    pub fn is_active(&self) -> bool {
        !self.op.takes_value() || !self.value.trim().is_empty()
    }

    /// Whether the typed value can't be read for this property (a word where a year goes).
    pub fn is_invalid(&self) -> bool {
        self.is_active() && self.op.takes_value() && self.property.kind() != Kind::Text && self.property.kind() != Kind::List && self.number().is_none()
    }

    fn number(&self) -> Option<f64> {
        let v = self.value.trim();
        match self.property.kind() {
            Kind::Duration => parse_duration(v),
            Kind::Date => parse_date(v).map(|s| s as f64),
            _ => v.parse().ok(),
        }
    }

    pub fn matches(&self, t: &impl Record) -> bool {
        if !self.is_active() {
            return true;
        }
        let value = t.value(self.property);
        match self.op {
            Operator::IsEmpty => return value.is_none(),
            Operator::IsNotEmpty => return !value.is_none(),
            _ => {}
        }
        let needle = self.value.trim().to_lowercase();
        match self.property.kind() {
            Kind::Text | Kind::List => {
                let items: Vec<String> = match value {
                    Value::Text(s) => vec![s.to_lowercase()],
                    Value::List(items) => items.iter().map(|s| s.to_lowercase()).collect(),
                    Value::None | Value::Number(_) => Vec::new(),
                };
                let any = |f: &dyn Fn(&str) -> bool| items.iter().any(|s| f(s));
                match self.op {
                    Operator::Is | Operator::Has => any(&|s| s == needle),
                    Operator::IsNot | Operator::HasNot => !any(&|s| s == needle),
                    Operator::Contains => any(&|s| s.contains(&needle)),
                    Operator::DoesNotContain => !any(&|s| s.contains(&needle)),
                    Operator::StartsWith => any(&|s| s.starts_with(&needle)),
                    Operator::EndsWith => any(&|s| s.ends_with(&needle)),
                    _ => false,
                }
            }
            Kind::Number | Kind::Duration | Kind::Date => {
                // A value that can't be read filters nothing out, so a half-typed number
                // doesn't empty the list.
                let Some(want) = self.number() else { return true };
                let Value::Number(have) = value else {
                    // A song without the property only matches "is not".
                    return self.op == Operator::IsNot;
                };
                let (have, want) = match self.property.kind() {
                    // Dates compare by day, and durations by whole seconds as shown.
                    Kind::Date => ((have / 86_400.0).floor(), (want / 86_400.0).floor()),
                    Kind::Duration => (have.floor(), want.floor()),
                    _ => (have, want),
                };
                match self.op {
                    Operator::Is => have == want,
                    Operator::IsNot => have != want,
                    Operator::Lt => have < want,
                    Operator::Le => have <= want,
                    Operator::Gt => have > want,
                    Operator::Ge => have >= want,
                    _ => false,
                }
            }
        }
    }

    /// One line, e.g. `Year ≥ 2000`, for chips and summaries.
    pub fn summary(&self) -> String {
        let kind = self.property.kind();
        let mut s = format!("{} {}", self.property.label(), self.op.label(kind));
        if self.op.takes_value() {
            let v = self.value.trim();
            s.push(' ');
            s.push_str(if v.is_empty() { "…" } else { v });
        }
        s
    }
}

/// `m:ss`, `h:mm:ss` or plain seconds.
pub fn parse_duration(s: &str) -> Option<f64> {
    let mut total = 0.0;
    for (i, part) in s.split(':').enumerate() {
        if i > 2 {
            return None;
        }
        let n: f64 = part.trim().parse().ok()?;
        total = total * 60.0 + n;
    }
    Some(total)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Condition(Condition),
    Group(Group),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub conjunction: Conjunction,
    pub children: Vec<Node>,
}

impl Group {
    /// An empty group matches everything, whatever its conjunction, as an empty filter does.
    pub fn matches(&self, t: &impl Record) -> bool {
        let mut active = self.children.iter().filter(|n| n.is_active()).peekable();
        if active.peek().is_none() {
            return true;
        }
        match self.conjunction {
            Conjunction::And => active.all(|n| n.matches(t)),
            Conjunction::Or => active.any(|n| n.matches(t)),
            Conjunction::Not => !active.any(|n| n.matches(t)),
        }
    }

    /// The group as one line for a tooltip, e.g. `Year ≥ 2000 and (Genre has Jazz or …)`.
    pub fn summary(&self) -> String {
        let join = match self.conjunction {
            Conjunction::And => " and ",
            Conjunction::Or | Conjunction::Not => " or ",
        };
        let parts: Vec<String> = self.children.iter().map(|n| match n {
            Node::Condition(c) => c.summary(),
            Node::Group(g) => format!("({})", g.summary()),
        }).collect();
        let body = parts.join(join);
        if self.conjunction == Conjunction::Not { format!("not ({body})") } else { body }
    }

    /// Drops conditions on properties outside `keep`, and groups left empty by it.
    pub fn retain_properties(&mut self, keep: &[Property]) {
        self.children.retain_mut(|n| match n {
            Node::Condition(c) => keep.contains(&c.property),
            Node::Group(g) => {
                g.retain_properties(keep);
                !g.children.is_empty()
            }
        });
    }

    /// Conditions in this group and every group inside it.
    pub fn condition_count(&self) -> usize {
        self.children.iter().map(|n| match n {
            Node::Condition(_) => 1,
            Node::Group(g) => g.condition_count(),
        }).sum()
    }

    /// The node at `path`, each step an index into a group's children.
    pub fn node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let (&first, rest) = path.split_first()?;
        let node = self.children.get_mut(first)?;
        if rest.is_empty() {
            return Some(node);
        }
        match node {
            Node::Group(g) => g.node_mut(rest),
            Node::Condition(_) => None,
        }
    }

    /// The group at `path` (empty for this one).
    pub fn group_mut(&mut self, path: &[usize]) -> Option<&mut Group> {
        if path.is_empty() {
            return Some(self);
        }
        match self.node_mut(path)? {
            Node::Group(g) => Some(g),
            Node::Condition(_) => None,
        }
    }

    pub fn remove(&mut self, path: &[usize]) -> Option<Node> {
        let (&last, parent) = path.split_last()?;
        let group = self.group_mut(parent)?;
        (last < group.children.len()).then(|| group.children.remove(last))
    }
}

impl Node {
    fn is_active(&self) -> bool {
        match self {
            Node::Condition(c) => c.is_active(),
            Node::Group(g) => g.children.iter().any(Node::is_active),
        }
    }

    fn matches(&self, t: &impl Record) -> bool {
        match self {
            Node::Condition(c) => c.matches(t),
            Node::Group(g) => g.matches(t),
        }
    }
}

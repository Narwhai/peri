//! The filter builder: a group's conjunction, its conditions (property, operator, value) and
//! nested groups.

use rune::{El, TextEdit, col, el, row};
use rune_kit::{Kit, TextInput};

use super::toolbar::{hint, menu_row, property_select, select, small_icon_button};
use crate::app::App;
use crate::query::filter::{Condition, Conjunction, Group, Node, Operator};
use crate::query::property::{Kind, Property};
use crate::icons;
use crate::palette::Palette;

/// Nested groups stop adding groups past this depth, keeping the popover readable.
const MAX_DEPTH: usize = 3;

pub(super) fn filter_body(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let filter = &s.views.active().filter;
    let mut body = col().key("filter-body").w(560.0).padding(8.0).gap(6.0);
    if filter.children.is_empty() {
        body = body.child(hint("No filters: every song shows. Add a filter to narrow the view.", p));
    }
    body.child(group(s, p, kit, layer, filter, &[]))
}

fn group(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>, g: &Group, path: &[usize]) -> El {
    let id = path_id(path);
    let at: Vec<usize> = path.to_vec();
    let conj_labels: Vec<&str> = Conjunction::ALL.iter().map(|c| c.label()).collect();
    let conj_path = at.clone();
    let conjunction = select(s, kit, layer, format!("conj{id}"), &conj_labels, g.conjunction.label(), 236.0, move |s, label| {
        if let (Some(c), Some(group)) = (Conjunction::from_label(label), s.views.active_mut().filter.group_mut(&conj_path)) {
            group.conjunction = c;
        }
        s.views_changed();
    });
    let mut head = row().key(format!("group{id}-head")).gap(6.0).center_y().child(conjunction).child(el().grow(1.0));
    if !path.is_empty() {
        let remove_path = at.clone();
        head = head.child(small_icon_button(format!("group{id}-remove"), &icons::TRASH, "Remove group", p).on_press(move |s: &mut App| {
            s.views.active_mut().filter.remove(&remove_path);
            s.filter_edits.clear();
            s.views_changed();
        }));
    }
    let mut rows = col().key(format!("group{id}-rows")).gap(4.0);
    for (n, node) in g.children.iter().enumerate() {
        let mut child_path = at.clone();
        child_path.push(n);
        rows = rows.child(match node {
            Node::Condition(c) => condition(s, p, kit, layer, c, &child_path),
            Node::Group(inner) => group(s, p, kit, layer, inner, &child_path),
        });
    }
    let add_path = at.clone();
    let mut actions = row().gap(4.0).child(menu_row_keyed(format!("add-filter{id}"), &icons::PLUS, "Add filter", p).on_press(move |s: &mut App| {
        if let Some(group) = s.views.active_mut().filter.group_mut(&add_path) {
            group.children.push(Node::Condition(Condition::new(Property::Genre)));
        }
        s.views_changed();
    }));
    if path.len() < MAX_DEPTH {
        let add_path = at.clone();
        actions = actions.child(menu_row_keyed(format!("add-group{id}"), &icons::GROUP, "Add filter group", p).on_press(move |s: &mut App| {
            if let Some(group) = s.views.active_mut().filter.group_mut(&add_path) {
                group.children.push(Node::Group(Group { conjunction: Conjunction::Or, children: vec![Node::Condition(Condition::new(Property::Genre))] }));
            }
            s.views_changed();
        }));
    }
    let inner = col().gap(6.0).child(head).child(rows).child(actions);
    if path.is_empty() {
        inner.key("filter-root")
    } else {
        // A nested group sits in a tinted box with a rule down its left edge.
        inner.key(format!("group{id}")).padding(8.0).radius(4.0).fill(p.ui.alpha(0.18)).border(1.0, p.ui.alpha(0.6))
    }
}

fn condition(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>, c: &Condition, path: &[usize]) -> El {
    let id = path_id(path);
    let kind = c.property.kind();
    let at: Vec<usize> = path.to_vec();

    let prop_path = at.clone();
    let property = property_select(s, kit, layer, format!("cond{id}-prop"), c.property, move |s, prop| {
        if let Some(Node::Condition(c)) = s.views.active_mut().filter.node_mut(&prop_path) {
            c.set_property(prop);
        }
    });

    let op_labels: Vec<&str> = Operator::for_kind(kind).iter().map(|o| o.label(kind)).collect();
    let op_path = at.clone();
    let operator = select(s, kit, layer, format!("cond{id}-op"), &op_labels, c.op.label(kind), 128.0, move |s, label| {
        if let Some(Node::Condition(c)) = s.views.active_mut().filter.node_mut(&op_path)
            && let Some(op) = Operator::from_label(c.property.kind(), label)
        {
            c.op = op;
        }
        s.views_changed();
    });

    let mut r = row().key(format!("cond{id}")).h(32.0).gap(6.0).center_y().child(property).child(operator);
    if c.op.takes_value() {
        r = r.child(value_input(s, p, kit, c, &at, kind));
    } else {
        r = r.child(el().grow(1.0));
    }
    let remove_path = at.clone();
    r.child(small_icon_button(format!("cond{id}-remove"), &icons::TRASH, "Remove filter", p).on_press(move |s: &mut App| {
        s.views.active_mut().filter.remove(&remove_path);
        // Paths after the removed one shift, so their editors would point at the wrong rows.
        s.filter_edits.clear();
        s.views_changed();
    }))
}

fn value_input(s: &App, _p: &Palette, kit: &Kit, c: &Condition, path: &[usize], kind: Kind) -> El {
    let edit = s.filter_edits.get(path).cloned().unwrap_or_else(|| TextEdit::new(c.value.clone()));
    let placeholder = match kind {
        Kind::Text | Kind::List => "Value",
        Kind::Number => "Number",
        Kind::Duration => "m:ss",
        Kind::Date => "YYYY-MM-DD",
    };
    let edit_path = path.to_vec();
    let change_path = path.to_vec();
    TextInput::new(&edit)
        .placeholder(placeholder)
        .label(format!("{} value", c.property.label()))
        .width(None)
        .invalid(c.is_invalid())
        .on_edit(move |s: &mut App| {
            let value = match s.views.active_mut().filter.node_mut(&edit_path) {
                Some(Node::Condition(c)) => c.value.clone(),
                // TODO: the condition was removed under a pending edit; this editor is discarded.
                _ => String::new(),
            };
            s.filter_edits.entry(edit_path.clone()).or_insert_with(|| TextEdit::new(value))
        })
        .on_change(move |s: &mut App, _tx| {
            let Some(text) = s.filter_edits.get(&change_path).map(|e| e.text.clone()) else { return };
            if let Some(Node::Condition(c)) = s.views.active_mut().filter.node_mut(&change_path) {
                c.value = text;
            }
            s.views_changed();
        })
        .build(kit)
        .key(format!("cond{}-value", path_id(path)))
        .grow(1.0)
        .shrink()
}

fn menu_row_keyed(key: String, glyph: &'static rune::Icon, label: &str, p: &Palette) -> El {
    // `menu_row` takes a static key; nested groups need one per path.
    menu_row("", glyph, label, p).key(key)
}

/// `-0-2` for path [0, 2]; empty for the root group.
fn path_id(path: &[usize]) -> String {
    path.iter().map(|n| format!("-{n}")).collect()
}

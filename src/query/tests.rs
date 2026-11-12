use std::path::PathBuf;

use super::filter::{Condition, Conjunction, Group, Node, Operator, parse_duration};
use super::property::{Property, Value};
use super::store::{Views, Store};
use super::view::{Direction, GroupBy, Sort, View, evaluate};
use crate::library::Track;

fn song(title: &str, artists: &[&str], genres: &[&str], year: Option<u32>, secs: f64) -> Track {
    Track {
        path: PathBuf::from(format!("/m/{title}.flac")),
        title: title.into(),
        artists: artists.iter().map(|s| s.to_string()).collect(),
        genres: genres.iter().map(|s| s.to_string()).collect(),
        year,
        duration: Some(secs),
        ..Track::default()
    }
}

fn library() -> Vec<Track> {
    vec![
        song("Alpha", &["Ann"], &["Rock", "Pop"], Some(1999), 200.0),
        song("Bravo", &["Bob"], &["Jazz"], Some(2005), 300.0),
        song("Charlie", &["Ann", "Bob"], &[], Some(2012), 100.0),
        song("Delta", &["Cy"], &["pop"], None, 250.0),
    ]
}

fn cond(property: Property, op: Operator, value: &str) -> Node {
    Node::Condition(Condition { property, op, value: value.into() })
}

fn titles(tracks: &[Track], order: &[usize]) -> Vec<String> {
    order.iter().map(|&i| tracks[i].title.clone()).collect()
}

#[test]
fn an_empty_view_is_the_whole_library_in_library_order() {
    let lib = library();
    let e = evaluate(&View::new("All"), &lib, "");
    assert_eq!(e.order, [0, 1, 2, 3]);
    assert!(e.sections.is_empty());
}

#[test]
fn and_or_not_and_nested_groups_combine() {
    let lib = library();
    let mut view = View::new("v");
    // Year ≥ 2000 AND (genre has jazz OR artist has ann)
    view.filter = Group {
        conjunction: Conjunction::And,
        children: vec![
            cond(Property::Year, Operator::Ge, "2000"),
            Node::Group(Group {
                conjunction: Conjunction::Or,
                children: vec![cond(Property::Genre, Operator::Has, "jazz"), cond(Property::Artist, Operator::Has, "Ann")],
            }),
        ],
    };
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Bravo", "Charlie"]);
    view.filter.conjunction = Conjunction::Not;
    // None of: Alpha (1999, Ann) has Ann; Delta has no year, so it is not ≥ 2000 and not Ann.
    // A missing year compares false, so Delta matches neither condition.
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Delta"], "none of: year ≥ 2000, (jazz or ann)");
}

#[test]
fn unfinished_and_unreadable_conditions_filter_nothing() {
    let lib = library();
    let mut view = View::new("v");
    view.filter.children = vec![cond(Property::Genre, Operator::Has, "  "), cond(Property::Year, Operator::Gt, "19x")];
    assert_eq!(evaluate(&view, &lib, "").order.len(), 4);
    let Node::Condition(c) = &view.filter.children[1] else { unreachable!() };
    assert!(c.is_invalid());
}

#[test]
fn empty_and_missing_values() {
    let lib = library();
    let mut view = View::new("v");
    view.filter.children = vec![cond(Property::Genre, Operator::IsEmpty, "")];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Charlie"]);
    view.filter.children = vec![cond(Property::Year, Operator::IsNot, "2005")];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Alpha", "Charlie", "Delta"], "a missing year is not 2005");
    view.filter.children = vec![cond(Property::Year, Operator::Lt, "3000")];
    assert_eq!(evaluate(&view, &lib, "").order.len(), 3, "a missing year is neither above nor below");
}

#[test]
fn durations_compare_as_typed_in_minutes() {
    assert_eq!(parse_duration("4:10"), Some(250.0));
    assert_eq!(parse_duration("1:00:05"), Some(3605.0));
    let lib = library();
    let mut view = View::new("v");
    view.filter.children = vec![cond(Property::Duration, Operator::Ge, "4:10")];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Bravo", "Delta"]);
}

#[test]
fn sorts_apply_in_order_with_missing_values_last_both_ways() {
    let lib = library();
    let mut view = View::new("v");
    view.sorts = vec![Sort { property: Property::Year, direction: Direction::Desc }];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Charlie", "Bravo", "Alpha", "Delta"]);
    view.sorts = vec![Sort { property: Property::Year, direction: Direction::Asc }];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "").order), ["Alpha", "Bravo", "Charlie", "Delta"]);
}

#[test]
fn grouping_by_a_list_puts_a_song_under_each_value_and_plays_it_once() {
    let lib = library();
    let mut view = View::new("v");
    view.group_by = Some(GroupBy { property: Property::Genre, direction: Direction::Asc });
    let e = evaluate(&view, &lib, "");
    let groups: Vec<(String, Vec<String>)> = e.sections.iter().map(|s| (s.title.clone(), titles(&lib, &s.items))).collect();
    assert_eq!(
        groups,
        [
            ("Jazz".to_string(), vec!["Bravo".to_string()]),
            ("Pop".to_string(), vec!["Alpha".to_string(), "Delta".to_string()]),
            ("Rock".to_string(), vec!["Alpha".to_string()]),
            ("No genre".to_string(), vec!["Charlie".to_string()]),
        ],
        "case-insensitive groups, sorted, with the empty group last"
    );
    assert_eq!(titles(&lib, &e.order), ["Bravo", "Alpha", "Delta", "Charlie"]);
    view.group_by = Some(GroupBy { property: Property::Genre, direction: Direction::Desc });
    let first = &evaluate(&view, &lib, "").sections[0];
    assert_eq!(first.value, Value::Text("Rock".into()));
}

#[test]
fn search_narrows_a_view_further() {
    let lib = library();
    let mut view = View::new("v");
    view.filter.children = vec![cond(Property::Artist, Operator::Has, "ann")];
    assert_eq!(titles(&lib, &evaluate(&view, &lib, "char").order), ["Charlie"]);
}

#[test]
fn changing_a_conditions_property_keeps_a_compatible_operator() {
    let mut c = Condition::new(Property::Year);
    c.op = Operator::IsEmpty;
    c.set_property(Property::Genre);
    assert_eq!(c.op, Operator::IsEmpty);
    c.op = Operator::Has;
    c.set_property(Property::Year);
    assert_eq!(c.op, Operator::Is, "years have no `has`: back to the first operator");
}

#[test]
fn a_filter_reads_as_one_line() {
    let g = Group {
        conjunction: Conjunction::And,
        children: vec![
            cond(Property::Year, Operator::Ge, "2000"),
            Node::Group(Group { conjunction: Conjunction::Or, children: vec![cond(Property::Genre, Operator::Has, "Jazz"), cond(Property::Title, Operator::IsEmpty, "")] }),
        ],
    };
    assert_eq!(g.summary(), "Year ≥ 2000 and (Genre has Jazz or Title is empty)");
}

#[test]
fn nodes_are_addressed_by_path() {
    let mut g = Group { conjunction: Conjunction::And, children: vec![cond(Property::Title, Operator::Is, "a"), Node::Group(Group::default())] };
    g.group_mut(&[1]).unwrap().children.push(cond(Property::Year, Operator::Is, "1"));
    assert_eq!(g.condition_count(), 2);
    assert!(matches!(g.remove(&[1, 0]), Some(Node::Condition(_))));
    assert!(g.group_mut(&[0]).is_none(), "a condition isn't a group");
    assert_eq!(g.condition_count(), 1);
}

#[test]
fn views_save_and_load_as_json() {
    let mut view = View::new("Recent jazz");
    view.filter.children = vec![cond(Property::Genre, Operator::Has, "Jazz"), Node::Group(Group { conjunction: Conjunction::Or, children: vec![] })];
    view.sorts = vec![Sort { property: Property::Added, direction: Direction::Desc }];
    view.group_by = Some(GroupBy { property: Property::Year, direction: Direction::Desc });
    let views = Views { list: vec![View::new("All songs"), view], active: 1, ..Views::default() };
    let json = serde_json::to_string(&views).unwrap();
    assert!(json.contains("\"genre\"") && json.contains("\"has\"") && json.contains("\"desc\""), "{json}");
    assert_eq!(serde_json::from_str::<Views>(&json).unwrap(), views);

    let dir = std::env::temp_dir().join(format!("rune-music-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("rune-music/prefs.json");
    let mut store = Store::at(&file);
    assert_eq!(store.load(), Views::default(), "nothing saved yet");
    store.save(&views).unwrap();
    assert_eq!(Store::at(&file).load(), views);
    assert!(file.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn durations_group_by_the_minute() {
    let lib = library();
    let mut view = View::new("v");
    view.group_by = Some(GroupBy { property: Property::Duration, direction: Direction::Asc });
    let groups: Vec<(String, usize)> = evaluate(&view, &lib, "").sections.iter().map(|s| (s.title.clone(), s.items.len())).collect();
    assert_eq!(groups, [("1:00 – 1:59".to_string(), 1), ("3:00 – 3:59".to_string(), 1), ("4:00 – 4:59".to_string(), 1), ("5:00 – 5:59".to_string(), 1)]);
}

#[test]
fn views_saved_without_the_covers_setting_show_covers_and_it_round_trips() {
    let old: View = serde_json::from_str(r#"{"name": "Old"}"#).expect("an old view parses");
    assert!(old.covers);
    let mut off = View::new("Off");
    off.covers = false;
    let back: View = serde_json::from_str(&serde_json::to_string(&off).expect("serializes")).expect("parses");
    assert!(!back.covers);
}

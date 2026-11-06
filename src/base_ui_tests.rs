//! The Bases toolbar driven headlessly: views, sort, group and filter popovers.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Row, header_key, row_key};
use crate::base::filter::{Conjunction, Node, Operator};
use crate::base::property::Property;
use crate::base::store::Store;
use crate::base::view::Direction;
use crate::view::view;

fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn type_text(rt: &mut Runtime<App>, text: &str) {
    for c in text.chars() {
        rt.key(KeyEvent { key: Key::Text(c.to_string()), mods: Mods::default() });
    }
    settle(rt);
}

/// Picks `option` from the select keyed `key`, scrolling the listbox to it first.
fn choose(rt: &mut Runtime<App>, key: &str, option: &str) {
    click(rt, &format!("{key}-trigger"));
    let find = |rt: &Runtime<App>| {
        rt.elements()
            .into_iter()
            .find(|e| e.role == "option" && e.label.as_deref() == Some(option) && e.path.contains(&format!("{key}-content")))
            .unwrap_or_else(|| panic!("{key} has no option {option}"))
    };
    let row = find(rt);
    if row.visible[3] < row.rect[3] {
        let [x, y, w, _] = row.rect;
        let list = rt.elements().into_iter().find(|e| e.path.ends_with(&format!("{key}-content"))).expect("the listbox");
        rt.wheel(x + w / 2.0, list.rect[1] + 10.0, -(y - list.rect[1]));
        settle(rt);
    }
    let row = find(rt);
    let [x, y, w, h] = row.visible;
    rt.pointer_down(x + w / 2.0, y + h / 2.0);
    rt.pointer_up(x + w / 2.0, y + h / 2.0);
    settle(rt);
}

fn titles(rt: &Runtime<App>) -> Vec<String> {
    rt.state.filtered.iter().map(|&i| rt.state.tracks[i].title.clone()).collect()
}

#[test]
fn building_a_filter_from_the_popover_narrows_the_list() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-filter");
    // A new condition starts as `Genre has …`, matching everything until it has a value.
    assert_eq!(rt.state.filtered.len(), 13);
    click(&mut rt, "cond-0-value");
    type_text(&mut rt, "ambient");
    assert_eq!(rt.state.filtered.len(), 7, "Laraaji and Nils Frahm are ambient");
    choose(&mut rt, "cond-0-prop", "Year");
    assert_eq!(rt.state.filtered.len(), 13, "a year can't be `ambient`, so the filter waits");
    choose(&mut rt, "cond-0-op", "≥");
    click(&mut rt, "cond-0-value");
    rt.update(|s, _| {
        let edit = s.filter_edits.get_mut(&vec![0]).expect("the value editor");
        edit.text.clear();
        edit.anchor = 0;
        edit.focus = 0;
    });
    type_text(&mut rt, "2020");
    let Node::Condition(c) = &rt.state.base.active().filter.children[0] else { panic!("a condition") };
    assert_eq!((c.property, c.op, c.value.as_str()), (Property::Year, Operator::Ge, "2020"));
    assert_eq!(titles(&rt), ["Mary", "Change", "New Jeans", "Super Shy", "ETA", "Cool With You"]);
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("filter")).and_then(|e| e.label).as_deref(), Some("1 filter"));
}

#[test]
fn nested_groups_are_added_switched_and_removed() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-group");
    assert!(matches!(&rt.state.base.active().filter.children[0], Node::Group(g) if g.conjunction == Conjunction::Or));
    choose(&mut rt, "conj-0", "None of the following are true");
    click(&mut rt, "cond-0-0-value");
    type_text(&mut rt, "pop");
    assert!(titles(&rt).iter().all(|t| !["New Jeans", "ETA"].contains(&t.as_str())), "none of: genre has pop");
    assert_eq!(rt.state.filtered.len(), 9);
    click(&mut rt, "group-0-remove");
    assert!(rt.state.base.active().filter.children.is_empty());
    assert_eq!(rt.state.filtered.len(), 13);
}

#[test]
fn grouping_shows_sticky_headers_that_collapse_and_next_follows_the_view() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    choose(&mut rt, "group-prop", "Genre");
    click(&mut rt, "group-dir");
    assert_eq!(rt.state.base.active().group_by.map(|g| g.direction), Some(Direction::Desc));
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.panel, None, "Escape closes the popover");
    let titles: Vec<&str> = rt.state.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Pop", "New Age", "K-Pop", "Folk", "Electronic", "Ambient"]);

    // Collapse Pop: its songs leave the list, the header stays.
    click(&mut rt, &header_key(0));
    assert!(rt.state.rows.iter().all(|r| !matches!(r, Row::Track { section: Some(0), .. })));
    assert!(rt.find(&header_key(0)).is_some());

    // Play the first New Age song; Next walks the view's order: the rest of New Age, then K-Pop.
    let first = rt.state.sections[1].items[0];
    rt.update(|s, tx| s.play_index(first, tx));
    let mut played = vec![rt.state.tracks[first].title.clone()];
    for _ in 0..3 {
        click(&mut rt, "next");
        played.push(rt.state.tracks[rt.state.current.expect("playing")].title.clone());
    }
    // The K-Pop songs already played in Pop, so the next song is Folk's first.
    assert_eq!(played, ["The Dance #1", "The Dance #2", "Meditation #1", "Mary"], "a song plays once, in its first group");
    assert!(rt.find(&row_key(first, Some(1))).is_some());
}

#[test]
fn sorts_are_added_reordered_and_flipped_from_the_popover() {
    let mut rt = runtime();
    click(&mut rt, "sort");
    click(&mut rt, "add-sort");
    choose(&mut rt, "sort0-prop", "Duration");
    assert_eq!(titles(&rt)[0], "New Jeans", "shortest first");
    click(&mut rt, "sort0-dir");
    assert_eq!(titles(&rt)[0], "Meditation #1", "longest first");
    click(&mut rt, "add-sort");
    choose(&mut rt, "sort1-prop", "Year");
    click(&mut rt, "sort1-up");
    let sorts = &rt.state.base.active().sorts;
    assert_eq!((sorts[0].property, sorts[1].property), (Property::Year, Property::Duration));
    assert_eq!(titles(&rt)[0], "Meditation #1", "1980 first, then the longest of them");
    click(&mut rt, "sort0-remove");
    click(&mut rt, "sort0-remove");
    assert_eq!(rt.state.filtered, (0..13).collect::<Vec<_>>(), "no sorts: library order");
}

#[test]
fn views_are_created_renamed_switched_duplicated_and_deleted() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);

    click(&mut rt, "views");
    click(&mut rt, "add-view");
    assert_eq!(rt.state.base.active, 1);
    assert!(rt.state.base.active().group_by.is_none(), "a new view starts flat");
    assert!(rt.state.sections.is_empty());
    // The new view's name is selected for renaming: typing replaces it.
    type_text(&mut rt, "Short songs");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.base.views[1].name, "Short songs");

    click(&mut rt, "view0");
    assert_eq!(rt.state.base.active, 0);
    assert!(!rt.state.sections.is_empty(), "back to the grouped view");

    click(&mut rt, "views");
    click(&mut rt, "duplicate0");
    assert_eq!(rt.state.base.views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["All songs", "All songs copy", "Short songs"]);
    assert!(rt.state.base.views[1].group_by.is_some());
    click(&mut rt, "delete0");
    assert_eq!(rt.state.base.views.len(), 2);
    assert_eq!(rt.state.base.active().name, "All songs copy", "deleting the active view moves to its neighbour");
}

#[test]
fn views_save_as_they_change_and_load_on_the_next_launch() {
    let dir = std::env::temp_dir().join(format!("rune-music-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.frame(0.0);
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    choose(&mut rt, "group-prop", "Year");

    let mut next = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut next);
    assert_eq!(next.base.active().group_by.map(|g| g.property), Some(Property::Year));
    assert_eq!(next.sections.first().map(|s| s.title.as_str()), Some("1980"));
    std::fs::remove_dir_all(&dir).unwrap();
}

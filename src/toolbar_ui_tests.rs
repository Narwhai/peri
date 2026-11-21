//! The toolbar driven headlessly: views, sort, group and filter popovers.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Row, header_key, row_key};
use crate::query::filter::{Conjunction, Node, Operator};
use crate::query::property::Property;
use crate::query::store::Store;
use crate::query::view::{Direction, View};
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

/// Drags the element keyed `key` by `dy` points, in steps as a hand moves.
fn drag_down(rt: &mut Runtime<App>, key: &str, dy: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    for step in 1..=4 {
        rt.pointer_move(x, y + dy * step as f32 / 4.0);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x, y + dy);
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
    rt.state.filtered.iter().map(|&i| rt.state.tracks()[i].title.clone()).collect()
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
    let Node::Condition(c) = &rt.state.views.active().filter.children[0] else { panic!("a condition") };
    assert_eq!((c.property, c.op, c.value.as_str()), (Property::Year, Operator::Ge, "2020"));
    assert_eq!(titles(&rt), ["Mary", "Change", "New Jeans", "Super Shy", "ETA", "Cool With You"]);
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("filter")).and_then(|e| e.label).as_deref(), Some("1 filter"));
    assert_eq!(result_count(&rt), "6 songs");
}

fn result_count(rt: &Runtime<App>) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("result-count")).and_then(|e| e.text).expect("the result count")
}

#[test]
fn the_toolbar_counts_the_views_songs_or_albums() {
    let mut rt = runtime();
    assert_eq!(result_count(&rt), "13 songs");
    click(&mut rt, "albums");
    assert_eq!(result_count(&rt), format!("{} albums", rt.state.shown_albums.len()));
}

#[test]
fn play_queues_the_views_songs_in_order_including_collapsed_groups() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    click(&mut rt, &header_key(0));
    let order = rt.state.filtered.clone();
    rt.state.shuffle = true;
    click(&mut rt, "play-view");
    assert_eq!(rt.state.current, Some(order[0]), "the view's first song, even though its group is collapsed");
    assert!(!rt.state.shuffle, "Play plays in order");
    let mut played = vec![order[0]];
    for _ in 1..order.len() {
        click(&mut rt, "next");
        played.push(rt.state.current.expect("playing"));
    }
    assert_eq!(played, order, "every song of the view, once, in its order");
}

#[test]
fn play_in_an_album_view_queues_its_albums_songs() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "play-view");
    let first_album = rt.state.shown_albums[0];
    assert_eq!(rt.state.current, rt.state.albums()[first_album].tracks.first().copied());
}

#[test]
fn play_is_disabled_while_the_view_is_empty() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-filter");
    choose(&mut rt, "cond-0-prop", "Year");
    click(&mut rt, "cond-0-value");
    type_text(&mut rt, "1000");
    choose(&mut rt, "cond-0-op", "<");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(rt.state.filtered.is_empty());
    assert_eq!(result_count(&rt), "0 songs");
    assert!(rt.find("play-view").is_some(), "shown, though disabled");
    click(&mut rt, "play-view");
    assert_eq!(rt.state.current, None);
}

#[test]
fn the_views_menu_plays_a_view_without_opening_it() {
    let mut rt = runtime();
    rt.update(|s, _| {
        let mut short = crate::query::view::View::new("Reversed".to_string());
        short.sorts.push(crate::query::view::Sort { property: Property::Title, direction: Direction::Desc });
        s.views.list.push(short);
    });
    let expected = rt.state.view_songs(1);
    click(&mut rt, "views");
    click(&mut rt, "play-view1");
    assert_eq!(rt.state.views.active, 0, "the open view stays open");
    assert_eq!(rt.state.panel, None, "the menu closes");
    assert_eq!(rt.state.current, Some(expected[0]));
    click(&mut rt, "next");
    assert_eq!(rt.state.current, Some(expected[1]), "Next follows the played view, not the open one");
}

#[test]
fn a_narrow_window_shows_the_toolbar_buttons_as_icons_with_their_labels_as_tooltips() {
    let mut rt = runtime();
    let shows_label = |rt: &Runtime<App>| rt.elements().into_iter().any(|e| e.path.contains("sort") && e.key.as_deref() == Some("label"));
    assert!(shows_label(&rt), "980 wide: Sort shows its label");
    rt.set_viewport((720.0, 640.0), 1.0);
    settle(&mut rt);
    assert!(!shows_label(&rt), "720 wide: Sort is an icon");
    let sort = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("sort")).expect("Sort");
    assert_eq!((sort.label.as_deref(), sort.help.as_deref()), (Some("Sort"), Some("Sort")));
    for key in ["play-view", "columns", "sort", "group", "filter"] {
        let [x, _, w, _] = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).expect(key).rect;
        assert!(x + w <= 720.5, "{key} stays in the window");
    }
}

#[test]
fn nested_groups_are_added_switched_and_removed() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-group");
    assert!(matches!(&rt.state.views.active().filter.children[0], Node::Group(g) if g.conjunction == Conjunction::Or));
    choose(&mut rt, "conj-0", "None of the following are true");
    click(&mut rt, "cond-0-0-value");
    type_text(&mut rt, "pop");
    assert!(titles(&rt).iter().all(|t| !["New Jeans", "ETA"].contains(&t.as_str())), "none of: genre has pop");
    assert_eq!(rt.state.filtered.len(), 9);
    click(&mut rt, "group-0-remove");
    assert!(rt.state.views.active().filter.children.is_empty());
    assert_eq!(rt.state.filtered.len(), 13);
}

#[test]
fn grouping_shows_sticky_headers_that_collapse_and_next_follows_the_view() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    choose(&mut rt, "group-prop", "Genre");
    click(&mut rt, "group-dir");
    assert_eq!(rt.state.views.active().group_by.map(|g| g.direction), Some(Direction::Desc));
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
    let mut played = vec![rt.state.tracks()[first].title.clone()];
    for _ in 0..3 {
        click(&mut rt, "next");
        played.push(rt.state.tracks()[rt.state.current.expect("playing")].title.clone());
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
    drag_down(&mut rt, "sort1-handle", -36.0);
    let sorts = &rt.state.views.active().sorts;
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
    assert_eq!(rt.state.views.active, 1);
    assert!(rt.state.views.active().group_by.is_none(), "a new view starts flat");
    assert!(rt.state.sections.is_empty());
    // The new view's name is selected for renaming: typing replaces it.
    type_text(&mut rt, "Short songs");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.views.list[1].name, "Short songs");

    click(&mut rt, "view0");
    assert_eq!(rt.state.views.active, 0);
    assert!(!rt.state.sections.is_empty(), "back to the grouped view");

    click(&mut rt, "views");
    click(&mut rt, "duplicate0");
    assert_eq!(rt.state.views.list.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["All songs", "All songs copy", "Short songs"]);
    assert!(rt.state.views.list[1].group_by.is_some());
    click(&mut rt, "delete0");
    assert_eq!(rt.state.views.list.len(), 2);
    assert_eq!(rt.state.views.active().name, "All songs copy", "deleting the active view moves to its neighbour");
}

#[test]
fn views_save_as_they_change_and_load_on_the_next_launch() {
    let dir = std::env::temp_dir().join(format!("peri-ui-{}", std::process::id()));
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
    assert_eq!(next.views.active().group_by.map(|g| g.property), Some(Property::Year));
    assert_eq!(next.sections.first().map(|s| s.title.as_str()), Some("1980"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn slash_focuses_search_and_then_types_in_it() {
    let mut rt = runtime();
    type_text(&mut rt, "/");
    assert!(rt.state.search.focused, "/ focuses the search field");
    assert_eq!(rt.state.search.text, "", "the / that focused it isn't typed");
    type_text(&mut rt, "a/b");
    assert_eq!(rt.state.search.text, "a/b", "in the field, / types");
}

/// Types `query` into the view search box, focusing it first.
fn search_view(rt: &mut Runtime<App>, query: &str) {
    click(rt, "view-search");
    type_text(rt, query);
}

fn sorted(mut titles: Vec<String>) -> Vec<String> {
    titles.sort();
    titles
}

#[test]
fn the_view_search_narrows_songs_by_title_and_shown_columns_and_play_plays_them() {
    let mut rt = runtime();
    search_view(&mut rt, "laraaji");
    assert_eq!(sorted(titles(&rt)), ["Change", "Mary", "Meditation #1", "The Dance #1", "The Dance #2"], "artist is a shown column");
    assert_eq!(result_count(&rt), "5 songs");
    let expected = rt.state.filtered.clone();
    click(&mut rt, "play-view");
    let mut played = vec![rt.state.current.expect("playing")];
    for _ in 1..expected.len() {
        click(&mut rt, "next");
        played.push(rt.state.current.expect("playing"));
    }
    assert_eq!(played, expected, "Play plays only the matching songs");
}

#[test]
fn the_view_search_ignores_properties_not_in_the_columns_until_they_are_shown() {
    let mut rt = runtime();
    search_view(&mut rt, "ambient");
    assert!(rt.state.filtered.is_empty(), "Genre isn't a shown column");
    assert_eq!(result_count(&rt), "0 songs");
    rt.update(|s, _| {
        s.views.active_mut().columns.push(Property::Genre);
        s.refilter();
    });
    assert_eq!(rt.state.filtered.len(), 7, "Laraaji and Nils Frahm are ambient");
}

#[test]
fn the_view_search_ands_words_across_fields_and_ignores_case() {
    let mut rt = runtime();
    search_view(&mut rt, "DANCE   laraaji");
    assert_eq!(titles(&rt), ["The Dance #1", "The Dance #2"], "title and artist");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    search_view(&mut rt, "dance nils");
    assert!(rt.state.filtered.is_empty(), "no song is a Dance by Nils Frahm");
}

#[test]
fn the_album_grid_groups_view_search_with_the_left_hand_controls() {
    let mut rt = runtime();
    rt.set_viewport((1440.0, 640.0), 1.0);
    settle(&mut rt);
    click(&mut rt, "albums");

    let elements = rt.elements();
    let rect = |key| elements.iter().find(|e| e.key.as_deref() == Some(key)).expect(key).rect;
    let [size_x, _, size_w, _] = rect("card-size-control");
    let [search_x, _, search_w, _] = rect("view-search");
    let [play_x, _, _, _] = rect("play-view");
    let gap_after_slider = search_x - (size_x + size_w);
    let gap_before_play = play_x - (search_x + search_w);
    assert!(gap_after_slider >= 0.0 && gap_after_slider < gap_before_play, "Search sits next to the card size slider, not Play");
}

#[test]
fn the_view_search_filters_albums_in_the_album_grid() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    search_view(&mut rt, "laraaji");
    let album = |rt: &Runtime<App>, title: &str| rt.state.albums().iter().position(|a| a.title == title).expect("the album");
    let mut shown = rt.state.shown_albums.clone();
    shown.sort();
    let mut expected = vec![album(&rt, "Day of Radiance"), album(&rt, "Live Sessions")];
    expected.sort();
    assert_eq!(shown, expected, "albums by Laraaji, or with him in their artists");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    search_view(&mut rt, "live");
    assert_eq!(rt.state.shown_albums, [album(&rt, "Live Sessions")]);
}

#[test]
fn switching_views_or_modes_clears_the_view_search() {
    let mut rt = runtime();
    rt.update(|s, _| s.views.list.push(View::new("Other")));
    search_view(&mut rt, "laraaji");
    click(&mut rt, "views");
    click(&mut rt, "view1");
    assert_eq!(rt.state.view_search.text, "", "a new view starts unsearched");
    assert_eq!(rt.state.filtered.len(), 13);

    search_view(&mut rt, "laraaji");
    click(&mut rt, "albums");
    assert_eq!(rt.state.view_search.text, "", "switching modes clears it");
    assert_eq!(rt.state.shown_albums.len(), rt.state.albums().len());
}

#[test]
fn the_view_search_is_absent_off_the_library_page() {
    let mut rt = runtime();
    assert!(rt.find("view-search").is_some());
    rt.update(|s, tx| s.show_album(0, tx));
    settle(&mut rt);
    assert!(rt.find("view-search").is_none(), "an album page has no view search");
    assert!(rt.find("play-view").is_none());
}

/// The keys of the song rows in the album card for album `title`, in order.
fn card_track_keys(rt: &Runtime<App>, title: &str) -> Vec<String> {
    let album = rt.state.albums().iter().position(|a| a.title == title).expect("the album");
    let card = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&crate::app::album_key(album, None))).expect("the album's card");
    let mut keys: Vec<(f32, String)> = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with('t') && k[1..].parse::<usize>().is_ok()) && e.path.starts_with(&card.path))
        .map(|e| (e.rect[1], e.key.unwrap_or_default()))
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    keys.into_iter().map(|(_, k)| k).collect()
}

#[test]
fn the_view_search_in_an_album_list_shows_only_albums_with_matching_songs_and_their_matching_songs() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "list");
    search_view(&mut rt, "dance");
    let dance = rt.state.albums().iter().position(|a| a.title == "Day of Radiance").expect("the album");
    assert_eq!(rt.state.shown_albums, [dance], "only the album with the Dance songs");
    assert_eq!(sorted(titles(&rt)), ["The Dance #1", "The Dance #2"], "filtered holds the matching songs only");
    assert_eq!(card_track_keys(&rt, "Day of Radiance"), [format!("t{}", rt.state.filtered[0]), format!("t{}", rt.state.filtered[1])]);
    assert_eq!(result_count(&rt), "1 album");
    assert!(rt.find(&format!("t{}", rt.state.tracks().iter().position(|t| t.title == "Meditation #1").expect("a song"))).is_none(), "the other songs are unlisted");
}

#[test]
fn an_album_title_matches_nothing_in_an_album_list_until_album_is_a_column() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "list");
    // Album is a default column, so take it out first.
    rt.update(|s, _| {
        s.views.active_mut().columns.retain(|&c| c != Property::Album);
        s.refilter();
    });
    search_view(&mut rt, "radiance");
    assert!(rt.state.shown_albums.is_empty(), "Album isn't a column, so no song matches");
    assert!(rt.state.filtered.is_empty());
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    rt.update(|s, _| {
        s.views.active_mut().columns.push(Property::Album);
        s.refilter();
    });
    search_view(&mut rt, "radiance");
    assert_eq!(rt.state.shown_albums.len(), 1, "Day of Radiance");
    assert_eq!(card_track_keys(&rt, "Day of Radiance").len(), 3, "all of its songs match");
}

#[test]
fn clearing_the_view_search_restores_every_song_in_an_album_list() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "list");
    search_view(&mut rt, "dance");
    assert_eq!(card_track_keys(&rt, "Day of Radiance").len(), 2);
    click(&mut rt, "view-search");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(rt.state.view_matches.is_none());
    assert_eq!(card_track_keys(&rt, "Day of Radiance").len(), 3);
    assert_eq!(rt.state.shown_albums.len(), rt.state.albums().len());
}

#[test]
fn a_searched_album_cards_play_button_still_plays_the_whole_album() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "list");
    search_view(&mut rt, "dance");
    let album = rt.state.albums().iter().position(|a| a.title == "Day of Radiance").expect("the album");
    let whole = rt.state.albums()[album].tracks.clone();
    click(&mut rt, "play-album");
    assert_eq!(rt.state.current, whole.first().copied());
    let mut played = vec![rt.state.current.expect("playing")];
    for _ in 1..whole.len() {
        click(&mut rt, "next");
        played.push(rt.state.current.expect("playing"));
    }
    assert_eq!(played, whole, "all three songs, though only two match");
}

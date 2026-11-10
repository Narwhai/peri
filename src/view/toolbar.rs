//! The toolbar under the header: the view menu, and Sort, Group and Filter popovers, as in
//! Obsidian's Bases.

use rune::{Cursor, El, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Placement, Select, TextInput, popover, scrim};

use super::*;
use crate::app::{App, Panel};
use crate::base::property::{Mode, Property};
use crate::base::view::{Direction, GroupBy, Layout, Sort, View};
use crate::icons;
use crate::palette::Palette;

const BAR_H: f32 = 40.0;

pub(super) fn toolbar(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let view = s.base.active();
    let views = views_menu(s, p, kit, layer);
    let sort_label = match view.sorts.len() {
        0 => "Sort".to_string(),
        1 => format!("Sort: {}", view.sorts[0].property.label()),
        n => format!("Sort: {n}"),
    };
    let group_label = view.group_by.map_or("Group".to_string(), |g| format!("Group: {}", g.property.label()));
    let filter_label = match view.filter.condition_count() {
        0 => "Filter".to_string(),
        1 => "1 filter".to_string(),
        n => format!("{n} filters"),
    };
    let sort = panel(s, kit, layer, Panel::Sort, tool_button("sort", &icons::SORT, sort_label, !view.sorts.is_empty(), s.panel == Some(Panel::Sort), p), sort_body(s, p, kit));
    let group = panel(s, kit, layer, Panel::Group, tool_button("group", &icons::GROUP, group_label, view.group_by.is_some(), s.panel == Some(Panel::Group), p), group_body(s, p, kit));
    let filter_open = s.panel == Some(Panel::Filter);
    let filter = panel(
        s,
        kit,
        layer,
        Panel::Filter,
        tool_button("filter", &icons::FILTER, filter_label, view.filter.condition_count() > 0, filter_open, p).help(if view.filter.children.is_empty() { "Filter songs".to_string() } else { view.filter.summary() }),
        |layer| super::filters::filter_body(s, p, kit, layer),
    );
    let columns_label = format!("Columns: {}", s.shown_columns().len());
    let columns = panel(s, kit, layer, Panel::Columns, tool_button("columns", &icons::COLUMNS, columns_label, false, s.panel == Some(Panel::Columns), p), columns_body(s, p));
    let albums = view.mode == Mode::Albums;
    let mut bar = row()
        .key("toolbar")
        .w_full()
        .h(BAR_H)
        .px(12.0)
        .gap(4.0)
        .center_y()
        .fill(p.bg)
        .border_bottom(p)
        .child(views)
        .child(el().w(8.0))
        .child(segmented("mode", &[("songs", "Songs"), ("albums", "Albums")], if albums { "albums" } else { "songs" }, p, |s: &mut App, v: &str| {
            let mode = if v == "albums" { Mode::Albums } else { Mode::Songs };
            if s.base.active().mode != mode {
                s.base.active_mut().set_mode(mode);
                s.collapsed.clear();
                s.filter_edits.clear();
                s.show_library();
                s.views_changed();
            }
        }));
    if albums {
        let layout = if view.layout == Layout::Grid { "grid" } else { "list" };
        bar = bar.child(el().w(4.0)).child(segmented("layout", &[("grid", "Grid"), ("list", "List")], layout, p, |s: &mut App, v: &str| {
            let layout = if v == "grid" { Layout::Grid } else { Layout::List };
            if s.base.active().layout != layout {
                s.base.active_mut().layout = layout;
                s.show_library();
                s.views_changed();
            }
        }));
    }
    // Album grids show no song tables, unless an album page is open.
    let tables = !(albums && view.layout == Layout::Grid) || s.page != crate::app::Page::Library;
    bar.child(el().grow(1.0))
        .children(tables.then_some(columns))
        .child(sort)
        .child(group)
        .child(filter)
}

/// A toolbar popover: `trigger` toggles `panel`, and the body shows while it's open. Selects
/// inside the body put their listboxes in a layer mounted above the popover.
fn panel(s: &App, kit: &Kit, layer: &mut Vec<El>, panel: Panel, trigger: El, body: impl FnOnce(&mut Vec<El>) -> El) -> El {
    let open = s.panel == Some(panel);
    let trigger = trigger.expanded(open).on_press(move |s: &mut App| s.toggle_panel(panel));
    if open {
        let anchor = trigger_key(panel);
        let placement = if panel == Panel::Views { Placement::BottomStart } else { Placement::BottomEnd };
        let mut above = Vec::new();
        let content = body(&mut above)
            .key(format!("{anchor}-panel"))
            .role("dialog")
            .label(format!("{panel:?}"))
            .modal(true, |s: &mut App| s.close_panel())
            .focus_return(anchor);
        // A press outside closes the panel; selects inside mount above it.
        layer.push(scrim(format!("{anchor}-scrim"), |s: &mut App| s.close_panel()));
        layer.push(popover::anchored(kit, popover::raised(kit, content, 10.0), anchor, placement, 6.0));
        layer.append(&mut above);
    }
    trigger
}

fn trigger_key(panel: Panel) -> &'static str {
    match panel {
        Panel::Views => "views",
        Panel::Sort => "sort",
        Panel::Group => "group",
        Panel::Filter => "filter",
        Panel::Columns => "columns",
        Panel::LinkArtist => "artist-link",
    }
}

fn tool_button(key: &'static str, glyph: &'static rune::Icon, label: String, active: bool, open: bool, p: &Palette) -> El {
    let ink = if active { p.blue } else { p.tx2 };
    row()
        .key(key)
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .fill(if open { p.ui.alpha(0.6) } else { p.bg.alpha(0.0) })
        .hover(p.ui.alpha(0.45))
        .press(p.ui.alpha(0.7))
        .cursor(Cursor::Pointer)
        .label(label.clone())
        .child(icon(glyph, 15.0).color(ink))
        .child(text(label).key("label").font(TITLE).color(ink))
}

/// A segmented control: one pill per option, the current one filled.
pub(super) fn segmented(key: &'static str, options: &[(&'static str, &'static str)], current: &str, p: &Palette, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let mut seg = row().key(key).h(28.0).padding(2.0).gap(2.0).radius(8.0).fill(p.ui.alpha(0.45)).role("radiogroup").label(key).focus_group();
    for &(value, label) in options {
        let on = value == current;
        let choose = choose.clone();
        seg = seg.child(
            row()
                .key(value)
                .h(24.0)
                .px(10.0)
                .radius(6.0)
                .center()
                .fill(if on { p.bg2 } else { p.bg.alpha(0.0) })
                .shadow(if on { 1.0 } else { 0.0 }, if on { 2.0 } else { 0.0 }, rune::Color::BLACK.alpha(if on { 0.18 } else { 0.0 }))
                .hover(p.ui.alpha(0.3))
                .cursor(Cursor::Pointer)
                .role("radio")
                .checked(on)
                .focusable(true)
                .label(label)
                .motion(SNAPPY)
                .on_press(move |s: &mut App| choose(s, value))
                .child(text(label).font(TITLE).color(if on { p.tx } else { p.tx2 })),
        );
    }
    seg
}

// Views

fn views_menu(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let view = s.base.active();
    let open = s.panel == Some(Panel::Views);
    let body = |_: &mut Vec<El>| views_body(s, p, kit);
    let trigger = row()
        .key("views")
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .fill(if open { p.ui.alpha(0.6) } else { p.bg.alpha(0.0) })
        .hover(p.ui.alpha(0.45))
        .cursor(Cursor::Pointer)
        .label(format!("View: {}", view.name))
        .child(icon(&icons::VIEWS, 15.0).color(p.tx2))
        .child(text(view.name.clone()).key("label").font(TITLE).color(p.tx).truncate().max_w(260.0))
        .child(icon(&rune_kit::glyphs::CHEVRON_DOWN, 14.0).color(p.tx3));
    panel(s, kit, layer, Panel::Views, trigger, body)
}

fn views_body(s: &App, p: &Palette, kit: &Kit) -> El {
    let mut list = col().key("view-list").padding(6.0).gap(2.0).w(300.0);
    for (i, view) in s.base.views.iter().enumerate() {
        let active = i == s.base.active;
        let renaming = s.renaming == Some(i);
        let name: El = match renaming {
            true => TextInput::new(&s.rename)
                .label("View name")
                .width(None)
                .on_edit(|s: &mut App| &mut s.rename)
                .on_submit(|s: &mut App, tx| commit_rename(s, tx))
                .on_key(|s: &mut App, ev, _tx| {
                    if ev.key == rune::Key::Escape {
                        s.renaming = None;
                        return true;
                    }
                    false
                })
                .build(kit)
                .key("rename")
                .grow(1.0)
                .shrink(),
            false => text(view.name.clone()).font(if active { TITLE } else { BODY }).color(p.tx).grow(1.0).shrink().truncate(),
        };
        let mut item = row()
            .key(format!("view{i}"))
            .h(32.0)
            .pl(8.0)
            .pr(4.0)
            .gap(6.0)
            .center_y()
            .radius(6.0)
            .fill(if active { p.ui.alpha(0.55) } else { p.bg.alpha(0.0) })
            .hover(p.ui.alpha(0.35))
            .role("option")
            .checked(active)
            .label(view.name.clone())
            .child(el().w(14.0).center().child(icon(&rune_kit::glyphs::CHECK_SMALL, 14.0).color(p.blue).opacity(if active { 1.0 } else { 0.0 })))
            .child(name);
        if !renaming {
            item = item
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut App, tx| {
                    // Picking a view closes the menu, as a menu pick does.
                    switch_view(s, i, tx);
                    s.close_panel();
                })
                .on_double_press(move |s: &mut App| start_rename(s, i))
                .child(small_icon_button(format!("rename{i}"), &icons::EDIT, "Rename view", p).on_press(move |s: &mut App| start_rename(s, i)))
                .child(small_icon_button(format!("duplicate{i}"), &icons::COPY, "Duplicate view", p).on_press(move |s: &mut App| duplicate_view(s, i)));
            if s.base.views.len() > 1 {
                item = item.child(small_icon_button(format!("delete{i}"), &icons::TRASH, "Delete view", p).on_press(move |s: &mut App| delete_view(s, i)));
            }
        }
        list = list.child(item);
    }
    col()
        .child(list)
        .child(divider(p))
        .child(
            col().padding(6.0).child(
                menu_row("add-view", &icons::PLUS, "New view", p).on_press_tx(|s: &mut App, tx| {
                    let name = unique_name(&s.base.views, "New view");
                    s.base.views.push(View::new(name));
                    let i = s.base.views.len() - 1;
                    switch_view(s, i, tx);
                    start_rename(s, i);
                }),
            ),
        )
}

fn switch_view(s: &mut App, i: usize, tx: &mut Tx) {
    if s.base.active != i {
        s.base.active = i;
        s.collapsed.clear();
        s.filter_edits.clear();
        s.views_changed();
        tx.scroll_to_instant("tracks", 0.0, 0.0);
    }
}

fn start_rename(s: &mut App, i: usize) {
    let name = s.base.views[i].name.clone();
    let mut edit = rune::TextEdit::new(name.clone());
    // The whole name selected, so typing replaces it.
    edit.anchor = 0;
    edit.focus = name.len();
    edit.focused = true;
    s.rename = edit;
    s.renaming = Some(i);
}

fn commit_rename(s: &mut App, tx: &mut Tx) {
    if let Some(i) = s.renaming.take() {
        let name = s.rename.text.trim().to_string();
        if !name.is_empty() {
            s.base.views[i].name = name;
            s.views_changed();
        }
    }
    tx.blur();
}

fn duplicate_view(s: &mut App, i: usize) {
    let mut copy = s.base.views[i].clone();
    copy.name = unique_name(&s.base.views, &format!("{} copy", copy.name));
    s.base.views.insert(i + 1, copy);
    if s.base.active > i {
        s.base.active += 1;
    }
    s.views_changed();
}

fn delete_view(s: &mut App, i: usize) {
    if s.base.views.len() <= 1 {
        return;
    }
    s.base.views.remove(i);
    if s.base.active > i || s.base.active == s.base.views.len() {
        s.base.active = s.base.active.saturating_sub(1);
    }
    s.renaming = None;
    s.collapsed.clear();
    s.filter_edits.clear();
    s.views_changed();
}

/// `base`, or `base 2`, `base 3`… whichever no view has yet.
pub(super) fn unique_name(views: &[View], base: &str) -> String {
    let taken = |n: &str| views.iter().any(|v| v.name == n);
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} {n}")).find(|n| !taken(n)).expect("an unused name")
}

// Sort

fn sort_body<'a>(s: &'a App, p: &'a Palette, kit: &'a Kit) -> impl FnOnce(&mut Vec<El>) -> El + 'a {
    move |layer| {
        let view = s.base.active();
        let mut body = col().key("sort-body").w(360.0).padding(8.0).gap(4.0);
        if view.sorts.is_empty() {
            body = body.child(hint(if view.mode == Mode::Albums { "Albums are in library order: artist, then album." } else { "Songs are in library order: artist, album, then track." }, p));
        }
        for (n, sort) in view.sorts.iter().enumerate() {
            let key = format!("sort{n}");
            let prop = property_select(s, kit, layer, format!("{key}-prop"), sort.property, move |s, prop| {
                s.base.active_mut().sorts[n].property = prop;
            });
            let dir = direction_button(format!("{key}-dir"), sort.direction, sort.property, p).on_press(move |s: &mut App| {
                let d = &mut s.base.active_mut().sorts[n].direction;
                *d = d.flip();
                s.views_changed();
            });
            let mut r = row().key(key).h(32.0).gap(6.0).center_y();
            r = r.child(text(if n == 0 { "Sort by" } else { "then by" }).font(SMALL).color(p.tx3).w(52.0));
            r = r.child(prop).child(dir).child(el().grow(1.0));
            if n > 0 {
                r = r.child(small_icon_button(format!("sort{n}-up"), &icons::ARROW_UP, "Sort first", p).on_press(move |s: &mut App| {
                    s.base.active_mut().sorts.swap(n, n - 1);
                    s.views_changed();
                }));
            }
            r = r.child(small_icon_button(format!("sort{n}-remove"), &icons::TRASH, "Remove sort", p).on_press(move |s: &mut App| {
                s.base.active_mut().sorts.remove(n);
                s.views_changed();
            }));
            body = body.child(r);
        }
        let unused = Property::for_mode(view.mode).iter().copied().find(|p| !view.sorts.iter().any(|s| s.property == *p));
        if let Some(next) = unused {
            body = body.child(menu_row("add-sort", &icons::PLUS, "Add sort", p).on_press(move |s: &mut App| {
                s.base.active_mut().sorts.push(Sort { property: next, direction: Direction::Asc });
                s.views_changed();
            }));
        }
        body
    }
}

// Group

fn group_body<'a>(s: &'a App, p: &'a Palette, kit: &'a Kit) -> impl FnOnce(&mut Vec<El>) -> El + 'a {
    move |layer| {
        let view = s.base.active();
        let mut body = col().key("group-body").w(360.0).padding(8.0).gap(4.0);
        match view.group_by {
            None => {
                let what = if view.mode == Mode::Albums { "albums" } else { "songs" };
                body = body.child(hint(&format!("Group {what} into sections by one property. One with several artists or genres shows under each."), p));
                body = body.child(menu_row("add-group", &icons::PLUS, "Group by…", p).on_press(|s: &mut App| {
                    let property = if s.mode() == Mode::Albums { Property::AlbumArtist } else { Property::Album };
                    s.base.active_mut().group_by = Some(GroupBy { property, direction: Direction::Asc });
                    s.collapsed.clear();
                    s.views_changed();
                }));
            }
            Some(group) => {
                let prop = property_select(s, kit, layer, "group-prop".into(), group.property, |s, prop| {
                    if let Some(g) = &mut s.base.active_mut().group_by {
                        g.property = prop;
                    }
                    s.collapsed.clear();
                });
                let dir = direction_button("group-dir".into(), group.direction, group.property, p).on_press(|s: &mut App| {
                    if let Some(g) = &mut s.base.active_mut().group_by {
                        g.direction = g.direction.flip();
                    }
                    s.views_changed();
                });
                let remove = small_icon_button("group-remove".into(), &icons::TRASH, "Stop grouping", p).on_press(|s: &mut App| {
                    s.base.active_mut().group_by = None;
                    s.collapsed.clear();
                    s.views_changed();
                });
                body = body.child(
                    row().key("group-row").h(32.0).gap(6.0).center_y()
                        .child(text("Group by").font(SMALL).color(p.tx3).w(52.0))
                        .child(prop).child(dir).child(el().grow(1.0)).child(remove),
                );
                let collapsed = !s.collapsed.is_empty();
                body = body.child(divider(p)).child(
                    menu_row("collapse-all", if collapsed { &icons::EXPAND } else { &icons::COLLAPSE }, if collapsed { "Expand all groups" } else { "Collapse all groups" }, p)
                        .on_press(|s: &mut App| {
                            if s.collapsed.is_empty() {
                                let titles: Vec<String> = s.sections.iter().map(|g| g.title.clone()).collect();
                                s.collapsed.extend(titles);
                            } else {
                                s.collapsed.clear();
                            }
                            s.refilter();
                        }),
                );
            }
        }
        body
    }
}

// Columns

/// Which properties the view's song tables show after the title, and in what order. Album
/// views use them for their track tables.
fn columns_body<'a>(s: &'a App, p: &'a Palette) -> impl FnOnce(&mut Vec<El>) -> El + 'a {
    move |_layer| {
        let shown = s.shown_columns();
        let page = s.page != crate::app::Page::Library;
        let mut body = col().key("columns-body").w(300.0).padding(8.0).gap(2.0);
        body = body.child(hint(if page { "Shown after the title in album and artist pages' song tables. Click to show or hide; ↑ moves a column left." } else { "Shown after the title in this view's song tables. Click to show or hide; ↑ moves a column left." }, p));
        let mut list = col().key("column-list").w_full().gap(2.0);
        // Shown columns first in their order, then the rest in property order.
        let order = shown.iter().copied().chain(Property::ALL.into_iter().filter(|c| *c != Property::Title && !shown.contains(c)));
        for c in order {
            let at = shown.iter().position(|x| *x == c);
            let key = format!("column-{}", c.label().to_lowercase().replace(' ', "-"));
            let mut r = row()
                .key(key.clone())
                .w_full()
                .h(30.0)
                .pl(8.0)
                .pr(4.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(p.ui.alpha(0.35))
                .cursor(Cursor::Pointer)
                .role("checkbox")
                .checked(at.is_some())
                .label(c.label())
                .on_press(move |s: &mut App| {
                    let columns = s.shown_columns_mut();
                    match columns.iter().position(|x| *x == c) {
                        Some(n) => {
                            columns.remove(n);
                        }
                        None => columns.push(c),
                    }
                    s.views_changed();
                })
                .child(el().w(14.0).center().child(icon(&rune_kit::glyphs::CHECK_SMALL, 14.0).color(p.blue).opacity(if at.is_some() { 1.0 } else { 0.0 })))
                .child(text(c.label()).font(BODY).color(if at.is_some() { p.tx } else { p.tx2 }).grow(1.0).shrink().truncate());
            if let Some(n) = at.filter(|&n| n > 0) {
                r = r.child(small_icon_button(format!("{key}-up"), &icons::ARROW_UP, "Move left", p).hover_alone().on_press(move |s: &mut App| {
                    s.shown_columns_mut().swap(n, n - 1);
                    s.views_changed();
                }));
            }
            list = list.child(r);
        }
        body = body.child(col().key("column-scroll").w_full().max_h(420.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list));
        body.child(divider(p)).child(menu_row("columns-reset", &icons::COLLAPSE, "Reset to default", p).on_press(|s: &mut App| {
            let default = if s.page == crate::app::Page::Library { crate::base::view::default_columns() } else { crate::base::store::page_columns() };
            *s.shown_columns_mut() = default;
            s.views_changed();
        }))
        .child(menu_row("columns-reset-widths", &icons::COLUMNS, "Reset widths", p).on_press(|s: &mut App| {
            s.shown_widths_mut().clear();
            s.views_changed();
        }))
    }
}

// Shared pieces

/// A select over every property; `set` applies the choice before the views save.
pub(super) fn property_select(s: &App, kit: &Kit, layer: &mut Vec<El>, key: String, current: Property, set: impl Fn(&mut App, Property) + Clone + 'static) -> El {
    let labels: Vec<&str> = Property::for_mode(s.mode()).iter().map(|p| p.label()).collect();
    select(s, kit, layer, key, &labels, current.label(), 128.0, move |s, label| {
        if let Some(prop) = Property::from_label(label) {
            set(s, prop);
            s.views_changed();
        }
    })
}

/// A kit `Select` whose open state lives in `App::open_select`; `choose` runs on a pick.
#[allow(clippy::too_many_arguments)]
pub(super) fn select(s: &App, kit: &Kit, layer: &mut Vec<El>, key: String, options: &[&str], current: &str, width: f32, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let open = s.open_select.as_deref() == Some(key.as_str());
    let toggle = key.clone();
    Select::new(key.clone(), options)
        .current(Some(current))
        .label(key.clone())
        .width(width)
        .open(open)
        .placement(Placement::BottomStart)
        .on_toggle(move |s: &mut App| s.open_select = if s.open_select.as_deref() == Some(toggle.as_str()) { None } else { Some(toggle.clone()) })
        .on_close(|s: &mut App| s.open_select = None)
        .on_choose(move |s: &mut App, v: &str, _tx: &mut Tx| {
            s.open_select = None;
            choose(s, v);
        })
        .build(kit, layer)
}

fn direction_button(key: String, direction: Direction, property: Property, p: &Palette) -> El {
    use crate::base::property::Kind;
    let label = match (property.kind(), direction) {
        (Kind::Text | Kind::List, Direction::Asc) => "A → Z",
        (Kind::Text | Kind::List, Direction::Desc) => "Z → A",
        (Kind::Date, Direction::Asc) => "Old → new",
        (Kind::Date, Direction::Desc) => "New → old",
        (_, Direction::Asc) => "0 → 9",
        (_, Direction::Desc) => "9 → 0",
    };
    row()
        .key(key)
        .h(24.0)
        .px(8.0)
        .radius(4.0)
        .center()
        .fill(p.ui.alpha(0.4))
        .hover(p.ui.alpha(0.35))
        .press(p.ui.alpha(0.7))
        .cursor(Cursor::Pointer)
        .label(format!("Direction: {label}"))
        .child(text(label).key("label").font(SMALL).color(p.tx))
}

pub(super) fn small_icon_button(key: String, glyph: &'static rune::Icon, help: &str, p: &Palette) -> El {
    el()
        .key(key)
        .size(24.0, 24.0)
        .radius(4.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Pointer)
        .label(help.to_string())
        .help(help.to_string())
        .child(icon(glyph, 14.0).color(p.tx3))
}

pub(super) fn menu_row(key: &'static str, glyph: &'static rune::Icon, label: &str, p: &Palette) -> El {
    row()
        .key(key)
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.ui.alpha(0.35))
        .press(p.ui.alpha(0.6))
        .cursor(Cursor::Pointer)
        .label(label.to_string())
        .child(icon(glyph, 14.0).color(p.tx2))
        .child(text(label.to_string()).font(BODY).color(p.tx2))
}

pub(super) fn hint(s: &str, p: &Palette) -> El {
    text(s.to_string()).font(SMALL).color(p.tx3).wrap().w_full().py(4.0).px(4.0)
}

pub(super) fn divider(p: &Palette) -> El {
    el().w_full().h(1.0).fill(p.ui.alpha(0.6))
}

//! The toolbar under the header: the view menu, and Sort, Group and Filter popovers.

use rune::{Cursor, El, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Placement, Select, TextInput, popover, scrim};

use super::*;
use crate::app::{App, Panel, QUEUE_ROW_H, Reorder, ReorderList};
use crate::query::property::{Mode, Property};
use crate::query::view::{Direction, GroupBy, Layout, Sort, View};
use crate::icons;
use crate::palette::Palette;

const BAR_H: f32 = 40.0;
/// Below this laid-out width, Columns, Sort, Group and Filter show only their icons.
const COMPACT_W: f32 = 900.0;
/// The album grid's toolbar also holds Grid/List and the card size slider, so it needs more.
const COMPACT_GRID_W: f32 = 940.0;
/// The width of a row's grip, or of the empty slot that keeps rows without one aligned.
pub(super) const GRIP_W: f32 = 18.0;

/// The toolbar for a window `width` points wide as laid out (unknown before the first layout).
pub(super) fn toolbar(s: &App, width: Option<f32>, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let view = s.views.active();
    let grid = view.mode == Mode::Albums && view.layout == Layout::Grid && s.page == crate::app::Page::Library;
    let compact = width.is_some_and(|w| w < if grid { COMPACT_GRID_W } else { COMPACT_W });
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
    let sort = panel(s, kit, layer, Panel::Sort, tool_button("sort", compact, &icons::SORT, sort_label, !view.sorts.is_empty(), s.panel == Some(Panel::Sort), p), sort_body(s, p, kit));
    let group = panel(s, kit, layer, Panel::Group, tool_button("group", compact, &icons::GROUP, group_label, view.group_by.is_some(), s.panel == Some(Panel::Group), p), group_body(s, p, kit));
    let filter_open = s.panel == Some(Panel::Filter);
    let filter = panel(
        s,
        kit,
        layer,
        Panel::Filter,
        tool_button("filter", compact, &icons::FILTER, filter_label, view.filter.condition_count() > 0, filter_open, p).help(if view.filter.children.is_empty() { "Filter songs".to_string() } else { view.filter.summary() }),
        |layer| super::filters::filter_body(s, p, kit, layer),
    );
    let columns_label = format!("Columns: {}", s.shown_columns().len());
    let columns = panel(s, kit, layer, Panel::Columns, tool_button("columns", compact, &icons::COLUMNS, columns_label, false, s.panel == Some(Panel::Columns), p), columns_body(s, p));
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
            if s.views.active().mode != mode {
                s.views.active_mut().set_mode(mode);
                s.clear_view_search();
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
            if s.views.active().layout != layout {
                s.views.active_mut().layout = layout;
                s.show_library();
                s.views_changed();
            }
        }));
        if view.layout == Layout::Grid && s.page == crate::app::Page::Library {
            bar = bar.child(el().w(12.0)).child(card_size_slider(s, compact, p));
        }
    }
    // Album grids show no song tables, unless an album page is open.
    let tables = !(albums && view.layout == Layout::Grid) || s.page != crate::app::Page::Library;
    bar.children((s.page == crate::app::Page::Library).then(|| view_search(s, compact, kit)))
        .child(el().grow(1.0))
        .children((s.page == crate::app::Page::Library).then(|| play_view_button(s, albums, p)))
        .children(tables.then_some(columns))
        .child(sort)
        .child(group)
        .child(filter)
}

/// The view search box: filters the active view by the title and its columns. Narrower when
/// `compact`, and shrinks rather than push the other controls out.
fn view_search(s: &App, compact: bool, kit: &Kit) -> El {
    TextInput::new(&s.view_search)
        .placeholder("Search view")
        .label("Search view")
        .leading_icon(&rune_kit::glyphs::SEARCH)
        .width(Some(if compact { 140.0 } else { 180.0 }))
        .clear_button()
        .on_edit(|s: &mut App| &mut s.view_search)
        .on_change(|s: &mut App, _tx| s.refilter())
        .on_action(|s: &mut App, _tx| {
            s.clear_view_search();
            s.refilter();
        })
        .on_key(|s: &mut App, ev, tx| match ev.key {
            rune::Key::Escape => {
                s.clear_view_search();
                s.refilter();
                tx.blur();
                true
            }
            _ => false,
        })
        .build(kit)
        .key("view-search")
        .radius(2.0)
        .shrink()
        .min_w(80.0)
}

/// How many songs, or albums in an album view, the active view shows, as a button that
/// plays them from the first, in the view's order. Dimmed while the view shows none.
fn play_view_button(s: &App, albums: bool, p: &Palette) -> El {
    let count = if albums { plural(s.shown_albums.len(), "album") } else { plural(s.filtered.len(), "song") };
    let empty = s.filtered.is_empty();
    let ink = if empty { p.tx3 } else { p.accent };
    let button = row()
        .key("play-view")
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(4.0)
        .fill(p.ui.alpha(0.45))
        .role("button")
        .label(format!("Play {count}"))
        .disabled(empty)
        .child(icon(&icons::PLAY, 13.0).color(ink))
        .child(text(count.clone()).key("result-count").label(count).font(TITLE).color(ink));
    if empty {
        return button.opacity(0.6);
    }
    let active = s.views.active;
    button
        .hover(p.ui.alpha(0.65))
        .press(p.ui.alpha(0.8))
        .cursor(Cursor::Pointer)
        .help("Play the songs in this view")
        .on_press_tx(move |s: &mut App, tx| s.play_view(active, tx))
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

/// A toolbar button: its icon and `label`, or the icon alone when `compact`, with `label`
/// as its tooltip.
fn tool_button(key: &'static str, compact: bool, glyph: &'static rune::Icon, label: String, active: bool, open: bool, p: &Palette) -> El {
    let ink = if active { p.accent } else { p.tx2 };
    let button = row()
        .key(key)
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(4.0)
        .fill(p.ui.alpha(if open { 0.75 } else { 0.45 }))
        .hover(p.ui.alpha(0.65))
        .press(p.ui.alpha(0.8))
        .cursor(Cursor::Pointer)
        .label(label.clone())
        .child(icon(glyph, 15.0).color(ink));
    match compact {
        true => button.px(7.0).help(label),
        false => button.child(text(label).key("label").font(TITLE).color(ink)),
    }
}

/// A segmented control: one pill per option, the current one tinted with the mode accent.
pub(super) fn segmented(key: &'static str, options: &[(&'static str, &'static str)], current: &str, p: &Palette, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let mut seg = row().key(key).h(28.0).padding(2.0).gap(2.0).radius(4.0).fill(p.ui.alpha(0.45)).role("radiogroup").label(key).focus_group();
    for &(value, label) in options {
        let on = value == current;
        let choose = choose.clone();
        seg = seg.child(
            row()
                .key(value)
                .h(24.0)
                .px(10.0)
                .radius(3.0)
                .center()
                .fill(if on { p.selected.alpha(if p.dark { 0.24 } else { 0.18 }) } else { p.bg.alpha(0.0) })
                .hover(p.ui.alpha(0.3))
                .cursor(Cursor::Pointer)
                .role("radio")
                .checked(on)
                .focusable(true)
                .label(label)
                .motion(SNAPPY)
                .on_press(move |s: &mut App| choose(s, value))
                .child(text(label).font(TITLE).color(if on { p.selected } else { p.tx2 })),
        );
    }
    seg
}

/// The album grid's card size: small and large grid glyphs either side of a slider. The
/// view keeps its own once dragged; double-click the slider to follow the default again, and
/// the dot after it makes this size the default. Narrower when `compact`.
fn card_size_slider(s: &App, compact: bool, p: &Palette) -> El {
    use crate::query::view::{CARD_SIZE_MAX, CARD_SIZE_MIN};
    let size = s.card_size();
    let fraction = (size - CARD_SIZE_MIN) / (CARD_SIZE_MAX - CARD_SIZE_MIN);
    let own = s.views.active().card_size.is_some();
    let track = super::player::slider_track("card-size", fraction, true, p)
        .w(if compact { 60.0 } else { 96.0 })
        .label("Card size")
        .help("Card size (double-click to use the default)")
        .a11y_value(format!("{}", size.round()))
        .on_step(|s: &mut App, steps, _tx| s.set_card_size(s.card_size() + steps as f32 * 8.0, true))
        .on_double_press(|s: &mut App| s.reset_card_size())
        .on_drag(|s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| {
            let at = |d: &rune::Drag| CARD_SIZE_MIN + (d.x / d.w).clamp(0.0, 1.0) * (CARD_SIZE_MAX - CARD_SIZE_MIN);
            match d.phase {
                rune::Phase::Down => {
                    d.data = s.card_size();
                    s.set_card_size(at(d), false);
                }
                rune::Phase::Move => s.set_card_size(at(d), false),
                rune::Phase::Up if d.cancelled() => s.set_card_size(d.data, true),
                rune::Phase::Up => s.set_card_size(at(d), true),
            }
        });
    let mut r = row()
        .key("card-size-control")
        .gap(6.0)
        .center_y()
        .child(icon(&icons::GRID_SMALL, 14.0).color(p.tx3))
        .child(track)
        .child(icon(&icons::GRID_LARGE, 14.0).color(p.tx3));
    if own && (size - s.views.card_size).abs() >= 0.5 {
        r = r.child(el().w(4.0)).child(
            super::pill_button("card-size-default", "Set as default", p)
                .help("Album grids without their own size use this one")
                .on_press(|s: &mut App| s.make_card_size_default()),
        );
    }
    r
}

// Views

fn views_menu(s: &App, p: &Palette, kit: &Kit, layer: &mut Vec<El>) -> El {
    let view = s.views.active();
    let open = s.panel == Some(Panel::Views);
    let body = |_: &mut Vec<El>| views_body(s, p, kit);
    let trigger = row()
        .key("views")
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(3.0)
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
    for (i, view) in s.views.list.iter().enumerate() {
        let active = i == s.views.active;
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
            .radius(3.0)
            .fill(if active { p.ui.alpha(0.55) } else { p.bg.alpha(0.0) })
            .hover(p.ui.alpha(0.35))
            .role("option")
            .checked(active)
            .label(view.name.clone())
            .child(el().w(14.0).center().child(icon(&rune_kit::glyphs::CHECK_SMALL, 14.0).color(p.accent).opacity(if active { 1.0 } else { 0.0 })))
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
                .child(small_icon_button(format!("play-view{i}"), &icons::PLAY, "Play view", p).on_press_tx(move |s: &mut App, tx| {
                    s.play_view(i, tx);
                    s.close_panel();
                }))
                .child(small_icon_button(format!("rename{i}"), &icons::EDIT, "Rename view", p).on_press(move |s: &mut App| start_rename(s, i)))
                .child(small_icon_button(format!("duplicate{i}"), &icons::COPY, "Duplicate view", p).on_press(move |s: &mut App| duplicate_view(s, i)));
            if s.views.list.len() > 1 {
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
                    let name = unique_name(&s.views.list, "New view");
                    s.views.list.push(View::new(name));
                    let i = s.views.list.len() - 1;
                    switch_view(s, i, tx);
                    start_rename(s, i);
                }),
            ),
        )
}

fn switch_view(s: &mut App, i: usize, tx: &mut Tx) {
    if s.views.active != i {
        s.views.active = i;
        s.clear_view_search();
        s.collapsed.clear();
        s.filter_edits.clear();
        s.views_changed();
        tx.scroll_to_instant("tracks", 0.0, 0.0);
    }
}

fn start_rename(s: &mut App, i: usize) {
    let name = s.views.list[i].name.clone();
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
            s.views.list[i].name = name;
            s.views_changed();
        }
    }
    tx.blur();
}

fn duplicate_view(s: &mut App, i: usize) {
    let mut copy = s.views.list[i].clone();
    copy.name = unique_name(&s.views.list, &format!("{} copy", copy.name));
    s.views.list.insert(i + 1, copy);
    if s.views.active > i {
        s.views.active += 1;
    }
    s.views_changed();
}

fn delete_view(s: &mut App, i: usize) {
    if s.views.list.len() <= 1 {
        return;
    }
    let deleted_active = s.views.active == i;
    s.views.list.remove(i);
    if s.views.active > i || s.views.active == s.views.list.len() {
        s.views.active = s.views.active.saturating_sub(1);
    }
    if deleted_active {
        s.clear_view_search();
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
        let view = s.views.active();
        let mut body = col().key("sort-body").w(360.0).padding(8.0).gap(4.0);
        if view.sorts.is_empty() {
            body = body.child(hint(if view.mode == Mode::Albums { "Albums are in library order: artist, then album." } else { "Songs are in library order: artist, album, then track." }, p));
        }
        for (n, sort) in view.sorts.iter().enumerate() {
            let key = format!("sort{n}");
            let prop = property_select(s, kit, layer, format!("{key}-prop"), sort.property, move |s, prop| {
                s.views.active_mut().sorts[n].property = prop;
            });
            let dir = direction_button(format!("{key}-dir"), sort.direction, sort.property, p).on_press(move |s: &mut App| {
                let d = &mut s.views.active_mut().sorts[n].direction;
                *d = d.flip();
                s.views_changed();
            });
            let mut r = row().key(key.clone()).h(32.0).gap(6.0).center_y().offset(0.0, reorder_shift(s, ReorderList::Sorts, n));
            if is_dragged(s, ReorderList::Sorts, n) {
                r = r.fill(p.ui.alpha(0.6)).radius(3.0).raise();
            }
            if view.sorts.len() > 1 {
                r = r.child(reorder_handle(format!("{key}-handle"), ReorderList::Sorts, n, p));
            }
            r = r.child(text(if n == 0 { "Sort by" } else { "then by" }).font(SMALL).color(p.tx3).w(52.0));
            r = r.child(prop).child(dir).child(el().grow(1.0));
            r = r.child(small_icon_button(format!("sort{n}-remove"), &icons::TRASH, "Remove sort", p).on_press(move |s: &mut App| {
                s.views.active_mut().sorts.remove(n);
                s.views_changed();
            }));
            body = body.child(r);
        }
        let unused = Property::for_mode(view.mode).iter().copied().find(|p| !view.sorts.iter().any(|s| s.property == *p));
        if let Some(next) = unused {
            body = body.child(menu_row("add-sort", &icons::PLUS, "Add sort", p).on_press(move |s: &mut App| {
                s.views.active_mut().sorts.push(Sort { property: next, direction: Direction::Asc });
                s.views_changed();
            }));
        }
        body
    }
}

// Group

fn group_body<'a>(s: &'a App, p: &'a Palette, kit: &'a Kit) -> impl FnOnce(&mut Vec<El>) -> El + 'a {
    move |layer| {
        let view = s.views.active();
        let mut body = col().key("group-body").w(360.0).padding(8.0).gap(4.0);
        match view.group_by {
            None => {
                let what = if view.mode == Mode::Albums { "albums" } else { "songs" };
                body = body.child(hint(&format!("Group {what} into sections by one property. One with several artists or genres shows under each."), p));
                body = body.child(menu_row("add-group", &icons::PLUS, "Group by…", p).on_press(|s: &mut App| {
                    let property = if s.mode() == Mode::Albums { Property::AlbumArtist } else { Property::Album };
                    s.views.active_mut().group_by = Some(GroupBy { property, direction: Direction::Asc });
                    s.collapsed.clear();
                    s.views_changed();
                }));
            }
            Some(group) => {
                let prop = property_select(s, kit, layer, "group-prop".into(), group.property, |s, prop| {
                    if let Some(g) = &mut s.views.active_mut().group_by {
                        g.property = prop;
                    }
                    s.collapsed.clear();
                });
                let dir = direction_button("group-dir".into(), group.direction, group.property, p).on_press(|s: &mut App| {
                    if let Some(g) = &mut s.views.active_mut().group_by {
                        g.direction = g.direction.flip();
                    }
                    s.views_changed();
                });
                let remove = small_icon_button("group-remove".into(), &icons::TRASH, "Stop grouping", p).on_press(|s: &mut App| {
                    s.views.active_mut().group_by = None;
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
        body = body.child(hint(if page { "Shown after the title in album and artist pages' song tables. Click to show or hide; drag a grip to reorder." } else { "Shown after the title in this view's song tables. Click to show or hide; drag a grip to reorder." }, p));
        if !page && s.mode() == crate::query::property::Mode::Songs {
            body = body.child(covers_switch(s.views.active().covers, p)).child(divider(p));
        }
        let mut list = col().key("column-list").w_full().gap(2.0);
        // Shown columns first in their order, then the rest in property order.
        let order = shown.iter().copied().chain(Property::ALL.into_iter().filter(|c| *c != Property::Title && !shown.contains(c)));
        for c in order {
            let at = shown.iter().position(|x| *x == c);
            let key = format!("column-{}", c.label().to_lowercase().replace(' ', "-"));
            let slot = match at {
                Some(n) if shown.len() > 1 => reorder_handle(format!("{key}-handle"), ReorderList::Columns, n, p),
                _ => el().w(GRIP_W),
            };
            let shift = at.map_or(0.0, |n| reorder_shift(s, ReorderList::Columns, n));
            let mut r = row()
                .key(key.clone())
                .offset(0.0, shift)
                .w_full()
                .h(30.0)
                .pl(8.0)
                .pr(4.0)
                .gap(8.0)
                .center_y()
                .radius(3.0)
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
                .child(slot)
                .child(el().w(14.0).center().child(icon(&rune_kit::glyphs::CHECK_SMALL, 14.0).color(p.accent).opacity(if at.is_some() { 1.0 } else { 0.0 })))
                .child(text(c.label()).font(BODY).color(if at.is_some() { p.tx } else { p.tx2 }).grow(1.0).shrink().truncate());
            if at.is_some_and(|n| is_dragged(s, ReorderList::Columns, n)) {
                r = r.fill(p.ui.alpha(0.6)).raise();
            }
            list = list.child(r);
        }
        body = body.child(col().key("column-scroll").w_full().max_h(420.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list));
        body.child(divider(p)).child(menu_row("columns-reset", &icons::COLLAPSE, "Reset to default", p).on_press(|s: &mut App| {
            let default = if s.page == crate::app::Page::Library { crate::query::view::default_columns() } else { crate::query::store::page_columns() };
            *s.shown_columns_mut() = default;
            s.views_changed();
        }))
        .child(menu_row("columns-reset-widths", &icons::COLUMNS, "Reset widths", p).on_press(|s: &mut App| {
            s.shown_widths_mut().clear();
            s.views_changed();
        }))
    }
}

/// Whether the song view's rows show their album's cover before the title.
fn covers_switch(on: bool, p: &Palette) -> El {
    let knob = el().size(12.0, 12.0).radius(6.0).fill(rune::Color::WHITE).motion(SNAPPY);
    let track = row().size(26.0, 16.0).radius(8.0).padding(2.0).center_y().fill(if on { p.accent } else { p.ui2 });
    let track = if on { track.justify_end() } else { track.justify_start() }.child(knob);
    row()
        .key("columns-covers")
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(3.0)
        .hover(p.ui.alpha(0.35))
        .cursor(Cursor::Pointer)
        .role("switch")
        .checked(on)
        .label("Show covers")
        .on_press(|s: &mut App| {
            let view = s.views.active_mut();
            view.covers = !view.covers;
            s.views_changed();
        })
        .child(text("Show covers").font(BODY).color(p.tx).grow(1.0))
        .child(track)
}

// Reordering

impl ReorderList {
    /// A row's height plus the gap below it: how far a row moves when a drag passes it.
    fn pitch(self) -> f32 {
        match self {
            ReorderList::Sorts => 36.0,
            ReorderList::Columns => 32.0,
            ReorderList::Queue => QUEUE_ROW_H,
        }
    }

    /// The first row a drag may land on: the queue's upcoming entries start after the current one.
    fn first(self, s: &App) -> usize {
        match self {
            ReorderList::Sorts | ReorderList::Columns => 0,
            ReorderList::Queue => s.queue.as_ref().map_or(0, |q| q.pos() + 1),
        }
    }

    /// How many rows the list has.
    fn len(self, s: &App) -> usize {
        match self {
            ReorderList::Sorts => s.views.active().sorts.len(),
            ReorderList::Columns => s.shown_columns().len(),
            ReorderList::Queue => s.queue.as_ref().map_or(0, |q| q.entries().len()),
        }
    }

    /// Moves the row at `from` to `to`, then saves the views.
    fn move_item(self, s: &mut App, from: usize, to: usize) {
        match self {
            ReorderList::Sorts => {
                let sorts = &mut s.views.active_mut().sorts;
                let item = sorts.remove(from);
                sorts.insert(to, item);
            }
            ReorderList::Columns => {
                let columns = s.shown_columns_mut();
                let item = columns.remove(from);
                columns.insert(to, item);
            }
            ReorderList::Queue => {
                s.queue_move(from, to);
                return;
            }
        }
        s.views_changed();
    }
}

/// The row a drag of `dy` points from row `from` lands on: one row per `pitch` points, kept
/// inside a list of `len` rows.
fn reorder_target(from: usize, dy: f32, pitch: f32, first: usize, len: usize) -> usize {
    let to = from as f32 + (dy / pitch).round();
    to.clamp(first as f32, len.saturating_sub(1).max(first) as f32) as usize
}

/// Whether row `n` of `list` is the one being dragged.
pub(super) fn is_dragged(s: &App, list: ReorderList, n: usize) -> bool {
    s.reorder.is_some_and(|o| o.list == list && o.from == n)
}

/// How far row `n` of `list` shifts while a drag reorders it: the dragged row follows the
/// pointer, and the rows it passes slide one row aside. Zero when no drag of `list` is on.
pub(super) fn reorder_shift(s: &App, list: ReorderList, n: usize) -> f32 {
    let Some(o) = s.reorder.filter(|o| o.list == list) else { return 0.0 };
    let to = reorder_target(o.from, o.dy, list.pitch(), list.first(s), list.len(s));
    if n == o.from {
        o.dy
    } else if to > o.from && n > o.from && n <= to {
        -list.pitch()
    } else if to < o.from && n >= to && n < o.from {
        list.pitch()
    } else {
        0.0
    }
}

/// A row's grip, at its left edge: drag it to move the row, or press Alt+Up or Alt+Down while
/// it has focus. `n` is the row's index in `list`.
pub(super) fn reorder_handle(key: String, list: ReorderList, n: usize, p: &Palette) -> El {
    let own = key.clone();
    el().key(key)
        .size(GRIP_W, 24.0)
        .radius(2.0)
        .center()
        .hover(p.ui.alpha(0.6))
        .press(p.ui)
        .cursor(Cursor::Grab)
        .label("Drag to reorder")
        .help("Drag to reorder; Alt+↑ and Alt+↓ move it too")
        .focusable(true)
        .child(icon(&icons::GRIP, 14.0).color(p.tx2))
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| match d.phase {
            rune::Phase::Down => s.reorder = Some(Reorder { list, from: n, dy: 0.0 }),
            rune::Phase::Move => {
                if let Some(o) = s.reorder.as_mut() {
                    o.dy = d.dy;
                }
            }
            rune::Phase::Up => {
                let taken = s.reorder.take();
                if taken.is_none() || d.cancelled() {
                    return;
                }
                let to = reorder_target(n, d.dy, list.pitch(), list.first(s), list.len(s));
                if to != n {
                    list.move_item(s, n, to);
                }
            }
        })
        .on_key(move |s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
            let to = match ev.key {
                rune::Key::Up if ev.mods.alt => n.checked_sub(1),
                rune::Key::Down if ev.mods.alt => Some(n + 1).filter(|&to| to < list.len(s)),
                _ => None,
            };
            let Some(to) = to else { return false };
            list.move_item(s, n, to);
            // A sort's grip is keyed by its place; a column's by its name, which moves with it.
            let next = match list {
                ReorderList::Sorts => format!("sort{to}-handle"),
                ReorderList::Columns | ReorderList::Queue => own.clone(),
            };
            tx.focus(next, true);
            true
        })
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
    use crate::query::property::Kind;
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
        .radius(2.0)
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
        .radius(2.0)
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
        .radius(3.0)
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

//! Transport and library icons, drawn on a 24-unit grid.

use rune::Icon;


pub static PLAY: Icon = Icon { name: "rm-play", view: 24.0, body: r#"<path d="M7.5 4.8 L19 12 L7.5 19.2 Z" fill="currentColor" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"# };

pub static PAUSE: Icon = Icon {
    name: "rm-pause",
    view: 24.0,
    body: r#"<rect x="6" y="5" width="4" height="14" rx="1.2" fill="currentColor"/><rect x="14" y="5" width="4" height="14" rx="1.2" fill="currentColor"/>"#,
};

pub static PREV: Icon = Icon {
    name: "rm-prev",
    view: 24.0,
    body: r#"<rect x="5" y="5.5" width="2.4" height="13" rx="1" fill="currentColor"/><path d="M19 6 L9.5 12 L19 18 Z" fill="currentColor" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/>"#,
};

pub static NEXT: Icon = Icon {
    name: "rm-next",
    view: 24.0,
    body: r#"<rect x="16.6" y="5.5" width="2.4" height="13" rx="1" fill="currentColor"/><path d="M5 6 L14.5 12 L5 18 Z" fill="currentColor" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/>"#,
};

macro_rules! stroked {
    ($name:literal, $($d:literal),+) => {
        Icon { name: $name, view: 24.0, body: concat!($("<path d=\"", $d, "\" stroke=\"currentColor\" stroke-width=\"1.8\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"),+) }
    };
}

pub static SHUFFLE: Icon = stroked!(
    "rm-shuffle",
    "M3 7 H6.5 C8.5 7 9.7 8 10.8 9.6 L13.2 13.4 C14.3 15 15.5 16 17.5 16 H20.5",
    "M3 16 H6.5 C8 16 9 15.5 9.9 14.5",
    "M14.1 8.5 C15 7.5 16 7 17.5 7 H20.5",
    "M18 4.5 L20.5 7 L18 9.5",
    "M18 13.5 L20.5 16 L18 18.5"
);

pub static REPEAT: Icon = stroked!(
    "rm-repeat",
    "M4 11 V9.5 C4 7.6 5.6 6 7.5 6 H19",
    "M16 3 L19 6 L16 9",
    "M20 13 V14.5 C20 16.4 18.4 18 16.5 18 H5",
    "M8 21 L5 18 L8 15"
);

pub static REPEAT_ONE: Icon = Icon {
    name: "rm-repeat-one",
    view: 24.0,
    body: concat!(
        r#"<path d="M4 11 V9.5 C4 7.6 5.6 6 7.5 6 H19" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
        r#"<path d="M16 3 L19 6 L16 9" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
        r#"<path d="M20 13 V14.5 C20 16.4 18.4 18 16.5 18 H5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
        r#"<path d="M8 21 L5 18 L8 15" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
        r#"<circle cx="12" cy="12" r="2" fill="currentColor"/>"#
    ),
};

pub static VOLUME: Icon = Icon {
    name: "rm-volume",
    view: 24.0,
    body: concat!(
        r#"<path d="M4 9.2 H7.4 L11.8 5.5 V18.5 L7.4 14.8 H4 Z" fill="currentColor" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#,
        r#"<path d="M15.2 9.2 C16.6 10.8 16.6 13.2 15.2 14.8" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>"#,
        r#"<path d="M17.8 6.6 C20.6 9.6 20.6 14.4 17.8 17.4" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>"#
    ),
};

pub static MUTED: Icon = Icon {
    name: "rm-muted",
    view: 24.0,
    body: concat!(
        r#"<path d="M4 9.2 H7.4 L11.8 5.5 V18.5 L7.4 14.8 H4 Z" fill="currentColor" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#,
        r#"<path d="M15.5 9.5 L20.5 14.5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>"#,
        r#"<path d="M20.5 9.5 L15.5 14.5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>"#
    ),
};

pub static NOTE: Icon = Icon {
    name: "rm-note",
    view: 24.0,
    body: concat!(
        r#"<path d="M9 17.5 V6.2 L19 4 V15.5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#,
        r#"<circle cx="6.6" cy="17.6" r="2.6" fill="currentColor"/>"#,
        r#"<circle cx="16.6" cy="15.6" r="2.6" fill="currentColor"/>"#
    ),
};

/// Three equalizer bars: the row that is playing.
pub static BARS: Icon = Icon {
    name: "rm-bars",
    view: 24.0,
    body: r#"<rect x="4" y="9" width="3.6" height="11" rx="1" fill="currentColor"/><rect x="10.2" y="4" width="3.6" height="16" rx="1" fill="currentColor"/><rect x="16.4" y="12" width="3.6" height="8" rx="1" fill="currentColor"/>"#,
};


macro_rules! line_icon {
    ($id:ident, $name:literal, $($d:literal),+) => {
        pub static $id: Icon = Icon {
            name: $name,
            view: 24.0,
            body: concat!($("<path d=\"", $d, "\" stroke=\"currentColor\" stroke-width=\"1.8\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"),+),
        };
    };
}

// Toolbar glyphs, after Lucide's (ISC), which Obsidian's Bases toolbar uses.
line_icon!(VIEWS, "rm-views", "M3 5 H21 V19 H3 Z", "M3 10 H21", "M9 10 V19");
line_icon!(SORT, "rm-sort", "M7 4 V20", "M3.5 16.5 L7 20 L10.5 16.5", "M17 20 V4", "M13.5 7.5 L17 4 L20.5 7.5");
line_icon!(GROUP, "rm-group", "M4 5 H20 V10 H4 Z", "M4 14 H20 V19 H4 Z");
line_icon!(FILTER, "rm-filter", "M3 6 H21", "M7 12 H17", "M10 18 H14");
line_icon!(PLUS, "rm-plus", "M12 5 V19", "M5 12 H19");
line_icon!(TRASH, "rm-trash", "M4 7 H20", "M9 7 V4.5 H15 V7", "M6 7 L7 20 H17 L18 7", "M10 11 V16", "M14 11 V16");
line_icon!(EDIT, "rm-edit", "M4 20 H8 L19 9 L15 5 L4 16 Z", "M13 7 L17 11");
line_icon!(COPY, "rm-copy", "M8 8 H20 V20 H8 Z", "M16 8 V4 H4 V16 H8");
line_icon!(ARROW_UP, "rm-arrow-up", "M12 19 V5", "M6 11 L12 5 L18 11");
line_icon!(COLLAPSE, "rm-collapse", "M7 9 L12 4 L17 9", "M7 15 L12 20 L17 15");
line_icon!(EXPAND, "rm-expand", "M7 4 L12 9 L17 4", "M7 20 L12 15 L17 20");
line_icon!(BACK, "rm-back", "M15 5 L8 12 L15 19");
line_icon!(INFO, "rm-info", "M12 3 A9 9 0 1 0 12.01 3 Z", "M12 11 V16.5", "M12 7.6 V7.7");
line_icon!(COLUMNS, "rm-columns", "M3 5 H21 V19 H3 Z", "M9 5 V19", "M15 5 V19");
line_icon!(FOLDER_OPEN, "rm-folder-open", "M3 7 V18 H19 L21.5 10.5 H7 L4.5 18", "M3 7 V5 H9 L11 7 H18 V10.5");
line_icon!(SETTINGS, "rm-settings", "M4 7 H14", "M18 7 H20", "M14 5 V9", "M4 17 H6", "M10 17 H20", "M10 15 V19");

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


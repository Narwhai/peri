//! Memoized page models. Pages are built from the whole library and views run every frame, so
//! each page keeps the model it last built and reuses it while its inputs are unchanged.

use std::cell::RefCell;
use std::rc::Rc;

use crate::artist::ArtistPage;
use crate::collab::CollabPage;
use crate::label::LabelPage;
use crate::relations::Relation;
use crate::search::SearchResults;
use crate::stats::Totals;
use crate::top::Ranked;

/// The home page's leaderboards: the top artists, albums and songs in one window.
pub struct TopModel {
    pub artists: Vec<Ranked<String>>,
    pub albums: Vec<Ranked<usize>>,
    pub songs: Vec<Ranked<usize>>,
}

/// An artist's page, and the artists involved with them, each with their page.
pub type ArtistModel = (ArtistPage, Vec<(Relation, ArtistPage)>);

/// The last model a page built, with the key it was built for. Views only have `&App`, so it
/// sits behind a `RefCell`.
pub struct Memo<K, V>(RefCell<Option<(K, Rc<V>)>>);

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Memo(RefCell::new(None))
    }
}

impl<K: PartialEq, V> Memo<K, V> {
    /// The model for `key`: the kept one when its key matches, else one `build` makes, which then
    /// replaces it.
    pub fn get(&self, key: K, build: impl FnOnce() -> V) -> Rc<V> {
        if let Some((kept, model)) = self.0.borrow().as_ref()
            && *kept == key
        {
            return model.clone();
        }
        let model = Rc::new(build());
        *self.0.borrow_mut() = Some((key, model.clone()));
        model
    }
}

/// Each page's memo. Keys carry the library's data version, so a change to the library rebuilds them.
#[derive(Default)]
pub struct Pages {
    /// Keyed by the trimmed, lowercase query.
    pub search: Memo<(String, u64), SearchResults>,
    /// Keyed by the artist's name and whether their aliases are in.
    pub artist: Memo<(String, bool, u64), ArtistModel>,
    pub collab: Memo<(Vec<String>, u64), CollabPage>,
    pub label: Memo<(String, u64), LabelPage>,
    pub recent_albums: Memo<u64, Vec<usize>>,
    pub recent_songs: Memo<u64, Vec<usize>>,
    pub totals: Memo<u64, Totals>,
    /// Keyed by the library's data version, the window and the day, which is what the windows
    /// count back from.
    pub top: Memo<(u64, crate::top::Window, i64), TopModel>,
    /// Keyed by the library's data version: the genre and decade shares.
    pub mix: Memo<u64, (crate::mix::Mix, crate::mix::Mix)>,
    /// Keyed by the library's data version, the local offset and the day: the heatmap's days.
    pub activity: Memo<(u64, i64, i64), crate::activity::Activity>,
    /// Keyed like `top`: each discovery tab's albums.
    pub discover: Memo<(u64, crate::discover::Tab, i64), Vec<usize>>,
    /// Keyed like `top`.
    pub people: Memo<(u64, crate::top::Window, i64), Vec<crate::people::RoleBoard>>,
    /// Keyed by the library's data version, the order and the Random seed.
    pub unlistened: Memo<(u64, crate::app::UnlistenedOrder, u64), Vec<usize>>,
}

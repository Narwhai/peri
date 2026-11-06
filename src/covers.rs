//! Album covers for the grid and lists: read from a song, decoded and shrunk on worker threads,
//! and cached by count so a library of thousands of albums holds only the covers in view.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use rune::{Image, ImageCache, Waker};

/// Cover edge in pixels for cards and lists: twice the largest card at 2× is ~440 px.
pub const THUMB_PX: u32 = 480;
/// Covers kept decoded. At 480 px a cover is 0.9 MB (plus mips), so this caps them near 200 MB.
const CAPACITY: usize = 160;
const WORKERS: usize = 3;

#[derive(Clone)]
enum Slot {
    /// Being read and shrunk.
    Pending,
    /// The song has no cover.
    Missing,
    Ready(Image),
}

struct Job {
    path: PathBuf,
}

/// A worker's answer: the song, and its shrunk cover as (width, height, RGBA) if it has one.
type Done = (PathBuf, Option<(u32, u32, Vec<u8>)>);

/// Covers keyed by the song they're read from (an album's first song). Views ask for covers
/// as they build cards, so the cache is behind a `RefCell`.
pub struct Covers {
    inner: RefCell<Inner>,
    jobs: Option<Sender<Job>>,
    done: Option<Receiver<Done>>,
    images: ImageCache,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<PathBuf, Slot>,
    /// Least recently used first.
    order: VecDeque<PathBuf>,
    /// Bumped when covers arrive, so memoized cards rebuild with them.
    generation: u64,
}

impl Covers {
    pub fn new(waker: Waker) -> Covers {
        let (jobs, rx) = channel::<Job>();
        let (done_tx, done) = channel();
        let rx = Arc::new(Mutex::new(rx));
        for n in 0..WORKERS {
            let (rx, done_tx, waker) = (rx.clone(), done_tx.clone(), waker.clone());
            std::thread::Builder::new()
                .name(format!("covers-{n}"))
                .spawn(move || loop {
                    let Ok(job) = rx.lock().unwrap_or_else(|e| e.into_inner()).recv() else { return };
                    let thumb = crate::library::read_cover(&job.path).and_then(|bytes| shrink(&bytes, THUMB_PX));
                    if done_tx.send((job.path, thumb)).is_err() {
                        return;
                    }
                    waker();
                })
                .expect("spawn a cover worker");
        }
        Covers { inner: RefCell::new(Inner::default()), jobs: Some(jobs), done: Some(done), images: ImageCache::new(waker) }
    }

    /// Reads and shrinks covers on the calling thread, for headless runs and tests, whose
    /// frames run on a virtual clock and must show what a settled window shows.
    pub fn synchronous(waker: Waker) -> Covers {
        Covers { inner: RefCell::new(Inner::default()), jobs: None, done: None, images: ImageCache::new(waker) }
    }

    /// The cover for `song`, asking for it when it isn't known yet. `None` while it loads or
    /// when there is none; the view draws a placeholder either way.
    pub fn get(&self, song: &PathBuf) -> Option<Image> {
        let known = self.inner.borrow().slots.get(song).cloned();
        match known {
            Some(Slot::Ready(image)) => {
                self.inner.borrow_mut().touch(song);
                Some(image)
            }
            Some(Slot::Pending | Slot::Missing) => None,
            None => match &self.jobs {
                Some(jobs) => {
                    self.inner.borrow_mut().slots.insert(song.clone(), Slot::Pending);
                    let _ = jobs.send(Job { path: song.clone() });
                    None
                }
                None => {
                    let thumb = crate::library::read_cover(song).and_then(|bytes| shrink(&bytes, THUMB_PX));
                    let image = thumb.map(|(w, h, rgba)| self.images.rgba(w, h, rgba));
                    if let Some(image) = &image {
                        // The cache decodes on its own worker; a snapshot waits for it.
                        let _ = image.wait();
                    }
                    self.inner.borrow_mut().arrive(song.clone(), image.clone().map_or(Slot::Missing, Slot::Ready));
                    image
                }
            },
        }
    }

    /// Takes finished covers; true when any arrived.
    pub fn drain(&self) -> bool {
        let Some(done) = &self.done else { return false };
        let mut any = false;
        while let Ok((path, thumb)) = done.try_recv() {
            any = true;
            let slot = match thumb {
                Some((w, h, rgba)) => Slot::Ready(self.images.rgba(w, h, rgba)),
                None => Slot::Missing,
            };
            self.inner.borrow_mut().arrive(path, slot);
        }
        any
    }

    pub fn generation(&self) -> u64 {
        self.inner.borrow().generation
    }
}

impl Inner {
    fn arrive(&mut self, path: PathBuf, slot: Slot) {
        self.slots.insert(path.clone(), slot);
        self.touch(&path);
        self.generation += 1;
        while self.order.len() > CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.slots.remove(&old);
            }
        }
    }

    fn touch(&mut self, path: &PathBuf) {
        if let Some(at) = self.order.iter().position(|p| p == path) {
            self.order.remove(at);
        }
        self.order.push_back(path.clone());
    }
}

/// Decodes `bytes` and shrinks the image to fit `px` square, as straight-alpha RGBA. Covers
/// already that small keep their size. None when the bytes aren't an image.
pub fn shrink(bytes: &[u8], px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > px || img.height() > px { img.thumbnail(px, px) } else { img };
    let rgba = img.into_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 60, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn large_covers_shrink_to_fit_and_small_ones_keep_their_size() {
        let (w, h, rgba) = shrink(&png(3000, 1500), THUMB_PX).unwrap();
        assert_eq!((w, h), (480, 240));
        assert_eq!(rgba.len(), 480 * 240 * 4);
        assert_eq!(&rgba[..4], &[200, 30, 60, 255]);
        assert_eq!(shrink(&png(300, 300), THUMB_PX).map(|(w, h, _)| (w, h)), Some((300, 300)));
        assert!(shrink(b"not an image", THUMB_PX).is_none());
    }
}

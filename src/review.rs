//! A dropped clip before anything runs: what it is, a frame of it, and an
//! estimate of the chosen settings, re-measured as they change.
use crate::{
    clip::{self, ClipInfo, Frame},
    estimate::{self, Estimate},
    settings::Settings,
    tools::Tools,
};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use zgui::{
    background::BackgroundPool,
    compose::Tasks,
    image::ImageData,
    reactive::{Runtime, Signal},
    timer::sleep,
};

/// Preview frames: about the preview's size on a Retina display.
const PREVIEW: (u32, u32) = (1280, 1280);
/// How long settings must hold still before they're measured.
const SETTLE: Duration = Duration::from_millis(250);

/// Decoded pixels, compared by identity.
#[derive(Clone)]
pub struct Pic(pub Arc<ImageData>);

impl PartialEq for Pic {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Pic {
    fn from(frame: Frame) -> Option<Pic> {
        ImageData::new(frame.width, frame.height, frame.rgba)
            .ok()
            .map(|i| Pic(Arc::new(i)))
    }
}

/// The compressed preview frame's size: the preview's, cropped as the
/// settings crop, so it lines up with [`Review::cropped_before`].
fn preview_size(settings: &Settings, info: &ClipInfo) -> (u32, u32) {
    let full = info.fit(PREVIEW.0, PREVIEW.1);
    match settings.crop.filter(|c| !c.is_full()) {
        Some(crop) => {
            let (w, h, _, _) = crop.pixels(full.0, full.1);
            (w, h)
        }
        None => full,
    }
}

/// What the plate shows: the compare, or the crop being edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Compare,
    Crop,
}

#[derive(Clone, PartialEq)]
pub enum Loaded {
    Reading,
    Ready(Rc<ClipInfo>),
    Failed(String),
}

#[derive(Clone)]
pub enum Est {
    Waiting,
    Ready(Rc<Estimate>),
    Failed(String),
}

impl PartialEq for Est {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Est::Waiting, Est::Waiting) => true,
            (Est::Ready(a), Est::Ready(b)) => Rc::ptr_eq(a, b),
            (Est::Failed(a), Est::Failed(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct Review {
    pub id: u64,
    pub input: PathBuf,
    pub clip: Signal<Loaded>,
    pub before: Signal<Option<Pic>>,
    pub after: Signal<Option<Pic>>,
    pub estimate: Signal<Est>,
    /// A measurement is running (the last result stays shown, dimmed).
    pub measuring: Signal<bool>,
    /// Compare or crop: the review screen and the Clip menu both set it.
    pub mode: Signal<Mode>,
    dir: PathBuf,
    generation: Arc<AtomicU64>,
    _cleanup: Rc<Cleanup>,
}

impl PartialEq for Review {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// When the review goes: stop its estimate and remove its samples.
struct Cleanup {
    dir: PathBuf,
    generation: Arc<AtomicU64>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.generation.store(u64::MAX, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn reviews_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is set");
    Path::new(&home).join("Library/Caches/compressy/review")
}

/// Remove every review's samples: called at launch, before any review.
pub fn clear_stale() {
    let _ = std::fs::remove_dir_all(reviews_dir());
}

impl Review {
    /// Start reading `input` in the background; the screen fills in as it does.
    pub fn open(
        id: u64,
        input: &Path,
        runtime: &Runtime,
        pool: &BackgroundPool,
        tasks: &Tasks,
        tools: &Tools,
    ) -> Result<Review, String> {
        let input = input.canonicalize().map_err(|e| e.to_string())?;
        let dir = reviews_dir().join(id.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let generation = Arc::new(AtomicU64::new(0));
        let review = Review {
            id,
            input: input.clone(),
            clip: runtime.signal(Loaded::Reading),
            before: runtime.signal(None),
            after: runtime.signal(None),
            estimate: runtime.signal(Est::Waiting),
            measuring: runtime.signal(false),
            mode: runtime.signal(Mode::Compare),
            dir: dir.clone(),
            generation: generation.clone(),
            _cleanup: Rc::new(Cleanup { dir, generation }),
        };
        let (ffprobe, ffmpeg) = (
            tools.ffprobe.clone().ok_or("ffprobe is missing")?,
            tools.ffmpeg.clone().ok_or("ffmpeg is missing")?,
        );
        let task = pool
            .spawn(move || {
                let info = clip::probe(&ffprobe, &input)?;
                let at = estimate::preview_time(&info);
                let frame = clip::frame(&ffmpeg, &input, at, info.fit(PREVIEW.0, PREVIEW.1)).ok();
                Ok::<_, String>((info, frame))
            })
            .map_err(|e| format!("{e:?}"))?;
        let (clip, before) = (review.clip.clone(), review.before.clone());
        tasks.spawn(async move {
            match task.await {
                Ok(Ok((info, frame))) => {
                    before.set(frame.and_then(Pic::from));
                    clip.set(Loaded::Ready(Rc::new(info)));
                }
                Ok(Err(e)) => {
                    clip.set(Loaded::Failed(e));
                }
                Err(e) => {
                    clip.set(Loaded::Failed(format!("{e:?}")));
                }
            }
        });
        Ok(review)
    }

    pub fn info(&self) -> Option<Rc<ClipInfo>> {
        match self.clip.get() {
            Loaded::Ready(info) => Some(info),
            _ => None,
        }
    }

    /// Measure `settings` once they've held still, superseding any
    /// measurement under way.
    pub fn measure(&self, settings: Settings, tools: Tools, pool: &BackgroundPool, tasks: &Tasks) {
        let Some(info) = self.clip.with_untracked(|c| match c {
            Loaded::Ready(info) => Some(info.clone()),
            _ => None,
        }) else {
            return;
        };
        let mine = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.measuring.set(true);
        // A frame of another crop wouldn't line up with this one's.
        let size = preview_size(&settings, &info);
        if self.after.with_untracked(|a| {
            a.as_ref()
                .is_some_and(|Pic(i)| (i.width(), i.height()) != size)
        }) {
            self.after.set(None);
        }
        let (review, pool) = (self.clone(), pool.clone());
        tasks.spawn(async move {
            sleep(SETTLE).await;
            if review.generation.load(Ordering::SeqCst) != mine {
                return;
            }
            let (input, dir, generation) = (
                review.input.clone(),
                review.dir.clone(),
                review.generation.clone(),
            );
            let clip = (*info).clone();
            let preview = preview_size(&settings, &info);
            let Ok(task) = pool.spawn(move || {
                estimate::run(
                    &tools,
                    &input,
                    &clip,
                    &settings,
                    &dir,
                    preview,
                    &estimate::Stop { generation, mine },
                )
            }) else {
                return;
            };
            let result = task.await;
            if review.generation.load(Ordering::SeqCst) != mine {
                return;
            }
            review.measuring.set(false);
            match result {
                Ok(Ok(mut estimate)) => {
                    if let Some(frame) = estimate.after.take() {
                        review.after.set(Pic::from(frame));
                    }
                    review.estimate.set(Est::Ready(Rc::new(estimate)));
                }
                Ok(Err(e)) if e == estimate::STALE => {}
                Ok(Err(e)) => {
                    review.estimate.set(Est::Failed(e));
                }
                Err(e) => {
                    review.estimate.set(Est::Failed(format!("{e:?}")));
                }
            }
        });
    }

    /// The preview frame as it will come out: cropped, if it is.
    pub fn cropped_before(&self, settings: &Settings, info: &ClipInfo) -> Option<Pic> {
        let Pic(full) = self.before.get()?;
        let Some(crop) = settings.crop.filter(|c| !c.is_full()) else {
            return Some(Pic(full));
        };
        let (fw, fh) = (full.width(), full.height());
        let (w, h, x, y) = crop.pixels(fw, fh);
        debug_assert_eq!((w, h), preview_size(settings, info));
        let stride = fw as usize * 4;
        let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
        for row in y..y + h {
            let start = row as usize * stride + x as usize * 4;
            pixels.extend_from_slice(&full.pixels()[start..start + w as usize * 4]);
        }
        ImageData::new(w, h, pixels).ok().map(|i| Pic(Arc::new(i)))
    }

    /// The last estimate's size, for the AI prompt's baseline.
    pub fn estimated_size(&self) -> Option<u64> {
        match self.estimate.get() {
            Est::Ready(e) => Some(e.size),
            _ => None,
        }
    }
}

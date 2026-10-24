//! What the user chose on the review screen.
use crate::{clip::ClipInfo, prompt::Goal, tools::Encoders};

/// The most the short side may be: `1080p` is a 1080 short side, portrait too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Source,
    P1080,
    P720,
    P480,
}

impl Resolution {
    pub const ALL: [Resolution; 4] = [Self::Source, Self::P1080, Self::P720, Self::P480];
    pub fn cap(self) -> Option<u32> {
        match self {
            Self::Source => None,
            Self::P1080 => Some(1080),
            Self::P720 => Some(720),
            Self::P480 => Some(480),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameRate {
    Source,
    F30,
    F24,
}

impl FrameRate {
    pub const ALL: [FrameRate; 3] = [Self::Source, Self::F30, Self::F24];
    pub fn cap(self) -> Option<f64> {
        match self {
            Self::Source => None,
            Self::F30 => Some(30.),
            Self::F24 => Some(24.),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::F30 => "30",
            Self::F24 => "24",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audio {
    Full,
    Small,
    Mono,
    Off,
}

impl Audio {
    pub const ALL: [Audio; 4] = [Self::Full, Self::Small, Self::Mono, Self::Off];
    /// AAC bitrate and channels, or `None` for no audio.
    pub fn aac(self) -> Option<(u32, Option<u32>)> {
        match self {
            Self::Full => Some((160, None)),
            Self::Small => Some((96, None)),
            Self::Mono => Some((48, Some(1))),
            Self::Off => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Small => "Small",
            Self::Mono => "Mono",
            Self::Off => "Off",
        }
    }
    pub fn describe(self) -> &'static str {
        match self {
            Self::Full => "AAC 160 kbps",
            Self::Small => "AAC 96 kbps",
            Self::Mono => "mono AAC 48 kbps",
            Self::Off => "no audio",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Hevc,
    H264,
    Av1,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Self::Hevc, Self::H264, Self::Av1];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hevc => "HEVC",
            Self::H264 => "H.264",
            Self::Av1 => "AV1",
        }
    }
    pub fn available(self, encoders: &Encoders) -> bool {
        match self {
            Self::Hevc => encoders.x265 || encoders.hevc_videotoolbox,
            Self::H264 => encoders.x264,
            Self::Av1 => encoders.svtav1,
        }
    }
    /// What it means for the file, in a line.
    pub fn note(self) -> &'static str {
        match self {
            Self::Hevc => "plays everywhere on Apple devices",
            Self::H264 => "plays anywhere, a little larger",
            Self::Av1 => "smallest; needs a recent Mac or browser",
        }
    }
}

/// A part of the frame to keep, as fractions of the shown (rotated) frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Crop {
    pub const FULL: Crop = Crop {
        x: 0.,
        y: 0.,
        w: 1.,
        h: 1.,
    };

    /// Whether it keeps (nearly) everything.
    pub fn is_full(&self) -> bool {
        self.w > 0.995 && self.h > 0.995
    }

    /// In pixels of a `width` × `height` frame: (w, h, x, y), sides even as
    /// 4:2:0 needs, inside the frame.
    pub fn pixels(&self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let even = |v: f32| (v.round().max(0.) as u32) & !1;
        let w = even(self.w * width as f32).clamp(16.min(width & !1), width & !1);
        let h = even(self.h * height as f32).clamp(16.min(height & !1), height & !1);
        let x = even(self.x * width as f32).min((width - w) & !1);
        let y = even(self.y * height as f32).min((height - h) & !1);
        (w, h, x, y)
    }

    /// The largest crop of pixel aspect `ratio` (w / h) centred on this one,
    /// for a `width` × `height` frame.
    pub fn with_aspect(&self, ratio: f32, width: u32, height: u32) -> Crop {
        let frame = width as f32 / height as f32;
        // In fractions, w / h = ratio / frame.
        let r = ratio / frame;
        let (mut w, mut h) = (self.w, self.w / r);
        if h > self.h {
            h = self.h;
            w = h * r;
        }
        // At least as large as fits the whole frame in that aspect, when
        // starting from the full frame.
        if self.is_full() {
            (w, h) = if r >= 1. { (1., 1. / r) } else { (r, 1.) };
        }
        let (cx, cy) = (self.x + self.w / 2., self.y + self.h / 2.);
        Crop {
            x: (cx - w / 2.).clamp(0., 1. - w),
            y: (cy - h / 2.).clamp(0., 1. - h),
            w,
            h,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub goal: Goal,
    pub resolution: Resolution,
    pub fps: FrameRate,
    pub audio: Audio,
    pub codec: Codec,
    /// Per clip: reset when one is opened.
    pub crop: Option<Crop>,
    /// A nudge on the goal's quality: negative is smaller, positive finer
    /// (CRF steps, the other way round). Reset with the goal.
    pub fine_tune: i8,
    /// Keep the source's metadata (dates, location, camera) rather than strip it.
    pub keep_metadata: bool,
}

impl Settings {
    pub fn new(encoders: &Encoders) -> Settings {
        let codec = Codec::ALL
            .into_iter()
            .find(|c| c.available(encoders))
            .unwrap_or(Codec::Hevc);
        Settings::for_goal(Goal::Balanced, codec)
    }

    /// A goal's resolution, frame rate and audio; the codec kept.
    pub fn for_goal(goal: Goal, codec: Codec) -> Settings {
        let (resolution, fps, audio) = match goal {
            Goal::Transparent => (Resolution::Source, FrameRate::Source, Audio::Full),
            Goal::Balanced => (Resolution::P1080, FrameRate::F30, Audio::Small),
            Goal::Tiny => (Resolution::P480, FrameRate::F24, Audio::Mono),
        };
        Settings {
            goal,
            resolution,
            fps,
            audio,
            codec,
            crop: None,
            fine_tune: 0,
            keep_metadata: false,
        }
    }

    /// Another goal's defaults; codec, crop and metadata kept.
    pub fn with_goal(self, goal: Goal) -> Settings {
        Settings {
            crop: self.crop,
            keep_metadata: self.keep_metadata,
            ..Settings::for_goal(goal, self.codec)
        }
    }

    /// The crop in effect, in the clip's pixels, if it crops at all.
    pub fn crop_pixels(&self, clip: &ClipInfo) -> Option<(u32, u32, u32, u32)> {
        let crop = self.crop.filter(|c| !c.is_full())?;
        Some(crop.pixels(clip.width, clip.height))
    }

    /// The frame's size once cropped (before any scaling).
    pub fn frame(&self, clip: &ClipInfo) -> (u32, u32) {
        self.crop_pixels(clip)
            .map_or((clip.width, clip.height), |(w, h, _, _)| (w, h))
    }

    /// The cropped frame's short side.
    pub fn source_short_side(&self, clip: &ClipInfo) -> u32 {
        let (w, h) = self.frame(clip);
        w.min(h)
    }

    /// The short side it will come out at.
    pub fn short_side(&self, clip: &ClipInfo) -> u32 {
        let short = self.source_short_side(clip);
        match self.resolution.cap() {
            Some(cap) if cap < short => cap,
            _ => short,
        }
    }

    /// The frame-rate cap that applies to this clip, if any.
    pub fn fps_cap(&self, clip: &ClipInfo) -> Option<f64> {
        let cap = self.fps.cap()?;
        clip.fps.is_some_and(|fps| fps > cap + 0.5).then_some(cap)
    }

    /// The audio it will have: none if the clip has none.
    pub fn audio_for(&self, clip: &ClipInfo) -> Audio {
        if clip.audio.is_some() {
            self.audio
        } else {
            Audio::Off
        }
    }
}

/// Whether an option changes anything for this clip (as cropped): no
/// upscaling, no raising the frame rate, no audio choices without audio.
pub fn resolution_applies(r: Resolution, settings: &Settings, clip: &ClipInfo) -> bool {
    r.cap()
        .is_none_or(|cap| cap < settings.source_short_side(clip))
}

pub fn fps_applies(f: FrameRate, clip: &ClipInfo) -> bool {
    match (f.cap(), clip.fps) {
        (None, _) => true,
        (Some(cap), Some(fps)) => fps > cap + 0.5,
        (Some(_), None) => false,
    }
}

pub fn audio_applies(a: Audio, clip: &ClipInfo) -> bool {
    clip.audio.is_some() || a == Audio::Off
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_land_on_even_pixels_inside_the_frame() {
        let c = Crop {
            x: 0.9,
            y: 0.33,
            w: 0.3,
            h: 0.5,
        };
        let (w, h, x, y) = c.pixels(1919, 1081);
        assert!(w % 2 == 0 && h % 2 == 0 && x % 2 == 0 && y % 2 == 0);
        assert!(x + w <= 1919 && y + h <= 1081);
    }

    #[test]
    fn aspect_presets_fit_the_frame() {
        let square = Crop::FULL.with_aspect(1., 1920, 1080);
        let (w, h, _, _) = square.pixels(1920, 1080);
        assert_eq!((w, h), (1080, 1080));
        let tall = Crop::FULL.with_aspect(9. / 16., 1920, 1080);
        let (w, h, x, _) = tall.pixels(1920, 1080);
        assert_eq!(h, 1080);
        assert!((w as i32 - 608).abs() <= 2 && x > 600);
    }
}

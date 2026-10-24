//! The ffmpeg command for a clip and its settings: the encode's
//! whole encode, and the estimator's samples.
use crate::{
    clip::ClipInfo,
    prompt::Goal,
    settings::{Codec, Settings},
    tools::{Encoders, Tools},
};
use std::path::Path;

/// The encoder behind a codec, best first among those present.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Encoder {
    X265,
    HevcVideoToolbox,
    X264,
    SvtAv1,
}

impl Encoder {
    fn pick(codec: Codec, encoders: &Encoders) -> Option<Encoder> {
        match codec {
            Codec::Hevc if encoders.x265 => Some(Encoder::X265),
            Codec::Hevc if encoders.hevc_videotoolbox => Some(Encoder::HevcVideoToolbox),
            Codec::H264 if encoders.x264 => Some(Encoder::X264),
            Codec::Av1 if encoders.svtav1 => Some(Encoder::SvtAv1),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Encoder::X265 => "x265",
            Encoder::HevcVideoToolbox => "VideoToolbox",
            Encoder::X264 => "x264",
            Encoder::SvtAv1 => "SVT-AV1",
        }
    }

    /// Codec arguments at a goal's quality.
    /// Codec arguments at a goal's quality, nudged by `fine_tune` (finer
    /// is a lower CRF, a higher VideoToolbox quality).
    fn args(self, goal: Goal, fine_tune: i8) -> Vec<String> {
        let level = |hi: u32, mid: u32, lo: u32| {
            let base = match goal {
                Goal::Transparent => hi,
                Goal::Balanced => mid,
                Goal::Tiny => lo,
            } as i32;
            let nudged = match self {
                Encoder::HevcVideoToolbox => (base + 3 * fine_tune as i32).clamp(1, 100),
                _ => (base - fine_tune as i32).clamp(0, 63),
            };
            nudged.to_string()
        };
        let args: Vec<String> = match self {
            Encoder::X265 => vec![
                "-c:v".into(),
                "libx265".into(),
                "-preset".into(),
                "slow".into(),
                "-crf".into(),
                level(20, 26, 32),
                "-tag:v".into(),
                "hvc1".into(),
                "-x265-params".into(),
                "log-level=error".into(),
            ],
            Encoder::HevcVideoToolbox => vec![
                "-c:v".into(),
                "hevc_videotoolbox".into(),
                "-q:v".into(),
                level(70, 55, 40),
                "-tag:v".into(),
                "hvc1".into(),
            ],
            Encoder::X264 => vec![
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "slow".into(),
                "-crf".into(),
                level(18, 23, 29),
            ],
            Encoder::SvtAv1 => vec![
                "-c:v".into(),
                "libsvtav1".into(),
                "-preset".into(),
                "6".into(),
                "-crf".into(),
                level(26, 34, 44),
            ],
        };
        args
    }
}

/// What the encode will do, in a line.
pub fn describe(settings: &Settings, clip: &ClipInfo, encoders: &Encoders) -> String {
    let Some(encoder) = Encoder::pick(settings.codec, encoders) else {
        return format!("This ffmpeg has no {} encoder.", settings.codec.label());
    };
    let mut parts = vec![format!("{} ({})", settings.codec.label(), encoder.name())];
    parts.push(format!("{}p", settings.short_side(clip)));
    let fps = settings.fps_cap(clip).or(clip.fps);
    if let Some(fps) = fps {
        parts.push(format!("{} fps", crate::clip::trim(fps)));
    }
    parts.push(settings.audio_for(clip).describe().into());
    parts.join(" · ")
}

/// The video filter: the crop, then scale to the short-side cap (even
/// sides, as 4:2:0 needs), then the frame-rate cap.
pub fn video_filter(settings: &Settings, clip: &ClipInfo) -> String {
    let mut filters = Vec::new();
    if let Some((w, h, x, y)) = settings.crop_pixels(clip) {
        filters.push(format!("crop={w}:{h}:{x}:{y}"));
    }
    let (w, h) = settings.frame(clip);
    filters.push(match settings.resolution.cap() {
        Some(cap) if cap < w.min(h) => {
            let cap = cap & !1;
            if h > w {
                format!("scale={cap}:-2")
            } else {
                format!("scale=-2:{cap}")
            }
        }
        _ => "scale=trunc(iw/2)*2:trunc(ih/2)*2".into(),
    });
    if let Some(cap) = settings.fps_cap(clip) {
        filters.push(format!("fps={cap}"));
    }
    filters.join(",")
}

/// ffmpeg's arguments (after the executable) encoding `input` to `output`.
/// With `sample` (start, length) it encodes that window, video alone, for
/// the estimator.
pub fn ffmpeg_args(
    tools: &Tools,
    input: &Path,
    output: &Path,
    settings: &Settings,
    clip: &ClipInfo,
    sample: Option<(f64, f64)>,
) -> Result<Vec<String>, String> {
    let encoder = Encoder::pick(settings.codec, &tools.encoders).ok_or_else(|| {
        format!(
            "This ffmpeg has no {} encoder. Try another codec, or `brew reinstall ffmpeg`.",
            settings.codec.label()
        )
    })?;
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    args.extend(["-loglevel", "error"].map(String::from));
    match sample {
        Some((start, length)) => {
            args.extend([
                "-ss".into(),
                format!("{start:.3}"),
                "-t".into(),
                format!("{length:.3}"),
            ]);
        }
        None => args.push("-stats".into()),
    }
    args.push("-i".into());
    args.push(input.to_string_lossy().into_owned());
    args.extend(["-map", "0:v:0", "-vf"].map(String::from));
    args.push(video_filter(settings, clip));
    args.extend(encoder.args(settings.goal, settings.fine_tune));
    args.extend(["-pix_fmt", "yuv420p"].map(String::from));
    match settings.audio_for(clip).aac() {
        Some((kbps, channels)) if sample.is_none() => {
            args.extend(["-map", "0:a:0?", "-c:a", "aac", "-b:a"].map(String::from));
            args.push(format!("{kbps}k"));
            if let Some(channels) = channels {
                args.extend(["-ac".into(), channels.to_string()]);
            }
        }
        _ => args.push("-an".into()),
    }
    let metadata = if settings.keep_metadata { "0" } else { "-1" };
    args.extend(["-map_metadata", metadata, "-movflags", "+faststart"].map(String::from));
    args.push(output.to_string_lossy().into_owned());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip;
    use std::{path::PathBuf, process::Command};

    /// A 3 s, 1080p60 test clip with a tone.
    pub fn clip_file(dir: &Path) -> PathBuf {
        let path = dir.join("source.mov");
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=60:duration=3",
            ])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
            .args(["-c:v", "libx264", "-crf", "8", "-c:a", "pcm_s16le"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    }

    fn stream(ffprobe: &Path, file: &Path, entries: &str) -> String {
        let out = Command::new(ffprobe)
            .args(["-v", "error", "-show_entries", entries, "-of", "csv=p=0"])
            .arg(file)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn every_goal_encodes_smaller_within_its_caps() {
        let tools = Tools::detect();
        let (Some(ffmpeg), Some(ffprobe)) = (tools.ffmpeg.clone(), tools.ffprobe.clone()) else {
            eprintln!("no ffmpeg: skipped");
            return;
        };
        let dir = std::env::temp_dir().join(format!("compressy-preset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = clip_file(&dir);
        let info = clip::probe(&ffprobe, &source).unwrap();
        for (goal, height, fps) in [
            (Goal::Transparent, "1080", "60/1"),
            (Goal::Balanced, "1080", "30/1"),
            (Goal::Tiny, "480", "24/1"),
        ] {
            let settings = Settings::for_goal(goal, Codec::Hevc);
            let out = dir.join(format!("{goal:?}.mp4"));
            let args = ffmpeg_args(&tools, &source, &out, &settings, &info, None).unwrap();
            let status = Command::new(&ffmpeg).args(&args).status().unwrap();
            assert!(status.success(), "{goal:?}: {args:?}");
            let size = std::fs::metadata(&out).unwrap().len();
            assert!(size < info.size, "{goal:?}: {size} >= {}", info.size);
            let video = stream(
                &ffprobe,
                &out,
                "stream=codec_tag_string,height,avg_frame_rate",
            );
            assert!(
                video.contains(height) && video.contains(fps),
                "{goal:?}: {video}"
            );
        }
        // A square crop comes out square.
        let mut settings = Settings::for_goal(Goal::Tiny, Codec::Hevc);
        settings.crop = Some(crate::settings::Crop::FULL.with_aspect(1., info.width, info.height));
        let out = dir.join("square.mp4");
        let args = ffmpeg_args(&tools, &source, &out, &settings, &info, None).unwrap();
        assert!(
            Command::new(&ffmpeg)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            stream(&ffprobe, &out, "stream=width,height").lines().next(),
            Some("480,480")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

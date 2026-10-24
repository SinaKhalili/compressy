//! A prompt for an AI with a shell (Claude Code, Codex, anything that can
//! run ffmpeg): this clip, these settings, the measured baseline, and how
//! to search for something smaller. Compressy copies it; it runs nothing.
use crate::{
    clip::ClipInfo,
    job::human_size,
    settings::{Audio, Codec, Settings},
    tools::Encoders,
};
use std::path::Path;

/// How far the encode may stray from the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    Transparent,
    Balanced,
    Tiny,
}

impl Goal {
    pub const ALL: [Goal; 3] = [Goal::Transparent, Goal::Balanced, Goal::Tiny];

    pub fn label(self) -> &'static str {
        match self {
            Goal::Transparent => "Identical",
            Goal::Balanced => "Balanced",
            Goal::Tiny => "Tiny",
        }
    }

    /// The VMAF a result must reach.
    pub fn vmaf(self) -> f64 {
        match self {
            Goal::Transparent => 95.,
            Goal::Balanced => 90.,
            Goal::Tiny => 75.,
        }
    }

    /// The same bar in SSIM, for an ffmpeg without libvmaf.
    pub fn ssim(self) -> f64 {
        match self {
            Goal::Transparent => 0.99,
            Goal::Balanced => 0.97,
            Goal::Tiny => 0.93,
        }
    }
}

pub fn build(
    input: &Path,
    output: &Path,
    settings: &Settings,
    clip: &ClipInfo,
    encoders: &Encoders,
    baseline: Option<u64>,
) -> String {
    let available: Vec<&str> = [
        (encoders.svtav1, "libsvtav1"),
        (encoders.x265, "libx265"),
        (encoders.x264, "libx264"),
        (encoders.vpx_vp9, "libvpx-vp9"),
        (encoders.hevc_videotoolbox, "hevc_videotoolbox"),
    ]
    .into_iter()
    .filter_map(|(present, name)| present.then_some(name))
    .collect();
    let goal = settings.goal;
    let bar = if encoders.vmaf {
        format!(
            "VMAF >= {} against the source, measured at the source resolution",
            goal.vmaf()
        )
    } else {
        format!(
            "SSIM >= {} against the source (this ffmpeg has no libvmaf), measured at the source resolution",
            goal.ssim()
        )
    };
    let resolution = match settings.resolution.cap() {
        Some(cap) if cap < settings.source_short_side(clip) => {
            format!("You may downscale, but the short side must stay at least {cap} px.")
        }
        _ => "Keep the original resolution.".into(),
    };
    let fps = match settings.fps_cap(clip) {
        Some(cap) => format!("Use at most {} fps.", crate::clip::trim(cap)),
        None => "Keep the original frame rate.".into(),
    };
    let audio = match settings.audio_for(clip) {
        _ if clip.audio.is_none() => "The clip has no audio.".to_owned(),
        Audio::Full => "Keep audio sounding unchanged (AAC or Opus, about 160 kbps stereo).".into(),
        Audio::Small => "Audio may go to about 96 kbps.".into(),
        Audio::Mono => "Audio may be mono at about 48 kbps.".into(),
        Audio::Off => "Remove the audio track.".into(),
    };
    let crop = settings
        .crop_pixels(clip)
        .map(|(w, h, x, y)| {
            format!(
                "\nCrop first, exactly as chosen: -vf crop={w}:{h}:{x}:{y} (before any scaling), and \
                 compare against the source cropped the same way."
            )
        })
        .unwrap_or_default();
    let codec = match settings.codec {
        Codec::Hevc => "Prefer HEVC (tag it hvc1). Use AV1 only if it's clearly smaller.",
        Codec::H264 => "Use H.264 only: maximum compatibility was asked for.",
        Codec::Av1 => {
            "Prefer AV1 (libsvtav1); fall back to HEVC (hvc1) if AV1 can't clear the bar."
        }
    };
    let baseline = baseline
        .map(|size| {
            format!(
                "\nCompressy measured a single ffmpeg pass with these settings at about {}: beat it.",
                human_size(size)
            )
        })
        .unwrap_or_default();
    let scratch = format!(
        "/tmp/compressy-{}",
        input.file_stem().unwrap_or_default().to_string_lossy()
    );
    format!(
        "Compress this video as small as you can while still meeting the quality bar.

Input:   {input}  ({summary}, {size})
Output:  {output}
Scratch: a folder of your own for test encodes (e.g. {scratch})

Quality bar: {bar}.
{resolution} {fps} {audio}{crop}
{codec}{baseline}

Rules:
- Never modify, move or delete the input. Put every encode, the final one included, in the \
scratch directory; when the winner is complete, `cp` it to the output path.
- The output must be an MP4 (with -movflags +faststart) that plays in QuickTime and modern \
browsers. {metadata}
- ffmpeg's encoders on this Mac: {encoders}. Use ffprobe to inspect files.

How to work:
1. If the clip is longer than ~20 s, cut 2-3 short representative segments into scratch and \
run your experiments on those instead of full encodes.
2. Try several candidates (e.g. a CRF sweep; film-grain synthesis for grainy sources with \
SVT-AV1; the resolution and frame rate changes allowed above).
3. Measure each against the source and keep the smallest that clears the bar.
4. Encode the full clip with the winner, check it with ffprobe (and the metric on the full \
file if that's quick), then cp it to the output path.
5. End with a short table: candidate, size, score; then the winning ffmpeg command and the \
final size versus the original.",
        input = input.display(),
        summary = clip.summary(),
        size = human_size(clip.size),
        output = output.display(),
        scratch = scratch,
        encoders = available.join(", "),
        metadata = if settings.keep_metadata {
            "Keep the source's metadata (-map_metadata 0)."
        } else {
            "Strip metadata you don't need."
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Crop;

    fn clip() -> ClipInfo {
        ClipInfo {
            duration: 15.,
            size: 50_000_000,
            width: 1920,
            height: 1080,
            fps: Some(30.),
            video_codec: "h264".into(),
            video_bitrate: None,
            audio: None,
        }
    }

    #[test]
    fn the_prompt_carries_the_clip_the_choices_and_the_baseline() {
        let mut settings = Settings::for_goal(Goal::Tiny, Codec::Av1);
        settings.crop = Some(Crop::FULL.with_aspect(1., 1920, 1080));
        let text = build(
            Path::new("/clips/a b.mov"),
            Path::new("/clips/a b.compressed.mp4"),
            &settings,
            &clip(),
            &Encoders::default(),
            Some(12_300_000),
        );
        for wanted in [
            "/clips/a b.mov",
            "/clips/a b.compressed.mp4",
            "crop=1080:1080:420:0",
            "short side must stay at least 480 px",
            "Prefer AV1",
            "12.3 MB",
            "SSIM >= 0.93",
            "The clip has no audio.",
        ] {
            assert!(text.contains(wanted), "missing {wanted:?} in:\n{text}");
        }
    }
}

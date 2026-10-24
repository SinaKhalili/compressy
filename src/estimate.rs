//! How big the result will be, how good it will look and how long it will
//! take: encode a few short windows of the clip with the real settings and
//! scale up, then score the middle one against the source.
use crate::{
    clip::{self, ClipInfo, Frame},
    preset,
    settings::Settings,
    tools::Tools,
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Seconds of each sampled window.
const WINDOW: f64 = 2.;
/// Clips up to this long are encoded whole.
const WHOLE: f64 = 7.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Metric {
    Vmaf,
    Ssim,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Estimate {
    pub size: u64,
    pub encode_seconds: f64,
    pub score: Option<(Metric, f64)>,
    /// The sampled frame after compression, the size of the preview.
    pub after: Option<Frame>,
}

/// What a superseded run returns.
pub const STALE: &str = "stale";

/// Where the preview frame comes from: the middle window's middle.
pub fn preview_time(clip: &ClipInfo) -> f64 {
    let (start, length) = windows(clip)[windows(clip).len() / 2];
    start + length / 2.
}

fn windows(clip: &ClipInfo) -> Vec<(f64, f64)> {
    let d = clip.duration;
    if d <= WHOLE {
        return vec![(0., d.max(0.1))];
    }
    [0.2, 0.5, 0.8]
        .map(|at| ((at * d - WINDOW / 2.).max(0.), WINDOW))
        .to_vec()
}

/// When a run should stop: its counter has moved past it.
pub struct Stop {
    pub generation: Arc<AtomicU64>,
    pub mine: u64,
}

impl Stop {
    fn stale(&self) -> bool {
        self.generation.load(Ordering::SeqCst) != self.mine
    }
}

/// Run one estimate; `stop` going stale ends it (and its encoders) with
/// `STALE`.
pub fn run(
    tools: &Tools,
    input: &Path,
    clip: &ClipInfo,
    settings: &Settings,
    dir: &Path,
    preview: (u32, u32),
    stop: &Stop,
) -> Result<Estimate, String> {
    let stale = || stop.stale();
    let mine = stop.mine;
    let ffmpeg = tools.ffmpeg.as_deref().ok_or("ffmpeg is missing")?;
    let windows = windows(clip);
    let samples: Vec<PathBuf> = (0..windows.len())
        .map(|i| dir.join(format!("sample-{mine}-{i}.mp4")))
        .collect();

    // The windows at once: each encoder gets a share of the cores, so the
    // wall time is about what one would take over the same frames.
    let started = Instant::now();
    let mut children: Vec<Child> = Vec::new();
    for (window, out) in windows.iter().zip(&samples) {
        let args = preset::ffmpeg_args(tools, input, out, settings, clip, Some(*window))?;
        let child = Command::new(ffmpeg)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
        children.push(child);
    }
    let finished = wait_all(&mut children, &stale)?;
    if let Some(failed) = finished.iter().position(|ok| !ok) {
        let mut err = String::new();
        if let Some(mut stderr) = children[failed].stderr.take() {
            use std::io::Read;
            let _ = stderr.read_to_string(&mut err);
        }
        cleanup(&samples);
        return Err(err
            .lines()
            .last()
            .unwrap_or("the sample encode failed")
            .to_owned());
    }
    let wall = started.elapsed().as_secs_f64();

    let covered: f64 = windows
        .iter()
        .map(|w| w.1)
        .sum::<f64>()
        .min(clip.duration.max(0.1));
    let bytes: u64 = samples
        .iter()
        .map(|s| std::fs::metadata(s).map_or(0, |m| m.len()))
        .sum();
    let scale = clip.duration.max(covered) / covered;
    let video = bytes as f64 * scale;
    let audio = settings.audio_for(clip).aac().map_or(0., |(kbps, _)| {
        kbps as f64 * 1000. / 8. * clip.duration * 1.02
    });
    // The container: a little over the streams, and a header.
    let size = ((video + audio) * 1.005 + 2048.) as u64;
    let encode_seconds = wall * scale;

    if stale() {
        cleanup(&samples);
        return Err(STALE.into());
    }
    let middle = windows.len() / 2;
    let score = score(
        tools,
        input,
        &samples[middle],
        windows[middle],
        clip,
        settings,
        &stale,
    );
    if stale() {
        cleanup(&samples);
        return Err(STALE.into());
    }
    let after = clip::frame(ffmpeg, &samples[middle], windows[middle].1 / 2., preview).ok();
    cleanup(&samples);
    Ok(Estimate {
        size,
        encode_seconds,
        score,
        after,
    })
}

/// Wait for every child, killing all of them once the run goes stale.
fn wait_all(children: &mut [Child], stale: &impl Fn() -> bool) -> Result<Vec<bool>, String> {
    let mut done: Vec<Option<bool>> = vec![None; children.len()];
    while done.iter().any(Option::is_none) {
        if stale() {
            for child in children.iter_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            return Err(STALE.into());
        }
        for (child, slot) in children.iter_mut().zip(done.iter_mut()) {
            if slot.is_none()
                && let Ok(Some(status)) = child.try_wait()
            {
                *slot = Some(status.success());
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(done.into_iter().map(Option::unwrap).collect())
}

/// VMAF (or SSIM without libvmaf) of a sample against the same window of
/// the source, both brought to the source's size (at most 1080p, for speed)
/// and frame rate.
fn score(
    tools: &Tools,
    input: &Path,
    sample: &Path,
    (start, length): (f64, f64),
    clip: &ClipInfo,
    settings: &Settings,
    stale: &impl Fn() -> bool,
) -> Option<(Metric, f64)> {
    let ffmpeg = tools.ffmpeg.as_deref()?;
    let frame = settings.frame(clip);
    let (w, h) = if frame.1 > frame.0 {
        clip::fit(frame, 1080, 1920)
    } else {
        clip::fit(frame, 1920, 1080)
    };
    let crop = settings
        .crop_pixels(clip)
        .map(|(cw, ch, cx, cy)| format!("crop={cw}:{ch}:{cx}:{cy},"))
        .unwrap_or_default();
    let fps = settings
        .fps_cap(clip)
        .map(|cap| format!("{crop}fps={cap},"))
        .unwrap_or(crop);
    let prep = |label: &str, extra: &str| {
        format!("{extra}scale={w}:{h}:flags=bicubic,format=yuv420p,setsar=1[{label}]")
    };
    let (metric, filter) = if tools.encoders.vmaf {
        (Metric::Vmaf, "libvmaf=n_threads=8:shortest=1")
    } else {
        (Metric::Ssim, "ssim=shortest=1")
    };
    let graph = format!(
        "[0:v]{};[1:v]{};[d][r]{filter}",
        prep("d", ""),
        prep("r", &fps)
    );
    let mut child = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(sample)
        .args([
            "-ss",
            &format!("{start:.3}"),
            "-t",
            &format!("{length:.3}"),
            "-i",
        ])
        .arg(input)
        .args(["-lavfi", &graph, "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stderr = child.stderr.take()?;
    // Read as it runs (the log could fill the pipe), stopping if stale.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        use std::io::Read;
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let ok = wait_all(std::slice::from_mut(&mut child), stale).ok()?;
    let text = reader.join().ok()?;
    if !ok[0] {
        return None;
    }
    let value = match metric {
        Metric::Vmaf => text
            .lines()
            .find_map(|l| l.split("VMAF score:").nth(1))
            .and_then(|v| v.trim().parse().ok()),
        Metric::Ssim => text
            .lines()
            .find_map(|l| l.split("All:").nth(1))
            .and_then(|v| v.split_whitespace().next()?.parse().ok()),
    }?;
    Some((metric, value))
}

fn cleanup(samples: &[PathBuf]) {
    for sample in samples {
        let _ = std::fs::remove_file(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prompt::Goal, settings::Codec};

    #[test]
    fn estimates_close_to_the_real_encode() {
        let tools = Tools::detect();
        let (Some(ffmpeg), Some(ffprobe)) = (tools.ffmpeg.clone(), tools.ffprobe.clone()) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("compressy-estimate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 12 s, so the estimate samples windows rather than the whole clip.
        let source = dir.join("long.mov");
        let made = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=30:duration=12",
            ])
            .args(["-f", "lavfi", "-i", "sine=duration=12"])
            .args(["-c:v", "libx264", "-crf", "10", "-c:a", "pcm_s16le"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(made.success());
        let info = clip::probe(&ffprobe, &source).unwrap();
        let settings = Settings::for_goal(Goal::Balanced, Codec::Hevc);
        let stop = Stop {
            generation: Arc::new(AtomicU64::new(1)),
            mine: 1,
        };
        let estimate = run(&tools, &source, &info, &settings, &dir, (320, 180), &stop).unwrap();
        let out = dir.join("real.mp4");
        let args = preset::ffmpeg_args(&tools, &source, &out, &settings, &info, None).unwrap();
        assert!(
            Command::new(&ffmpeg)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
        let real = std::fs::metadata(&out).unwrap().len() as f64;
        let error = (estimate.size as f64 - real).abs() / real;
        eprintln!(
            "estimate {} vs real {real} ({:.0}% off), {:?}",
            estimate.size,
            error * 100.,
            estimate.score
        );
        assert!(error < 0.35, "estimate {} vs real {real}", estimate.size);
        assert!(estimate.score.is_some());
        let after = estimate.after.unwrap();
        assert_eq!((after.width, after.height), (320, 180));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_newer_request_stops_a_run() {
        let tools = Tools::detect();
        let (Some(ffmpeg), Some(ffprobe)) = (tools.ffmpeg.clone(), tools.ffprobe.clone()) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("compressy-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("s.mov");
        assert!(
            Command::new(&ffmpeg)
                .args([
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=1920x1080:duration=20"
                ])
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        let info = clip::probe(&ffprobe, &source).unwrap();
        let settings = Settings::for_goal(Goal::Transparent, Codec::Hevc);
        let generation = Arc::new(AtomicU64::new(1));
        let bump = generation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            bump.store(2, Ordering::SeqCst);
        });
        let started = Instant::now();
        let stop = Stop {
            generation,
            mine: 1,
        };
        let result = run(&tools, &source, &info, &settings, &dir, (320, 180), &stop);
        assert_eq!(result.unwrap_err(), STALE);
        assert!(started.elapsed() < Duration::from_secs(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

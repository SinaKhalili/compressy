//! What a clip is (ffprobe) and a frame of it as pixels (ffmpeg, as PPM).
use std::{collections::HashMap, path::Path, process::Command};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioInfo {
    pub codec: String,
    pub channels: u32,
    pub bitrate: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipInfo {
    pub duration: f64,
    pub size: u64,
    /// As shown: rotation applied.
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub video_codec: String,
    pub video_bitrate: Option<u64>,
    pub audio: Option<AudioInfo>,
}

impl ClipInfo {
    pub fn short_side(&self) -> u32 {
        self.width.min(self.height)
    }

    pub fn portrait(&self) -> bool {
        self.height > self.width
    }

    /// `1920×1080 · 60 fps · 0:05 · H.264`
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{}×{}", self.width, self.height)];
        if let Some(fps) = self.fps {
            parts.push(format!("{} fps", trim(fps)));
        }
        parts.push(clock(self.duration));
        parts.push(codec_name(&self.video_codec));
        parts.join(" · ")
    }

    /// A frame's size fitting `max_w` × `max_h`, the clip's aspect kept, even.
    pub fn fit(&self, max_w: u32, max_h: u32) -> (u32, u32) {
        fit((self.width, self.height), max_w, max_h)
    }
}

/// `size` scaled to fit `max_w` × `max_h`, never up, sides even.
pub fn fit((width, height): (u32, u32), max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = (max_w as f64 / width as f64)
        .min(max_h as f64 / height as f64)
        .min(1.);
    let even = |v: f64| ((v.round() as u32) & !1).max(2);
    (even(width as f64 * scale), even(height as f64 * scale))
}

/// `30`, `29.97`.
pub fn trim(fps: f64) -> String {
    let rounded = (fps * 100.).round() / 100.;
    if (rounded - rounded.round()).abs() < 0.01 {
        format!("{}", rounded.round() as i64)
    } else {
        format!("{rounded}")
    }
}

/// `0:05`, `12:30`, `1:02:03`.
pub fn clock(seconds: f64) -> String {
    let s = seconds.round().max(0.) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn codec_name(codec: &str) -> String {
    match codec {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "vp8" => "VP8".into(),
        "prores" => "ProRes".into(),
        "mpeg4" => "MPEG-4".into(),
        "gif" => "GIF".into(),
        other => other.to_uppercase(),
    }
}

pub fn probe(ffprobe: &Path, input: &Path) -> Result<ClipInfo, String> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries"])
        .arg(
            "format=duration,size:stream=codec_type,codec_name,width,height,avg_frame_rate,\
             bit_rate,channels:stream_side_data=rotation",
        )
        .arg(input)
        .output()
        .map_err(|e| format!("couldn't run ffprobe: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err
            .lines()
            .last()
            .unwrap_or("ffprobe couldn't read it")
            .to_owned());
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// ffprobe's default writer: `[STREAM]`/`[FORMAT]` sections of `key=value`,
/// side data nested in its stream.
fn parse(text: &str) -> Result<ClipInfo, String> {
    let mut streams: Vec<HashMap<String, String>> = Vec::new();
    let mut format: HashMap<String, String> = HashMap::new();
    let mut section = "";
    for line in text.lines().map(str::trim) {
        match line {
            "[STREAM]" => {
                section = "stream";
                streams.push(HashMap::new());
            }
            "[FORMAT]" => section = "format",
            "[/STREAM]" | "[/FORMAT]" => section = "",
            _ => {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let map = match section {
                    "stream" => streams.last_mut(),
                    "format" => Some(&mut format),
                    _ => None,
                };
                if let Some(map) = map {
                    map.entry(key.to_owned())
                        .or_insert_with(|| value.to_owned());
                }
            }
        }
    }
    let kind = |s: &HashMap<String, String>, k: &str| s.get("codec_type").is_some_and(|t| t == k);
    let number =
        |s: &HashMap<String, String>, k: &str| s.get(k).and_then(|v| v.parse::<f64>().ok());
    let video = streams
        .iter()
        .find(|s| kind(s, "video"))
        .ok_or("it has no video stream")?;
    let (mut width, mut height) = (
        number(video, "width").ok_or("no width")? as u32,
        number(video, "height").ok_or("no height")? as u32,
    );
    if number(video, "rotation").is_some_and(|r| (r.abs() - 90.).abs() < 1.) {
        std::mem::swap(&mut width, &mut height);
    }
    let fps = video.get("avg_frame_rate").and_then(|r| {
        let (n, d) = r.split_once('/')?;
        let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
        (d > 0. && n > 0.).then(|| n / d)
    });
    let audio = streams
        .iter()
        .find(|s| kind(s, "audio"))
        .map(|a| AudioInfo {
            codec: a.get("codec_name").cloned().unwrap_or_default(),
            channels: number(a, "channels").unwrap_or(2.) as u32,
            bitrate: number(a, "bit_rate").map(|b| b as u64),
        });
    Ok(ClipInfo {
        duration: number(&format, "duration").unwrap_or(0.),
        size: number(&format, "size").unwrap_or(0.) as u64,
        width,
        height,
        fps,
        video_codec: video.get("codec_name").cloned().unwrap_or_default(),
        video_bitrate: number(video, "bit_rate").map(|b| b as u64),
        audio,
    })
}

/// RGBA pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The frame `at` seconds into `input`, scaled to exactly `size`.
pub fn frame(ffmpeg: &Path, input: &Path, at: f64, size: (u32, u32)) -> Result<Frame, String> {
    let out = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-ss",
            &format!("{at:.3}"),
            "-i",
        ])
        .arg(input)
        .args(["-frames:v", "1", "-vf"])
        .arg(format!("scale={}:{}:flags=bicubic", size.0, size.1))
        .args(["-f", "image2pipe", "-vcodec", "ppm", "-"])
        .output()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    ppm(&out.stdout).ok_or_else(|| {
        let err = String::from_utf8_lossy(&out.stderr);
        err.lines().last().unwrap_or("no frame came out").to_owned()
    })
}

/// A binary PPM (`P6`, 8-bit) as RGBA.
fn ppm(bytes: &[u8]) -> Option<Frame> {
    // Header: magic, width, height, max, each separated by whitespace.
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while bytes.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while !bytes.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        fields.push(std::str::from_utf8(&bytes[start..i]).ok()?);
    }
    i += 1;
    if fields[0] != "P6" || fields[3] != "255" {
        return None;
    }
    let (width, height): (u32, u32) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    let rgb = bytes.get(i..i + (width * height * 3) as usize)?;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Some(Frame {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffprobe_sections_and_rotation() {
        let text = "[STREAM]\ncodec_name=hevc\ncodec_type=video\nwidth=1920\nheight=1080\n\
                    avg_frame_rate=30000/1001\nbit_rate=8000000\n[SIDE_DATA]\nrotation=-90\n\
                    [/SIDE_DATA]\n[/STREAM]\n[STREAM]\ncodec_name=aac\ncodec_type=audio\n\
                    channels=2\nbit_rate=128000\n[/STREAM]\n[FORMAT]\nduration=12.5\nsize=9000000\n[/FORMAT]\n";
        let clip = parse(text).unwrap();
        assert_eq!((clip.width, clip.height), (1080, 1920));
        assert!(clip.portrait());
        assert_eq!(trim(clip.fps.unwrap()), "29.97");
        assert_eq!(clip.audio.unwrap().channels, 2);
        assert_eq!(clock(clip.duration), "0:13");
        assert_eq!(clip.size, 9_000_000);
    }

    #[test]
    fn fits_frames_inside_a_box() {
        let text = "[STREAM]\ncodec_type=video\nwidth=3840\nheight=2160\n[/STREAM]\n[FORMAT]\nduration=1\n[/FORMAT]\n";
        assert_eq!(parse(text).unwrap().fit(960, 540), (960, 540));
    }
}

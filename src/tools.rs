//! Finding ffmpeg and Homebrew, and what this ffmpeg can do.
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// Encoders and filters of the ffmpeg found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Encoders {
    pub svtav1: bool,
    pub x265: bool,
    pub x264: bool,
    pub vpx_vp9: bool,
    pub hevc_videotoolbox: bool,
    pub vmaf: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub brew: Option<PathBuf>,
    pub encoders: Encoders,
}

impl Tools {
    pub fn detect() -> Tools {
        let ffmpeg = locate("ffmpeg");
        let encoders = ffmpeg.as_deref().map(probe).unwrap_or_default();
        Tools {
            ffmpeg,
            ffprobe: locate("ffprobe"),
            brew: locate("brew"),
            encoders,
        }
    }

    /// Whether a fresh look would find something different: cheap, only
    /// checks which executables exist.
    pub fn stale(&self) -> bool {
        locate("ffmpeg") != self.ffmpeg
            || locate("ffprobe") != self.ffprobe
            || locate("brew") != self.brew
    }

    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some() && self.ffprobe.is_some()
    }

    /// PATH for the terminal: where each tool was found first, then ours.
    /// Ghostty starts commands through login(1), with a bare PATH.
    pub fn path_env(&self) -> String {
        let mut dirs: Vec<String> = Vec::new();
        for tool in [&self.ffmpeg, &self.ffprobe, &self.brew]
            .into_iter()
            .flatten()
        {
            if let Some(dir) = tool.parent().map(|d| d.to_string_lossy().into_owned())
                && !dirs.contains(&dir)
            {
                dirs.push(dir);
            }
        }
        dirs.extend(
            search_dirs()
                .into_iter()
                .map(|d| d.to_string_lossy().into_owned()),
        );
        dirs.dedup();
        dirs.join(":")
    }
}

/// PATH, then where installers usually put these: a Finder-launched app
/// gets a PATH without any of them.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let home = Path::new(&home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".npm-global/bin"));
    }
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(dir.into());
    }
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    dirs
}

/// `COMPRESSY_HIDE=ffmpeg,brew` pretends those aren't installed, to see
/// the setup screens without uninstalling anything.
fn hidden(name: &str) -> bool {
    std::env::var("COMPRESSY_HIDE").is_ok_and(|v| v.split(',').any(|h| h.trim() == name))
}

fn locate(name: &str) -> Option<PathBuf> {
    if hidden(name) {
        return None;
    }
    search_dirs().into_iter().map(|d| d.join(name)).find(|p| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

fn probe(ffmpeg: &Path) -> Encoders {
    let list = |what: &str| {
        Command::new(ffmpeg)
            .args(["-hide_banner", what])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let encoders = list("-encoders");
    let filters = list("-filters");
    // Lines are " V....D libx265  ...": the name is the second column.
    let has = |text: &str, name: &str| {
        text.lines()
            .any(|l| l.split_whitespace().nth(1) == Some(name))
    };
    Encoders {
        svtav1: has(&encoders, "libsvtav1"),
        x265: has(&encoders, "libx265"),
        x264: has(&encoders, "libx264"),
        vpx_vp9: has(&encoders, "libvpx-vp9"),
        hevc_videotoolbox: has(&encoders, "hevc_videotoolbox"),
        vmaf: has(&filters, "libvmaf"),
    }
}

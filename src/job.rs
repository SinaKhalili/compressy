//! One clip being compressed: its files on disk and the script the terminal runs.
use crate::{clip::ClipInfo, preset, settings::Settings, tools::Tools};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mts", "m2ts", "ts", "3gp", "mpg",
    "mpeg", "ogv", "gif",
];

pub struct Job {
    pub input: PathBuf,
    pub output: PathBuf,
    pub original_size: u64,
    /// The script the terminal runs: the encode, then a shell.
    pub script: PathBuf,
    /// ffmpeg's `-progress` output.
    pub progress: PathBuf,
    /// Written when the encode ends: `ok` or `failed`.
    pub status: PathBuf,
    /// The clip's length, for the progress.
    pub duration: f64,
}

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn jobs_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is set");
    Path::new(&home).join("Library/Caches/compressy/jobs")
}

impl Job {
    pub fn prepare(
        input: &Path,
        settings: &Settings,
        clip: &ClipInfo,
        tools: &Tools,
    ) -> Result<Job, String> {
        let input = input.canonicalize().map_err(|e| e.to_string())?;
        let original_size = fs::metadata(&input).map_err(|e| e.to_string())?.len();
        let output = free_output_path(&input);

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let dir = jobs_dir().join(stamp.to_string());
        let scratch = dir.join("scratch");
        fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let status = dir.join("status");
        let progress = dir.join("progress");

        let ffmpeg = tools.ffmpeg.as_deref().ok_or("ffmpeg isn't installed")?;
        // Encoded beside the scratch files, moved into place once whole, so
        // the output only appears when it's done.
        let partial = scratch.join("output.mp4");
        let mut args = preset::ffmpeg_args(tools, &input, &partial, settings, clip, None)?;
        args.splice(
            0..0,
            [
                "-progress".to_owned(),
                progress.to_string_lossy().into_owned(),
            ],
        );
        let command: Vec<String> = std::iter::once(quote_path(ffmpeg))
            .chain(args.iter().map(|a| quote(a)))
            .collect();
        let work = format!(
            "printf '\\033[2m[compressy] %s\\033[0m\\n\\n' {summary}\n\
             if {command}; then\n  mv {partial} {output}\n  echo ok > {status}\n  \
             printf '\\n\\033[32m[compressy] Done.\\033[0m\\n'\nelse\n  echo failed > {status}\n  \
             printf '\\n\\033[31m[compressy] ffmpeg failed (see above).\\033[0m\\n'\nfi\n",
            summary = quote(&preset::describe(settings, clip, &tools.encoders)),
            command = command.join(" "),
            partial = quote_path(&partial),
            output = quote_path(&output),
            status = quote_path(&status),
        );

        let script = dir.join("run.sh");
        let input_dir = input.parent().unwrap_or(Path::new("/"));
        let text = format!(
            "#!/bin/zsh\nexport PATH={path}\n{work}cd {input_dir}\n\
             printf '\\n\\033[2m[compressy] Finished; this is a shell in the clip'\"'\"'s folder.\\033[0m\\n'\n\
             exec \"${{SHELL:-/bin/zsh}}\" -l\n",
            path = quote(&tools.path_env()),
            input_dir = quote_path(input_dir),
        );
        fs::write(&script, text).map_err(|e| e.to_string())?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        Ok(Job {
            input,
            output,
            original_size,
            script,
            progress,
            status,
            duration: clip.duration,
        })
    }

    /// The terminal's command line.
    pub fn command(&self) -> String {
        quote_path(&self.script)
    }
}

/// `clip.compressed.mp4` beside the input, or `clip.compressed-2.mp4`, ...
pub fn free_output_path(input: &Path) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("/"));
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    (1..)
        .map(|n| {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            dir.join(format!("{stem}.compressed{suffix}.mp4"))
        })
        .find(|p| !p.exists())
        .unwrap()
}

/// Single-quoted for the shell.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn quote_path(path: &Path) -> String {
    quote(&path.to_string_lossy())
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1000. && unit < UNITS.len() - 1 {
        size /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{clip, prompt::Goal, settings::Codec};
    use std::process::Command;

    #[test]
    fn scripts_quote_awkward_names() {
        let tools = Tools::detect();
        let Some(ffprobe) = tools.ffprobe.clone().filter(|_| tools.has_ffmpeg()) else {
            return;
        };
        let dir =
            std::env::temp_dir().join(format!("compressy job's \"dir\" {}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("it's a $clip `x`.mov");
        let made = Command::new("ffmpeg")
            .args([
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=duration=1",
            ])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let info = clip::probe(&ffprobe, &input).unwrap();
        let settings = Settings::for_goal(Goal::Balanced, Codec::Hevc);
        let job = Job::prepare(&input, &settings, &info, &tools).unwrap();
        assert_eq!(
            job.output.file_name().unwrap(),
            "it's a $clip `x`.compressed.mp4"
        );
        let check = Command::new("/bin/zsh")
            .arg("-n")
            .arg(&job.script)
            .status()
            .unwrap();
        assert!(
            check.success(),
            "{}",
            fs::read_to_string(&job.script).unwrap()
        );
        // The whole script, with the shell it ends in exiting at once.
        let run = Command::new(&job.script)
            .env("SHELL", "/usr/bin/true")
            .output()
            .unwrap();
        assert!(run.status.success());
        assert!(String::from_utf8_lossy(&run.stdout).contains("Done."));
        assert!(job.output.is_file());
        assert_eq!(fs::read_to_string(&job.status).unwrap().trim(), "ok");
        let progress = fs::read_to_string(&job.progress).unwrap();
        assert!(progress.contains("progress=end"), "{progress}");
        fs::remove_file(&job.output).unwrap();
        fs::remove_dir_all(job.script.parent().unwrap()).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}

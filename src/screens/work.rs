//! A clip being compressed. Its frame, dithered to 1 bit, fills a window;
//! over it a small dialog says how far along the encode is, then what it
//! came to. ffmpeg's log is an ink bar along the bottom that opens into a
//! drawer.
use crate::{
    Active, App,
    clip::clock,
    job::human_size,
    menu, native,
    review::Pic,
    ui::{
        controls::{Tone, aside, button_with, label, meter, pulse, steps, tag, window},
        dither,
        motion::{Spec, Tween},
        theme::{
            ACCENT, BACKSTAGE, INK, MONO, ON_INK, PAPER, TEXT, TEXT_FAINT, TEXT_MUTED, alpha,
            glyph, size, weight,
        },
    },
};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};
use zgui::{compose::prelude::*, terminal::terminal, timer::sleep};
use zgui_desktop::AppMenuEntry;

/// The drawer's height when open.
const DRAWER: f32 = 300.;
const SLIDE: Spec = Spec(0.32, crate::ui::motion::EASE_OUT_EXPO);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum End {
    Ok,
    Failed,
}

/// How the work is going, read from its files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// Of the clip encoded (the preset alone reports it).
    pub fraction: Option<f32>,
    pub out_time: f64,
    /// Encoding speed against real time.
    pub speed: Option<f32>,
    /// The output's size, once it's there.
    pub output: Option<u64>,
    pub ended: Option<End>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Working,
    Done(u64),
    Failed,
}

impl Progress {
    fn phase(&self) -> Phase {
        match (self.output, self.ended) {
            (Some(size), _) => Phase::Done(size),
            (None, Some(End::Failed)) => Phase::Failed,
            _ => Phase::Working,
        }
    }
}

/// Read the job's files: ffmpeg's progress (its last block), the output,
/// the status the script writes when the work ends.
fn read(active: &Active) -> Progress {
    let job = &active.job;
    let mut p = Progress {
        output: std::fs::metadata(&job.output)
            .ok()
            .map(|m| m.len())
            .filter(|&n| n > 0),
        ended: std::fs::read_to_string(&job.status)
            .ok()
            .and_then(|s| match s.trim() {
                "ok" => Some(End::Ok),
                "failed" => Some(End::Failed),
                _ => None,
            }),
        ..Progress::default()
    };
    if let Some(text) = tail(&job.progress) {
        for line in text.lines().rev() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "out_time_us" if p.out_time == 0. => {
                    p.out_time = value.parse::<f64>().unwrap_or(0.) / 1e6;
                }
                "speed" if p.speed.is_none() => {
                    p.speed = value.trim_end_matches('x').trim().parse().ok();
                }
                _ => {}
            }
        }
        if job.duration > 0. {
            let done = text.lines().rev().any(|l| l == "progress=end");
            p.fraction = Some(if done {
                1.
            } else {
                (p.out_time / job.duration).clamp(0., 1.) as f32
            });
        }
    }
    p
}

/// The file's last few kilobytes: ffmpeg appends a block every half second.
fn tail(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(4096))).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

pub fn view(app: &App, active: Active) -> View {
    let (app, active) = (app.clone(), active.clone());
    component(move |cx| {
        {
            let (active, signal) = (active.clone(), active.progress.clone());
            cx.tasks().spawn(async move {
                loop {
                    signal.set(read(&active));
                    sleep(Duration::from_millis(300)).await;
                }
            });
        }
        // The drawer follows `active.log`: the bar, ⌘L and a failure set it.
        let shown = active.log.with_untracked(|o| *o);
        let open = cx.state(Tween::settled(if shown { 1. } else { 0. }));
        {
            let (log, open, anim, term) = (
                active.log.clone(),
                open.clone(),
                app.anim.clone(),
                active.term.clone(),
            );
            let effect = cx.runtime().effect(move || {
                let to = if log.get() { 1. } else { 0. };
                anim.retarget(&open, to, SLIDE);
                if to > 0.5 {
                    term.focus();
                }
            });
            cx.retain(effect);
            // A failure opens it, to show why.
            let (progress, log) = (active.progress.clone(), active.log.clone());
            let effect = cx.runtime().effect(move || {
                if progress.with(|p| p.phase() == Phase::Failed) {
                    log.set(true);
                }
            });
            cx.retain(effect);
        }
        // For zgui's progress bar in the log's bar: the fraction encoded, or
        // NaN (indeterminate) until ffmpeg reports.
        let fraction = cx.state(f32::NAN);
        {
            let (progress, fraction) = (active.progress.clone(), fraction.clone());
            let effect = cx.runtime().effect(move || {
                let f = progress.with(|p| match p.phase() {
                    Phase::Done(_) => 1.,
                    _ => p.fraction.unwrap_or(f32::NAN),
                });
                if fraction.with_untracked(|v| v.to_bits() != f.to_bits()) {
                    fraction.set(f);
                }
            });
            cx.retain(effect);
        }
        let entrance = app.anim.entrance();
        column()
            .w_full()
            .h_full()
            .items_stretch()
            .reactive_style(move || {
                let t = entrance();
                Styles::new().opacity(t).translate(0., 10. * (1. - t))
            })
            .child(stage(&app, &active))
            .child(drawer(&app, &active, open, fraction))
    })
}

// Stage -----------------------------------------------------------------------

fn stage(app: &App, active: &Active) -> View {
    let which = {
        let p = active.progress.clone();
        move || p.with(Progress::phase)
    };
    let (window_handle, active_progress, active_log) = (
        app.window.clone(),
        active.progress.clone(),
        active.log.clone(),
    );
    let title = format!("Fig. 1 — {}", crate::file_name(&active.job.input));
    let (app, active) = (app.clone(), active.clone());
    // The frame as a 1-bit plate: Atkinson-dithered, ink on paper, in cells
    // of four pixels.
    let plate = active
        .preview
        .as_ref()
        .map(|Pic(image)| dither::atkinson(image.pixels(), image.width(), image.height(), 4));
    let inside = column()
        .w_full()
        .h_full()
        .relative()
        .overflow_hidden()
        .bg(PAPER)
        .child(
            row()
                .absolute()
                .left(0.)
                .top(0.)
                .right(0.)
                .bottom(0.)
                .items_center()
                .justify_center()
                .child(match plate {
                    Some(image) => zgui::compose::image("Frame, dithered", image)
                        .w_full()
                        .h_full()
                        .object_contain(),
                    None => dither::patch(dither::level(INK, 0.12, 2.))
                        .w_full()
                        .h_full(),
                }),
        )
        .child(
            switch(which, move |phase, _| match phase {
                Phase::Working => encoding(&active),
                Phase::Done(size) => done(&app, &active, size),
                Phase::Failed => failed(&app, &active),
            })
            .absolute()
            .left(0.)
            .top(0.)
            .right(0.)
            .bottom(0.),
        );
    // A right-click on the plate: AppKit's menu, the result's commands
    // first once there is one.
    let (progress, log) = (active_progress, active_log);
    let plate_menu = move || {
        let done = progress.with(|p| p.output.is_some());
        vec![
            native::entry("Open Result", menu::OPEN_RESULT, done),
            native::entry("Reveal Result in Finder", menu::REVEAL_RESULT, done),
            AppMenuEntry::Separator,
            native::entry("Reveal Original in Finder", menu::REVEAL_ORIGINAL, true),
            native::entry("Copy Original's Path", menu::COPY_PATH, true),
            AppMenuEntry::Separator,
            native::entry("Log", menu::COULISSES, true).checked(log.get()),
        ]
    };
    column()
        .w_full()
        .grow()
        .min_h(0.)
        .px(40.)
        .pt(8.)
        .pb(22.)
        .child(
            column().w_full().h_full().pr(5.).pb(5.).child(
                window(
                    title,
                    None,
                    native::context_menu(inside, window_handle, plate_menu),
                )
                .w_full()
                .h_full(),
            ),
        )
}

/// A small window in the middle of the plate.
fn dialog(title: &str, close: Option<Box<dyn Fn()>>, body: View) -> View {
    column()
        .w_full()
        .h_full()
        .items_center()
        .justify_center()
        .child(
            column()
                .w(400.)
                .pr(5.)
                .pb(5.)
                .child(window(title, close, body.p(18.).gap(12.)).w_full()),
        )
}

/// How much is encoded, how fast, how long to go.
fn encoding(active: &Active) -> View {
    let (big, bar, line) = (
        active.progress.clone(),
        active.progress.clone(),
        active.progress.clone(),
    );
    let duration = active.job.duration;
    dialog(
        "Compressing",
        None,
        column()
            .child(
                row()
                    .items_end()
                    .gap(6.)
                    .child(
                        text_signal(move || {
                            let f = big.with(|p| p.fraction.unwrap_or(0.));
                            format!("{:.0}", f * 100.)
                        })
                        .font_family(MONO)
                        .font_weight(weight::BOLD)
                        .text_size(56.)
                        .line_height(58.)
                        .letter_spacing(-2.),
                    )
                    .child(
                        text("%")
                            .pb(9.)
                            .font_family(MONO)
                            .font_weight(weight::BOLD)
                            .text_size(22.)
                            .text_color(TEXT_MUTED),
                    ),
            )
            .child(meter(move || bar.with(|p| p.fraction.unwrap_or(0.)), 14.))
            .child(
                text_signal(move || {
                    line.with(|p| {
                        let mut parts =
                            vec![format!("{} / {}", clock(p.out_time), clock(duration))];
                        if let Some(speed) = p.speed.filter(|s| *s > 0.) {
                            parts.push(format!("{speed:.1}×"));
                            let left = (duration - p.out_time).max(0.) / speed as f64;
                            parts.push(format!("{} left", clock(left)));
                        }
                        parts.join("  ·  ").to_uppercase()
                    })
                })
                .font_family(MONO)
                .text_size(size::CAPTION)
                .letter_spacing(1.)
                .text_color(TEXT_MUTED),
            ),
    )
}

/// What it came to, before and after, and what next.
fn done(app: &App, active: &Active, size: u64) -> View {
    let original = active.job.original_size.max(1);
    let ratio = size as f64 / original as f64;
    let saving = if ratio <= 1. {
        format!("−{:.0}%", (1. - ratio) * 100.)
    } else {
        format!("+{:.0}%", (ratio - 1.) * 100.)
    };
    let (play, reveal) = (active.job.output.clone(), active.job.output.clone());
    let (leave, closer) = (app.clone(), app.clone());
    let stat = |name: &str, value: String| {
        column().gap(3.).child(label(name)).child(
            text(value)
                .font_family(MONO)
                .font_weight(weight::BOLD)
                .text_size(22.)
                .letter_spacing(-0.5)
                .text_color(TEXT),
        )
    };
    dialog(
        "Done",
        Some(Box::new(move || closer.close_clip())),
        column()
            .child(
                row()
                    .gap(28.)
                    .items_end()
                    .child(stat("Before", human_size(original)))
                    .child(stat("After", human_size(size)))
                    .child(div().grow())
                    .child(tag(&saving).mb(4.)),
            )
            .child(meter(move || ratio.min(1.) as f32, 10.))
            .child(
                text(if ratio <= 1. {
                    format!(
                        "Saved beside the original as {}.",
                        crate::file_name(&active.job.output)
                    )
                } else {
                    "It came out larger: the original was already well made. Try a lower \
                     quality."
                        .into()
                })
                .font_family(crate::ui::theme::DISPLAY)
                .text_size(size::BODY)
                .text_color(TEXT_MUTED)
                .text_wrap(true),
            )
            .child(
                row()
                    .mt(6.)
                    .gap(14.)
                    .items_center()
                    .child(
                        button_with(Tone::Primary, None, "View")
                            .on_click(move || {
                                let _ = std::process::Command::new("open").arg(&play).spawn();
                            })
                            .tooltip("Open the result"),
                    )
                    .child(
                        button_with(Tone::Secondary, None, "Reveal")
                            .on_click(move || {
                                let _ = std::process::Command::new("open")
                                    .arg("-R")
                                    .arg(&reveal)
                                    .spawn();
                            })
                            .tooltip("Show it in Finder"),
                    )
                    .child(div().grow())
                    .child(
                        button_with(Tone::Ghost, None, "Another")
                            .on_click(move || leave.close_clip())
                            .tooltip("Close the clip (⌘.)"),
                    ),
            ),
    )
}

fn failed(app: &App, active: &Active) -> View {
    let (log, leave, closer) = (active.log.clone(), app.clone(), app.clone());
    dialog(
        "Stopped",
        Some(Box::new(move || closer.close_clip())),
        column()
            .child(aside(
                "ffmpeg set the work down before it finished. The log below says why.",
                size::BODY,
            ))
            .child(
                row()
                    .mt(4.)
                    .gap(14.)
                    .child(
                        button_with(Tone::Primary, None, "Show log").on_click(move || {
                            log.set(true);
                        }),
                    )
                    .child(div().grow())
                    .child(
                        button_with(Tone::Ghost, None, "Another")
                            .on_click(move || leave.close_clip()),
                    ),
            ),
    )
}

// Log -------------------------------------------------------------------------

/// The log: an ink bar along the bottom, as solweb's `pre`, with what's
/// running and its latest word; the terminal itself rises when opened.
fn drawer(
    app: &App,
    active: &Active,
    open: zgui::reactive::Signal<Tween>,
    fraction: zgui::reactive::Signal<f32>,
) -> View {
    let anim = app.anim.clone();
    let height = DRAWER;
    let slide = {
        let (anim, open) = (anim.clone(), open.clone());
        move || anim.tween(&open, SLIDE)
    };
    let (body, chevron) = (slide.clone(), slide);
    let toggle = {
        let log = active.log.clone();
        move || {
            log.update(|open| *open = !*open);
        }
    };
    let (state, progress) = (active.term.state(), active.progress.clone());
    let paper = |a: f32| alpha(ON_INK, a);
    column()
        .w_full()
        .shrink_0()
        .bg(BACKSTAGE)
        .child(
            row()
                .h(34.)
                .px(40.)
                .gap(14.)
                .items_center()
                .cursor(zgui::cursor::Cursor::Pointer)
                .on_click(toggle.clone())
                .child(
                    text("LOG")
                        .font_family(MONO)
                        .font_weight(weight::BOLD)
                        .text_size(size::XS)
                        .letter_spacing(1.2)
                        .text_color(ON_INK),
                )
                .child(
                    text("FFMPEG")
                        .font_family(MONO)
                        .text_size(size::CAPTION)
                        .letter_spacing(1.)
                        .text_color(paper(0.55)),
                )
                .child(theme_scope(
                    // Paper on ink: the track is the theme's input colour,
                    // the fill the text colour.
                    zgui::theme::ThemeOverride::new()
                        .with(zgui::theme::tok::INPUT_BG, alpha(ON_INK, 0.14)),
                    zgui::compose::progress("Encoding", fraction)
                        .w(110.)
                        .h(4.)
                        .text_color(ON_INK)
                        .shrink_0(),
                ))
                .child(pulse(&anim, ACCENT).reactive_style({
                    let progress = progress.clone();
                    move || {
                        if progress.with(|p| p.phase() == Phase::Working) {
                            Styles::new().flex()
                        } else {
                            Styles::new().hidden()
                        }
                    }
                }))
                .child(
                    text_signal(move || {
                        let title = state.with(|s| s.title.clone());
                        progress.with(|p| match p.phase() {
                            Phase::Done(_) => "finished".into(),
                            Phase::Failed => "stopped".into(),
                            Phase::Working if !title.trim().is_empty() => title,
                            Phase::Working => "running".into(),
                        })
                    })
                    .grow()
                    .min_w(0.)
                    .font_family(MONO)
                    .text_size(size::XS)
                    .text_color(paper(0.45))
                    .truncate(),
                )
                .child(
                    row()
                        .gap(8.)
                        .items_center()
                        .text_color(paper(0.8))
                        .child(
                            text("SHOW")
                                .font_family(MONO)
                                .text_size(size::CAPTION)
                                .letter_spacing(1.),
                        )
                        .child(
                            row()
                                .child(glyph("arrow-down", 11.))
                                .reactive_style(move || {
                                    // Up to open, down to close.
                                    Styles::new().rotate(std::f32::consts::PI * (1. - chevron()))
                                }),
                        ),
                ),
        )
        .child(
            column()
                .w_full()
                .overflow_hidden()
                .reactive_style(move || {
                    let t = body();
                    let s = Styles::new().h(height * t).opacity(t.min(1.));
                    if t <= 0.001 { s.hidden() } else { s.flex() }
                })
                .child(
                    terminal(&active.term)
                        .w_full()
                        .h(height - 10.)
                        .px(40.)
                        .pb(10.),
                ),
        )
}

/// The title bar while a clip is worked on: the figure, how it goes, the
/// steps, a way out.
pub fn header(_cx: &mut Context, app: &App, active: Active) -> View {
    let leave = app.clone();
    let progress = active.progress.clone();
    row()
        .w_full()
        .min_w(0.)
        .gap(12.)
        .items_center()
        .child(tag("Fig. 1"))
        .child(
            column()
                .grow()
                .min_w(0.)
                .gap(2.)
                .child(
                    text(crate::file_name(&active.job.input))
                        .font_family(crate::ui::theme::DISPLAY)
                        .text_size(size::BODY)
                        .font_weight(weight::BOLD)
                        .text_color(TEXT)
                        .truncate(),
                )
                .child(
                    text_signal(move || {
                        progress.with(|p| match p.phase() {
                            Phase::Working => match p.fraction {
                                Some(f) => format!("COMPRESSING · {:.0}%", f * 100.),
                                None => "COMPRESSING".into(),
                            },
                            Phase::Done(_) => "DONE".into(),
                            Phase::Failed => "STOPPED".into(),
                        })
                    })
                    .font_family(MONO)
                    .text_size(size::CAPTION)
                    .letter_spacing(1.)
                    .text_color(TEXT_FAINT),
                ),
        )
        .child(steps(3))
        .child(
            button_with(Tone::Ghost, Some("close"), "Another")
                .on_click(move || leave.close_clip())
                .tooltip("Close the clip (⌘.)"),
        )
}

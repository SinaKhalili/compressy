//! A clip being compressed. The plate is the stage: the frame under a
//! vellum wash while the work goes on, with how far along it is; then the
//! frame bare, and a card pinned to it with what it came to. The terminal
//! doing the work (ffmpeg and its log) is backstage, the coulisses,
//! a dark bar along the bottom that opens into a drawer.
use crate::{
    Active, App,
    clip::clock,
    job::human_size,
    menu, native,
    review::Pic,
    ui::{
        controls::{
            Tone, button_with, crop_marks, display, label, label_grid_small, numbered, pulse,
        },
        motion::{Spec, Tween},
        theme::{
            ACCENT, BACKSTAGE, DISPLAY, INK, MONO, ON_INK, PAPER, PAPER_DEEP, TEXT_FAINT,
            TEXT_MUTED, alpha, glyph, ink, size, weight,
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
        // For zgui's progress bar in the coulisses: the fraction encoded, or
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
    let phase = {
        let p = active.progress.clone();
        move || p.with(Progress::phase)
    };
    let (vellum, which) = (phase.clone(), phase);
    let (window, active_progress, active_log) = (
        app.window.clone(),
        active.progress.clone(),
        active.log.clone(),
    );
    let (app, active) = (app.clone(), active.clone());
    let preview = active.preview.clone();
    let mut frame = column()
        .w_full()
        .h_full()
        .relative()
        .bg(PAPER_DEEP)
        .border(1.)
        .border_color(ink(0.35));
    for mark in crop_marks(ink(0.5)) {
        frame = frame.child(mark);
    }
    let frame = frame.child(
        column()
            .w_full()
            .h_full()
            .overflow_hidden()
            .relative()
            .child(
                row()
                    .absolute()
                    .left(0.)
                    .top(0.)
                    .right(0.)
                    .bottom(0.)
                    .items_center()
                    .justify_center()
                    .child(match preview {
                        Some(Pic(image)) => zgui::compose::image("Frame", image)
                            .w_full()
                            .h_full()
                            .object_contain(),
                        None => div(),
                    }),
            )
            // Vellum over the frame while the work goes on.
            .child(
                div()
                    .absolute()
                    .left(0.)
                    .top(0.)
                    .right(0.)
                    .bottom(0.)
                    .bg(alpha(PAPER, 0.86))
                    .reactive_style(move || {
                        Styles::new().opacity(match vellum() {
                            Phase::Done(_) => 0.,
                            _ => 1.,
                        })
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
            ),
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
            native::entry("Coulisses", menu::COULISSES, true).checked(log.get()),
        ]
    };
    column()
        .w_full()
        .grow()
        .min_h(0.)
        .pl(48.)
        .pr(48.)
        .pt(22.)
        .pb(26.)
        .child(native::context_menu(frame, window, plate_menu))
}

/// A centred column over the frame.
fn overlay() -> View {
    column()
        .w_full()
        .h_full()
        .items_center()
        .justify_center()
        .gap(14.)
}

/// The preset: how much is made, how fast, how long to go.
fn encoding(active: &Active) -> View {
    let (big, bar, line) = (
        active.progress.clone(),
        active.progress.clone(),
        active.progress.clone(),
    );
    let duration = active.job.duration;
    overlay()
        .child(numbered(3, "Compression en cours"))
        .child(
            row()
                .items_start()
                .child(
                    text_signal(move || {
                        let f = big.with(|p| p.fraction.unwrap_or(0.));
                        format!("{:.0}", f * 100.)
                    })
                    .font_family(DISPLAY)
                    .font_weight(weight::THIN)
                    .text_size(168.)
                    .line_height(160.)
                    .letter_spacing(-6.),
                )
                .child(
                    text("%")
                        .mt(22.)
                        .ml(6.)
                        .font_family(DISPLAY)
                        .font_weight(weight::LIGHT)
                        .text_size(48.)
                        .text_color(ACCENT),
                ),
        )
        .child(
            div()
                .w(440.)
                .h(3.)
                .relative()
                .child(
                    div()
                        .absolute()
                        .left(0.)
                        .right(0.)
                        .top(1.)
                        .h(1.)
                        .bg(ink(0.25)),
                )
                .child(
                    div()
                        .absolute()
                        .left(0.)
                        .top(0.)
                        .h(3.)
                        .bg(ACCENT)
                        .reactive_style(move || {
                            let f = bar.with(|p| p.fraction.unwrap_or(0.));
                            Styles::new().w_percent(f * 100.)
                        }),
                ),
        )
        .child(
            text_signal(move || {
                line.with(|p| {
                    let mut parts = vec![format!("{} / {}", clock(p.out_time), clock(duration))];
                    if let Some(speed) = p.speed.filter(|s| *s > 0.) {
                        parts.push(format!("{speed:.1}×"));
                        let left = (duration - p.out_time).max(0.) / speed as f64;
                        parts.push(format!("{} remaining", clock(left)));
                    }
                    parts.join("   —   ").to_uppercase()
                })
            })
            .mt(4.)
            .font_family(MONO)
            .text_size(size::CAPTION)
            .letter_spacing(2.2)
            .text_color(TEXT_MUTED),
        )
}

/// What it came to, on a card pinned to the bare frame, with what next.
fn done(app: &App, active: &Active, size: u64) -> View {
    let original = active.job.original_size.max(1);
    let ratio = size as f64 / original as f64;
    let saving = if ratio <= 1. {
        format!("−{:.0}%", (1. - ratio) * 100.)
    } else {
        format!("+{:.0}%", (ratio - 1.) * 100.)
    };
    let shown = human_size(size);
    let (number, unit) = shown.split_once(' ').unwrap_or((&shown, ""));
    let entrance = app.anim.entrance();
    let (play, reveal) = (active.job.output.clone(), active.job.output.clone());
    let leave = app.clone();
    column()
        .w_full()
        .h_full()
        .justify_end()
        .items_start()
        .p(22.)
        .child(
            column()
                .p(22.)
                .gap(4.)
                .bg(PAPER)
                .border(1.)
                .border_color(INK)
                .reactive_style(move || {
                    let t = entrance();
                    Styles::new().opacity(t).translate(0., 16. * (1. - t))
                })
                .child(numbered(4, "Livré"))
                .child(
                    row()
                        .mt(6.)
                        .gap(12.)
                        .items_end()
                        .child(
                            text(number.to_owned())
                                .font_family(DISPLAY)
                                .font_weight(weight::THIN)
                                .text_size(88.)
                                .line_height(84.)
                                .letter_spacing(-3.),
                        )
                        .child(
                            text(unit.to_owned())
                                .pb(12.)
                                .font_family(DISPLAY)
                                .font_weight(weight::LIGHT)
                                .text_size(24.)
                                .text_color(TEXT_MUTED),
                        )
                        .child(
                            text(saving)
                                .pb(10.)
                                .ml(10.)
                                .font_family(DISPLAY)
                                .font_weight(weight::LIGHT)
                                .text_size(40.)
                                .text_color(ACCENT),
                        ),
                )
                .child(
                    text(if ratio <= 1. {
                        format!("from {}.", human_size(original))
                    } else {
                        format!(
                            "from {}: it was already well made; try a lower quality.",
                            human_size(original)
                        )
                    })
                    .font_family(DISPLAY)
                    .font_weight(weight::LIGHT)
                    .text_size(16.)
                    .text_color(TEXT_MUTED),
                )
                .child(
                    row()
                        .mt(14.)
                        .gap(10.)
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
                        .child(
                            button_with(Tone::Ghost, None, "Another")
                                .on_click(move || leave.close_clip())
                                .tooltip("Close the clip (⌘.)"),
                        ),
                ),
        )
}

fn failed(app: &App, active: &Active) -> View {
    let (log, leave) = (active.log.clone(), app.clone());
    let why = "ffmpeg set the work down. The coulisses, below, say why.";
    overlay()
        .child(numbered(3, "Interrompu"))
        .child(display("UNFINISHED", 96., weight::HAIR))
        .child(
            text(why)
                .font_family(DISPLAY)
                .font_weight(weight::LIGHT)
                .text_size(16.)
                .text_color(TEXT_MUTED),
        )
        .child(
            row()
                .mt(10.)
                .gap(10.)
                .child(
                    button_with(Tone::Secondary, None, "Open the coulisses").on_click(move || {
                        log.set(true);
                    }),
                )
                .child(
                    button_with(Tone::Ghost, None, "Another").on_click(move || leave.close_clip()),
                ),
        )
}

// Coulisses -------------------------------------------------------------------

/// Backstage: a dark bar along the bottom with what's running and its
/// latest word, and the terminal itself, rising when opened.
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
                .h(44.)
                .pl(48.)
                .pr(36.)
                .gap(16.)
                .items_center()
                .cursor(zgui::cursor::Cursor::Pointer)
                .on_click(toggle.clone())
                .child(
                    text("COULISSES")
                        .font_family(DISPLAY)
                        .font_weight(weight::BOLD)
                        .text_size(11.)
                        .letter_spacing(3.)
                        .text_color(ON_INK),
                )
                .child(
                    text("FFMPEG")
                        .font_family(MONO)
                        .text_size(size::CAPTION)
                        .letter_spacing(2.2)
                        .text_color(paper(0.55)),
                )
                // zgui's own progress bar: the share encoded, or moving
                // without end until ffmpeg reports.
                .child(theme_scope(
                    // Paper on ink, in the dark bar: the track is the theme's
                    // input colour, the fill the text colour.
                    zgui::theme::ThemeOverride::new()
                        .with(zgui::theme::tok::INPUT_BG, alpha(ON_INK, 0.14)),
                    zgui::compose::progress("Encoding", fraction)
                        .w(120.)
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
                    .text_color(paper(0.4))
                    .truncate(),
                )
                .child(
                    row()
                        .gap(8.)
                        .items_center()
                        .text_color(paper(0.75))
                        .child(
                            text("LOG")
                                .font_family(MONO)
                                .text_size(size::CAPTION)
                                .letter_spacing(2.2),
                        )
                        .child(
                            row()
                                .child(glyph("arrow-down", 12.))
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
/// hand at work, a way out.
pub fn header(_cx: &mut Context, app: &App, active: Active) -> View {
    let leave = app.clone();
    let progress = active.progress.clone();
    let (mark, hand) = (glyph("fast", 11.), "BY PRESET");
    row()
        .w_full()
        .min_w(0.)
        .gap(14.)
        .items_center()
        .child(
            text("FIG. 01")
                .font_family(DISPLAY)
                .font_weight(weight::BOLD)
                .text_size(12.)
                .letter_spacing(2.4)
                .text_color(ACCENT),
        )
        .child(
            column()
                .grow()
                .min_w(0.)
                .gap(3.)
                .child(
                    text(crate::file_name(&active.job.input))
                        .text_size(size::LG)
                        .font_weight(500)
                        .truncate(),
                )
                .child(
                    text_signal(move || {
                        progress.with(|p| match p.phase() {
                            Phase::Working => match p.fraction {
                                Some(f) => format!("EN COURS — {:.0}%", f * 100.),
                                None => "EN COURS".into(),
                            },
                            Phase::Done(_) => "LIVRÉ".into(),
                            Phase::Failed => "INTERROMPU".into(),
                        })
                    })
                    .font_family(MONO)
                    .text_size(size::CAPTION)
                    .letter_spacing(1.8)
                    .text_color(TEXT_FAINT),
                ),
        )
        .child(
            row()
                .gap(7.)
                .items_center()
                .text_color(TEXT_MUTED)
                .child(mark)
                .child(label(hand)),
        )
        .child(label_grid_small(3))
        .child(
            button_with(Tone::Ghost, Some("close"), "Another")
                .on_click(move || leave.close_clip())
                .tooltip("Close the clip (⌘.)"),
        )
}

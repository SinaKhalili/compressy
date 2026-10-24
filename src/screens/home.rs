//! The desk: what Compressy is and how it goes, on the left; a window to
//! drop a video in, on the right. Nothing runs from here: a dropped clip
//! goes to review. If ffmpeg is missing, a note above the footer says how
//! to get it, and goes once it's installed.
use crate::{
    App, menu, native,
    tools::Tools,
    ui::{
        controls::{Tone, aside, button_with, command, display, label, numbered, pulse, window},
        dither,
        motion::HOVER_FADE,
        theme::{ACCENT, INK, MONO, PAPER, TEXT, TEXT_MUTED, glyph, size, weight},
    },
};
use std::time::Duration;
use zgui::{compose::prelude::*, reactive::Signal, timer::sleep};

pub fn view(app: &App) -> View {
    let app = app.clone();
    component(move |cx| {
        // Watch for ffmpeg being installed: cheap, it only checks which
        // executables exist.
        let tools = app.tools.clone();
        cx.tasks().spawn(async move {
            loop {
                sleep(Duration::from_secs(2)).await;
                if tools.with(Tools::stale) {
                    tools.set(Tools::detect());
                }
            }
        });
        // The little guy blinks now and then.
        let blink = app.runtime.signal(false);
        let eyes = blink.clone();
        cx.tasks().spawn(async move {
            loop {
                sleep(Duration::from_millis(4200)).await;
                eyes.set(true);
                sleep(Duration::from_millis(140)).await;
                eyes.set(false);
            }
        });
        let entrance = app.anim.entrance();
        let (which, note_app) = (app.tools.clone(), app.clone());
        column()
            .w_full()
            .h_full()
            .px(40.)
            .reactive_style(move || {
                let t = entrance();
                Styles::new().opacity(t).translate(0., 8. * (1. - t))
            })
            .child(
                row()
                    .w_full()
                    .grow()
                    .min_h(0.)
                    .pt(10.)
                    .pb(28.)
                    .gap(48.)
                    .items_stretch()
                    .child(about(blink))
                    .child(drop_window(&app).grow().min_w(0.)),
            )
            .child(
                switch(
                    move || which.with(|t| (!t.has_ffmpeg()).then_some(t.brew.is_some())),
                    move |missing, _| match missing {
                        Some(brew) => ffmpeg_note(&note_app, brew),
                        None => div(),
                    },
                )
                .w_full()
                .shrink_0(),
            )
            .child(footer())
    })
}

/// The left column, as solweb's front page: a kicker, the title and the
/// little guy, an "About" box with its label set into the frame, and the
/// steps.
fn about(blink: Signal<bool>) -> View {
    column()
        .w(360.)
        .shrink_0()
        .gap(26.)
        .child(
            row()
                .w_full()
                .pb(8.)
                .border_edges(zgui::scene::Insets {
                    top: 0.,
                    right: 0.,
                    bottom: 1.,
                    left: 0.,
                })
                .border_color(INK)
                .child(label("A small video compressor").grow())
                .child(label("v0.1")),
        )
        .child(
            row()
                .items_end()
                .gap(14.)
                .child(display("make a small file", 30., weight::BOLD).text_wrap(true))
                .child(
                    switch(move || blink.get(), |shut, _| dither::little_guy(shut, 3.))
                        .shrink_0()
                        .tooltip("hi"),
                ),
        )
        .child(
            column()
                .relative()
                .mt(4.)
                .p(16.)
                .pt(18.)
                .border(1.)
                .border_color(INK)
                .child(
                    row()
                        .absolute()
                        .left(12.)
                        .top(-7.)
                        .px(5.)
                        .bg(PAPER)
                        .child(label("About")),
                )
                .child(aside(
                    "Drop a clip. Tune it against an estimate Compressy measures by \
                     encoding samples, crop it if you like, then compress it with ffmpeg. \
                     The original is never touched, and nothing leaves your Mac.",
                    13.,
                )),
        )
        .child(
            column()
                .gap(16.)
                .child(step(1, "Drop", "a video, any video"))
                .child(step(2, "Tune and crop", "against a measured estimate"))
                .child(step(3, "Compress", "one ffmpeg pass, its log beside")),
        )
}

fn step(n: u8, name: &str, aside_text: &str) -> View {
    column()
        .gap(5.)
        .child(numbered(n, name))
        .child(aside(aside_text, 12.).pl(32.))
}

/// The drop window: its desk a slow radial dither that thickens as a file
/// comes over, the video glyph, what to do, and the open panel for those
/// who'd rather not drag.
fn drop_window(app: &App) -> View {
    let hover = {
        let (anim, tween) = (app.anim.clone(), app.drop_hover.clone());
        move || anim.tween(&tween, HOVER_FADE)
    };
    let (field, words) = (hover.clone(), hover);
    let title = app.drop_hover.clone();
    let (error, shown) = (app.error.clone(), app.error.clone());
    let chooser = app.clone();
    let inside = column()
        .w_full()
        .h_full()
        .relative()
        .items_center()
        .justify_center()
        .gap(14.)
        // The dither under it all, in steps of density, never grey.
        .child(
            div()
                .absolute()
                .left(0.)
                .top(0.)
                .right(0.)
                .bottom(0.)
                .reactive_style(move || {
                    let t = field();
                    dither::radial(INK, 0.04 + 0.5 * t, 0.16 + 0.3 * t, 0.9, 2.)
                }),
        )
        .child(
            column()
                .relative()
                .items_center()
                .gap(12.)
                .px(22.)
                .py(18.)
                .bg(PAPER)
                .border(1.)
                .border_color(INK)
                .reactive_style(move || {
                    let t = words();
                    Styles::new().translate(0., -3. * t)
                })
                .child(row().text_color(TEXT).child(glyph("video", 22.)))
                .child(
                    text_signal(move || {
                        if title.with(|t| t.target > 0.5) {
                            "RELEASE TO OPEN".into()
                        } else {
                            "DROP A VIDEO HERE".into()
                        }
                    })
                    .font_family(MONO)
                    .text_size(15.)
                    .font_weight(weight::BOLD)
                    .letter_spacing(1.5)
                    .text_color(TEXT),
                )
                .child(label("MP4 · MOV · MKV · WEBM · GIF"))
                .child(
                    button_with(Tone::Secondary, None, "Choose a file…")
                        .mt(4.)
                        .on_click(move || chooser.choose_file())
                        .tooltip("Or press ⌘O"),
                ),
        )
        .child(
            row()
                .absolute()
                .left(0.)
                .right(0.)
                .bottom(14.)
                .justify_center()
                .reactive_style(move || {
                    if shown.with(String::is_empty) {
                        Styles::new().hidden()
                    } else {
                        Styles::new().flex()
                    }
                })
                .child(
                    row()
                        .px(8.)
                        .py(3.)
                        .bg(PAPER)
                        .border(1.)
                        .border_color(ACCENT)
                        .child(
                            text_signal(move || error.get())
                                .font_family(MONO)
                                .text_size(size::XS)
                                .text_color(TEXT)
                                .truncate(),
                        ),
                ),
        );
    // A right-click on the window: AppKit's menu, to open one from there.
    let opens = app.active.clone();
    let inside = native::context_menu(inside, app.window.clone(), move || {
        vec![native::entry(
            "Open…",
            menu::OPEN,
            opens.with(Option::is_none),
        )]
    });
    column()
        .h_full()
        .pr(5.)
        .pb(5.)
        .child(window("Drop", None, inside).w_full().h_full())
}

/// The footer, as solweb's: a rule, then small mono notes.
fn footer() -> View {
    row()
        .w_full()
        .shrink_0()
        .mb(14.)
        .pt(10.)
        .gap(18.)
        .border_edges(zgui::scene::Insets {
            top: 1.,
            right: 0.,
            bottom: 0.,
            left: 0.,
        })
        .border_color(INK)
        .child(label("Compressy").text_color(TEXT))
        .child(label("Made with zgui"))
        .child(label("Encodes with ffmpeg"))
        .child(div().grow())
        .child(label("Saved beside the original as name.compressed.mp4"))
}

/// ffmpeg is missing: the one command (Homebrew first, if that's missing
/// too), a terminal to run it in, and a blinking dot saying the window is
/// watching.
fn ffmpeg_note(app: &App, brew: bool) -> View {
    let anim = app.anim.clone();
    let mut content = row()
        .gap(12.)
        .items_center()
        .child(
            row().px(5.).py(2.).bg(ACCENT).shrink_0().child(
                text("NOTE")
                    .font_family(MONO)
                    .text_size(size::CAPTION)
                    .font_weight(weight::BOLD)
                    .text_color(PAPER),
            ),
        )
        .child(
            text(if brew {
                "ffmpeg isn't installed. It does the encoding: run this, and Compressy wakes up."
            } else {
                "ffmpeg isn't installed. Get Homebrew first, then run this."
            })
            .grow()
            .min_w(0.)
            .font_family(crate::ui::theme::DISPLAY)
            .text_size(size::BODY)
            .text_color(TEXT_MUTED)
            .truncate(),
        );
    if !brew {
        content = content.child(
            button_with(Tone::Secondary, Some("arrow-up-right"), "brew.sh").on_click(|| {
                let _ = std::process::Command::new("open")
                    .arg("https://brew.sh")
                    .spawn();
            }),
        );
    }
    content
        .mb(12.)
        .p(10.)
        .border(1.)
        .border_color(INK)
        .child(
            row()
                .w(230.)
                .shrink_0()
                .child(command("brew install ffmpeg").w_full()),
        )
        .child(
            button_with(Tone::Secondary, Some("terminal"), "Terminal").on_click(|| {
                let _ = std::process::Command::new("open")
                    .args(["-a", "Terminal"])
                    .spawn();
            }),
        )
        .child(pulse(&anim, ACCENT).tooltip("Watching for ffmpeg"))
}

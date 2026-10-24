//! The drop zone, set as a spread: the headline and the index on the left,
//! the plate awaiting material on the right. Nothing runs from here: a
//! dropped clip goes to review. If ffmpeg is missing, a footnote along the
//! bottom says how to get it, and goes once it's installed.
use crate::{
    App, menu, native,
    tools::Tools,
    ui::{
        controls::{
            Tone, aside, button_with, command, crop_marks, display, label, label_grid, pulse,
        },
        motion::HOVER_FADE,
        theme::{
            ACCENT, DISPLAY, INK, ON_INK, PAPER_DEEP, TEXT, TEXT_FAINT, TEXT_MUTED, glyph, ink,
            mix, size, weight,
        },
    },
};
use std::time::Duration;
use zgui::{compose::prelude::*, layout::Length, timer::sleep};

pub fn view(app: &App) -> View {
    let app = app.clone();
    component(move |cx| {
        // Watch for ffmpeg being installed: cheap, it only checks
        // which executables exist.
        let tools = app.tools.clone();
        cx.tasks().spawn(async move {
            loop {
                sleep(Duration::from_secs(2)).await;
                if tools.with(Tools::stale) {
                    tools.set(Tools::detect());
                }
            }
        });
        let entrance = app.anim.entrance();
        let which = app.tools.clone();
        let bar_app = app.clone();
        column()
            .w_full()
            .h_full()
            .relative()
            .reactive_style(move || {
                let t = entrance();
                Styles::new().opacity(t).translate(0., 10. * (1. - t))
            })
            .child(
                row()
                    .w_full()
                    .grow()
                    .min_h(0.)
                    .pl(48.)
                    .pr(64.)
                    .pt(18.)
                    .pb(36.)
                    .gap(56.)
                    .items_stretch()
                    .child(spread())
                    .child(plate(&app)),
            )
            .child(vertical_caption(
                "Collection automne–hiver · MMXXVI · Compression",
            ))
            .child(
                switch(
                    move || {
                        which.with(|t| {
                            if t.has_ffmpeg() {
                                Missing::Nothing
                            } else {
                                Missing::Ffmpeg {
                                    brew: t.brew.is_some(),
                                }
                            }
                        })
                    },
                    move |missing, _| match missing {
                        Missing::Ffmpeg { brew } => ffmpeg_note(&bar_app, brew),
                        Missing::Nothing => div(),
                    },
                )
                .w_full()
                .shrink_0(),
            )
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Missing {
    Ffmpeg { brew: bool },
    Nothing,
}

/// The left page: the house's line, the headline (bold over hairline),
/// the aside, and the house's label saying which step this is.
fn spread() -> View {
    column()
        .w(400.)
        .shrink_0()
        .justify_between()
        .child(label("Maison Compressy — atelier de compression"))
        .child(
            column()
                .child(display("Drop", 132., weight::BOLD))
                .child(display("a film.", 132., weight::HAIR).ml(4.).mt(-30.))
                .child(
                    aside("We take it in, quietly, and give it back lighter.", 19.)
                        .mt(24.)
                        .ml(8.)
                        .max_w(270.),
                ),
        )
        .child(
            row().gap(20.).items_end().child(label_grid(1)).child(
                column()
                    .gap(7.)
                    .child(step(1, "Drop", true))
                    .child(step(2, "Tune and crop", false))
                    .child(step(3, "Compress", false)),
            ),
        )
}

/// A line of the index beside the label: this step bold, the rest light.
fn step(n: u8, name: &str, here: bool) -> View {
    row()
        .gap(10.)
        .items_center()
        .child(
            text(format!("{n:02}"))
                .font_family(crate::ui::theme::MONO)
                .text_size(size::CAPTION)
                .letter_spacing(1.)
                .text_color(if here { ACCENT } else { TEXT_FAINT }),
        )
        .child(
            text(name.to_uppercase())
                .font_family(DISPLAY)
                .font_weight(if here { weight::BOLD } else { weight::LIGHT })
                .text_size(11.)
                .letter_spacing(2.)
                .text_color(if here { TEXT } else { TEXT_MUTED }),
        )
}

/// The plate: a matted frame with printer's marks, awaiting material. A
/// file over the window floods it with ink.
fn plate(app: &App) -> View {
    let chooser = app.clone();
    let hover = {
        let (anim, tween) = (app.anim.clone(), app.drop_hover.clone());
        move || anim.tween(&tween, HOVER_FADE)
    };
    let (mat, words, mark, rule) = (hover.clone(), hover.clone(), hover.clone(), hover);
    let title = app.drop_hover.clone();
    let (error, shown) = (app.error.clone(), app.error.clone());
    let mut frame = column()
        .w_full()
        .h_full()
        .relative()
        .items_center()
        .justify_center()
        .border(1.)
        .reactive_style(move || {
            let t = mat();
            Styles::new()
                .bg(mix(PAPER_DEEP, INK, t))
                .border_color(mix(ink(0.35), INK, t))
        });
    for m in crop_marks(ink(0.55)) {
        frame = frame.child(m);
    }
    frame = frame
        .child(
            row()
                .absolute()
                .left(18.)
                .top(16.)
                .child(
                    text("PLATE I")
                        .font_family(DISPLAY)
                        .font_weight(weight::BOLD)
                        .text_size(11.)
                        .letter_spacing(3.),
                )
                .reactive_style({
                    let t = mark.clone();
                    move || Styles::new().text_color(mix(TEXT, ON_INK, t()))
                }),
        )
        .child(
            row()
                .absolute()
                .right(18.)
                .top(19.)
                .child(label("Awaiting material"))
                .reactive_style({
                    let t = mark.clone();
                    move || Styles::new().opacity(1. - t())
                }),
        )
        .child(
            column()
                .items_center()
                .gap(16.)
                .reactive_style(move || {
                    let t = words();
                    Styles::new().text_color(mix(TEXT, ON_INK, t))
                })
                .child(glyph("video", 26.))
                .child(
                    text_signal(move || {
                        if title.with(|t| t.target > 0.5) {
                            "“RELEASE”".into()
                        } else {
                            "“LAY THE MATERIAL HERE”".into()
                        }
                    })
                    .font_family(DISPLAY)
                    .font_weight(weight::HAIR)
                    .text_size(30.)
                    .letter_spacing(2.),
                )
                .child(div().h(1.).reactive_style(move || {
                    // The rule draws out as a file comes over.
                    let t = rule();
                    Styles::new().w(40. + 120. * t).bg(mix(ink(0.4), ON_INK, t))
                }))
                .child(
                    text("MP4 · MOV · MKV · WEBM · GIF")
                        .font_family(crate::ui::theme::MONO)
                        .text_size(size::CAPTION)
                        .letter_spacing(2.2)
                        .reactive_style({
                            let t = mark.clone();
                            move || Styles::new().opacity(0.55 * (1. - t()))
                        }),
                )
                // Or the native open panel, for those who'd rather not drag.
                .child(
                    button_with(Tone::Secondary, None, "Choose a file…")
                        .mt(14.)
                        .on_click(move || chooser.choose_file())
                        .reactive_style({
                            let t = mark;
                            move || Styles::new().opacity(1. - t())
                        })
                        .tooltip("Or press ⌘O"),
                ),
        )
        .child(
            row()
                .absolute()
                .left(0.)
                .right(0.)
                .bottom(18.)
                .justify_center()
                .reactive_style(move || {
                    if shown.with(String::is_empty) {
                        Styles::new().hidden()
                    } else {
                        Styles::new().flex()
                    }
                })
                .child(
                    text_signal(move || error.get())
                        .font_family(DISPLAY)
                        .font_weight(weight::LIGHT)
                        .text_size(15.)
                        .text_color(ACCENT)
                        .truncate(),
                ),
        );
    // A right-click on the plate: AppKit's menu, to open one from there.
    let opens = app.active.clone();
    let frame = native::context_menu(frame, app.window.clone(), move || {
        vec![native::entry(
            "Open…",
            menu::OPEN,
            opens.with(Option::is_none),
        )]
    });
    column().grow().min_w(0.).py(22.).px(20.).child(frame)
}

/// A line of type turned up the right margin, as a magazine runs its folio.
fn vertical_caption(s: &str) -> View {
    // Rotation doesn't move layout: a fixed box, turned about its centre,
    // set so its centre sits on the margin.
    const W: f32 = 420.;
    row()
        .absolute()
        .right(18. - W / 2. + 6.)
        .top(Length::Percent(0.5))
        .w(W)
        .h(12.)
        .justify_center()
        .translate(0., -40.)
        .rotate(std::f32::consts::FRAC_PI_2)
        .child(label(s))
}

/// A footnote along the bottom: a rule, *N.B.*, and what it's about.
pub fn note(content: View) -> View {
    column().mx(48.).mb(18.).child(div().h(1.).bg(INK)).child(
        row()
            .pt(12.)
            .gap(16.)
            .items_center()
            .child(
                text("N.B.")
                    .font_family(DISPLAY)
                    .font_weight(weight::BOLD)
                    .text_size(18.)
                    .text_color(ACCENT),
            )
            .child(content.grow().min_w(0.)),
    )
}

/// ffmpeg is missing: the one command (Homebrew first, if that's missing
/// too), a terminal to run it in, and a pulse saying the window is watching.
fn ffmpeg_note(app: &App, brew: bool) -> View {
    let anim = app.anim.clone();
    let mut content = row().gap(14.).items_center().child(
        column()
            .grow()
            .min_w(0.)
            .gap(3.)
            .child(label("ffmpeg is not installed"))
            .child(
                text(if brew {
                    "It does the encoding. Run this, and the atelier opens."
                } else {
                    "It does the encoding. Homebrew first, then this."
                })
                .font_family(DISPLAY)
                .font_weight(weight::LIGHT)
                .text_size(15.)
                .text_color(TEXT)
                .truncate(),
            ),
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
    content = content
        .child(
            row()
                .w(240.)
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
        .child(pulse(&anim, ACCENT).tooltip("Watching for ffmpeg"));
    note(content)
}

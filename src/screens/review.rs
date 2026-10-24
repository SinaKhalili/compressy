//! A dropped clip, before anything runs: a frame of it to crop and to
//! compare before and after, the settings, and what they will come to,
//! measured.
use crate::{
    App,
    clip::ClipInfo,
    estimate::Metric,
    job::human_size,
    menu, native,
    prompt::Goal,
    review::{Est, Loaded, Mode, Pic, Review},
    settings::{self, Audio, Codec, Crop, FrameRate, Resolution, Settings},
    ui::{
        controls::{
            Segment, Tone, button_with, card, crop_marks, display, divider, label,
            label_grid_small, numbered, pulse, segmented,
        },
        motion::{FADE_IN, Tween},
        theme::{
            ACCENT, DISPLAY, INK, MONO, PAPER, PAPER_DEEP, TEXT, TEXT_FAINT, TEXT_MUTED, WARNING,
            alpha, glyph, icon, ink, size, weight,
        },
    },
};
use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::prelude::*,
    cursor::Cursor,
    input::{EventPhase, InputEvent, PointerButton},
    layout::Length,
    reactive::Signal,
    scene::Rect,
};

/// The settings column's width.
const PANEL: f32 = 360.;
/// How near a handle a press grabs it, in points.
const GRAB: f32 = 12.;
/// The smallest crop, as a fraction of a side.
const MIN_CROP: f32 = 0.05;

/// The title bar while a clip is under review: what it is, and a way out.
pub fn header(app: &App, review: Review) -> View {
    let clip = review.clip.clone();
    let back = app.review.clone();
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
                    text(crate::file_name(&review.input))
                        .text_size(size::LG)
                        .font_weight(500)
                        .truncate(),
                )
                .child(
                    text_signal(move || {
                        match clip.get() {
                            Loaded::Reading => "reading…".into(),
                            Loaded::Ready(info) => {
                                format!("{} · {}", info.summary(), human_size(info.size))
                            }
                            Loaded::Failed(_) => "couldn't read it".into(),
                        }
                        .to_uppercase()
                    })
                    .font_family(MONO)
                    .text_size(size::CAPTION)
                    .letter_spacing(1.8)
                    .text_color(TEXT_FAINT)
                    .truncate(),
                ),
        )
        .child(label_grid_small(2))
        .child(
            button_with(Tone::Ghost, Some("close"), "Cancel")
                .on_click(move || {
                    back.set(None);
                })
                .tooltip("Close the clip (⌘.)"),
        )
}

pub fn view(app: &App, review: Review) -> View {
    let (app, review) = (app.clone(), review.clone());
    component(move |cx| {
        // Measure whenever the settings that shape the file change, or the
        // clip becomes readable.
        {
            let (app, review, tasks) = (app.clone(), review.clone(), cx.tasks());
            let last: Rc<RefCell<Option<Settings>>> = Rc::default();
            let runtime = cx.runtime();
            let effect = cx.runtime().effect(move || {
                let settings = app.settings.get();
                let ready = matches!(review.clip.get(), Loaded::Ready(_));
                let tools = app.tools.get();
                if !ready || last.borrow().as_ref() == Some(&settings) {
                    return;
                }
                *last.borrow_mut() = Some(settings);
                runtime.untracked(|| review.measure(settings, tools, &app.pool, &tasks));
            });
            cx.retain(effect);
        }
        let entrance = app.anim.entrance();
        // Screenshot hooks: COMPRESSY_PREVIEW_CROP=1:1 opens in crop at that
        // aspect; COMPRESSY_PREVIEW_COMPRESS=1 presses Compress once it can.
        let preview_crop = std::env::var("COMPRESSY_PREVIEW_CROP").ok().and_then(|v| {
            let (w, h) = v.split_once(':')?;
            Some(w.parse::<f32>().ok()? / h.parse::<f32>().ok()?)
        });
        // The Clip menu sets it too (⌘1, ⌘2).
        let mode = review.mode.clone();
        if preview_crop.is_some() {
            mode.set(Mode::Crop);
        }
        let aspect = cx.state(preview_crop);
        if let Some(ratio) = preview_crop {
            let (app, review) = (app.clone(), review.clone());
            let effect = cx.runtime().effect(move || {
                if let Loaded::Ready(info) = review.clip.get() {
                    let crop = Crop::FULL.with_aspect(ratio, info.width, info.height);
                    app.settings.update(|s| s.crop = Some(crop));
                }
            });
            cx.retain(effect);
        }
        if std::env::var("COMPRESSY_PREVIEW_COMPRESS").is_ok_and(|v| !v.is_empty()) {
            let (app, clip) = (app.clone(), review.clip.clone());
            cx.tasks().spawn(async move {
                while !clip.with_untracked(|c| matches!(c, Loaded::Ready(_))) {
                    zgui::timer::sleep(std::time::Duration::from_millis(100)).await;
                }
                app.compress();
            });
        }
        let (loaded, stage_loaded) = (review.clip.clone(), review.clip.clone());
        let (panel_app, panel_review, stage_app, stage_review) =
            (app.clone(), review.clone(), app.clone(), review.clone());
        let (stage_mode, stage_aspect) = (mode.clone(), aspect.clone());
        row()
            .w_full()
            .h_full()
            .items_stretch()
            .pl(26.)
            .pr(40.)
            .pb(28.)
            .gap(40.)
            .reactive_style(move || {
                let t = entrance();
                Styles::new().opacity(t).translate(0., 10. * (1. - t))
            })
            .child(
                switch(
                    move || stage_loaded.get(),
                    move |loaded, cx| match loaded {
                        Loaded::Ready(info) => stage(
                            cx,
                            &stage_app,
                            &stage_review,
                            info,
                            stage_mode.clone(),
                            stage_aspect.clone(),
                        ),
                        _ => reading(&stage_app, &stage_review),
                    },
                )
                .grow()
                .min_w(0.)
                .h_full(),
            )
            .child(
                switch(
                    move || loaded.get(),
                    move |loaded, cx| match loaded {
                        Loaded::Ready(info) => panel(cx, &panel_app, &panel_review, info),
                        Loaded::Reading => column().w(PANEL),
                        Loaded::Failed(e) => failed(e),
                    },
                )
                .shrink_0(),
            )
    })
}

fn failed(error: String) -> View {
    card()
        .w(PANEL)
        .p(18.)
        .gap(10.)
        .child(label("Unreadable"))
        .child(display("This material won't take.", 26., weight::LIGHT).text_wrap(true))
        .child(
            text(error)
                .font_family(MONO)
                .text_size(size::XS)
                .text_color(ACCENT)
                .text_wrap(true),
        )
}

/// Before the clip is read: an empty stage with a pulse.
fn reading(app: &App, review: &Review) -> View {
    let _ = review;
    column()
        .w_full()
        .h_full()
        .gap(10.)
        .child(div().h(30.))
        .child(plate(stage_box().items_center().justify_center().child(
            chip(Some(pulse(&app.anim, ACCENT)), "Reading the material"),
        )))
}

// Stage: the toolbar and the frame ---------------------------------------------

fn stage(
    cx: &mut Context,
    app: &App,
    review: &Review,
    info: Rc<ClipInfo>,
    mode: Signal<Mode>,
    aspect: Signal<Option<f32>>,
) -> View {
    // The crop being edited: committed to the settings on release, so a drag
    // doesn't set off an estimate per pointer move.
    let draft = cx.state(
        app.settings
            .with_untracked(|s| s.crop)
            .unwrap_or(Crop::FULL),
    );
    // Crops set from outside the editor (an aspect, a reset) show in it.
    {
        let (settings, draft) = (app.settings.clone(), draft.clone());
        let effect = cx.runtime().effect(move || {
            let crop = settings.with(|s| s.crop).unwrap_or(Crop::FULL);
            draft.set(crop);
        });
        cx.retain(effect);
    }
    let (which, frame_app, frame_review, frame_info) =
        (mode.clone(), app.clone(), review.clone(), info.clone());
    let (frame_draft, frame_aspect) = (draft.clone(), aspect.clone());
    let caption_mode = mode.clone();
    // A right-click on the plate: AppKit's menu, with the Clip menu's
    // commands for the frame and the File menu's for the original.
    let (menu_mode, menu_settings) = (mode.clone(), app.settings.clone());
    let plate_menu = move || {
        let cropping = menu_mode.get() == Mode::Crop;
        let cropped = menu_settings.with(|s| s.crop.is_some());
        vec![
            native::entry("Compare", menu::COMPARE, true).checked(!cropping),
            native::entry("Crop", menu::CROP, true).checked(cropping),
            native::entry("Reset Crop", menu::RESET_CROP, cropped),
            zgui_desktop::AppMenuEntry::Separator,
            native::entry("Reveal Original in Finder", menu::REVEAL_ORIGINAL, true),
            native::entry("Copy Original's Path", menu::COPY_PATH, true),
        ]
    };
    column()
        .w_full()
        .h_full()
        .gap(6.)
        .child(toolbar(app, &info, mode.clone(), aspect, draft))
        .child(native::context_menu(
            plate(
                switch(
                    move || which.get(),
                    move |mode, cx| match mode {
                        Mode::Compare => compare(cx, &frame_app, &frame_review, &frame_info),
                        Mode::Crop => crop_editor(
                            cx,
                            &frame_app,
                            &frame_review,
                            &frame_info,
                            frame_draft.clone(),
                            frame_aspect.clone(),
                        ),
                    },
                )
                .w_full()
                .h_full(),
            ),
            app.window.clone(),
            plate_menu,
        ))
        .child(caption(&info, &caption_mode))
}

/// Under the plate, as a museum captions a work: what it is, and how to
/// handle it.
fn caption(info: &Rc<ClipInfo>, mode: &Signal<Mode>) -> View {
    let mode = mode.clone();
    row()
        .h(22.)
        .shrink_0()
        .px(22.)
        .gap(10.)
        .items_center()
        .child(
            text(format!("FIG. 01 · {}", info.summary()))
                .font_family(DISPLAY)
                .font_weight(weight::LIGHT)
                .text_size(13.)
                .text_color(TEXT_MUTED),
        )
        .child(div().grow())
        .child(
            text_signal(move || match mode.get() {
                Mode::Compare => "MOVE ACROSS THE PLATE TO COMPARE".into(),
                Mode::Crop => "DRAG THE FRAME, OR DRAW ANEW".into(),
            })
            .font_family(MONO)
            .text_size(size::CAPTION)
            .letter_spacing(2.)
            .text_color(TEXT_FAINT),
        )
}

/// The frame mounted as a plate: room around it for printer's marks.
fn plate(inner: View) -> View {
    let mut frame = column().w_full().h_full().relative();
    for mark in crop_marks(ink(0.5)) {
        frame = frame.child(mark);
    }
    column()
        .w_full()
        .grow()
        .min_h(0.)
        .px(22.)
        .py(20.)
        .child(frame.child(inner))
}

const ASPECTS: [(Option<f32>, &str); 5] = [
    (None, "Free"),
    (Some(16. / 9.), "16:9"),
    (Some(9. / 16.), "9:16"),
    (Some(1.), "1:1"),
    (Some(4. / 5.), "4:5"),
];

/// Compare or crop; in crop, the aspect, a reset and done. In compare, what
/// the crop is.
fn toolbar(
    app: &App,
    info: &Rc<ClipInfo>,
    mode: Signal<Mode>,
    aspect: Signal<Option<f32>>,
    draft: Signal<Crop>,
) -> View {
    let modes = {
        let (shown, set, start, settings) = (
            mode.clone(),
            mode.clone(),
            draft.clone(),
            app.settings.clone(),
        );
        segmented(
            "View",
            vec![
                Segment::new(Mode::Compare, "Compare").mark("split", None),
                Segment::new(Mode::Crop, "Crop").mark("crop", None),
            ],
            move || shown.get(),
            |_| true,
            move |m| {
                if m == Mode::Crop {
                    start.set(settings.with_untracked(|s| s.crop).unwrap_or(Crop::FULL));
                }
                set.set(m);
            },
            false,
        )
    };
    let (which, app, info) = (mode.clone(), app.clone(), info.clone());
    row()
        .h(32.)
        .shrink_0()
        .px(22.)
        .gap(18.)
        .items_center()
        .child(modes)
        .child(div().grow())
        .child(switch(
            move || which.get(),
            move |m, _| match m {
                Mode::Compare => crop_summary(&app, &info),
                Mode::Crop => crop_tools(&app, &info, mode.clone(), aspect.clone(), draft.clone()),
            },
        ))
}

fn crop_summary(app: &App, info: &Rc<ClipInfo>) -> View {
    let (settings, info) = (app.settings.clone(), info.clone());
    text_signal(move || {
        settings.with(|s| match s.crop_pixels(&info) {
            Some((w, h, _, _)) => format!("CROPPED TO {w}×{h}"),
            None => "FULL FRAME".into(),
        })
    })
    .font_family(MONO)
    .text_size(size::XS)
    .font_weight(600)
    .letter_spacing(1.2)
    .text_color(TEXT_FAINT)
}

fn crop_tools(
    app: &App,
    info: &Rc<ClipInfo>,
    mode: Signal<Mode>,
    aspect: Signal<Option<f32>>,
    draft: Signal<Crop>,
) -> View {
    let (w, h) = (info.width, info.height);
    let ratios = {
        let (shown, set, crop, settings) = (
            aspect.clone(),
            aspect.clone(),
            draft.clone(),
            app.settings.clone(),
        );
        segmented(
            "Aspect",
            ASPECTS
                .map(|(value, name)| Segment::new(value.map(|r| (r * 1000.).round() as i32), name))
                .into(),
            move || shown.get().map(|r| (r * 1000.).round() as i32),
            |_| true,
            move |r: Option<i32>| {
                let ratio = r.map(|r| r as f32 / 1000.);
                set.set(ratio);
                if let Some(ratio) = ratio {
                    let next = crop.with_untracked(|c| c.with_aspect(ratio, w, h));
                    crop.set(next);
                    settings.update(|s| s.crop = Some(next));
                }
            },
            false,
        )
    };
    let (reset, reset_aspect, reset_settings) =
        (draft.clone(), aspect.clone(), app.settings.clone());
    row()
        .gap(8.)
        .items_center()
        .child(ratios)
        .child(button_with(Tone::Ghost, None, "Reset").on_click(move || {
            reset.set(Crop::FULL);
            reset_aspect.set(None);
            reset_settings.update(|s| s.crop = None);
        }))
        .child(
            button_with(Tone::Secondary, Some("check"), "Done")
                .on_click(move || {
                    mode.set(Mode::Compare);
                })
                .tooltip("Back to the compare (⌘1)"),
        )
}

/// A child filling its parent.
fn fill() -> View {
    row()
        .absolute()
        .left(0.)
        .top(0.)
        .right(0.)
        .bottom(0.)
        .items_center()
        .justify_center()
}

fn frame_image(label: &str, image: std::sync::Arc<zgui::image::ImageData>) -> View {
    zgui::compose::image(label.to_owned(), image)
        .w_full()
        .h_full()
        .object_contain()
}

/// The stage's box: a dark card the frame sits in.
fn stage_box() -> View {
    column()
        .w_full()
        .h_full()
        .relative()
        .overflow_hidden()
        .bg(PAPER_DEEP)
        .border(1.)
        .border_color(ink(0.35))
}

/// A paper label pinned over the plate.
fn chip(lead: Option<View>, label: &str) -> View {
    let mut c = row().h(22.).px(9.).gap(7.).items_center().bg(PAPER);
    if let Some(lead) = lead {
        c = c.child(lead);
    }
    c.child(
        text(label.to_uppercase())
            .font_family(MONO)
            .text_size(size::CAPTION)
            .font_weight(500)
            .letter_spacing(2.)
            .text_color(TEXT),
    )
}

/// A drag under way: what it grabbed, the crop it started from, and where.
type Dragging = Rc<RefCell<Option<(Grab, Crop, (f32, f32))>>>;

/// The node's window bounds, once mounted, for mapping the pointer.
type Bounds = Rc<RefCell<Option<Signal<Rect>>>>;

/// The frame (cropped as it will come out); once measured, the same frame
/// compressed on the right of a split that follows the pointer.
fn compare(cx: &mut Context, app: &App, review: &Review, info: &Rc<ClipInfo>) -> View {
    let split = cx.state(0.5_f32);
    let bounds: Bounds = Rc::default();
    let (mounted, tracked) = (bounds.clone(), bounds);
    let before = {
        let (review, settings, info) = (review.clone(), app.settings.clone(), info.clone());
        move || {
            let key = (review.before.get(), settings.with(|s| s.crop));
            (key, review.clone(), settings.clone(), info.clone())
        }
    };
    let (after, has_after) = (review.after.clone(), review.after.clone());
    let (clip_split, line_split, handle_split) = (split.clone(), split.clone(), split.clone());
    let measuring = review.measuring.clone();
    let anim = app.anim.clone();
    let (shown_line, shown_handle, shown_chip) =
        (has_after.clone(), has_after.clone(), has_after.clone());
    stage_box()
        .on_mount(move |node| {
            *mounted.borrow_mut() = node.observe_bounds();
        })
        .on_event(move |cx| {
            if let InputEvent::PointerMove { x, .. } = cx.event
                && let Some(rect) = tracked.borrow().as_ref().map(Signal::get)
                && rect.width > 0.
            {
                split.set(((x - rect.x) / rect.width).clamp(0., 1.));
            }
        })
        .reactive_style(move || {
            if has_after.with(Option::is_some) {
                Styles::new().cursor(Cursor::ColResize)
            } else {
                Styles::new()
            }
        })
        .child(
            fill().child(
                switch(move || before().0, {
                    let (review, settings, info) =
                        (review.clone(), app.settings.clone(), info.clone());
                    move |_, _| {
                        let s = settings.get();
                        match review.cropped_before(&s, &info) {
                            Some(Pic(image)) => frame_image("Original frame", image),
                            None => div(),
                        }
                    }
                })
                .w_full()
                .h_full(),
            ),
        )
        .child(
            fill()
                .reactive_style(move || {
                    Styles::new().clip_inset_percent(0., 0., 0., clip_split.get() * 100.)
                })
                .child(
                    switch(
                        move || after.get(),
                        move |pic, _| match pic {
                            Some(Pic(image)) => frame_image("Compressed frame", image),
                            None => div(),
                        },
                    )
                    .w_full()
                    .h_full(),
                ),
        )
        // The split: a hard line and a square handle on it.
        .child(
            div()
                .absolute()
                .top(0.)
                .bottom(0.)
                .w(1.)
                .bg(PAPER)
                .reactive_style(move || {
                    let on = shown_line.with(Option::is_some);
                    Styles::new()
                        .left(Length::Percent(line_split.get()))
                        .translate(-0.5, 0.)
                        .opacity(if on { 1. } else { 0. })
                }),
        )
        .child(
            row()
                .absolute()
                .top(Length::Percent(0.5))
                .size(26., 26.)
                .items_center()
                .justify_center()
                .bg(PAPER)
                .border(1.)
                .border_color(INK)
                .text_color(INK)
                .child(glyph("split", 13.))
                .reactive_style(move || {
                    let on = shown_handle.with(Option::is_some);
                    Styles::new()
                        .left(Length::Percent(handle_split.get()))
                        .translate(-13., -13.)
                        .opacity(if on { 1. } else { 0. })
                }),
        )
        .child(
            row()
                .absolute()
                .left(14.)
                .top(14.)
                .child(chip(None, "Avant")),
        )
        .child(
            row()
                .absolute()
                .right(14.)
                .top(14.)
                .child(chip(None, "Après"))
                .reactive_style(move || {
                    if shown_chip.with(Option::is_some) {
                        Styles::new().flex()
                    } else {
                        Styles::new().hidden()
                    }
                }),
        )
        .child(
            row()
                .absolute()
                .left(0.)
                .right(0.)
                .bottom(14.)
                .justify_center()
                .child(switch(
                    move || measuring.get(),
                    move |on, _| {
                        if on {
                            chip(Some(pulse(&anim, ACCENT)), "Sampling the cloth")
                        } else {
                            div().hidden()
                        }
                    },
                )),
        )
}

// Crop ------------------------------------------------------------------------

/// What a press on the crop grabbed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Grab {
    /// Moves the box.
    Inside,
    /// A corner, by which of its sides: (left?, top?).
    Corner(bool, bool),
    /// One side: left, right, top or bottom.
    Left,
    Right,
    Top,
    Bottom,
    /// A new box from the press.
    Draw,
}

impl Grab {
    fn cursor(self) -> Cursor {
        match self {
            Grab::Inside => Cursor::Move,
            Grab::Corner(l, t) if l == t => Cursor::NwseResize,
            Grab::Corner(_, _) => Cursor::NeswResize,
            Grab::Left | Grab::Right => Cursor::EwResize,
            Grab::Top | Grab::Bottom => Cursor::NsResize,
            Grab::Draw => Cursor::Crosshair,
        }
    }
}

/// Where the frame sits in a `bw` × `bh` box, fitted (`object-fit: contain`).
fn placed(bw: f32, bh: f32, aspect: f32) -> Rect {
    if bw / bh > aspect {
        let w = bh * aspect;
        Rect {
            x: (bw - w) / 2.,
            y: 0.,
            width: w,
            height: bh,
        }
    } else {
        let h = bw / aspect;
        Rect {
            x: 0.,
            y: (bh - h) / 2.,
            width: bw,
            height: h,
        }
    }
}

/// Which part of `crop` (in `frame`'s points) is at `(x, y)`.
fn hit(crop: Crop, frame: Rect, x: f32, y: f32) -> Grab {
    let (l, t) = (
        frame.x + crop.x * frame.width,
        frame.y + crop.y * frame.height,
    );
    let (r, b) = (l + crop.w * frame.width, t + crop.h * frame.height);
    let near = |a: f32, b: f32| (a - b).abs() <= GRAB;
    let within_x = x > l - GRAB && x < r + GRAB;
    let within_y = y > t - GRAB && y < b + GRAB;
    match (near(x, l), near(x, r), near(y, t), near(y, b)) {
        (true, _, true, _) => Grab::Corner(true, true),
        (_, true, true, _) => Grab::Corner(false, true),
        (true, _, _, true) => Grab::Corner(true, false),
        (_, true, _, true) => Grab::Corner(false, false),
        (true, _, _, _) if within_y => Grab::Left,
        (_, true, _, _) if within_y => Grab::Right,
        (_, _, true, _) if within_x => Grab::Top,
        (_, _, _, true) if within_x => Grab::Bottom,
        _ if x > l && x < r && y > t && y < b => Grab::Inside,
        _ => Grab::Draw,
    }
}

/// A box spanning `anchor` to `p`, both fractions, at least `MIN_CROP` a
/// side, kept to `ratio` (in fractions: w / h) when given.
fn span(anchor: (f32, f32), p: (f32, f32), ratio: Option<f32>) -> Crop {
    let (mut w, mut h) = ((p.0 - anchor.0).abs(), (p.1 - anchor.1).abs());
    w = w.max(MIN_CROP);
    h = h.max(MIN_CROP);
    if let Some(r) = ratio {
        if w / h > r {
            w = h * r;
        } else {
            h = w / r;
        }
    }
    // Toward the pointer from the anchor, inside the frame.
    let (left, up) = (p.0 < anchor.0, p.1 < anchor.1);
    let room_w = if left { anchor.0 } else { 1. - anchor.0 };
    let room_h = if up { anchor.1 } else { 1. - anchor.1 };
    let shrink = (room_w / w).min(room_h / h).min(1.);
    let (w, h) = (w * shrink, h * shrink);
    Crop {
        x: if left { anchor.0 - w } else { anchor.0 },
        y: if up { anchor.1 - h } else { anchor.1 },
        w,
        h,
    }
}

/// `start` dragged by `grab` to `p` (fractions of the frame).
fn drag(start: Crop, grab: Grab, from: (f32, f32), p: (f32, f32), ratio: Option<f32>) -> Crop {
    let (r, b) = (start.x + start.w, start.y + start.h);
    match grab {
        Grab::Inside => Crop {
            x: (start.x + p.0 - from.0).clamp(0., 1. - start.w),
            y: (start.y + p.1 - from.1).clamp(0., 1. - start.h),
            ..start
        },
        Grab::Draw => span(from, p, ratio),
        Grab::Corner(left, top) => {
            let anchor = (
                if left { r } else { start.x },
                if top { b } else { start.y },
            );
            span(anchor, p, ratio)
        }
        side => {
            // One side moves; with a ratio, the other dimension follows,
            // centred.
            let mut c = start;
            match side {
                Grab::Left => {
                    c.x = p.0.clamp(0., r - MIN_CROP);
                    c.w = r - c.x;
                }
                Grab::Right => c.w = (p.0 - c.x).clamp(MIN_CROP, 1. - c.x),
                Grab::Top => {
                    c.y = p.1.clamp(0., b - MIN_CROP);
                    c.h = b - c.y;
                }
                _ => c.h = (p.1 - c.y).clamp(MIN_CROP, 1. - c.y),
            }
            if let Some(ratio) = ratio {
                let horizontal = matches!(side, Grab::Left | Grab::Right);
                if horizontal {
                    let h = (c.w / ratio).min(1.);
                    c.y = (start.y + start.h / 2. - h / 2.).clamp(0., 1. - h);
                    c.h = h;
                    c.w = c.h * ratio;
                } else {
                    let w = (c.h * ratio).min(1.);
                    c.x = (start.x + start.w / 2. - w / 2.).clamp(0., 1. - w);
                    c.w = w;
                    c.h = c.w / ratio;
                }
            }
            c
        }
    }
}

/// The whole frame, the outside of the crop dimmed, a hard acid box with
/// thirds and square handles, and its size on a sticker.
fn crop_editor(
    cx: &mut Context,
    app: &App,
    review: &Review,
    info: &Rc<ClipInfo>,
    draft: Signal<Crop>,
    aspect: Signal<Option<f32>>,
) -> View {
    let frame_aspect = info.width as f32 / info.height as f32;
    // The ratio in fractions of the frame: pixel ratio over the frame's.
    let fraction_ratio = {
        let aspect = aspect.clone();
        move || aspect.with_untracked(|a| a.map(|r| r / frame_aspect))
    };
    let bounds: Bounds = Rc::default();
    let (mounted, events) = (bounds.clone(), bounds.clone());
    let size = cx.state((0_f32, 0_f32));
    let size_from = size.clone();
    let dragging: Dragging = Rc::default();
    let cursor = cx.state(Cursor::Crosshair);
    let (set_cursor, read_cursor) = (cursor.clone(), cursor);
    let (edit, commit, settings) = (draft.clone(), draft.clone(), app.settings.clone());
    let (w, h) = (info.width, info.height);
    let before = review.before.clone();
    // The frame's rect in the box, and the crop's in points.
    let frame = move || {
        let (bw, bh) = size.get();
        placed(bw.max(1.), bh.max(1.), frame_aspect)
    };
    let crop_rect = {
        let (frame, draft) = (frame.clone(), draft.clone());
        move || {
            let f = frame();
            let c = draft.get();
            Rect {
                x: f.x + c.x * f.width,
                y: f.y + c.y * f.height,
                width: c.w * f.width,
                height: c.h * f.height,
            }
        }
    };
    // The dimmed outside, as four rects around the crop.
    let shade = |side: u8| {
        let (frame, crop) = (frame.clone(), crop_rect.clone());
        div()
            .absolute()
            .bg(alpha(INK, 0.62))
            .reactive_style(move || {
                let (f, c) = (frame(), crop());
                let (x, y, w, h) = match side {
                    0 => (f.x, f.y, f.width, c.y - f.y),
                    1 => (
                        f.x,
                        c.y + c.height,
                        f.width,
                        f.y + f.height - c.y - c.height,
                    ),
                    2 => (f.x, c.y, c.x - f.x, c.height),
                    _ => (c.x + c.width, c.y, f.x + f.width - c.x - c.width, c.height),
                };
                Styles::new().left(x).top(y).w(w.max(0.)).h(h.max(0.))
            })
    };
    let handle = |fx: f32, fy: f32| {
        let crop = crop_rect.clone();
        div()
            .absolute()
            .size(9., 9.)
            .bg(PAPER)
            .border(1.)
            .border_color(INK)
            .reactive_style(move || {
                let c = crop();
                Styles::new()
                    .left(c.x + fx * c.width - 4.5)
                    .top(c.y + fy * c.height - 4.5)
            })
    };
    let thirds = |vertical: bool, at: f32| {
        let crop = crop_rect.clone();
        div()
            .absolute()
            .bg(alpha(PAPER, 0.4))
            .reactive_style(move || {
                let c = crop();
                if vertical {
                    Styles::new()
                        .left(c.x + at * c.width)
                        .top(c.y)
                        .w(1.)
                        .h(c.height)
                } else {
                    Styles::new()
                        .left(c.x)
                        .top(c.y + at * c.height)
                        .w(c.width)
                        .h(1.)
                }
            })
    };
    let (outline, tag_rect, tag_draft) = (crop_rect.clone(), crop_rect.clone(), draft.clone());
    let (press_frame, move_frame) = (frame.clone(), frame.clone());
    let press_ratio = fraction_ratio.clone();
    stage_box()
        .on_mount(move |node| {
            *mounted.borrow_mut() = node.observe_bounds();
            // The box's own size, to place the frame in it.
            if let Some(rect) = mounted.borrow().clone() {
                let size = size_from.clone();
                node.on_layout(move || {
                    let r = rect.get();
                    size.set((r.width, r.height));
                });
            }
        })
        .reactive_style(move || Styles::new().cursor(read_cursor.get()))
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            let Some(rect) = events.borrow().as_ref().map(Signal::get) else {
                return;
            };
            let local = |x: f32, y: f32| (x - rect.x, y - rect.y);
            let fraction = |f: Rect, (x, y): (f32, f32)| {
                (
                    ((x - f.x) / f.width).clamp(0., 1.),
                    ((y - f.y) / f.height).clamp(0., 1.),
                )
            };
            match cx.event {
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Primary,
                } if !cx.pointer_modifiers().control => {
                    let f = press_frame();
                    let (lx, ly) = local(x, y);
                    let start = edit.with_untracked(|c| *c);
                    let grab = hit(start, f, lx, ly);
                    *dragging.borrow_mut() = Some((grab, start, fraction(f, (lx, ly))));
                    cx.capture_pointer();
                }
                InputEvent::PointerMove { x, y } => {
                    let f = move_frame();
                    let (lx, ly) = local(x, y);
                    if let Some((grab, start, from)) = *dragging.borrow() {
                        let next = drag(start, grab, from, fraction(f, (lx, ly)), press_ratio());
                        edit.set(next);
                    } else {
                        let c = edit.with_untracked(|c| *c);
                        set_cursor.set(hit(c, f, lx, ly).cursor());
                    }
                }
                InputEvent::PointerUp { .. } | InputEvent::PointerCancel
                    if dragging.borrow_mut().take().is_some() =>
                {
                    let c = commit.with_untracked(|c| *c);
                    settings.update(|s| s.crop = (!c.is_full()).then_some(c));
                }
                _ => {}
            }
        })
        .child(
            fill().child(
                switch(
                    move || before.get(),
                    move |pic, _| match pic {
                        Some(Pic(image)) => frame_image("Frame to crop", image),
                        None => div(),
                    },
                )
                .w_full()
                .h_full(),
            ),
        )
        .child(shade(0))
        .child(shade(1))
        .child(shade(2))
        .child(shade(3))
        .child(thirds(true, 1. / 3.))
        .child(thirds(true, 2. / 3.))
        .child(thirds(false, 1. / 3.))
        .child(thirds(false, 2. / 3.))
        .child(
            div()
                .absolute()
                .border(1.)
                .border_color(PAPER)
                .reactive_style(move || {
                    let c = outline();
                    Styles::new().left(c.x).top(c.y).w(c.width).h(c.height)
                }),
        )
        .child(handle(0., 0.))
        .child(handle(0.5, 0.))
        .child(handle(1., 0.))
        .child(handle(0., 0.5))
        .child(handle(1., 0.5))
        .child(handle(0., 1.))
        .child(handle(0.5, 1.))
        .child(handle(1., 1.))
        .child(
            row()
                .absolute()
                .child(
                    text_signal(move || {
                        let (cw, ch, _, _) = tag_draft.get().pixels(w, h);
                        format!("{cw}×{ch}")
                    })
                    .font_family(MONO)
                    .text_size(size::CAPTION)
                    .letter_spacing(1.6)
                    .text_color(INK),
                )
                .px(7.)
                .py(3.)
                .bg(PAPER)
                .reactive_style(move || {
                    let c = tag_rect();
                    Styles::new().left(c.x + 10.).top(c.y + 10.)
                }),
        )
}

// Settings --------------------------------------------------------------------

fn panel(cx: &mut Context, app: &App, review: &Review, info: Rc<ClipInfo>) -> View {
    let s = app.settings.clone();
    let encoders = app.tools.with(|t| t.encoders.clone());

    // Each row: what's in effect, what applies to this clip, and a pick.
    let quality = {
        let (shown, set) = (s.clone(), s.clone());
        segmented(
            "Quality",
            Goal::ALL.map(|g| Segment::new(g, g.label())).into(),
            move || shown.with(|s| s.goal),
            |_| true,
            move |g| {
                set.update(|s| *s = s.with_goal(g));
            },
            true,
        )
    };
    let resolution = {
        let (shown, set, clip, can, can_s) =
            (s.clone(), s.clone(), info.clone(), info.clone(), s.clone());
        segmented(
            "Resolution",
            Resolution::ALL.map(|r| Segment::new(r, r.label())).into(),
            move || {
                shown.with(|s| {
                    if settings::resolution_applies(s.resolution, s, &clip) {
                        s.resolution
                    } else {
                        Resolution::Source
                    }
                })
            },
            move |r| can_s.with(|s| settings::resolution_applies(r, s, &can)),
            move |r| {
                set.update(|s| s.resolution = r);
            },
            true,
        )
    };
    let fps = {
        let (shown, set, clip, can) = (s.clone(), s.clone(), info.clone(), info.clone());
        segmented(
            "Frame rate",
            FrameRate::ALL.map(|f| Segment::new(f, f.label())).into(),
            move || {
                let f = shown.with(|s| s.fps);
                if settings::fps_applies(f, &clip) {
                    f
                } else {
                    FrameRate::Source
                }
            },
            move |f| settings::fps_applies(f, &can),
            move |f| {
                set.update(|s| s.fps = f);
            },
            true,
        )
    };
    let audio = {
        let (shown, set, clip, can) = (s.clone(), s.clone(), info.clone(), info.clone());
        segmented(
            "Audio",
            Audio::ALL.map(|a| Segment::new(a, a.label())).into(),
            move || shown.with(|s| s.audio_for(&clip)),
            move |a| settings::audio_applies(a, &can),
            move |a| {
                set.update(|s| s.audio = a);
            },
            true,
        )
    };
    let codec = {
        let (shown, set) = (s.clone(), s.clone());
        segmented(
            "Codec",
            Codec::ALL.map(|c| Segment::new(c, c.label())).into(),
            move || shown.with(|s| s.codec),
            move |c| c.available(&encoders),
            move |c| {
                set.update(|s| s.codec = c);
            },
            true,
        )
    };
    let note = app.clone();
    let start = app.clone();
    // The sheet scrolls in zgui's own scroll view (macOS's scrollbars and
    // rubber-banding) when it's taller than the window; Compress stays put.
    let offset = cx.state(0_f32);
    let sheet = column()
        .w_full()
        .gap(16.)
        .pt(6.)
        .pb(12.)
        .child(
            column()
                .w_full()
                .shrink_0()
                .child(numbered(1, "Specification").pb(10.))
                .child(div().h(1.).bg(INK))
                .child(setting("Quality", quality))
                .child(divider())
                .child(setting("Size", resolution))
                .child(divider())
                .child(setting("Rate", fps))
                .child(divider())
                .child(setting("Sound", audio))
                .child(divider())
                .child(setting("Codec", codec))
                .child(divider())
                .child(
                    text_signal(move || {
                        let s = note.settings.get();
                        format!(
                            "{}: {}; one pass, exactly as specified.",
                            s.codec.label(),
                            s.codec.note()
                        )
                    })
                    .pt(10.)
                    .font_family(DISPLAY)
                    .font_weight(weight::LIGHT)
                    .text_size(14.)
                    .line_height(19.)
                    .text_color(TEXT_MUTED)
                    .text_wrap(true),
                ),
        )
        .child(advanced(cx, app))
        .child(estimate_card(cx, app, review, info));
    column()
        .w(PANEL)
        .h_full()
        .gap(14.)
        .items_stretch()
        .child(
            scroll(offset)
                .w_full()
                .grow()
                .min_h(0.)
                .scrollbar(true)
                .child(sheet),
        )
        .child(
            button_with(Tone::Primary, None, "Compress")
                .w_full()
                .h(46.)
                .letter_spacing(3.)
                .child(text_view("→"))
                // No tooltip: it would wrap the button at its own width.
                // The Clip menu shows ⌘↩.
                .on_click(move || start.compress()),
        )
        .child(copy_prompt(cx, app))
}

/// A quiet line under Compress: the same job, written up as a prompt for
/// an AI with a shell, copied to the clipboard. Nothing runs.
fn copy_prompt(cx: &mut Context, app: &App) -> View {
    let copied = cx.state(false);
    let (mark, word, flag) = (copied.clone(), copied.clone(), copied);
    let (app, tasks) = (app.clone(), cx.tasks());
    row().w_full().shrink_0().justify_end().child(
        button()
            .h(20.)
            .px(0.)
            .gap(6.)
            .rounded(0.)
            .border(0.)
            .bg(ink(0.))
            .items_center()
            .font_family(MONO)
            .text_size(size::CAPTION)
            .letter_spacing(1.8)
            .text_color(TEXT_FAINT)
            .cursor(Cursor::Pointer)
            .focus(|s| s)
            .hover(|s| s.text_color(TEXT))
            .accessible_label("Copy a prompt for an AI")
            .child(
                switch(
                    move || mark.get(),
                    |done, _| {
                        if done {
                            icon("check", 10., ACCENT)
                        } else {
                            glyph("copy", 10.)
                        }
                    },
                )
                .size(10., 10.),
            )
            .child(text_signal(move || {
                if word.get() {
                    "COPIED".into()
                } else {
                    "COPY PROMPT FOR AI".into()
                }
            }))
            .on_click(move || {
                if app.copy_prompt() {
                    flag.set(true);
                    let flag = flag.clone();
                    tasks.spawn(async move {
                        zgui::timer::sleep(std::time::Duration::from_millis(1600)).await;
                        flag.set(false);
                    });
                }
            }),
    )
}

fn text_view(s: &str) -> View {
    text(s.to_owned())
        .font_family(DISPLAY)
        .font_weight(weight::LIGHT)
        .text_size(18.)
}

/// The finer points, behind zgui's disclosure: a slider nudging the goal's
/// quality, and whether to keep the metadata. Both are zgui's own controls,
/// bound both ways to the settings.
fn advanced(cx: &mut Context, app: &App) -> View {
    // COMPRESSY_PREVIEW_ADVANCED=1: open it (screenshots).
    let open = cx.state(std::env::var("COMPRESSY_PREVIEW_ADVANCED").is_ok_and(|v| !v.is_empty()));
    let fine = cx.state(app.settings.with_untracked(|s| f32::from(s.fine_tune)));
    let keep = cx.state(app.settings.with_untracked(|s| s.keep_metadata));
    // The controls write the settings; the settings (a new goal resets the
    // nudge) write the controls back.
    {
        let (settings, from_fine, from_keep) = (app.settings.clone(), fine.clone(), keep.clone());
        let effect = cx.runtime().effect(move || {
            let (f, k) = (from_fine.get().round() as i8, from_keep.get());
            if settings.with_untracked(|s| s.fine_tune != f || s.keep_metadata != k) {
                settings.update(|s| {
                    s.fine_tune = f;
                    s.keep_metadata = k;
                });
            }
        });
        cx.retain(effect);
        let (settings, to_fine, to_keep) = (app.settings.clone(), fine.clone(), keep.clone());
        let effect = cx.runtime().effect(move || {
            let (f, k) = settings.with(|s| (f32::from(s.fine_tune), s.keep_metadata));
            to_fine.set(f);
            to_keep.set(k);
        });
        cx.retain(effect);
    }
    let shown = fine.clone();
    disclosure(open, label("Advanced"), move || {
        let (fine, keep, shown) = (fine.clone(), keep.clone(), shown.clone());
        column()
            .w_full()
            .gap(10.)
            .pt(4.)
            .pb(6.)
            .child(
                row()
                    .gap(12.)
                    .items_center()
                    .child(
                        text("Fine-tune")
                            .w(56.)
                            .shrink_0()
                            .font_family(DISPLAY)
                            .font_weight(weight::LIGHT)
                            .text_size(13.),
                    )
                    .child(
                        slider("Fine-tune quality", fine, -6.0..=6.0)
                            .step(1.)
                            .w(168.)
                            .shrink_0()
                            .tooltip("Left for smaller, right for finer"),
                    )
                    .child(
                        text_signal(move || match shown.get().round() as i8 {
                            0 => "AS SET".into(),
                            n if n > 0 => format!("+{n} FINER"),
                            n => format!("{} SMALLER", -n),
                        })
                        .w(70.)
                        .shrink_0()
                        .font_family(MONO)
                        .text_size(size::CAPTION)
                        .letter_spacing(1.4)
                        .text_color(TEXT_FAINT),
                    ),
            )
            .child(checkbox("Keep the metadata (dates, place, camera)", keep))
    })
}

fn setting(name: &str, control: View) -> View {
    row()
        .h(40.)
        .gap(12.)
        .items_center()
        .child(
            text(name.to_owned())
                .w(62.)
                .shrink_0()
                .font_family(DISPLAY)
                .font_weight(weight::LIGHT)
                .text_size(15.)
                .text_color(TEXT),
        )
        .child(row().grow().min_w(0.).child(control))
}

// Estimate --------------------------------------------------------------------

/// `12.3 MB` as its number and its unit, to set them apart.
fn split_size(bytes: u64) -> (String, String) {
    let shown = human_size(bytes);
    match shown.split_once(' ') {
        Some((n, unit)) => (n.to_owned(), unit.to_owned()),
        None => (shown, String::new()),
    }
}

/// The estimate, set like a price in a catalogue: hairline numerals, the unit
/// small beside them, the saving in oxblood; a hairline measured
/// against the original; the sampled frames' score against the bar; the
/// time it will take.
fn estimate_card(cx: &mut Context, app: &App, review: &Review, info: Rc<ClipInfo>) -> View {
    let original = info.size.max(1);
    // The rule glides to each new ratio.
    let ratio = cx.state(Tween::settled(1.));
    {
        let (estimate, ratio, anim) = (review.estimate.clone(), ratio.clone(), app.anim.clone());
        let effect = cx.runtime().effect(move || {
            if let Est::Ready(e) = estimate.get() {
                let r = (e.size as f32 / original as f32).clamp(0., 1.);
                anim.retarget(&ratio, r, FADE_IN);
            }
        });
        cx.retain(effect);
    }
    let (est_num, est_unit, est_badge, est_time, est_score, est_error) = (
        review.estimate.clone(),
        review.estimate.clone(),
        review.estimate.clone(),
        review.estimate.clone(),
        review.estimate.clone(),
        review.estimate.clone(),
    );
    let (dim, measuring) = (review.measuring.clone(), review.measuring.clone());
    let score_app = app.clone();
    let anim = app.anim.clone();
    let bar = {
        let anim = anim.clone();
        move || anim.tween(&ratio, FADE_IN)
    };
    column()
        .w_full()
        .shrink_0()
        .gap(12.)
        .child(
            row()
                .items_center()
                .gap(8.)
                .child(numbered(2, "Estimation").grow())
                .child(switch(
                    move || measuring.get(),
                    move |on, _| {
                        if on {
                            row()
                                .gap(7.)
                                .items_center()
                                .child(pulse(&anim, ACCENT))
                                .child(label("Sampling"))
                        } else {
                            div()
                        }
                    },
                )),
        )
        .child(div().h(1.).bg(INK))
        .child(
            column()
                .gap(12.)
                .reactive_style(move || Styles::new().opacity(if dim.get() { 0.45 } else { 1. }))
                .child(
                    row()
                        .gap(10.)
                        .items_end()
                        .child(
                            text_signal(move || match est_num.get() {
                                Est::Ready(e) => split_size(e.size).0,
                                _ => "…".into(),
                            })
                            .font_family(DISPLAY)
                            .font_weight(weight::THIN)
                            .text_size(64.)
                            .line_height(66.)
                            .letter_spacing(-1.),
                        )
                        .child(
                            text_signal(move || match est_unit.get() {
                                Est::Ready(e) => split_size(e.size).1,
                                _ => String::new(),
                            })
                            .pb(8.)
                            .font_family(DISPLAY)
                            .font_weight(weight::LIGHT)
                            .text_size(20.)
                            .text_color(TEXT_MUTED),
                        )
                        .child(div().grow())
                        .child(switch(
                            move || match est_badge.get() {
                                Est::Ready(e) => Some(e.size),
                                _ => None,
                            },
                            move |size, _| match size {
                                Some(size) => badge(size, original).pb(10.),
                                None => div(),
                            },
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .h(3.)
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .left(0.)
                                .right(0.)
                                .top(1.)
                                .h(1.)
                                .bg(ink(0.2)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(0.)
                                .top(0.)
                                .h(3.)
                                .bg(ACCENT)
                                .reactive_style(move || Styles::new().w_percent(bar() * 100.)),
                        ),
                )
                .child(
                    row()
                        .child(
                            text(format!("FROM {}", human_size(info.size)))
                                .grow()
                                .font_family(MONO)
                                .text_size(size::CAPTION)
                                .letter_spacing(2.)
                                .text_color(TEXT_FAINT),
                        )
                        .child(
                            text_signal(move || match est_time.get() {
                                Est::Ready(e) => format!("~{} to make", duration(e.encode_seconds))
                                    .to_uppercase(),
                                _ => String::new(),
                            })
                            .font_family(MONO)
                            .text_size(size::CAPTION)
                            .letter_spacing(2.)
                            .text_color(TEXT_FAINT),
                        ),
                )
                .child(switch(
                    move || match est_score.get() {
                        Est::Ready(e) => e.score,
                        _ => None,
                    },
                    move |score, _| match score {
                        Some((metric, value)) => {
                            score_row(metric, value, score_app.settings.with(|s| s.goal))
                        }
                        None => div(),
                    },
                )),
        )
        .child(
            text_signal(move || match est_error.get() {
                Est::Failed(e) => e,
                _ => String::new(),
            })
            .font_family(DISPLAY)
            .font_weight(weight::LIGHT)
            .text_size(14.)
            .line_height(19.)
            .text_color(TEXT_MUTED)
            .text_wrap(true),
        )
}

/// The saving, in oxblood.
fn badge(size: u64, original: u64) -> View {
    let ratio = size as f64 / original as f64;
    let shown = if ratio <= 1. {
        format!("−{:.0}%", (1. - ratio) * 100.)
    } else {
        format!("+{:.0}%", (ratio - 1.) * 100.)
    };
    text(shown)
        .font_family(DISPLAY)
        .font_weight(weight::LIGHT)
        .text_size(30.)
        .letter_spacing(-0.6)
        .text_color(ACCENT)
}

/// The sampled frames' score against the goal's bar.
fn score_row(metric: Metric, value: f64, goal: Goal) -> View {
    let (name, shown, target, passes) = match metric {
        Metric::Vmaf => (
            "VMAF",
            format!("{value:.1}"),
            format!("{}", goal.vmaf()),
            value >= goal.vmaf(),
        ),
        Metric::Ssim => (
            "SSIM",
            format!("{value:.3}"),
            format!("{}", goal.ssim()),
            value >= goal.ssim(),
        ),
    };
    row()
        .gap(10.)
        .items_center()
        .child(label(name))
        .child(
            text(shown)
                .font_family(DISPLAY)
                .font_weight(weight::LIGHT)
                .text_size(20.)
                .text_color(if passes { TEXT } else { WARNING }),
        )
        .child(
            text(if passes {
                format!("clears the {target} bar")
            } else {
                format!("under {target}; raise the quality")
            })
            .font_family(DISPLAY)
            .font_weight(weight::LIGHT)
            .text_size(14.)
            .text_color(TEXT_MUTED)
            .truncate(),
        )
}

/// `12 s`, `3 min`, `1 h 5 min`.
fn duration(seconds: f64) -> String {
    let s = seconds.round().max(1.) as u64;
    if s < 90 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", (s + 30) / 60)
    } else {
        format!("{} h {} min", s / 3600, s / 60 % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Rect {
        Rect {
            x: 0.,
            y: 0.,
            width: 1000.,
            height: 500.,
        }
    }

    #[test]
    fn presses_find_corners_edges_and_inside() {
        let c = Crop {
            x: 0.2,
            y: 0.2,
            w: 0.6,
            h: 0.6,
        };
        assert_eq!(hit(c, frame(), 200., 100.), Grab::Corner(true, true));
        assert_eq!(hit(c, frame(), 800., 400.), Grab::Corner(false, false));
        assert_eq!(hit(c, frame(), 500., 100.), Grab::Top);
        assert_eq!(hit(c, frame(), 800., 250.), Grab::Right);
        assert_eq!(hit(c, frame(), 500., 250.), Grab::Inside);
        assert_eq!(hit(c, frame(), 50., 20.), Grab::Draw);
    }

    #[test]
    fn drags_stay_inside_and_keep_the_ratio() {
        let c = Crop {
            x: 0.2,
            y: 0.2,
            w: 0.4,
            h: 0.4,
        };
        let moved = drag(c, Grab::Inside, (0.3, 0.3), (0.95, 0.95), None);
        assert!((moved.x + moved.w - 1.).abs() < 1e-5 && (moved.y + moved.h - 1.).abs() < 1e-5);
        let square = drag(
            c,
            Grab::Corner(false, false),
            (0.6, 0.6),
            (0.9, 0.7),
            Some(1.),
        );
        assert!((square.w - square.h).abs() < 1e-5 && square.x + square.w <= 1.0001);
        let drawn = drag(c, Grab::Draw, (0.9, 0.9), (0.1, 0.5), Some(2.));
        assert!((drawn.w / drawn.h - 2.).abs() < 1e-4 && drawn.x >= 0. && drawn.y >= 0.);
    }
}

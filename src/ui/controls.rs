//! The pieces every screen shares, so the same thing looks the same
//! everywhere (as zterm's `kit.luau`): buttons by tone, an editorial
//! segmented control, hairline frames with printer's marks, captions and
//! display type, a command to copy, a live pulse.
use super::{
    motion::Anim,
    theme::{
        self, ACCENT, INK, ON_INK, PAPER, TEXT, TEXT_FAINT, TEXT_MUTED, glyph, icon, ink, size,
        weight,
    },
};
use std::{f32::consts::TAU, io::Write, process::Stdio, time::Duration};
use zgui::{
    compose::prelude::*,
    cursor::Cursor,
    input::{EventPhase, FocusGroup, InputEvent},
    scene::Color,
    semantics::Role,
    timer::sleep,
};

/// What a button does: `Primary` an ink block that turns oxblood under the
/// pointer (the one thing to do), `Secondary` a hairline, `Ghost` words.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Primary,
    Secondary,
    Ghost,
}

/// A 32 pt button, square-cornered: an optional icon, then its label in
/// tracked mono capitals.
pub fn button_with(tone: Tone, icon_name: Option<&str>, label: &str) -> View {
    let mut b = button()
        .h(32.)
        .px(if tone == Tone::Ghost { 8. } else { 16. })
        .gap(8.)
        .rounded(0.)
        .items_center()
        .justify_center()
        .shrink_0()
        .font_family(theme::MONO)
        .text_size(10.5)
        .font_weight(500)
        .letter_spacing(1.8)
        .cursor(Cursor::Pointer)
        .focus(|s| s)
        .disabled_style(|s| s.opacity(0.3));
    b = match tone {
        Tone::Primary => b
            .bg(INK)
            .text_color(ON_INK)
            .border(0.)
            .hover(|s| s.bg(ACCENT)),
        Tone::Secondary => b
            .bg(ink(0.))
            .text_color(TEXT)
            .border(1.)
            .border_color(INK)
            .hover(|s| s.bg(ink(0.06))),
        Tone::Ghost => b
            .bg(ink(0.))
            .text_color(TEXT_MUTED)
            .border(0.)
            .hover(|s| s.text_color(TEXT).text_underline(true)),
    };
    if let Some(name) = icon_name {
        b = b.child(glyph(name, 13.));
    }
    if !label.is_empty() {
        b = b.child(text(label.to_uppercase()));
    }
    b
}

/// One option of a [`segmented`] control, with an optional mark (tinted,
/// or `None` to follow the text).
pub struct Segment<T> {
    pub value: T,
    pub label: &'static str,
    pub mark: Option<(&'static str, Option<Color>)>,
}

impl<T> Segment<T> {
    pub fn new(value: T, label: &'static str) -> Self {
        Segment {
            value,
            label,
            mark: None,
        }
    }

    /// A mark before the label: tinted, or `None` to follow the text.
    pub fn mark(mut self, name: &'static str, tint: Option<Color>) -> Self {
        self.mark = Some((name, tint));
        self
    }
}

/// Options set like a catalogue's index: tracked capitals, the chosen one
/// in ink over a rule, the rest faint. A radio group to assistive
/// technology, one Tab stop that the arrows move through (as zgui's own
/// `segmented`). `shown` is the choice in effect (which `available` can make
/// differ from the last pick); `pick` makes one. `fill` spreads the options
/// over the width, so rows of them line up.
pub fn segmented<T: Copy + PartialEq + 'static>(
    label: &str,
    options: Vec<Segment<T>>,
    shown: impl Fn() -> T + Clone + 'static,
    available: impl Fn(T) -> bool + Clone + 'static,
    pick: impl Fn(T) + Clone + 'static,
    fill: bool,
) -> View {
    let mut track = row()
        .role(Role::RadioGroup)
        .accessible_label(label.to_owned())
        .focus_group(FocusGroup::Horizontal)
        .shrink_0()
        .gap(if fill { 4. } else { 18. });
    if fill {
        track = track.w_full();
    }
    for option in options {
        let v = option.value;
        let (checked, styled, ruled, current) =
            (shown.clone(), shown.clone(), shown.clone(), shown.clone());
        let (can_click, can_focus, can_show) =
            (available.clone(), available.clone(), available.clone());
        let (on_click, on_focus) = (pick.clone(), pick.clone());
        let mut face = row().gap(6.).items_center();
        if let Some((name, tint)) = option.mark {
            face = face.child(match tint {
                Some(color) => icon(name, 11., color),
                None => glyph(name, 11.),
            });
        }
        face = face.child(text(option.label.to_uppercase()));
        let mut b = button()
            .role(Role::RadioButton)
            .checked_when(move || checked() == v)
            .disabled_when(move || !can_show(v))
            .disabled_style(|s| s.opacity(0.28))
            .h(28.)
            .px(0.)
            .gap(5.)
            .rounded(0.)
            .border(0.)
            .bg(ink(0.))
            .items_center()
            .justify_center()
            .font_family(theme::MONO)
            .text_size(10.5)
            .font_weight(500)
            .letter_spacing(1.3)
            .cursor(Cursor::Pointer)
            .focus(|s| s)
            .reactive_style(move || {
                Styles::new().text_color(if styled() == v { TEXT } else { TEXT_FAINT })
            })
            .hover(|s| s.text_color(TEXT))
            .on_click(move || {
                if can_click(v) {
                    on_click(v);
                }
            })
            // The arrows move focus along the group; the choice follows.
            .on_event(move |cx| {
                if cx.phase == EventPhase::Target
                    && matches!(cx.event, InputEvent::Focus)
                    && can_focus(v)
                    && current() != v
                {
                    on_focus(v);
                }
            })
            // The label over its rule, stacked in a column of their own
            // (a button lays out in a row).
            .child(column().w_full().gap(5.).items_start().child(face).child(
                div().h(1.5).w_full().reactive_style(move || {
                    Styles::new().bg(if ruled() == v { INK } else { ink(0.) })
                }),
            ));
        if fill {
            b = b.flex_grow(1.).flex_basis(0.).min_w(0.);
        }
        track = track.child(b);
    }
    track
}

/// A sheet: a hairline frame on the paper.
pub fn card() -> View {
    column().border(1.).border_color(ink(0.16))
}

pub fn divider() -> View {
    div().h(1.).shrink_0().bg(ink(0.12))
}

/// A museum label: tiny mono capitals, tracked wide.
pub fn label(s: &str) -> View {
    text(s.to_uppercase())
        .font_family(theme::MONO)
        .text_size(size::CAPTION)
        .font_weight(500)
        .letter_spacing(2.2)
        .text_color(TEXT_FAINT)
}

/// `N°01 — LABEL`: a numbered section, as a catalogue numbers its looks.
pub fn numbered(n: u8, s: &str) -> View {
    row()
        .gap(10.)
        .items_center()
        .child(
            text(format!("N°{n:02}"))
                .font_family(theme::DISPLAY)
                .font_weight(weight::BOLD)
                .text_size(12.)
                .text_color(ACCENT),
        )
        .child(label(s))
}

/// Display type: Helvetica Neue at `weight`, set tight.
pub fn display(s: impl Into<String>, size: f32, weight: u16) -> View {
    text(s.into())
        .font_family(theme::DISPLAY)
        .font_weight(weight)
        .text_size(size)
        .line_height(size * 1.12)
        .letter_spacing(-size * 0.035)
        .text_color(TEXT)
}

/// Light reading type in the display face: asides, notes, captions.
pub fn aside(s: impl Into<String>, size: f32) -> View {
    text(s.into())
        .font_family(theme::DISPLAY)
        .font_weight(weight::LIGHT)
        .text_size(size)
        .line_height(size * 1.35)
        .text_color(TEXT_MUTED)
        .text_wrap(true)
}

/// The house's label, after Margiela's: the numbers 0 to 23 on a white
/// tag, `active` circled. Here it says which step this is.
pub fn label_grid(active: u8) -> View {
    tag(active, 17., size::TAG, 6.)
}

/// The label small enough for the title bar.
pub fn label_grid_small(active: u8) -> View {
    tag(active, 13., 7.5, 3.)
}

fn tag(active: u8, side: f32, text_size: f32, pad: f32) -> View {
    let cell = move |n: u8| {
        let on = n == active;
        row()
            .size(side, side)
            .items_center()
            .justify_center()
            .rounded(side / 2.)
            .border(if on { 1. } else { 0. })
            .border_color(INK)
            .child(
                text(n.to_string())
                    .font_family(theme::DISPLAY)
                    .font_weight(if on { weight::BOLD } else { weight::LIGHT })
                    .text_size(text_size)
                    .text_color(if on { TEXT } else { TEXT_MUTED }),
            )
    };
    let line = move |from: u8| row().gap(2.).children((from..from + 12).map(cell));
    column()
        .shrink_0()
        .px(pad + 2.)
        .py(pad)
        .gap(1.)
        .bg(Color(252, 250, 246, 255))
        .border(1.)
        .border_color(ink(0.14))
        .child(line(0))
        .child(line(12))
}

/// Printer's crop marks outside the corners of the view it's added to (a
/// `relative` one): what frames a plate in a catalogue.
pub fn crop_marks(color: Color) -> Vec<View> {
    const LEN: f32 = 14.;
    const GAP: f32 = 6.;
    let mut marks = Vec::new();
    for (left, top) in [(true, true), (false, true), (true, false), (false, false)] {
        // The horizontal stroke, then the vertical, off each corner.
        let h = div().absolute().w(LEN).h(1.).bg(color);
        let v = div().absolute().w(1.).h(LEN).bg(color);
        let h = if left {
            h.left(-(LEN + GAP))
        } else {
            h.right(-(LEN + GAP))
        };
        let h = if top { h.top(0.) } else { h.bottom(0.) };
        let v = if left { v.left(0.) } else { v.right(0.) };
        let v = if top {
            v.top(-(LEN + GAP))
        } else {
            v.bottom(-(LEN + GAP))
        };
        marks.push(h);
        marks.push(v);
    }
    marks
}

/// A command to run: monospaced, selectable, and a button that copies it.
pub fn command(line: &'static str) -> View {
    component(move |cx| {
        let copied = cx.state(false);
        let tasks = cx.tasks();
        let (show, flag) = (copied.clone(), copied.clone());
        row()
            .h(34.)
            .pl(12.)
            .pr(3.)
            .gap(8.)
            .items_center()
            .bg(PAPER)
            .border(1.)
            .border_color(ink(0.3))
            .child(
                text("$")
                    .font_family(theme::MONO)
                    .text_size(size::SM)
                    .text_color(ACCENT),
            )
            .child(
                row().grow().min_w(0.).selectable(true).child(
                    text(line)
                        .font_family(theme::MONO)
                        .text_size(size::SM)
                        .text_color(TEXT)
                        .truncate(),
                ),
            )
            .child(
                button()
                    .size(28., 28.)
                    .p(0.)
                    .rounded(0.)
                    .border(0.)
                    .items_center()
                    .justify_center()
                    .bg(ink(0.))
                    .text_color(TEXT_MUTED)
                    .cursor(Cursor::Pointer)
                    .focus(|s| s)
                    .hover(|s| s.bg(ink(0.06)).text_color(TEXT))
                    .accessible_label("Copy command")
                    .child(
                        switch(
                            move || show.get(),
                            |done, _| {
                                if done {
                                    icon("check", 13., ACCENT)
                                } else {
                                    glyph("copy", 13.)
                                }
                            },
                        )
                        .size(13., 13.),
                    )
                    .on_click(move || {
                        copy_text(line);
                        flag.set(true);
                        let flag = flag.clone();
                        tasks.spawn(async move {
                            sleep(Duration::from_millis(1400)).await;
                            flag.set(false);
                        });
                    })
                    .tooltip("Copy"),
            )
    })
}

/// To the clipboard, through pbcopy (zgui has no public clipboard API).
pub fn copy_text(text: &str) {
    if let Ok(mut child) = std::process::Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

/// A live dot: a core and a halo that swells and fades, at Zeron's pulse
/// rate, only while it is on screen.
pub fn pulse(anim: &Anim, color: Color) -> View {
    let anim = anim.clone();
    component(move |cx| {
        cx.retain(anim.ambient());
        let clock = anim.ambient_clock.clone();
        let phase = move || (clock.get() / 1.8).fract();
        let halo = phase.clone();
        row()
            .size(10., 10.)
            .shrink_0()
            .relative()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .left(0.)
                    .top(0.)
                    .size(10., 10.)
                    .rounded(5.)
                    .bg(color)
                    .reactive_style(move || {
                        let p = halo();
                        Styles::new().opacity((1. - p) * 0.35).scale(0.4 + 0.8 * p)
                    }),
            )
            .child(
                div()
                    .size(5., 5.)
                    .rounded(2.5)
                    .bg(color)
                    .reactive_style(move || {
                        let p = phase();
                        Styles::new().opacity(0.7 + 0.3 * (TAU * p).cos())
                    }),
            )
    })
}

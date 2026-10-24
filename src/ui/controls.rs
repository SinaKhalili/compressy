//! The pieces every screen shares, so the same thing looks the same
//! everywhere (as zterm's `kit.luau`), in solweb's and smallfile's idiom:
//! the default button with its ring, tabs that invert, numbered section
//! heads over a dithered rule, little OS windows with striped title bars and
//! dithered shadows, a dithered meter, a command to copy, a blinking dot.
use super::{
    dither,
    motion::Anim,
    theme::{
        self, ACCENT, HAIR, INK, ON_INK, PAPER, TEXT, TEXT_FAINT, TEXT_MUTED, glyph, icon, ink,
        size, weight,
    },
};
use std::{io::Write, process::Stdio, time::Duration};
use zgui::{
    compose::prelude::*,
    cursor::Cursor,
    input::{EventPhase, FocusGroup, InputEvent},
    layout::Length,
    scene::{BoxShadow, Color, Transform},
    semantics::Role,
    timer::sleep,
};

/// A hard ring `spread` points out from the box (no blur, no offset).
fn ring(color: Color, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: Transform { x: 0., y: 0. },
        blur_radius: 0.,
        spread,
    }
}

/// What a button does. `Primary` is the default button (a paper gap, then
/// an outer ink ring, as solweb's download and the Mac's ⏎ button), the one
/// thing to do; `Secondary` a 2 pt box; `Ghost` a word that inverts under
/// the pointer, as solweb's links.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Primary,
    Secondary,
    Ghost,
}

/// A square-cornered button: an optional icon, then its label in mono capitals.
pub fn button_with(tone: Tone, icon_name: Option<&str>, label: &str) -> View {
    let mut b = button()
        .h(30.)
        .px(if tone == Tone::Ghost { 6. } else { 14. })
        .gap(8.)
        .rounded(0.)
        .items_center()
        .justify_center()
        .shrink_0()
        .font_family(theme::MONO)
        .text_size(size::XS)
        .font_weight(weight::BOLD)
        .letter_spacing(1.1)
        .cursor(Cursor::Pointer)
        .focus(|s| s.outline(2., ACCENT).outline_offset(3.))
        .disabled_style(|s| s.opacity(0.35));
    b = match tone {
        Tone::Primary => b
            .bg(PAPER)
            .text_color(TEXT)
            .border(2.)
            .border_color(INK)
            .shadows(vec![ring(PAPER, 2.), ring(INK, 3.)])
            .hover(|s| s.bg(INK).text_color(ON_INK)),
        Tone::Secondary => b
            .bg(PAPER)
            .text_color(TEXT)
            .border(2.)
            .border_color(INK)
            .hover(|s| s.bg(INK).text_color(ON_INK)),
        Tone::Ghost => b
            .bg(ink(0.))
            .text_color(TEXT_MUTED)
            .border(0.)
            .hover(|s| s.bg(INK).text_color(ON_INK)),
    };
    if let Some(name) = icon_name {
        b = b.child(glyph(name, 12.));
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

/// Options in a 1 pt box split by rules, as solweb's tabs: the chosen one
/// inverted, the rest dotted under the pointer. A radio group to assistive
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
        .border(1.)
        .border_color(INK);
    if fill {
        track = track.w_full();
    }
    let count = options.len();
    for (i, option) in options.into_iter().enumerate() {
        let v = option.value;
        let (checked, styled, current) = (shown.clone(), shown.clone(), shown.clone());
        let (can_click, can_focus, can_show) =
            (available.clone(), available.clone(), available.clone());
        let (on_click, on_focus) = (pick.clone(), pick.clone());
        let mut b = button()
            .role(Role::RadioButton)
            .checked_when(move || checked() == v)
            .disabled_when(move || !can_show(v))
            .disabled_style(|s| s.opacity(0.3))
            .h(24.)
            .px(if fill { 4. } else { 10. })
            .gap(5.)
            .rounded(0.)
            .border(0.)
            .items_center()
            .justify_center()
            .font_family(theme::MONO)
            .text_size(size::CAPTION)
            .font_weight(weight::BOLD)
            .letter_spacing(0.8)
            .cursor(Cursor::Pointer)
            .focus(|s| s.outline(2., ACCENT).outline_offset(-4.))
            .reactive_style(move || {
                if styled() == v {
                    Styles::new().bg(INK).text_color(ON_INK)
                } else {
                    Styles::new().bg(PAPER).text_color(TEXT)
                }
            })
            .hover(|s| s.style(dither::level(ink(0.4), 0.25, 1.)))
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
            });
        if i + 1 < count {
            b = b
                .border_edges(zgui::scene::Insets {
                    top: 0.,
                    right: 1.,
                    bottom: 0.,
                    left: 0.,
                })
                .border_color(INK);
        }
        if fill {
            b = b.flex_grow(1.).flex_basis(0.).min_w(0.);
        }
        if let Some((name, tint)) = option.mark {
            b = b.child(match tint {
                Some(color) => icon(name, 11., color),
                None => glyph(name, 11.),
            });
        }
        track = track.child(b.child(text(option.label.to_uppercase())));
    }
    track
}

/// A sheet: a 1 pt ink box on the paper.
pub fn card() -> View {
    column().bg(PAPER).border(1.).border_color(INK)
}

/// A hairline between rows (`--hair`).
pub fn divider() -> View {
    div().h(1.).shrink_0().bg(HAIR)
}

/// A label: mono capitals, tracked, in the soft tone.
pub fn label(s: &str) -> View {
    text(s.to_uppercase())
        .font_family(theme::MONO)
        .text_size(size::CAPTION)
        .letter_spacing(1.)
        .text_color(TEXT_MUTED)
}

/// A tag: mono capitals on ink, as solweb's `.tag`.
pub fn tag(s: &str) -> View {
    row().px(5.).py(1.).bg(INK).shrink_0().child(
        text(s.to_uppercase())
            .font_family(theme::MONO)
            .text_size(size::CAPTION)
            .letter_spacing(0.6)
            .text_color(ON_INK),
    )
}

/// A section head, as solweb's `h2`: its number on ink, its name in bold
/// capitals, then a dithered rule to the edge.
pub fn numbered(n: u8, s: &str) -> View {
    row()
        .w_full()
        .gap(10.)
        .items_center()
        .child(
            row().px(5.).py(2.).bg(INK).shrink_0().child(
                text(format!("{n:02}"))
                    .font_family(theme::MONO)
                    .text_size(size::XS)
                    .font_weight(weight::BOLD)
                    .text_color(ON_INK),
            ),
        )
        .child(
            text(s.to_uppercase())
                .font_family(theme::MONO)
                .text_size(size::XS)
                .font_weight(weight::BOLD)
                .letter_spacing(1.1)
                .text_color(TEXT)
                .shrink_0(),
        )
        .child(
            dither::patch(dither::checker(INK))
                .grow()
                .h(6.)
                .opacity(0.55),
        )
}

/// A title: mono bold, set tight (solweb's `.title`).
pub fn display(s: impl Into<String>, size: f32, weight: u16) -> View {
    text(s.into())
        .font_family(theme::MONO)
        .font_weight(weight)
        .text_size(size)
        .line_height(size * 1.15)
        .letter_spacing(-size * 0.03)
        .text_color(TEXT)
}

/// Reading text in Helvetica, the soft tone.
pub fn aside(s: impl Into<String>, size: f32) -> View {
    text(s.into())
        .font_family(theme::DISPLAY)
        .text_size(size)
        .line_height(size * 1.6)
        .text_color(TEXT_MUTED)
        .text_wrap(true)
}

/// A little OS window, as smallfile's and the old Mac's: a 1 pt ink frame,
/// a striped title bar with the title in a paper box, an optional close box,
/// and a dithered shadow 5 pt down and right. `content` fills the rest.
pub fn window(title: impl Into<String>, close: Option<Box<dyn Fn()>>, content: View) -> View {
    let title = title.into();
    let mut bar = row()
        .h(22.)
        .shrink_0()
        .relative()
        .items_center()
        .justify_center()
        .border_edges(zgui::scene::Insets {
            top: 0.,
            right: 0.,
            bottom: 1.,
            left: 0.,
        })
        .border_color(INK)
        .child(
            dither::patch(dither::stripes(INK))
                .absolute()
                .left(3.)
                .right(3.)
                .top(5.)
                .bottom(5.),
        )
        .child(
            row().relative().px(8.).bg(PAPER).child(
                text(title.to_uppercase())
                    .font_family(theme::MONO)
                    .text_size(size::CAPTION)
                    .font_weight(weight::BOLD)
                    .letter_spacing(1.)
                    .text_color(TEXT)
                    .truncate(),
            ),
        );
    if let Some(close) = close {
        bar = bar.child(
            row().absolute().left(9.).top(5.).p(2.).bg(PAPER).child(
                button()
                    .size(11., 11.)
                    .p(0.)
                    .rounded(0.)
                    .bg(PAPER)
                    .border(1.)
                    .border_color(INK)
                    .cursor(Cursor::Pointer)
                    .focus(|s| s.outline(2., ACCENT).outline_offset(2.))
                    .hover(|s| s.bg(INK))
                    .accessible_label("Close")
                    .on_click(close),
            ),
        );
    }
    column()
        .relative()
        .child(
            dither::patch(dither::checker(INK))
                .absolute()
                .left(5.)
                .top(5.)
                .right(-5.)
                .bottom(-5.),
        )
        .child(
            column()
                .relative()
                .w_full()
                .h_full()
                .bg(PAPER)
                .border(1.)
                .border_color(INK)
                .child(bar)
                .child(content.grow().min_h(0.)),
        )
}

/// A meter, as solweb's `.bar`: a 1 pt box, `fraction` of it filled with the
/// checker, its leading edge a solid line.
pub fn meter(fraction: impl Fn() -> f32 + Clone + 'static, height: f32) -> View {
    let (fill, edge) = (fraction.clone(), fraction);
    div()
        .w_full()
        .h(height)
        .relative()
        .border(1.)
        .border_color(INK)
        .child(
            dither::patch(dither::checker(INK))
                .absolute()
                .left(0.)
                .top(0.)
                .bottom(0.)
                .reactive_style(move || Styles::new().w_percent(fill().clamp(0., 1.) * 100.)),
        )
        .child(
            div()
                .absolute()
                .top(0.)
                .bottom(0.)
                .w(1.)
                .bg(INK)
                .reactive_style(move || {
                    let f = edge().clamp(0., 1.);
                    Styles::new()
                        .left(Length::Percent(f))
                        .opacity(if f > 0.001 && f < 0.999 { 1. } else { 0. })
                }),
        )
}

/// Three small squares, the step under way filled: 01 drop, 02 tune,
/// 03 compress.
pub fn steps(active: u8) -> View {
    let mut r = row().gap(4.).items_center().shrink_0();
    for n in 1..=3u8 {
        let mut cell = div().size(7., 7.).border(1.).border_color(INK);
        if n == active {
            cell = cell.bg(INK);
        } else if n < active {
            cell = cell.style(dither::checker(INK));
        }
        r = r.child(cell);
    }
    r.child(
        text(format!("STEP {active} OF 3"))
            .ml(4.)
            .font_family(theme::MONO)
            .text_size(size::CAPTION)
            .letter_spacing(1.)
            .text_color(TEXT_FAINT),
    )
}

/// A command to run: monospaced, selectable, and a button that copies it.
pub fn command(line: &'static str) -> View {
    component(move |cx| {
        let copied = cx.state(false);
        let tasks = cx.tasks();
        let (show, flag) = (copied.clone(), copied.clone());
        row()
            .h(32.)
            .pl(10.)
            .pr(2.)
            .gap(8.)
            .items_center()
            .bg(PAPER)
            .border(1.)
            .border_color(INK)
            .child(
                text("$")
                    .font_family(theme::MONO)
                    .text_size(size::SM)
                    .text_color(TEXT_FAINT),
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
                    .size(26., 26.)
                    .p(0.)
                    .rounded(0.)
                    .border(0.)
                    .items_center()
                    .justify_center()
                    .bg(PAPER)
                    .text_color(TEXT)
                    .cursor(Cursor::Pointer)
                    .focus(|s| s.outline(2., ACCENT).outline_offset(-3.))
                    .hover(|s| s.bg(INK).text_color(ON_INK))
                    .accessible_label("Copy command")
                    .child(
                        switch(
                            move || show.get(),
                            |done, _| {
                                if done {
                                    glyph("check", 12.)
                                } else {
                                    glyph("copy", 12.)
                                }
                            },
                        )
                        .size(12., 12.),
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

/// A live dot, 1-bit: a small square that blinks, on and off, never fading.
/// It runs only while on screen.
pub fn pulse(anim: &Anim, color: Color) -> View {
    let anim = anim.clone();
    component(move |cx| {
        cx.retain(anim.ambient());
        let clock = anim.ambient_clock.clone();
        div()
            .size(6., 6.)
            .shrink_0()
            .bg(color)
            .reactive_style(move || {
                let on = (clock.get() * 1.6).floor() as i64 % 2 == 0;
                Styles::new().opacity(if on { 1. } else { 0.15 })
            })
    })
}

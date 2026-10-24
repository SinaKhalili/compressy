//! Compressy's look: a maison's catalogue, set in sans alone. Bone paper and
//! ink, one oxblood accent; Helvetica Neue at its extremes (hairline
//! UltraLight against Bold) for display, Geist for reading, Geist Mono for
//! the tiny tracked captions. No serifs, no italics; hairlines, sharp
//! corners, no shadows. The structure
//! (tokens, motion, kit) still follows zgui's `zeron` example.
use std::{collections::HashMap, sync::Arc};
use zgui::{compose::prelude::*, scene::Color, svg::SvgData};
use zgui_desktop::FontData;

// Tokens ----------------------------------------------------------------------

/// Bone: the paper everything is printed on.
pub const PAPER: Color = Color(241, 237, 228, 255);
/// A deeper bone, for what sits under the paper (the plate's mat).
pub const PAPER_DEEP: Color = Color(231, 226, 214, 255);
/// Ink.
pub const INK: Color = Color(20, 19, 17, 255);
pub const TEXT: Color = INK;
pub const TEXT_MUTED: Color = Color(20, 19, 17, 160);
pub const TEXT_FAINT: Color = Color(20, 19, 17, 105);
/// Oxblood: the one colour. Selection, progress, the saving.
pub const ACCENT: Color = Color(128, 20, 30, 255);
pub const WARNING: Color = Color(160, 110, 20, 255);
/// The dark of backstage (the terminal's drawer) and text on ink.
pub const BACKSTAGE: Color = Color(16, 15, 14, 255);
pub const ON_INK: Color = PAPER;

/// `color` at `alpha` times its own alpha.
pub fn alpha(color: Color, alpha: f32) -> Color {
    Color(
        color.0,
        color.1,
        color.2,
        (f32::from(color.3) * alpha.clamp(0., 1.)).round() as u8,
    )
}

/// `ink(a)`: ink at `a`, for rules and washes on paper.
pub fn ink(a: f32) -> Color {
    alpha(INK, a)
}

/// sRGB lerp with premultiplied alpha (Zeron's `motion::mix`).
pub fn mix(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    let (fa, ta) = (f32::from(from.3) / 255., f32::from(to.3) / 255.);
    let a = fa + (ta - fa) * t;
    if a <= 0. {
        return Color(0, 0, 0, 0);
    }
    let channel = |f: u8, c: u8| {
        let premultiplied = f32::from(f) * fa + (f32::from(c) * ta - f32::from(f) * fa) * t;
        (premultiplied / a).round().clamp(0., 255.) as u8
    };
    Color(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
        (a * 255.).round() as u8,
    )
}

// Type ------------------------------------------------------------------------

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";
/// The display face: Helvetica Neue, which every Mac carries with its
/// UltraLight and Bold.
pub const DISPLAY: &str = "Helvetica Neue";

/// Display weights: the extremes are the voice.
pub mod weight {
    pub const HAIR: u16 = 100;
    /// Numerals: a hair heavier, so a decimal point reads as one.
    pub const THIN: u16 = 200;
    pub const LIGHT: u16 = 300;
    pub const BOLD: u16 = 700;
}

pub mod size {
    /// Museum-label captions.
    pub const CAPTION: f32 = 9.5;
    /// The house's label: the Margiela grid's numbers.
    pub const TAG: f32 = 9.;
    pub const XS: f32 = 11.;
    pub const SM: f32 = 12.;
    pub const LG: f32 = 13.;
}

/// The title bar strip, under the traffic lights.
pub const TITLEBAR: f32 = 52.;

pub fn fonts() -> Vec<FontData> {
    [
        &include_bytes!("../../assets/fonts/Geist.ttf")[..],
        include_bytes!("../../assets/fonts/Geist-Medium.ttf"),
        include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
        include_bytes!("../../assets/fonts/Geist-Bold.ttf"),
        include_bytes!("../../assets/fonts/GeistMono.ttf"),
        include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
        include_bytes!("../../assets/fonts/GeistMono-SemiBold.ttf"),
    ]
    .into_iter()
    .map(|bytes| FontData::new(Arc::<[u8]>::from(bytes)).expect("bundled font"))
    .collect()
}

// Icons -----------------------------------------------------------------------

const ICONS: &[(&str, &[u8])] = &[
    (
        "arrow-down",
        include_bytes!("../../assets/icons/arrow-down.svg"),
    ),
    (
        "arrow-up-right",
        include_bytes!("../../assets/icons/arrow-up-right.svg"),
    ),
    (
        "arrow-right",
        include_bytes!("../../assets/icons/arrow-right.svg"),
    ),
    ("check", include_bytes!("../../assets/icons/check.svg")),
    ("close", include_bytes!("../../assets/icons/close.svg")),
    ("copy", include_bytes!("../../assets/icons/copy.svg")),
    ("crop", include_bytes!("../../assets/icons/crop.svg")),
    (
        "danger-triangle",
        include_bytes!("../../assets/icons/danger-triangle.svg"),
    ),
    (
        "document",
        include_bytes!("../../assets/icons/document.svg"),
    ),
    (
        "fast",
        include_bytes!("../../assets/icons/fast-tier-bold.svg"),
    ),
    ("folder", include_bytes!("../../assets/icons/folder.svg")),
    ("plus", include_bytes!("../../assets/icons/plus.svg")),
    ("split", include_bytes!("../../assets/icons/split.svg")),
    (
        "terminal",
        include_bytes!("../../assets/icons/terminal.svg"),
    ),
    ("video", include_bytes!("../../assets/icons/video.svg")),
];

thread_local! {
    static ICON_DATA: HashMap<&'static str, SvgData> = ICONS
        .iter()
        .map(|(name, bytes)| (*name, SvgData::new(*bytes).expect("bundled icon")))
        .collect();
}

fn data(name: &str) -> SvgData {
    ICON_DATA.with(|icons| icons.get(name).cloned().expect("icon"))
}

/// A monochrome icon in `color`, `size` square.
pub fn icon(name: &str, size: f32, color: Color) -> View {
    svg(name, Arc::new(data(name).tinted(color))).size(size, size)
}

/// An icon in its view's text colour, so it follows hover and state.
pub fn glyph(name: &str, size: f32) -> View {
    svg(name, Arc::new(data(name).current_color())).size(size, size)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_icon_the_screens_use_is_bundled() {
        for name in [
            "arrow-down",
            "arrow-right",
            "arrow-up-right",
            "check",
            "close",
            "copy",
            "fast",
            "folder",
            "plus",
            "split",
            "terminal",
            "video",
        ] {
            super::data(name);
        }
    }
}

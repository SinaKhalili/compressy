//! Compressy's look: smallfile. A calm 1-bit desk, after solweb.sina.town
//! and smallfile.ca: paper and ink, one orange used only to point (focus,
//! the live dot), mono capitals for labels, Helvetica for reading, and
//! every tone made of dither, never of grey (see `dither`). Little OS
//! windows hold the work. The structure (tokens, motion, kit) still follows
//! zgui's `zeron` example.
use std::{collections::HashMap, sync::Arc};
use zgui::{compose::prelude::*, scene::Color, svg::SvgData};
use zgui_desktop::FontData;

// Tokens ----------------------------------------------------------------------
//
// solweb's: --paper #f3f2ec, --ink #0d0d0c, --soft #5f5e58, --hair #c9c7bd,
// --accent #d9521f.

pub const PAPER: Color = Color(243, 242, 236, 255);
/// The desk under windows: a breath deeper than paper.
pub const PAPER_DEEP: Color = Color(234, 233, 226, 255);
pub const INK: Color = Color(13, 13, 12, 255);
pub const TEXT: Color = INK;
/// Secondary text (`--soft`).
pub const TEXT_MUTED: Color = Color(95, 94, 88, 255);
/// Tertiary text: between soft and hair.
pub const TEXT_FAINT: Color = Color(140, 139, 131, 255);
/// Hairlines between rows (`--hair`).
pub const HAIR: Color = Color(201, 199, 189, 255);
/// Orange: it only points. Focus, the live dot, a warning.
pub const ACCENT: Color = Color(217, 82, 31, 255);
pub const WARNING: Color = ACCENT;
/// The log's dark (solweb's `pre`): ink, with paper on it.
pub const BACKSTAGE: Color = INK;
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

/// `ink(a)`: ink at `a`. Only for washes under the pointer; tones are dither.
pub fn ink(a: f32) -> Color {
    alpha(INK, a)
}

// Type ------------------------------------------------------------------------

/// Geist, for zgui's own controls (checkbox labels, tooltips).
pub const SANS: &str = "Geist";
/// Labels, numbers, titles: mono, as solweb's `ui-monospace`.
pub const MONO: &str = "Geist Mono";
/// Reading text: Helvetica Neue, solweb's body fallback, on every Mac.
pub const DISPLAY: &str = "Helvetica Neue";

pub mod weight {
    pub const BOLD: u16 = 700;
}

pub mod size {
    pub const CAPTION: f32 = 10.;
    pub const XS: f32 = 11.;
    pub const SM: f32 = 12.;
    pub const BODY: f32 = 13.;
    pub const LG: f32 = 14.;
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

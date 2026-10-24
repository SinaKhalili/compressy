//! The menu bar: macOS's own, from zgui's shared menu model. Every command
//! the app has is here, with its shortcut; what can be done right now is
//! enabled and what's chosen is checked, rebuilt as the app changes. The
//! native context menus offer the same actions (zgui dispatches only those
//! the menu model has, enabled).
use crate::prompt::Goal;
use zgui::input::Key;
use zgui_desktop::{AppMenu, AppMenuEntry, MenuAccelerator, StandardItem};

pub const OPEN: &str = "open";
pub const CLOSE: &str = "close";
pub const REVEAL_ORIGINAL: &str = "reveal-original";
pub const COPY_PATH: &str = "copy-path";
pub const COMPRESS: &str = "compress";
pub const COMPARE: &str = "compare";
pub const CROP: &str = "crop";
pub const RESET_CROP: &str = "reset-crop";
pub const OPEN_RESULT: &str = "open-result";
pub const REVEAL_RESULT: &str = "reveal-result";
pub const COPY_PROMPT: &str = "copy-prompt";
pub const COULISSES: &str = "coulisses";

pub fn quality(goal: Goal) -> &'static str {
    match goal {
        Goal::Transparent => "quality-identical",
        Goal::Balanced => "quality-balanced",
        Goal::Tiny => "quality-tiny",
    }
}

pub fn goal(action: &str) -> Option<Goal> {
    Goal::ALL.into_iter().find(|g| quality(*g) == action)
}

/// What the menus show: whether each command can run, and what's chosen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// A clip is open, for review or being compressed.
    pub clip: bool,
    /// The clip under review has been read: it can be compressed, cropped.
    pub reviewing: bool,
    /// Something is being compressed (or was, and is still on screen).
    pub working: bool,
    pub cropping: bool,
    pub cropped: bool,
    /// The result exists.
    pub done: bool,
    pub goal: Option<Goal>,
    pub coulisses: bool,
}

fn primary(c: &str) -> MenuAccelerator {
    MenuAccelerator::primary(Key::Character(c.into()))
}

fn item(label: &str, action: &str, enabled: bool) -> AppMenuEntry {
    AppMenuEntry::item(label, action).enabled(enabled)
}

pub fn model(s: &State) -> Vec<AppMenu> {
    let quality_items = Goal::ALL
        .into_iter()
        .map(|g| item(g.label(), quality(g), !s.working).checked(s.goal == Some(g)));
    vec![
        AppMenu::new(
            "File",
            [
                item("Open…", OPEN, !s.working).accelerator(primary("o")),
                item("Close Clip", CLOSE, s.clip).accelerator(primary(".")),
                AppMenuEntry::Separator,
                item("Reveal Original in Finder", REVEAL_ORIGINAL, s.clip),
                item("Copy Original's Path", COPY_PATH, s.clip),
            ],
        ),
        AppMenu::new(
            "Edit",
            [
                AppMenuEntry::Standard(StandardItem::Undo),
                AppMenuEntry::Standard(StandardItem::Redo),
                AppMenuEntry::Separator,
                AppMenuEntry::Standard(StandardItem::Cut),
                AppMenuEntry::Standard(StandardItem::Copy),
                AppMenuEntry::Standard(StandardItem::Paste),
                AppMenuEntry::Standard(StandardItem::SelectAll),
            ],
        ),
        AppMenu::new(
            "Clip",
            [
                item("Compress", COMPRESS, s.reviewing)
                    .accelerator(MenuAccelerator::primary(Key::Enter)),
                AppMenuEntry::Separator,
                item("Compare", COMPARE, s.reviewing)
                    .checked(s.reviewing && !s.cropping)
                    .accelerator(primary("1")),
                item("Crop", CROP, s.reviewing)
                    .checked(s.reviewing && s.cropping)
                    .accelerator(primary("2")),
                item("Reset Crop", RESET_CROP, s.reviewing && s.cropped),
                AppMenuEntry::Separator,
                item("Copy Prompt for AI", COPY_PROMPT, s.reviewing),
                AppMenuEntry::Separator,
                item("Open Result", OPEN_RESULT, s.done),
                item("Reveal Result in Finder", REVEAL_RESULT, s.done),
            ],
        ),
        AppMenu::new("Quality", quality_items),
        AppMenu::new(
            "View",
            [item("Coulisses", COULISSES, s.working)
                .checked(s.coulisses)
                .accelerator(primary("l"))],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_is_valid_in_every_state() {
        // zgui rejects duplicate actions and shortcuts when menus are set.
        for working in [false, true] {
            let state = State {
                clip: true,
                reviewing: !working,
                working,
                goal: Some(Goal::Balanced),
                ..State::default()
            };
            zgui_desktop::Application::new()
                .menus(model(&state))
                .expect("a valid menu model");
        }
    }

    #[test]
    fn quality_actions_round_trip() {
        for g in Goal::ALL {
            assert_eq!(goal(quality(g)), Some(g));
        }
    }
}

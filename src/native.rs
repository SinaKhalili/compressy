//! What the platform does natively: the menu bar's commands, the open
//! panel, an alert that confirms stopping, context menus (AppKit's own),
//! the window's title. zgui gives each; this routes them to the app.
use crate::{
    App, file_name, job, menu,
    review::{Loaded, Mode},
    screens::work::Progress,
};
use std::cell::RefCell;
use zgui::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, PointerButton},
};
use zgui_desktop::{
    AppMenuEntry, FileDialogOptions, FileFilter, PromptButtons, PromptLevel, PromptOptions,
    PromptResponse, WindowHandle,
};

impl App {
    /// A command from the menu bar or a context menu.
    pub fn menu(&self, action: &str) {
        if let Some(goal) = menu::goal(action) {
            self.settings.update(|s| *s = s.with_goal(goal));
            return;
        }
        match action {
            menu::OPEN => self.choose_file(),
            menu::CLOSE => self.close_clip(),
            menu::REVEAL_ORIGINAL => {
                if let Some(path) = self.original() {
                    reveal(&path);
                }
            }
            menu::COPY_PATH => {
                if let Some(path) = self.original() {
                    crate::ui::controls::copy_text(&path.to_string_lossy());
                }
            }
            menu::COMPRESS => self.compress(),
            menu::COMPARE | menu::CROP => {
                if let Some(review) = self.review.get() {
                    review.mode.set(if action == menu::CROP {
                        Mode::Crop
                    } else {
                        Mode::Compare
                    });
                }
            }
            menu::RESET_CROP => {
                self.settings.update(|s| s.crop = None);
            }
            menu::OPEN_RESULT | menu::REVEAL_RESULT => {
                if let Some(active) = self.active.get() {
                    let output = &active.job.output;
                    if action == menu::OPEN_RESULT {
                        let _ = std::process::Command::new("open").arg(output).spawn();
                    } else {
                        reveal(output);
                    }
                }
            }
            menu::COPY_PROMPT => {
                self.copy_prompt();
            }
            menu::COULISSES => {
                if let Some(active) = self.active.get() {
                    active.log.update(|open| *open = !*open);
                }
            }
            _ => {}
        }
    }

    fn original(&self) -> Option<std::path::PathBuf> {
        self.review
            .with(|r| r.as_ref().map(|r| r.input.clone()))
            .or_else(|| {
                self.active
                    .with(|a| a.as_ref().map(|a| a.job.input.clone()))
            })
    }

    /// The open panel, for a video: what dropping one does, by the menu.
    pub fn choose_file(&self) {
        if self.active.with(Option::is_some) {
            return;
        }
        let (dialogs, app) = (self.dialogs.clone(), self.clone());
        self.tasks.spawn(async move {
            let options = FileDialogOptions::new("Open a video").filter(FileFilter::new(
                "Video",
                job::VIDEO_EXTENSIONS.iter().copied(),
            ));
            if let Ok(Some(path)) = dialogs.open_file(options).await {
                app.open(&path);
            }
        });
    }

    /// Whether an encode is under way (not finished, not stopped).
    pub fn running(&self) -> bool {
        self.active.with(|a| {
            a.as_ref()
                .is_some_and(|a| a.progress.with(|p| p.output.is_none() && p.ended.is_none()))
        })
    }

    /// Leave the clip on screen: under review, straight away; while it's
    /// being compressed, after an alert asks, since the work would be lost.
    pub fn close_clip(&self) {
        if self.review.with(Option::is_some) {
            self.review.set(None);
            return;
        }
        let Some(active) = self.active.get() else {
            return;
        };
        let leave = {
            let back = self.active.clone();
            move || {
                active.term.close();
                back.set(None);
            }
        };
        if !self.running() {
            leave();
            return;
        }
        let dialogs = self.dialogs.clone();
        self.tasks.spawn(async move {
            if stop_alert(&dialogs).await {
                leave();
            }
        });
    }

    /// The window's close button while an encode runs: ask first.
    pub fn close_requested(&self) -> bool {
        if !self.running() {
            return true;
        }
        let (dialogs, window) = (self.dialogs.clone(), self.window.clone());
        self.tasks.spawn(async move {
            if stop_alert(&dialogs).await {
                window.close();
            }
        });
        false
    }

    /// What the menus should show now.
    fn menu_state(&self) -> menu::State {
        let (reviewing, cropping) = self.review.with(|r| match r {
            Some(r) => (
                matches!(r.clip.get(), Loaded::Ready(_)),
                r.mode.get() == Mode::Crop,
            ),
            None => (false, false),
        });
        let (working, done, coulisses) = self.active.with(|a| match a {
            Some(a) => (
                true,
                a.progress.with(|p: &Progress| p.output.is_some()),
                a.log.get(),
            ),
            None => (false, false, false),
        });
        let settings = self.settings.get();
        menu::State {
            clip: reviewing || working || self.review.with(Option::is_some),
            reviewing,
            working,
            cropping,
            cropped: settings.crop.is_some(),
            done,
            goal: Some(settings.goal),
            coulisses,
        }
    }

    /// Keep the menu bar and the window's title in step with the app: the
    /// menus rebuilt only when what they show changes.
    pub fn sync_native(&self, cx: &mut Context) {
        let app = self.clone();
        let shown = RefCell::new((None::<menu::State>, String::new()));
        let effect = cx.runtime().effect(move || {
            let state = app.menu_state();
            let title = match (app.review.get(), app.active.get()) {
                (Some(r), _) => format!("{} — Compressy", file_name(&r.input)),
                (_, Some(a)) => format!("{} — Compressy", file_name(&a.job.input)),
                _ => "Compressy".to_owned(),
            };
            let mut last = shown.borrow_mut();
            if last.0.as_ref() != Some(&state) {
                let _ = app.windows.set_menus(menu::model(&state));
                last.0 = Some(state);
            }
            if last.1 != title {
                app.window.set_title(title.clone());
                last.1 = title;
            }
        });
        cx.retain(effect);
    }
}

/// "Stop compressing?", as an alert; whether the answer was yes.
async fn stop_alert(dialogs: &zgui_desktop::FileDialogs) -> bool {
    let answer = dialogs
        .prompt(
            PromptOptions::new(
                "Stop compressing?",
                "The encode under way stops, and what it has made so far is lost.",
            )
            .buttons(PromptButtons::OkCancel)
            .level(PromptLevel::Warning),
        )
        .await;
    matches!(answer, Ok(PromptResponse::Ok))
}

fn reveal(path: &std::path::Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
}

/// A secondary click (or a control-click) on `view` opens AppKit's own
/// menu of `entries` at the pointer; a choice arrives as a menu action.
pub fn context_menu(
    view: View,
    window: WindowHandle,
    entries: impl Fn() -> Vec<AppMenuEntry> + 'static,
) -> View {
    view.on_event(move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        let at = match cx.event {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Secondary,
            } => Some((x, y)),
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } if cx.pointer_modifiers().control => Some((x, y)),
            _ => None,
        };
        if let Some((x, y)) = at {
            window.show_context_menu(entries(), f64::from(x), f64::from(y));
            cx.stop_propagation();
            cx.prevent_default();
        }
    })
}

/// A context-menu entry for `action`, enabled when the menu bar's is.
pub fn entry(label: &str, action: &str, enabled: bool) -> AppMenuEntry {
    AppMenuEntry::item(label, action).enabled(enabled)
}

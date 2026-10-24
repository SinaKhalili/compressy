//! Compressy: drop a clip in, crop it and tune what to aim for against a
//! measured estimate, then compress it with ffmpeg, its log in a terminal
//! drawer. Or copy a prompt that hands the same job to an AI. Without
//! ffmpeg, it says how to get it.
mod clip;
mod estimate;
mod job;
mod menu;
mod native;
mod preset;
mod prompt;
mod review;
mod screens;
mod settings;
mod tools;
mod ui;

use job::Job;
use review::Review;
use settings::Settings;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
use tools::Tools;
use ui::{
    motion::{Anim, HOVER_FADE, Tween},
    theme::{self, TEXT, TITLEBAR},
};
use zgui::{
    background::BackgroundPool,
    compose::{Tasks, prelude::*},
    input::{EventPhase, InputEvent},
    reactive::{Runtime, Signal},
    terminal::{Terminal, TerminalEvent, TerminalOptions, TerminalSubscription},
};
use zgui_desktop::{
    Application, FileDialogs, Titlebar, WindowFactory, WindowHandle, WindowOptions,
};

/// The clip on screen, and the terminal working on it.
#[derive(Clone)]
pub struct Active {
    pub id: u64,
    pub job: Rc<Job>,
    pub term: Terminal,
    /// The frame from review, cropped as it will come out.
    pub preview: Option<review::Pic>,
    /// How the work is going, read from its files.
    pub progress: Signal<screens::work::Progress>,
    /// Whether the coulisses (the terminal's drawer) are open.
    pub log: Signal<bool>,
    _exit: Rc<TerminalSubscription>,
}

impl PartialEq for Active {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// What every screen reads and writes.
#[derive(Clone)]
pub struct App {
    pub tools: Signal<Tools>,
    /// What to aim for; kept from clip to clip.
    pub settings: Signal<Settings>,
    /// A file over the window: 0 to 1, eased.
    pub drop_hover: Signal<Tween>,
    pub error: Signal<String>,
    /// A dropped clip being looked over, before anything runs.
    pub review: Signal<Option<Review>>,
    pub active: Signal<Option<Active>>,
    pub anim: Anim,
    /// The window's corner radius: insets keep their corners concentric.
    pub corner: Signal<f64>,
    /// Where the traffic lights are, to start the title bar's content after them.
    pub controls: Signal<Option<zgui::scene::Rect>>,
    /// Probes and estimates run here, off the UI thread.
    pub pool: BackgroundPool,
    /// App-lifetime tasks.
    pub tasks: Tasks,
    /// The native open panel and alerts.
    pub dialogs: FileDialogs,
    /// This window: its title, its context menus, closing it.
    pub window: WindowHandle,
    /// The menu bar, shared by the app's windows.
    pub windows: WindowFactory,
    runtime: Runtime,
    next_id: Rc<Cell<u64>>,
}

impl App {
    /// The prompt for an AI, for the clip under review, to the clipboard:
    /// whether there was one to copy.
    pub fn copy_prompt(&self) -> bool {
        let Some(review) = self.review.get() else {
            return false;
        };
        let Some(info) = review.info() else {
            return false;
        };
        let text = prompt::build(
            &review.input,
            &job::free_output_path(&review.input),
            &self.settings.get(),
            &info,
            &self.tools.with(|t| t.encoders.clone()),
            review.estimated_size(),
        );
        ui::controls::copy_text(&text);
        true
    }

    fn next(&self) -> u64 {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }

    /// A dropped file: look it over first.
    pub fn open(&self, path: &std::path::Path) {
        let error = &self.error;
        if !self.tools.with(Tools::has_ffmpeg) {
            error.set("Install ffmpeg first: the bar below says how.".into());
            return;
        }
        if !job::is_video(path) {
            error.set(format!(
                "{} isn't a video Compressy can read.",
                file_name(path)
            ));
            return;
        }
        let tools = self.tools.get();
        match Review::open(
            self.next(),
            path,
            &self.runtime,
            &self.pool,
            &self.tasks,
            &tools,
        ) {
            Ok(review) => {
                error.set(String::new());
                // A crop belongs to its clip.
                self.settings.update(|s| s.crop = None);
                self.review.set(Some(review));
            }
            Err(e) => {
                error.set(format!("Couldn't open {}: {e}", file_name(path)));
            }
        }
    }

    /// The review's clip, with the settings as they stand.
    pub fn compress(&self) {
        let Some(review) = self.review.get() else {
            return;
        };
        let Some(info) = review.info() else {
            return;
        };
        let settings = self.settings.get();
        let tools = self.tools.get();
        let job = match Job::prepare(&review.input, &settings, &info, &tools) {
            Ok(job) => job,
            Err(e) => {
                review
                    .estimate
                    .set(review::Est::Failed(format!("Couldn't start: {e}")));
                return;
            }
        };
        let term = Terminal::new(
            &self.runtime,
            TerminalOptions {
                command: Some(job.command()),
                env: vec![("GHOSTTY_QUIET_LOGIN".into(), "1".into())],
                font_size: Some(12.5),
                ..Default::default()
            },
        );
        let id = self.next();
        let preview = review.cropped_before(&settings, &info);
        // The shell after the work exited: back to the drop zone.
        let gone = self.active.clone();
        let exit = term.on_event(move |event| {
            if matches!(event, TerminalEvent::Exited { .. })
                && gone.with(|a| a.as_ref().is_some_and(|a| a.id == id))
            {
                gone.set(None);
            }
        });
        term.focus();
        self.review.set(None);
        self.active.set(Some(Active {
            id,
            job: Rc::new(job),
            term,
            preview,
            progress: self.runtime.signal(screens::work::Progress::default()),
            log: self.runtime.signal(false),
            _exit: Rc::new(exit),
        }));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut application = Application::new();
    for font in theme::fonts() {
        application = application.font(font);
    }
    let pool = BackgroundPool::new(2, 8)?;
    // Samples a previous run left behind (it was killed before cleaning up).
    review::clear_stale();
    application
        .menus(menu::model(&menu::State::default()))?
        .window(WindowOptions {
            title: "Compressy".into(),
            width: 1080.,
            height: 760.,
            min_size: Some((960., 700.)),
            decorations: cfg!(target_os = "macos"),
            titlebar: Titlebar::Inset,
            // It opens where it was left, as an AppKit document window does.
            frame_autosave: Some("compressy".into()),
            // Paper, not glass: an opaque bone sheet.
            ..Default::default()
        })
        .run(move |cx| {
            // The light theme under it (tooltips, menus print on paper),
            // without built-in hover washes; zgui's own controls (slider,
            // checkbox, progress) take the house's oxblood.
            {
                use zgui::theme::{LIGHT, Theme, ThemeKind, tok};
                cx.ui.theme().register(
                    Theme::new("compressy", ThemeKind::Light)
                        .extending(LIGHT)
                        .with(tok::HOVER, theme::ink(0.))
                        .with(tok::BUTTON_HOVER, theme::ink(0.))
                        .with(tok::MENU_HOVER, theme::ink(0.))
                        .with(tok::ACCENT, theme::ACCENT)
                        // Sliders' and progress bars' tracks: the deeper bone.
                        .with(tok::INPUT_BG, theme::PAPER_DEEP),
                );
                cx.ui.theme().set("compressy").unwrap();
            }
            // Menu commands (and context menus) reach the app once it exists.
            let slot: Rc<RefCell<Option<App>>> = Rc::default();
            {
                let slot = slot.clone();
                cx.on_menu_action(move |action| {
                    let app = slot.borrow().clone();
                    if let Some(app) = app {
                        app.menu(&action.0);
                    }
                });
            }
            {
                let slot = slot.clone();
                cx.on_close_requested(move || {
                    let app = slot.borrow().clone();
                    app.is_none_or(|app| app.close_requested())
                });
            }
            let (dialogs, window, windows) =
                (cx.dialogs.clone(), cx.window.clone(), cx.windows.clone());
            let corner = cx.window_info.corner_radius.clone();
            let controls = cx.window_info.controls.clone();
            let runtime = cx.ui.runtime.clone();
            let pool = pool.clone();
            cx.render(
                component(move |cx| {
                    let tools = Tools::detect();
                    let app = App {
                        settings: cx.state(Settings::new(&tools.encoders)),
                        tools: cx.state(tools),
                        drop_hover: cx.state(Tween::settled(0.)),
                        error: cx.state(String::new()),
                        review: cx.state(None),
                        active: cx.state(None),
                        anim: Anim::new(cx),
                        corner,
                        controls,
                        pool,
                        tasks: cx.tasks(),
                        dialogs,
                        window,
                        windows,
                        runtime,
                        next_id: Rc::new(Cell::new(0)),
                    };
                    *slot.borrow_mut() = Some(app.clone());
                    app.sync_native(cx);
                    // COMPRESSY_PREVIEW_HOVER=1: as if a file were over the window (screenshots).
                    if std::env::var("COMPRESSY_PREVIEW_HOVER").is_ok_and(|v| !v.is_empty()) {
                        app.anim.retarget(&app.drop_hover, 1., HOVER_FADE);
                    }
                    // `compressy clip.mov` (or Open With) opens it for review.
                    if let Some(path) = std::env::args_os().nth(1) {
                        app.open(&PathBuf::from(path));
                    }
                    root(app)
                })
                .w_full()
                .h_full(),
            );
        })
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Review(u64),
    Work(u64),
}

fn screen(app: &App) -> Screen {
    if let Some(id) = app.active.with(|a| a.as_ref().map(|a| a.id)) {
        return Screen::Work(id);
    }
    if let Some(id) = app.review.with(|r| r.as_ref().map(|r| r.id)) {
        return Screen::Review(id);
    }
    Screen::Home
}

fn root(app: App) -> View {
    let (hover, drops, which) = (app.clone(), app.clone(), app.clone());
    column()
        .w_full()
        .h_full()
        .items_stretch()
        .bg(theme::PAPER)
        .font_family(theme::SANS)
        .text_size(14.)
        .text_color(TEXT)
        .child(titlebar(app.clone()))
        .child(
            switch(
                move || screen(&which),
                move |screen, _| match screen {
                    Screen::Work(_) => match app.active.get() {
                        Some(active) => screens::work::view(&app, active),
                        None => div(),
                    },
                    Screen::Review(_) => match app.review.get() {
                        Some(review) => screens::review::view(&app, review),
                        None => div(),
                    },
                    Screen::Home => screens::home::view(&app),
                },
            )
            .grow()
            .min_h(0.),
        )
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            let target = match &cx.event {
                InputEvent::FileHover { .. } => 1.,
                InputEvent::FileHoverCancelled | InputEvent::FileDrop { .. } => 0.,
                _ => return,
            };
            hover.anim.retarget(&hover.drop_hover, target, HOVER_FADE);
        })
        .on_files_drop(move |paths, _| {
            // A drop on the drop zone or over a review opens it; one mid-job
            // is ignored.
            if drops.active.with(|a| a.is_none())
                && let Some(path) = paths.first()
            {
                drops.open(path);
            }
        })
}

/// The strip under the traffic lights: the app's name at rest, the clip's
/// header under review or while worked on. The window moves by it.
fn titlebar(app: App) -> View {
    let (which, inset, resting) = (app.clone(), app.controls.clone(), app.clone());
    row()
        .h(TITLEBAR)
        .shrink_0()
        .relative()
        .items_center()
        .window_drag()
        .reactive_style(move || {
            let after = inset.get().map_or(78., |r| r.x + r.width + 14.);
            Styles::new().pl(after).pr(12.)
        })
        // The name, centred on the window rather than on what's past the lights.
        .child(
            row()
                .absolute()
                .left(0.)
                .top(0.)
                .right(0.)
                .bottom(0.)
                .items_center()
                .justify_center()
                .reactive_style(move || {
                    if matches!(screen(&resting), Screen::Home) {
                        Styles::new().flex()
                    } else {
                        Styles::new().hidden()
                    }
                })
                // A house's logotype: bold capitals, spaced out.
                .child(
                    text("COMPRESSY")
                        .font_family(theme::DISPLAY)
                        .font_weight(theme::weight::BOLD)
                        .text_size(13.)
                        .letter_spacing(7.)
                        .text_color(TEXT),
                ),
        )
        .child(
            switch(
                move || screen(&which),
                move |screen, cx| match screen {
                    Screen::Work(_) => match app.active.get() {
                        Some(active) => screens::work::header(cx, &app, active),
                        None => div(),
                    },
                    Screen::Review(_) => match app.review.get() {
                        Some(review) => screens::review::header(&app, review),
                        None => div(),
                    },
                    _ => div(),
                },
            )
            .grow()
            .min_w(0.),
        )
}

pub fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

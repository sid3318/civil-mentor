//! Civil Mentor: a desktop study companion for UPSC CSE.
//!
//! - [`content`] is the read-only content pack (syllabus, notes, questions, briefs).
//! - [`store`] is the SQLite file holding everything the student does.
//! - `app` is the window shell: title bar, sidebar, search and the active screen.
//! - `notes` is the reader; [`syllabus`] tracks topics and coverage.
//! - `practice` lists the tests, `exam` sits them and `review` shows the
//!   result; [`mentor`] marks them and chooses their questions.

mod app;
pub mod content;
mod exam;
pub mod mentor;
mod notes;
mod practice;
mod review;
mod screens;
mod settings;
pub mod store;
pub mod syllabus;

use std::borrow::Cow;
use std::collections::HashMap;

use chrono::{Local, NaiveDate};
use gpui_kit::assets::Assets;
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry, TitleBar};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AssetSource, Bounds, Entity, Global, Result,
    SharedString, WindowBounds, WindowOptions, px, size,
};

pub use app::{Screen, Shell};
use content::Library;
use store::{Reading, Settings, Store, ThemeChoice};

// Lucide icons the app uses beyond the set bundled with the components.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        AArrowDown,
        AArrowUp,
        Bookmark,
        BookmarkCheck,
        ChartColumn,
        ChevronsLeftRight,
        Circle,
        CircleDot,
        Flag,
        ListTree,
        LogOut,
        Newspaper,
        NotebookText,
        PenLine,
        Repeat,
        SquarePen,
        Strikethrough,
        Timer
    ]
);

/// The component icons plus the extra ones above.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = Assets.list(path)?;
        names.extend(ExtraIcons.list(path)?);
        Ok(names)
    }
}

/// App-wide state. Views read it with [`AppState::get`].
pub struct AppState {
    pub library: Library,
    pub store: Store,
    pub settings: Settings,
    /// What the student has done with each note, by note id. Kept in step
    /// with the store by [`AppState::update_reading`].
    pub reading: HashMap<String, Reading>,
    /// Set when the data file cannot be used. The shell shows it as a banner,
    /// because from then on nothing the student does is being saved.
    pub store_error: Option<String>,
}

impl Global for AppState {}

impl AppState {
    pub fn get(cx: &App) -> &AppState {
        cx.global::<AppState>()
    }

    /// Changes the settings, saves them and redraws every window.
    pub fn update_settings(cx: &mut App, change: impl FnOnce(&mut Settings)) {
        let state = cx.global_mut::<AppState>();
        change(&mut state.settings);
        if let Err(err) = state.store.save_settings(&state.settings) {
            state.store_error = Some(format!("{err:#}"));
        }
        let theme = state.settings.theme;
        apply_theme(theme, cx);
        cx.refresh_windows();
    }

    /// Shows a failed save as the banner: from then on the student's work
    /// may not be kept.
    pub fn report(cx: &mut App, err: anyhow::Error) {
        cx.global_mut::<AppState>().store_error = Some(format!("{err:#}"));
        cx.refresh_windows();
    }

    /// Changes what the student has done with a note, saves it and redraws
    /// every window. A note opened for the first time starts a fresh record.
    pub fn update_reading(cx: &mut App, note: &str, change: impl FnOnce(&mut Reading, NaiveDate)) {
        let today = Local::now().date_naive();
        let state = cx.global_mut::<AppState>();
        let reading = state
            .reading
            .entry(note.to_string())
            .or_insert_with(|| Reading::new(today));
        change(reading, today);
        if let Err(err) = state.store.save_reading(note, reading) {
            state.store_error = Some(format!("{err:#}"));
        }
        cx.refresh_windows();
    }
}

/// Makes the app's warm "Paper" palette the light theme. Dark stays the
/// component library's default.
fn install_paper_theme(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    registry
        .load_themes_from_str(include_str!("../assets/theme.json"))
        .expect("assets/theme.json is a valid theme file");
    let paper = registry
        .themes()
        .get("Paper")
        .cloned()
        .expect("assets/theme.json defines Paper");
    Theme::global_mut(cx).light_theme = paper;
}

fn apply_theme(choice: ThemeChoice, cx: &mut App) {
    match choice {
        ThemeChoice::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ThemeChoice::System => Theme::sync_system_appearance(None, cx),
    }
}

/// Sets up the component library, loads the content and installs the app
/// state, key bindings and menus. Call once, before opening the window.
pub fn init(mut store: Store, mut store_error: Option<String>, cx: &mut App) {
    gpui_kit::init(cx);
    install_paper_theme(cx);

    let library = Library::embedded();
    for problem in &library.problems {
        eprintln!("content: {problem}");
    }
    let settings = store.settings();
    apply_theme(settings.theme, cx);
    // Saving on top of rows that could not be read would overwrite them, so
    // carry on with a throwaway store instead and say so.
    let reading = store.readings().unwrap_or_else(|err| {
        store_error = Some(format!("{err:#}"));
        store = Store::in_memory();
        HashMap::new()
    });
    cx.set_global(AppState {
        library,
        store,
        settings,
        reading,
        store_error,
    });
    app::init(cx);
    exam::init(cx);
}

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1280.), px(820.)),
            cx,
        ))),
        window_min_size: Some(size(px(900.), px(600.))),
        ..TitleBar::window_options()
    }
}

// ponytail: one window. The app-level shortcuts are routed to the window opened
// here; give each window its own routing if a second one is ever needed.
pub fn open_main_window(
    options: WindowOptions,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<Shell>)> {
    let (window, shell) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    })?;
    app::route_actions(window, &shell, cx);
    Ok((window, shell))
}

/// Starts the app and blocks until it quits.
pub fn run() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        // If the data file cannot be opened, run on a throwaway store and say
        // so, rather than refuse to start or touch a file we could not read.
        let opened = store::default_path().and_then(|path| Store::open(&path));
        let (store, store_error) = match opened {
            Ok(store) => (store, None),
            Err(err) => (Store::in_memory(), Some(format!("{err:#}"))),
        };
        init(store, store_error, cx);
        open_main_window(window_options(cx), cx).expect("failed to open the main window");
        cx.activate(true);
    });
}

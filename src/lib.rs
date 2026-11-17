//! Civil Mentor: a desktop study companion for UPSC CSE.
//!
//! - [`content`] is the read-only content pack (syllabus, notes, questions, briefs).
//! - [`store`] is the SQLite file holding everything the student does.
//! - `app` is the window shell: title bar, sidebar, search and the active screen.
//! - `notes` is the reader; [`syllabus`] tracks topics and coverage.
//! - `practice` lists the tests, `exam` sits them and `review` shows the
//!   result; [`mentor`] marks them and chooses their questions.
//! - `current_affairs` is the daily brief, its reader and its quiz.
//! - `today` is the daily plan and `analysis` the charts; [`mentor`] makes
//!   the plan and the insights, and [`demo`] makes up history to show them.
//! - `revision` is the mistake book and flashcards, `notebook` the
//!   student's highlights, clips and own notes, and `mains` answer writing.

mod analysis;
mod app;
pub mod content;
mod current_affairs;
pub mod demo;
mod exam;
mod mains;
mod marks;
pub mod mentor;
mod notebook;
mod notes;
mod practice;
mod review;
mod revision;
mod screens;
mod settings;
pub mod store;
pub mod syllabus;
mod today;

use std::borrow::Cow;
use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate};
use gpui_kit::assets::Assets;
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry, TitleBar};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AssetSource, Bounds, Entity, Global, Result,
    SharedString, WindowBounds, WindowOptions, px, size,
};

pub use app::{Screen, Shell};
use content::Library;
use store::{
    MainsAnswer, Mark, MyNote, NewMark, Reading, Review, Settings, Store, Study, ThemeChoice,
};

// Lucide icons the app uses beyond the set bundled with the components.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        AArrowDown,
        AArrowUp,
        Bookmark,
        BookmarkCheck,
        Calendar,
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
    /// Every mistake and flashcard in the review boxes, by review key. Kept
    /// in step with the store by [`AppState::save_reviews`].
    pub reviews: HashMap<String, Review>,
    /// The student's highlights and clips, by the reading key of the item
    /// they were made in. Kept in step with the store by add_mark.
    pub marks: HashMap<String, Vec<Mark>>,
    /// The notes the student wrote, newest first. Kept in step with the
    /// store by save_my_note.
    pub my_notes: Vec<MyNote>,
    /// Every Mains answer, drafts included, oldest first. Kept in step with
    /// the store by save_mains_answer.
    pub mains: Vec<MainsAnswer>,
    /// Days added to the system date. Always 0 in the app; the UI test
    /// moves it forward to see a review fall due.
    pub days_ahead: i64,
    /// Set when the data file cannot be used. The shell shows it as a banner,
    /// because from then on nothing the student does is being saved.
    pub store_error: Option<String>,
}

impl Global for AppState {}

impl AppState {
    pub fn get(cx: &App) -> &AppState {
        cx.global::<AppState>()
    }

    /// The day the app works to: the local date, moved by `days_ahead`.
    pub fn today(cx: &App) -> NaiveDate {
        Local::now().date_naive() + Duration::days(AppState::get(cx).days_ahead)
    }

    /// Saves where review items stand and redraws every window.
    pub fn save_reviews(cx: &mut App, reviews: Vec<(String, Review)>) {
        let state = cx.global_mut::<AppState>();
        if let Err(err) = state.store.save_reviews(&reviews) {
            state.store_error = Some(format!("{err:#}"));
        }
        state.reviews.extend(reviews);
        cx.refresh_windows();
    }

    /// The marks made in `item`, oldest first.
    pub fn marks_of(&self, item: &str) -> &[Mark] {
        self.marks.get(item).map(Vec::as_slice).unwrap_or_default()
    }

    /// Saves a highlight or clip and redraws every window.
    pub fn add_mark(cx: &mut App, mark: NewMark) {
        let today = AppState::today(cx);
        let state = cx.global_mut::<AppState>();
        let added = state.store.add_mark(mark, today);
        match added {
            Ok(mark) => state.marks.entry(mark.item.clone()).or_default().push(mark),
            Err(err) => state.store_error = Some(format!("{err:#}")),
        }
        cx.refresh_windows();
    }

    /// Deletes one highlight or clip and redraws every window.
    pub fn remove_mark(cx: &mut App, id: i64) {
        let state = cx.global_mut::<AppState>();
        if let Err(err) = state.store.remove_mark(id) {
            state.store_error = Some(format!("{err:#}"));
        }
        for marks in state.marks.values_mut() {
            marks.retain(|mark| mark.id != id);
        }
        cx.refresh_windows();
    }

    /// Saves a note of the student's own, and returns it as it was stored.
    pub fn save_my_note(cx: &mut App, note: MyNote) -> Option<MyNote> {
        let state = cx.global_mut::<AppState>();
        let saved = match state.store.save_my_note(&note) {
            Ok(saved) => saved,
            Err(err) => {
                state.store_error = Some(format!("{err:#}"));
                return None;
            }
        };
        state.my_notes.retain(|note| note.id != saved.id);
        state.my_notes.insert(0, saved.clone());
        cx.refresh_windows();
        Some(saved)
    }

    /// Saves a Mains answer, and returns it as it was stored.
    pub fn save_mains_answer(cx: &mut App, answer: MainsAnswer) -> Option<MainsAnswer> {
        let state = cx.global_mut::<AppState>();
        let saved = match state.store.save_mains_answer(&answer) {
            Ok(saved) => saved,
            Err(err) => {
                state.store_error = Some(format!("{err:#}"));
                cx.refresh_windows();
                return None;
            }
        };
        match state.mains.iter_mut().find(|answer| answer.id == saved.id) {
            Some(kept) => *kept = saved.clone(),
            None => state.mains.push(saved.clone()),
        }
        cx.refresh_windows();
        Some(saved)
    }

    /// Deletes a note of the student's own and redraws every window.
    pub fn remove_my_note(cx: &mut App, id: i64) {
        let state = cx.global_mut::<AppState>();
        if let Err(err) = state.store.remove_my_note(id) {
            state.store_error = Some(format!("{err:#}"));
        }
        state.my_notes.retain(|note| note.id != id);
        cx.refresh_windows();
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
        let today = AppState::today(cx);
        let state = cx.global_mut::<AppState>();
        let reading = state
            .reading
            .entry(note.to_string())
            .or_insert_with(|| Reading::new(today));
        // The student's own use of a note replaces a made-up record.
        if reading.demo {
            *reading = Reading::new(today);
        }
        change(reading, today);
        if let Err(err) = state.store.save_reading(note, reading) {
            state.store_error = Some(format!("{err:#}"));
        }
        cx.refresh_windows();
    }

    /// Adds `seconds` of study to the block in `block`, starting the block
    /// the first time.
    pub fn record_study(
        cx: &mut App,
        kind: Study,
        item: &str,
        block: &mut Option<i64>,
        seconds: u32,
    ) {
        let today = AppState::today(cx);
        let store = &AppState::get(cx).store;
        let id = match *block {
            Some(id) => Ok(id),
            None => store.start_session(today, kind, item),
        };
        let added = id.and_then(|id| {
            *block = Some(id);
            store.add_session_seconds(id, seconds)
        });
        if let Err(err) = added {
            AppState::report(cx, err);
        }
    }

    /// Adds time spent reading a note or story to its record and to the
    /// day's study. Nothing on screen shows it live, so this does not redraw.
    pub fn add_reading_time(
        cx: &mut App,
        kind: Study,
        note: &str,
        block: &mut Option<i64>,
        seconds: u32,
    ) {
        AppState::record_study(cx, kind, note, block, seconds);
        let today = AppState::today(cx);
        let state = cx.global_mut::<AppState>();
        let reading = state
            .reading
            .entry(note.to_string())
            .or_insert_with(|| Reading::new(today));
        if reading.demo {
            *reading = Reading::new(today);
        }
        reading.seconds += seconds;
        if let Err(err) = state.store.save_reading(note, reading) {
            state.store_error = Some(format!("{err:#}"));
        }
    }

    /// Adds made-up history, or removes it, and redraws. Today's plan is
    /// made again to match, unless the student has ticked a task in it.
    pub fn set_demo(cx: &mut App, on: bool) {
        let today = AppState::today(cx);
        let state = cx.global_mut::<AppState>();
        let changed = if on {
            state.store.add_demo(&demo::history(&state.library, today))
        } else {
            state.store.remove_demo()
        };
        let replan = state
            .store
            .plan(today)
            .map(|plan| !plan.unwrap_or_default().iter().any(|task| task.done))
            .and_then(|untouched| match untouched {
                true => state.store.clear_plan(today),
                false => Ok(()),
            });
        let reloaded = state.store.readings().and_then(|reading| {
            state.reading = reading;
            state.reviews = state.store.reviews()?;
            state.marks = by_item(state.store.marks()?);
            Ok(())
        });
        if let Err(err) = changed.and(replan).and(reloaded) {
            state.store_error = Some(format!("{err:#}"));
        }
        cx.refresh_windows();
    }
}

/// The highlights and clips, grouped by the item they were made in.
fn by_item(marks: Vec<Mark>) -> HashMap<String, Vec<Mark>> {
    let mut by_item: HashMap<String, Vec<Mark>> = HashMap::new();
    for mark in marks {
        by_item.entry(mark.item.clone()).or_default().push(mark);
    }
    by_item
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
    let loaded = store.readings().and_then(|reading| {
        Ok((
            reading,
            store.reviews()?,
            store.marks()?,
            store.my_notes()?,
            store.mains_answers()?,
        ))
    });
    let (reading, mut reviews, marks, my_notes, mains) = loaded.unwrap_or_else(|err| {
        store_error = Some(format!("{err:#}"));
        store = Store::in_memory();
        (
            HashMap::new(),
            HashMap::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    });
    // Mistakes from tests sat before the reviews existed join them once,
    // spread over the days ahead.
    if !store.mistakes_backfilled() {
        let today = Local::now().date_naive();
        let backfilled = store.past_mistakes().and_then(|questions| {
            let rows = mentor::backfill(&questions, &reviews, today);
            store.backfill_mistakes(today, &rows)?;
            Ok(rows)
        });
        match backfilled {
            Ok(rows) => reviews.extend(rows),
            Err(err) => store_error = Some(format!("{err:#}")),
        }
    }
    cx.set_global(AppState {
        library,
        store,
        settings,
        reading,
        reviews,
        marks: by_item(marks),
        my_notes,
        mains,
        days_ahead: 0,
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

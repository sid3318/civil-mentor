//! Drives the real shell in an offscreen window: clicks every sidebar item,
//! uses the shortcuts, searches, reads and marks notes, changes settings, and
//! saves a PNG of each state to `target/shots/` so the screens can be looked at.
//!
//! Run with `cargo test --test screens`. It needs the main thread and a GPU
//! renderer (Metal on macOS), so it does not use the default test harness.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use civil_mentor::content::Target;
use civil_mentor::mentor::{self, SUBJECTS};
use civil_mentor::store::{
    Attempt, MainsAnswer, Mark, MarkKind, Reading, Review, Settings, Store, ThemeChoice,
};
use civil_mentor::syllabus::{coverage, statuses};
use civil_mentor::{AppAssets, AppState, Screen, Shell};
use gpui_kit::component::{Theme, WindowExt as _};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, App, Bounds, ElementId, Entity, HeadlessAppContext, MouseButton, Pixels,
    ScrollDelta, SharedString, Window, WindowBounds, WindowOptions, point, px, size,
};

struct Harness {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Entity<Shell>,
    shots: PathBuf,
}

impl Harness {
    fn new(store: Store, store_error: Option<&str>) -> Harness {
        let store_error = store_error.map(str::to_string);
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(AppAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            civil_mentor::init(store, store_error, cx);
            // Animations run on wall time; without this a screenshot catches
            // a dialog halfway through fading in.
            cx.set_reduce_motion(true);
        });
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Default::default(),
                size: size(px(1280.), px(820.)),
            })),
            focus: false,
            show: false,
            ..Default::default()
        };
        let (window, shell) = cx
            .update(|cx| civil_mentor::open_main_window(options, cx))
            .expect("the main window opens");
        let shots = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shots");
        std::fs::create_dir_all(&shots).expect("target/shots can be created");
        Harness {
            cx,
            window,
            shell,
            shots,
        }
    }

    /// Runs `step` inside the window on a fresh frame, then lets deferred work finish.
    fn act<R>(&mut self, step: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        let result = self
            .cx
            .update_window(self.window, |_, window, cx| {
                window.render_frame(cx);
                step(window, cx)
            })
            .expect("the window is open");
        self.cx.run_until_parked();
        result
    }

    fn click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.act(|window, cx| window.click(id, cx));
    }

    /// Scrolls whatever scrolls under `at` down by `by` pixels.
    fn scroll_down(&mut self, at: impl Into<ElementId>, by: f32) {
        let at = at.into();
        let delta = ScrollDelta::Pixels(point(px(0.), px(-by)));
        self.act(|window, cx| window.scroll(at, delta, cx));
    }

    /// Scrolls the page until `id` is on screen, scrolling at whichever of
    /// `anchors` is in view.
    fn reveal(&mut self, id: impl Into<ElementId>, anchors: &[ElementId]) {
        let id = id.into();
        for _ in 0..20 {
            let (target, anchor) = self.act(|window, _| {
                let target = window.try_find(id.clone()).expect("the element exists");
                let anchor = anchors
                    .iter()
                    .find(|a| window.try_find((*a).clone()).is_some_and(|e| e.visible()))
                    .cloned();
                (target, anchor)
            });
            if target.visible() {
                return;
            }
            let bounds = target.bounds();
            let by = f32::from(bounds.origin.y + bounds.size.height / 2.) - 410.;
            self.scroll_down(
                anchor.expect("something on the page is in view"),
                by.clamp(-600., 600.),
            );
        }
        panic!("{id:?} never came into view");
    }

    /// Scrolls until `id` is near the middle of the window. `reveal` stops
    /// as soon as any part of an element shows, so a click at its centre can
    /// land past the edge of the window.
    fn centre(&mut self, id: impl Into<ElementId>, anchors: &[ElementId]) {
        let id = id.into();
        self.reveal(id.clone(), anchors);
        let (bounds, anchor) = self.act(|window, _| {
            let anchor = anchors
                .iter()
                .find(|a| window.try_find((*a).clone()).is_some_and(|e| e.visible()))
                .cloned();
            (window.find(id.clone()).bounds(), anchor)
        });
        let by = f32::from(bounds.origin.y + bounds.size.height / 2.) - 410.;
        if by.abs() > 300. {
            self.scroll_down(anchor.expect("something on the page is in view"), by);
        }
    }

    fn topic_ix(&mut self, id: &str) -> usize {
        self.cx.update(|cx| {
            let topics = &AppState::get(cx).library.topics;
            topics
                .iter()
                .position(|topic| topic.id == id)
                .expect("the topic exists")
        })
    }

    /// Clicks something on a scrolling page, bringing it into view first.
    fn tap(&mut self, id: impl Into<ElementId>, anchors: &[&str]) {
        let id = id.into();
        let visible = self.act(|window, _| {
            window
                .try_find(id.clone())
                .is_some_and(|found| found.visible())
        });
        if !visible {
            let anchors: Vec<ElementId> = anchors
                .iter()
                .map(|id| ElementId::from(SharedString::from(*id)))
                .collect();
            self.reveal(id.clone(), &anchors);
        }
        self.click(id);
    }

    fn shows(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.act(|window, _| window.try_find(id).is_some())
    }

    fn press(&mut self, key: &'static str) {
        self.act(|window, cx| window.press(key, cx));
    }

    fn screen(&mut self) -> Screen {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).screen())
    }

    fn note(&mut self) -> Option<usize> {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).note(cx))
    }

    fn note_ix(&mut self, id: &str) -> usize {
        self.cx.update(|cx| {
            let notes = &AppState::get(cx).library.notes;
            notes
                .iter()
                .position(|note| note.id == id)
                .expect("the note exists")
        })
    }

    /// What the student has done with a note, checked against the data file.
    fn reading(&mut self, id: &str) -> Option<Reading> {
        self.cx.update(|cx| {
            let state = AppState::get(cx);
            let saved = state.store.readings().expect("readings load");
            assert_eq!(
                saved.get(id),
                state.reading.get(id),
                "every change is saved"
            );
            state.reading.get(id).copied()
        })
    }

    fn coverage(&mut self, paper: &str) -> (usize, usize) {
        self.cx.update(|cx| {
            let state = AppState::get(cx);
            coverage(
                &state.library,
                &statuses(&state.library, &state.reading),
                paper,
            )
        })
    }

    fn settings(&mut self) -> Settings {
        self.cx.update(|cx| {
            let state = AppState::get(cx);
            assert_eq!(
                state.store.settings(),
                state.settings,
                "every change is saved"
            );
            state.settings.clone()
        })
    }

    fn in_exam(&mut self) -> bool {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).in_exam())
    }

    /// The sitting in the exam room, checked against the data file.
    fn exam(&mut self) -> Attempt {
        let shell = self.shell.clone();
        self.cx.update(|cx| {
            let attempt = shell.read(cx).exam_attempt(cx).expect("a test is open");
            let saved = AppState::get(cx).store.attempt(attempt.id).unwrap();
            assert_eq!(
                saved.as_ref(),
                Some(&attempt),
                "every change is saved at once"
            );
            attempt
        })
    }

    fn saved(&mut self, id: i64) -> Attempt {
        self.cx
            .update(|cx| AppState::get(cx).store.attempt(id).unwrap())
            .expect("the test is in the data file")
    }

    fn result(&mut self) -> Option<i64> {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).result())
    }

    /// Moves the app's clock on, as if the student sat there for `seconds`.
    fn wait(&mut self, seconds: u64) {
        self.cx.advance_clock(Duration::from_secs(seconds));
        self.cx.run_until_parked();
        self.act(|_, _| {});
    }

    fn open(&mut self, target: Target) {
        let shell = self.shell.clone();
        self.act(|_, cx| shell.update(cx, |shell, cx| shell.open(target, cx)));
    }

    /// Pixels in an element that are clearly coloured, not paper, ink, grey
    /// or a border: what a chart's line, bars or cells draw. An empty chart
    /// box has none.
    fn coloured_pixels(&mut self, id: impl Into<ElementId>) -> usize {
        let id = id.into();
        let bounds = self.act(|window, _| window.find(id).bounds());
        let image = self
            .cx
            .capture_screenshot(self.window)
            .expect("offscreen rendering works");
        let scale = image.width() as f32 / 1280.;
        let to = |value: Pixels| (f32::from(value) * scale) as u32;
        let (left, top) = (to(bounds.origin.x), to(bounds.origin.y));
        let right = to(bounds.origin.x + bounds.size.width).min(image.width());
        let bottom = to(bounds.origin.y + bounds.size.height).min(image.height());
        let mut count = 0;
        for y in top..bottom {
            for x in left..right {
                let [r, g, b, _] = image.get_pixel(x, y).0;
                let (max, min) = (r.max(g).max(b), r.min(g).min(b));
                count += usize::from(max - min > 40);
            }
        }
        count
    }

    fn store<R>(&mut self, read: impl FnOnce(&Store) -> R) -> R {
        self.cx.update(|cx| read(&AppState::get(cx).store))
    }

    /// Where a review item stands, checked against the data file.
    fn review(&mut self, key: &str) -> Option<Review> {
        self.cx.update(|cx| {
            let state = AppState::get(cx);
            let saved = state.store.reviews().expect("reviews load");
            assert_eq!(
                saved.get(key),
                state.reviews.get(key),
                "every answer is saved"
            );
            state.reviews.get(key).copied()
        })
    }

    /// The review key on screen in the review session.
    fn review_item(&mut self) -> Option<String> {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).review_item(cx))
    }

    /// Moves the app's date on by `days` from the real one.
    fn days_ahead(&mut self, days: i64) {
        self.cx
            .update(|cx| cx.global_mut::<AppState>().days_ahead = days);
        self.act(|_, _| {});
    }

    /// The student's highlights and clips, oldest first.
    fn marks(&mut self) -> Vec<Mark> {
        let mut marks: Vec<Mark> = self.cx.update(|cx| {
            AppState::get(cx)
                .marks
                .values()
                .flatten()
                .cloned()
                .collect()
        });
        marks.sort_by_key(|mark| mark.id);
        marks
    }

    /// How many highlights the readers are painting.
    fn painted(&mut self) -> usize {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).painted(cx))
    }

    /// Drags across the text of an element, to select part of it.
    fn select(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        let (start, end) = self.act(|window, _| {
            let bounds = window.find(id).bounds();
            let y = bounds.origin.y + px(8.);
            (
                point(bounds.origin.x + px(4.), y),
                point(bounds.origin.x + bounds.size.width - px(30.), y),
            )
        });
        self.act(|window, cx| {
            window.pointer_down(start, MouseButton::Left, cx);
            window.pointer_move(
                point(start.x + px(20.), start.y),
                Some(MouseButton::Left),
                cx,
            );
            window.pointer_move(end, Some(MouseButton::Left), cx);
            window.pointer_up(end, MouseButton::Left, cx);
        });
    }

    /// The Mains answer being written, checked against the data file.
    fn mains_draft(&mut self) -> MainsAnswer {
        let shell = self.shell.clone();
        self.cx.update(|cx| {
            let draft = shell
                .read(cx)
                .mains_draft(cx)
                .expect("an answer is being written");
            let saved = AppState::get(cx).store.mains_answers().unwrap();
            assert_eq!(
                saved.iter().find(|answer| answer.id == draft.id),
                Some(&draft),
                "every keystroke and second is saved"
            );
            draft
        })
    }

    fn analysis_tab(&mut self, ix: usize) {
        self.act(|window, cx| window.within("analysis-tabs").click(ix, cx));
        let shell = self.shell.clone();
        assert_eq!(self.cx.update(|cx| shell.read(cx).analysis_tab(cx)), ix);
    }

    fn dialog_open(&mut self) -> bool {
        self.act(|window, cx| window.has_active_dialog(cx))
    }

    fn shot(&mut self, name: &str) {
        self.act(|_, _| {});
        let image = self
            .cx
            .capture_screenshot(self.window)
            .expect("offscreen rendering works");
        image
            .save(self.shots.join(format!("{name}.png")))
            .expect("the screenshot is written");
    }
}

fn main() {
    let mut app = Harness::new(Store::in_memory(), None);
    assert_eq!(app.screen(), Screen::Today, "the app opens on Today");
    let theme = app.cx.update(|cx| Theme::global(cx).theme_name().clone());
    assert_eq!(theme, "Paper", "the light theme is the app's own palette");

    // Every sidebar item opens its screen. The ids are group-menu-item.
    let primary = Screen::PRIMARY
        .into_iter()
        .enumerate()
        .map(|(ix, s)| (format!("0-0-{ix}"), s));
    let secondary = Screen::SECONDARY
        .into_iter()
        .enumerate()
        .map(|(ix, s)| (format!("1-0-{ix}"), s));
    let footer = [("sidebar-footer-1".to_string(), Screen::Settings)];
    let items: Vec<(String, Screen)> = primary.chain(secondary).chain(footer).collect();
    assert_eq!(
        items.len(),
        Screen::ALL.len(),
        "every screen is in the sidebar"
    );
    for (id, screen) in items.into_iter().rev() {
        app.click(SharedString::from(id.clone()));
        assert_eq!(app.screen(), screen, "sidebar item {id}");
        app.shot(&screen.title().to_lowercase().replace(' ', "-"));
    }

    // Shortcuts work with nothing focused.
    app.press("secondary-3");
    assert_eq!(app.screen(), Screen::Practice, "Cmd+3");
    app.press("secondary-,");
    assert_eq!(app.screen(), Screen::Settings, "Cmd+,");
    app.press("secondary-1");
    assert_eq!(app.screen(), Screen::Today, "Cmd+1");

    // Search: the shortcut opens the palette, typing finds content, Enter opens it.
    app.press("secondary-k");
    assert!(app.dialog_open(), "Cmd+K opens the palette");
    app.shot("search-empty");
    app.act(|window, cx| window.input("monsoon", cx));
    app.shot("search-monsoon");
    app.press("enter");
    assert!(!app.dialog_open(), "choosing a result closes the palette");
    let monsoon = app.note_ix("gs1-geography-indian-monsoon");
    assert_eq!(app.screen(), Screen::Notes);
    assert_eq!(
        app.note(),
        Some(monsoon),
        "the first hit for `monsoon` opens that note"
    );
    assert!(
        app.reading("gs1-geography-indian-monsoon").is_some(),
        "opening a note is recorded"
    );

    // The title-bar search box opens the same palette; a section name navigates.
    app.click("search-trigger");
    assert!(app.dialog_open(), "the search box opens the palette");
    app.act(|window, cx| window.input("analysis", cx));
    app.press("enter");
    assert_eq!(
        app.screen(),
        Screen::Analysis,
        "typing a section name goes to it"
    );

    // Escape closes the palette without going anywhere.
    app.press("secondary-k");
    app.press("escape");
    assert!(!app.dialog_open(), "Escape closes the palette");
    assert_eq!(app.screen(), Screen::Analysis);

    // Settings: each control changes the setting and saves it.
    app.click("countdown");
    assert_eq!(
        app.screen(),
        Screen::Settings,
        "the countdown opens Settings"
    );
    assert_eq!(app.settings(), Settings::default());
    app.click("more-time");
    app.click("more-time");
    app.click("less-time");
    assert_eq!(
        app.settings().daily_minutes,
        Settings::default().daily_minutes + 15
    );
    app.click("Dark");
    assert_eq!(app.settings().theme, ThemeChoice::Dark);
    assert!(
        app.cx.update(|cx| Theme::global(cx).is_dark()),
        "the dark theme is applied"
    );
    app.shot("settings-dark");
    app.click("0-0-0");
    app.shot("today-dark");
    app.click("theme-toggle");
    assert_eq!(
        app.settings().theme,
        ThemeChoice::Light,
        "the title-bar button toggles the theme"
    );

    // The sidebar collapses to icons and comes back.
    app.click("collapse");
    app.shot("sidebar-collapsed");
    app.press("secondary-\\");
    app.shot("today");

    // Notes: the reader comes back to the note it showed last, and the list
    // opens another one.
    app.click("0-0-1");
    assert_eq!(app.note(), Some(monsoon), "the reader keeps its note");
    let rights_id = "gs2-polity-fundamental-rights";
    let rights = app.note_ix(rights_id);
    app.click(("note", rights));
    assert_eq!(app.note(), Some(rights));
    assert_eq!(app.reading(rights_id).map(|r| r.done), Some(None));
    app.shot("notes-read");

    // The outline scrolls the reader to a section.
    app.click(("outline", 3usize));
    app.shot("notes-outline");

    // Distilled mode, then marking the note read moves syllabus coverage.
    app.click("distilled-mode");
    app.shot("notes-distilled");
    let before = (app.coverage("mains.gs2"), app.coverage("pre.gs"));
    app.click("mark-read");
    assert!(
        app.reading(rights_id).unwrap().done.is_some(),
        "marked read"
    );
    for (paper, before) in [("mains.gs2", before.0), ("pre.gs", before.1)] {
        let after = app.coverage(paper);
        assert!(after.0 > before.0, "{paper} coverage goes up");
        assert_eq!(after.1, before.1, "{paper} keeps its topics");
    }
    app.shot("notes-marked");
    app.click("mark-read");
    assert_eq!(app.reading(rights_id).unwrap().done, None, "and back");
    assert_eq!(app.coverage("mains.gs2"), before.0);
    app.click("mark-read");

    // The questions on the note, with an answer showing.
    let question = app.cx.update(|cx| {
        let library = &AppState::get(cx).library;
        library.questions_for(&library.notes[rights])[0]
    });
    let questions_entry = app.cx.update(|cx| {
        let note = &AppState::get(cx).library.notes[rights];
        civil_mentor::content::sections(&note.distilled)
            .iter()
            .filter(|section| section.heading.is_some())
            .count()
    });
    app.click(("outline", questions_entry));
    app.click(("answer", question));
    app.shot("notes-answer");

    // A bookmark puts the note at the top of the list.
    assert!(!app.shows(("bookmarked", rights)));
    app.click("bookmark");
    assert!(app.reading(rights_id).unwrap().bookmarked);
    assert!(app.shows(("bookmarked", rights)), "listed under Bookmarked");

    // Text size and column width are settings, so they are saved.
    app.click("read-mode");
    app.click("larger-text");
    app.click("larger-text");
    app.click("wide");
    let settings = app.settings();
    assert_eq!(settings.reader_size, Settings::default().reader_size + 2);
    assert!(settings.reader_wide);
    app.shot("notes-larger-wide");
    app.click("smaller-text");
    app.click("smaller-text");
    app.click("wide");
    let settings = app.settings();
    assert_eq!(settings.reader_size, Settings::default().reader_size);
    assert!(!settings.reader_wide);

    app.click("theme-toggle");
    app.shot("notes-dark");
    app.click("theme-toggle");

    // Back and forward walk the notes that were open, by button or shortcut.
    app.press("secondary-[");
    assert_eq!(app.note(), Some(monsoon), "back to the note before");
    app.press("secondary-[");
    assert_eq!(app.screen(), Screen::Today);
    app.click("forward");
    app.click("forward");
    assert_eq!(app.note(), Some(rights));
    app.click("back");
    assert_eq!(app.note(), Some(monsoon));
    app.press("secondary-]");
    assert_eq!(app.note(), Some(rights));

    // Syllabus: papers start folded and open row by row down to the topics.
    app.click("1-0-2");
    assert_eq!(app.screen(), Screen::Syllabus);
    let leaf = app.topic_ix("pre.gs.polity.rights.fundamental-rights.right-to-equality");
    assert!(!app.shows(("topic", leaf)), "folded at first");
    for (step, id) in [
        "pre.gs",
        "pre.gs.polity",
        "pre.gs.polity.rights",
        "pre.gs.polity.rights.fundamental-rights",
    ]
    .into_iter()
    .enumerate()
    {
        if step == 2 {
            // The rows under Polity run past the bottom of the window.
            let polity = app.topic_ix("pre.gs.polity");
            app.scroll_down(("topic", polity), 400.);
        }
        let row = app.topic_ix(id);
        app.click(("topic", row));
    }
    assert!(app.shows(("topic", leaf)), "open down to the topic");
    app.shot("syllabus-read");

    // A topic opens the note that covers it, and back returns to the tree.
    app.click(("topic", leaf));
    assert_eq!(app.note(), Some(rights));
    app.press("secondary-[");
    assert_eq!(app.screen(), Screen::Syllabus, "back to the syllabus");
    assert!(app.shows(("topic", leaf)), "still open");

    // When the data file cannot be opened the app still starts, on a
    // throwaway store, and says so on every screen.
    practice();
    today_and_analysis();
    current_affairs();
    revision();
    highlighting();
    mains();

    let mut unsaved = Harness::new(
        Store::in_memory(),
        Some("could not open civil-mentor.db: disk is read-only"),
    );
    assert!(
        unsaved
            .cx
            .update(|cx| AppState::get(cx).store_error.is_some())
    );
    unsaved.click("0-0-1");
    assert_eq!(unsaved.screen(), Screen::Notes, "the app stays usable");
    unsaved.shot("store-error");

    println!("screens: ok, screenshots in {}", app.shots.display());
}

/// Phase 2: a drill is sat against the clock, left, resumed, survives a
/// crash, is submitted and reviewed; a test submits itself when time runs
/// out; past papers and single questions open.
fn practice() {
    // Things on the Practice hub to scroll it by.
    let hub: Vec<ElementId> = ["start-micro", "start-custom"]
        .into_iter()
        .map(ElementId::from)
        .chain((0..SUBJECTS.len()).flat_map(|ix| {
            [
                ElementId::from(("start-subject", ix)),
                ElementId::from(("custom-subject", ix)),
            ]
        }))
        .collect();

    let dir = std::env::temp_dir().join(format!("civil-mentor-screens-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = dir.join("civil-mentor.db");
    let mut app = Harness::new(Store::open(&db).expect("the data file opens"), None);

    app.click("0-0-2");
    assert_eq!(app.screen(), Screen::Practice);
    app.shot("practice-hub");
    let polity = SUBJECTS.iter().position(|s| s.id == "polity").unwrap();
    app.click(("start-subject", polity));
    assert!(app.in_exam(), "the drill opens in the exam room");
    assert!(
        !app.shows("search-trigger"),
        "no title bar in the exam room"
    );
    assert!(!app.shows("0-0-0"), "no sidebar in the exam room");
    let test = app.exam();
    assert_eq!(test.answers.len(), 20);
    assert_eq!(test.seconds_allowed, 20 * 72, "72 seconds a question");
    assert!(test.answers.iter().all(|a| a.subject == "polity"));
    let id = test.id;
    app.shot("exam-room");

    // The shortcuts that leave a screen do nothing in the exam room.
    app.press("secondary-1");
    app.press("secondary-k");
    assert!(
        app.in_exam() && !app.dialog_open(),
        "nothing leads out but Leave or Submit"
    );

    // Keys answer and move; the panel tags confidence; options strike out.
    app.press("b");
    assert_eq!(app.exam().answers[0].chosen, Some(1), "B answers");
    app.click("fifty-fifty");
    assert_eq!(
        app.exam().answers[0].confidence,
        Some(civil_mentor::store::Confidence::FiftyFifty)
    );
    app.press("right");
    assert_eq!(app.exam().current, 1, "the right arrow moves on");
    app.click(("strike", 0usize));
    app.click(("option", 2usize));
    app.press("m");
    let second = &app.exam().answers[1];
    assert!(second.is_struck(0) && second.marked && second.chosen == Some(2));
    app.wait(90);
    let test = app.exam();
    assert_eq!(test.seconds_used, 90, "the clock runs");
    assert_eq!(
        test.answers[1].seconds, 90,
        "time counts against the question on screen"
    );
    app.click(("cell", 4usize));
    assert_eq!(app.exam().current, 4, "the grid jumps to a question");
    app.shot("exam-answered");

    // Leaving keeps the test open; Practice offers to resume it.
    app.click("leave");
    assert!(!app.in_exam());
    assert_eq!(app.screen(), Screen::Practice);
    assert!(app.shows(("resume", id as u64)), "listed as unfinished");
    app.shot("practice-resume");
    app.click(("resume", id as u64));
    let test = app.exam();
    assert_eq!(
        (test.current, test.seconds_used, test.answered()),
        (4, 90, 2)
    );
    app.press("a");
    app.wait(30);

    // The app dies mid-test. Reopened on the same file, the test is waiting.
    drop(app);
    let mut app = Harness::new(Store::open(&db).expect("the data file reopens"), None);
    app.click("0-0-2");
    assert!(
        app.shows(("resume", id as u64)),
        "still unfinished after a crash"
    );
    let saved = app.saved(id);
    assert_eq!((saved.seconds_used, saved.answered()), (120, 3));
    app.click(("resume", id as u64));
    assert_eq!(
        app.exam().seconds_left(),
        20 * 72 - 120,
        "with its time left"
    );

    // Submitting with blanks asks first.
    app.click("submit");
    assert!(
        app.shows("submit-anyway"),
        "asks before submitting with blanks"
    );
    app.shot("exam-confirm");
    app.click("submit-anyway");
    assert!(!app.in_exam());
    assert_eq!(app.result(), Some(id), "the result follows");
    let done = app.saved(id);
    assert!(done.finished.is_some());
    // The score is +2 for each right answer and -2/3 for each wrong one.
    let right = done
        .answers
        .iter()
        .filter(|a| a.correct == Some(true))
        .count();
    let wrong = done
        .answers
        .iter()
        .filter(|a| a.correct == Some(false))
        .count();
    assert_eq!(right + wrong, 3);
    let score = mentor::score(&done);
    assert!((score.marks - (2.0 * right as f64 - 2.0 / 3.0 * wrong as f64)).abs() < 1e-9);
    assert_eq!(score.blank, 17);
    app.shot("result");
    app.scroll_down("score", 900.);
    app.shot("review");
    app.press("secondary-[");
    assert_eq!(app.screen(), Screen::Practice, "back skips the exam room");
    assert!(!app.in_exam());

    // A test submits itself when its time runs out.
    app.press("secondary-3");
    app.click("start-micro");
    let micro = app.exam().id;
    app.press("c");
    app.wait(10 * 72);
    assert!(!app.in_exam(), "submitted when time ran out");
    assert_eq!(app.result(), Some(micro));
    let micro = app.saved(micro);
    assert!(micro.finished.is_some());
    assert_eq!(micro.seconds_used, 720);
    assert_eq!(micro.answered(), 1);

    // Recent results reopen a result.
    app.press("secondary-3");
    app.reveal(("result", id as u64), &hub);
    app.shot("practice-results");
    assert!(app.shows(("result", id as u64)));
    app.click(("result", id as u64));
    assert_eq!(app.result(), Some(id));

    // Past papers: read one with its answers, or sit it as a mock.
    let papers = app.cx.update(|cx| AppState::get(cx).library.papers());
    if let Some((year, questions)) = papers.first().cloned() {
        app.press("secondary-3");
        app.reveal(("read-paper", year as u64), &hub);
        app.click(("read-paper", year as u64));
        app.click(("answer", questions[0]));
        app.shot("paper");
        app.click("sit-this-paper");
        assert!(app.in_exam());
        let mock = app.exam();
        assert_eq!(mock.answers.len(), questions.len(), "the whole paper");
        assert_eq!(mock.seconds_allowed, questions.len() as u32 * 72);
        app.shot("exam-paper");
        app.click("leave");
    }

    // A search result for a question lands on the question itself.
    let (word, question) = app.cx.update(|cx| {
        let library = &AppState::get(cx).library;
        library
            .questions
            .iter()
            .enumerate()
            .filter(|(_, q)| q.note.is_some())
            .flat_map(|(ix, q)| {
                q.stem
                    .split_whitespace()
                    .map(move |w| (w.to_lowercase(), ix))
            })
            .find(|(word, ix)| {
                let hits = library.search(word, 30);
                word.len() > 7
                    && word.chars().all(char::is_alphabetic)
                    && hits.first().map(|hit| hit.target) == Some(Target::Question(*ix))
                    && hits
                        .iter()
                        .all(|hit| matches!(hit.target, Target::Question(_)))
            })
            .expect("a word only one question uses")
    });
    app.press("secondary-k");
    app.act(|window, cx| window.input(&word, cx));
    app.press("enter");
    assert_eq!(app.screen(), Screen::Practice);
    assert!(
        app.shows(("answer", question)),
        "the question itself, not the hub"
    );
    app.click(("answer", question));
    app.shot("question");
    app.click(("open-note", question));
    assert!(app.note().is_some(), "the question links to its note");
    app.press("secondary-[");
    assert!(app.shows(("answer", question)), "and back");
    app.open(Target::Question(question));

    drop(app);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Phase 3: the diagnostic is offered once and can be declined; reading and
/// tests count as study time; demo history fills Today and Analysis, whose
/// charts draw; and removing it leaves the student's own data alone.
fn today_and_analysis() {
    let today = chrono::Local::now().date_naive();
    let dir = std::env::temp_dir().join(format!("civil-mentor-today-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // A first launch: Today offers the diagnostic, which can be taken.
    let taken = dir.join("taken.db");
    let mut app = Harness::new(Store::open(&taken).expect("the data file opens"), None);
    assert!(app.shows("diagnostic-offer"), "offered on first launch");
    app.shot("today-diagnostic");
    app.click("take-diagnostic");
    assert!(app.in_exam(), "the diagnostic opens in the exam room");
    let diagnostic = app.exam();
    assert_eq!(diagnostic.kind, "diagnostic");
    assert_eq!(diagnostic.answers.len(), 20);
    let mut subjects: Vec<&str> = diagnostic
        .answers
        .iter()
        .map(|a| a.subject.as_str())
        .collect();
    subjects.sort();
    subjects.dedup();
    assert!(
        subjects.len() >= 6,
        "spread across the subjects: {subjects:?}"
    );
    app.shot("diagnostic-exam");
    app.click("leave");
    app.press("secondary-1");
    assert!(
        !app.shows("diagnostic-offer"),
        "not offered again once taken"
    );

    // Or declined, and then never asked again, even after a restart.
    drop(app);
    let db = dir.join("civil-mentor.db");
    let mut app = Harness::new(Store::open(&db).expect("the data file opens"), None);
    assert!(app.shows("diagnostic-offer"));
    app.click("decline-diagnostic");
    assert!(!app.shows("diagnostic-offer"), "declined");
    assert!(app.store(|store| store.diagnostic_settled()));
    drop(app);
    let mut app = Harness::new(Store::open(&db).expect("the data file reopens"), None);
    assert!(
        !app.shows("diagnostic-offer"),
        "not asked again after a restart"
    );
    assert!(app.shows("plan") && app.shows("mentor") && app.shows("this-week"));

    // With no record yet, Analysis says so instead of drawing empty charts.
    app.click("0-0-4");
    assert_eq!(app.screen(), Screen::Analysis);
    assert!(!app.shows("score-chart"));
    app.shot("analysis-empty");

    // Reading counts as study time while the student is active.
    let note_id = "gs3-economy-inflation";
    let note = app.note_ix(note_id);
    app.open(Target::Note(note));
    app.wait(60);
    assert_eq!(app.reading(note_id).unwrap().seconds, 60);
    assert_eq!(app.store(|s| s.study_days().unwrap())[&today], 60);
    // Left open with no scrolling or mouse movement, it stops after five minutes.
    app.wait(20 * 60);
    assert_eq!(app.reading(note_id).unwrap().seconds, 5 * 60);
    // Leaving the note stops the clock.
    app.press("secondary-1");
    app.wait(60);
    assert_eq!(app.reading(note_id).unwrap().seconds, 5 * 60);

    // Time in the exam room counts too.
    app.press("secondary-3");
    app.click("start-micro");
    app.wait(30);
    app.click("leave");
    assert_eq!(app.store(|s| s.study_days().unwrap())[&today], 5 * 60 + 30);
    let streak = app.store(|s| mentor::streak(&s.study_days().unwrap(), today));
    assert_eq!(streak, 1, "studied today");

    // Settings adds the demo history.
    let own = app.store(|s| {
        (
            s.readings().unwrap(),
            s.unfinished().unwrap(),
            s.finished(100).unwrap(),
        )
    });
    app.press("secondary-,");
    let settings: Vec<ElementId> = ["more-time", "Dark", "add-demo"]
        .into_iter()
        .map(ElementId::from)
        .collect();
    app.reveal("add-demo", &settings);
    app.click("add-demo");
    assert!(
        app.store(|s| s.has_demo().unwrap()),
        "demo history is loaded"
    );
    assert!(app.shows("remove-demo"), "and can be removed");
    app.shot("settings-demo");

    // Today: the plan, remade with the demo record, has the brief, the
    // reviews the demo left due, a note, a drill and a Mains answer (the
    // demo writes none), and the mentor gives advice with Open note and
    // Drill buttons.
    app.press("secondary-1");
    app.shot("today-plan");
    let plan = app
        .store(|s| s.plan(today).unwrap())
        .expect("today's plan is saved");
    let kinds: Vec<&str> = plan.iter().map(|t| t.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["brief", "review", "note", "drill", "mains"],
        "{plan:?}"
    );
    assert!(app.shows(("task", 1usize)) && app.shows(("task", 3usize)));
    for ix in 0..3usize {
        assert!(app.shows(("insight", ix)), "insight card {ix}");
    }
    assert!(
        app.shows(("insight-drill", 0usize)),
        "the weakest topic has a Drill button"
    );
    assert!(app.shows("continue-reading"));
    let today_page: Vec<ElementId> = ["plan", "mentor", "this-week", "question-of-the-day"]
        .into_iter()
        .map(ElementId::from)
        .collect();
    app.reveal("qotd-answer", &today_page);
    app.click("qotd-answer");
    app.shot("today-question");
    app.reveal(("task-done", 2usize), &today_page);

    // Ticking a task saves it; the plan stays frozen for the day.
    app.click(("task-done", 2usize));
    assert!(app.store(|s| s.plan(today).unwrap().unwrap()[2].done));
    // The review task opens the review session, the note task its note,
    // the drill task its drill.
    app.click(("task", 1usize));
    assert_eq!(app.screen(), Screen::Revision);
    app.press("secondary-1");
    app.click(("task", 2usize));
    assert_eq!(app.screen(), Screen::Notes);
    app.press("secondary-1");
    app.click(("task", 3usize));
    assert!(app.in_exam(), "the drill task opens the exam room");
    assert_eq!(app.exam().kind, format!("topic:{}", plan[3].target));
    app.click("leave");
    let after = app.store(|s| s.plan(today).unwrap().unwrap());
    assert_eq!(
        after
            .iter()
            .map(|t| (&t.title, t.minutes))
            .collect::<Vec<_>>(),
        plan.iter()
            .map(|t| (&t.title, t.minutes))
            .collect::<Vec<_>>(),
        "frozen for the day"
    );

    // Analysis: every tab, and the charts draw their data.
    app.click("0-0-4");
    assert!(app.shows("demo-banner"));
    let page: Vec<ElementId> = ["score-chart", "subject-chart", "heatmap"]
        .into_iter()
        .map(ElementId::from)
        .chain([ElementId::from(("drill-topic", 0usize))])
        .collect();
    app.shot("analysis-overview");
    for chart in ["score-chart", "subject-chart"] {
        let drawn = app.coloured_pixels(chart);
        assert!(
            drawn > 300,
            "{chart} draws its data: {drawn} coloured pixels"
        );
    }
    assert!(
        app.shows(("drill-topic", 0usize)),
        "a weak topic with a Drill button"
    );
    assert!(app.shows("confidence-Guess"), "guess analysis");
    app.reveal("heatmap", &page);
    assert!(
        app.coloured_pixels("heatmap") > 300,
        "the study calendar has days in it"
    );
    app.shot("analysis-overview-lower");
    app.click(("drill-topic", 0usize));
    assert!(app.in_exam(), "Drill sits a drill on the topic");
    assert!(app.exam().kind.starts_with("topic:"));
    app.click("leave");

    app.click("0-0-4");
    app.analysis_tab(1);
    assert!(app.coloured_pixels("subject-chart") > 300);
    app.shot("analysis-subjects");
    app.analysis_tab(2);
    assert!(app.coloured_pixels("score-chart") > 300);
    app.shot("analysis-tests");
    app.analysis_tab(3);
    assert!(app.coloured_pixels("heatmap") > 300);
    let time: Vec<ElementId> = ["heatmap", "week-chart"]
        .into_iter()
        .map(ElementId::from)
        .collect();
    app.reveal("week-chart", &time);
    assert!(app.coloured_pixels("week-chart") > 300, "hours a week draw");
    app.shot("analysis-time");
    app.analysis_tab(4);
    app.shot("analysis-coverage");
    app.analysis_tab(0);

    // Removing the demo history leaves every row of the student's own.
    app.press("secondary-,");
    app.reveal(
        "remove-demo",
        &["more-time", "Dark", "remove-demo"]
            .into_iter()
            .map(ElementId::from)
            .collect::<Vec<_>>(),
    );
    app.click("remove-demo");
    assert!(!app.store(|s| s.has_demo().unwrap()));
    let (readings, unfinished, finished) = app.store(|s| {
        (
            s.readings().unwrap(),
            s.unfinished().unwrap(),
            s.finished(100).unwrap(),
        )
    });
    for (id, reading) in &own.0 {
        assert_eq!(readings.get(id), Some(reading), "{id} is as it was");
    }
    assert!(
        readings.values().all(|r| !r.demo),
        "no demo reading is left"
    );
    assert!(
        unfinished.len() >= own.1.len(),
        "every unfinished test of the student's own"
    );
    assert!(
        own.1
            .iter()
            .all(|test| unfinished.iter().any(|t| t.id == test.id))
    );
    assert_eq!(finished, own.2, "no demo result is left");
    assert_eq!(app.store(|s| s.study_days().unwrap())[&today], 5 * 60 + 30);

    drop(app);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Current affairs: today's brief from the plan, the date bar and the
/// calendar, a story and an editorial read in Full mode, Rapid mode, the
/// day's quiz, the monthly issue, and both showing in Today and Analysis.
fn current_affairs() {
    let today = chrono::Local::now().date_naive();
    let dir = std::env::temp_dir().join(format!("civil-mentor-ca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = dir.join("civil-mentor.db");
    let mut app = Harness::new(Store::open(&db).expect("the data file opens"), None);
    app.click("decline-diagnostic");

    // The plan starts with the latest brief, on any day after the sample's.
    let (brief, date, stories, keys, briefs) = app.cx.update(|cx| {
        let library = &AppState::get(cx).library;
        let brief = library.todays_brief(today).expect("there are briefs");
        let day = &library.briefs[brief];
        let keys: Vec<String> = day.stories.iter().map(|s| s.reading_key()).collect();
        (
            brief,
            day.date,
            day.stories.len(),
            keys,
            library.briefs.len(),
        )
    });
    assert_eq!(stories, 5);
    let plan = app
        .store(|s| s.plan(today).unwrap())
        .expect("today's plan is saved");
    assert_eq!(plan[0].kind, "brief", "{plan:?}");
    assert_eq!(plan[0].target, date.to_string());
    assert_eq!(plan[0].minutes, 25, "five stories at five minutes");
    assert!(
        app.shows(("brief-quiz", 0usize)),
        "the brief's quiz is offered"
    );
    app.shot("today-brief");

    // The task opens the day at a glance.
    app.click(("task", 0usize));
    assert_eq!(app.screen(), Screen::CurrentAffairs);
    let shell = app.shell.clone();
    assert_eq!(app.cx.update(|cx| shell.read(cx).brief(cx)), Some(brief));
    assert!(app.shows("brief-day") && app.shows("brief-count"));
    let bar_fits = app.act(|window, _| {
        let count = window.find("brief-count");
        count.visible() && f32::from(count.bounds().origin.x + count.bounds().size.width) <= 1280.
    });
    assert!(bar_fits, "the date bar fits the window");
    app.shot("ca-day");

    // The date bar moves a day back and forward; the calendar picks a day.
    app.click("older-brief");
    assert_eq!(
        app.cx.update(|cx| shell.read(cx).brief(cx)),
        Some(brief + 1)
    );
    app.click("newer-brief");
    assert_eq!(app.cx.update(|cx| shell.read(cx).brief(cx)), Some(brief));
    app.click("calendar");
    assert!(app.shows("brief-calendar"));
    app.shot("ca-calendar");
    let oldest = app.cx.update(|cx| {
        use chrono::Datelike as _;
        let library = &AppState::get(cx).library;
        library.briefs[briefs - 1].date.num_days_from_ce() as u64
    });
    app.click(("calendar-day", oldest));
    assert_eq!(
        app.cx.update(|cx| shell.read(cx).brief(cx)),
        Some(briefs - 1)
    );
    assert!(
        !app.shows("brief-calendar"),
        "the calendar closes on a pick"
    );
    app.click("calendar");
    {
        use chrono::Datelike as _;
        app.click(("calendar-day", date.num_days_from_ce() as u64));
    }
    assert_eq!(app.cx.update(|cx| shell.read(cx).brief(cx)), Some(brief));

    // Full mode: a story opens with every part, and opening it reads it.
    app.click(("story", 0usize));
    let first = Target::Story { brief, story: 0 };
    assert_eq!(app.cx.update(|cx| shell.read(cx).story(cx)), Some(first));
    assert!(app.shows("key-facts") && app.shows("practice-question"));
    assert!(
        app.reading(&keys[0]).unwrap().done == Some(today),
        "read once opened"
    );
    let reader: Vec<ElementId> = vec![ElementId::from("ca-reader")];
    app.reveal("story-answer", &reader);
    app.click("story-answer");
    app.shot("ca-story");
    // Reading a story counts as study time while the student is active.
    app.wait(30);
    assert_eq!(app.reading(&keys[0]).unwrap().seconds, 30);
    assert_eq!(app.store(|s| s.study_days().unwrap())[&today], 30);
    app.reveal("ca-bookmark", &reader);
    app.click("ca-bookmark");
    assert!(app.reading(&keys[0]).unwrap().bookmarked, "saved");
    app.shot("ca-story-end");
    // Next follows the list, which groups by subject: the day's other Polity story.
    app.click("next-story");
    assert_eq!(
        app.cx.update(|cx| shell.read(cx).story(cx)),
        Some(Target::Story { brief, story: 4 })
    );
    // Back returns to the first story; leaving and returning keeps the place.
    app.press("secondary-[");
    assert_eq!(app.cx.update(|cx| shell.read(cx).story(cx)), Some(first));
    app.press("secondary-1");
    app.press("secondary-4");
    assert_eq!(app.cx.update(|cx| shell.read(cx).story(cx)), Some(first));

    // An editorial reads as Markdown.
    app.click(("editorial", 0usize));
    assert!(app.shows("editorial"));
    app.shot("ca-editorial");

    // Rapid mode shows every title and its key facts, and can mark them read.
    app.click("rapid-mode");
    assert!(app.cx.update(|cx| shell.read(cx).rapid(cx)));
    assert!(app.shows("rapid") && app.shows(("rapid-story", 4usize)));
    app.shot("ca-rapid");
    app.reveal("rapid-mark-read", &reader);
    app.click("rapid-mark-read");
    for key in &keys {
        assert!(app.reading(key).unwrap().done.is_some(), "{key} is read");
    }
    app.click("full-mode");
    assert!(!app.cx.update(|cx| shell.read(cx).rapid(cx)));

    // The brief task is ticked by itself now every story is read.
    app.press("secondary-1");
    let done = app.cx.update(|cx| {
        let state = AppState::get(cx);
        let plan = state.store.plan(today).unwrap().unwrap();
        mentor::task_done(
            &state.library,
            &plan[0],
            today,
            &state.reading,
            &[],
            &state.reviews,
            &[],
        )
    });
    assert!(done, "all five stories are read");
    app.shot("today-brief-read");

    // The day's quiz: its five questions in the exam room, as their own kind.
    app.press("secondary-4");
    app.click("take-quiz");
    assert!(app.in_exam(), "the quiz opens the exam room");
    let quiz = app.exam();
    assert_eq!(quiz.kind, format!("quiz:{date}"));
    assert_eq!(quiz.answers.len(), 5);
    assert_eq!(quiz.seconds_allowed, 5 * 72);
    for _ in 0..5 {
        app.press("b");
        app.press("right");
    }
    app.click("submit");
    if app.shows("submit-anyway") {
        app.click("submit-anyway");
    }
    assert_eq!(app.result(), Some(quiz.id), "the result shows");
    assert!(app.saved(quiz.id).finished.is_some());
    app.shot("ca-quiz-result");

    // Today and Analysis both show the quiz.
    app.press("secondary-1");
    assert!(
        app.shows(("brief-quiz-score", 0usize)),
        "Today shows the score"
    );
    app.shot("today-brief-quiz");
    app.press("secondary-4");
    assert!(app.shows("quiz-last"), "the brief shows its last score");
    app.click("0-0-4");
    app.analysis_tab(2);
    app.reveal(
        ("ca-quiz", quiz.id as u64),
        &["score-chart", "current-affairs"]
            .into_iter()
            .map(ElementId::from)
            .collect::<Vec<_>>(),
    );
    assert!(app.shows("current-affairs"));
    app.shot("analysis-current-affairs");
    app.analysis_tab(0);

    // The monthly issue lists the month's stories and sits its quiz.
    app.press("secondary-4");
    app.click("monthly-issue");
    assert!(app.shows("monthly"));
    app.shot("ca-monthly");
    app.click("monthly-quiz");
    assert!(app.in_exam());
    let monthly = app.exam();
    {
        use chrono::Datelike as _;
        assert_eq!(
            monthly.kind,
            format!("month:{}-{:02}", date.year(), date.month())
        );
    }
    assert_eq!(monthly.answers.len(), 35, "seven briefs of five questions");
    app.click("leave");

    drop(app);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Phase 5: a question answered wrong in a test joins the mistake book and
/// comes back in the review session on its due day, where answering it
/// moves it up a box; flashcards are flipped and marked.
fn revision() {
    let today = chrono::Local::now().date_naive();
    let day = |n: i64| today + chrono::Duration::days(n);
    let dir = std::env::temp_dir().join(format!("civil-mentor-revision-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = dir.join("civil-mentor.db");
    let mut app = Harness::new(Store::open(&db).expect("the data file opens"), None);

    // A short test with one question answered wrong.
    app.press("secondary-3");
    app.click("start-micro");
    let test = app.exam();
    let (cell, question, right, wrong) = app.cx.update(|cx| {
        let library = &AppState::get(cx).library;
        test.answers
            .iter()
            .enumerate()
            .find_map(|(cell, answer)| {
                let question = library.question(&answer.question)?;
                let right = question.answer?;
                let wrong = (right + 1) % question.options.len();
                let ix = library.question_ix(&question.id)?;
                Some((cell, ix, right, wrong))
            })
            .expect("a question with an answer")
    });
    let key = app
        .cx
        .update(|cx| mentor::mistake_key(&AppState::get(cx).library.questions[question].id));
    app.click(("cell", cell));
    app.click(("option", wrong));
    app.click("submit");
    app.click("submit-anyway");
    assert!(!app.in_exam());
    let entry = app
        .review(&key)
        .expect("the wrong answer joins the mistake book");
    assert_eq!(
        (entry.level, entry.due),
        (1, Some(day(1))),
        "due the next day"
    );

    // The mistake book lists it by subject. Today only new flashcards are
    // due, so the mistake is not in today's session.
    app.click("1-0-0");
    assert_eq!(app.screen(), Screen::Revision);
    assert!(app.shows("mistake-book") && app.shows(("mistake", question)));
    assert!(app.shows("start-review"), "new flashcards are due today");
    app.shot("revision-book");
    let queue = app.cx.update(|cx| {
        let state = AppState::get(cx);
        mentor::review_queue(&state.library, &state.reviews, today)
    });
    assert_eq!(queue.len(), mentor::NEW_CARDS, "{queue:?}");
    assert!(!queue.contains(&key));

    // The next day, Today's plan has the review task, which opens the
    // session with the mistake first.
    app.days_ahead(1);
    app.press("secondary-1");
    let plan = app
        .store(|s| s.plan(day(1)).unwrap())
        .expect("tomorrow's plan is saved");
    let task = plan
        .iter()
        .position(|t| t.kind == "review")
        .unwrap_or_else(|| panic!("a review task in {plan:?}"));
    assert!(plan[task].minutes <= mentor::REVIEW_MINUTES);
    let today_page: Vec<ElementId> = ["plan", "mentor", "this-week", "question-of-the-day"]
        .into_iter()
        .map(ElementId::from)
        .collect();
    app.reveal(("task", task), &today_page);
    app.shot("today-review-task");
    app.click(("task", task));
    assert_eq!(app.screen(), Screen::Revision);
    assert_eq!(
        app.review_item(),
        Some(key.clone()),
        "the mistake comes back"
    );
    assert!(app.shows("review-mistake"));
    app.shot("review-mistake");

    // Answering it right moves it to box 2, due three days on.
    app.tap(("review-option", right), &["review-mistake"]);
    let entry = app.review(&key).unwrap();
    assert_eq!((entry.level, entry.due), (2, Some(day(4))));
    app.shot("review-answered");

    // Then the flashcards due today, after any other mistake in the queue.
    app.tap("review-next", &["review-mistake"]);
    for _ in 0..4 {
        let item = app.review_item().expect("the session goes on");
        if item.starts_with("card:") {
            break;
        }
        assert!(item.starts_with("question:"), "{item}");
        app.tap("review-next", &["review-mistake"]);
    }
    let card = app.review_item().expect("a flashcard follows");
    assert!(card.starts_with("card:"), "{card}");
    assert!(app.shows("review-card") && !app.shows("card-back"));
    app.tap("flip", &["review-card"]);
    assert!(app.shows("card-back"));
    app.shot("review-card");
    app.tap("got-it", &["review-card"]);
    let entry = app.review(&card).unwrap();
    assert_eq!((entry.level, entry.due), (2, Some(day(4))));
    assert_ne!(app.review_item(), Some(card), "the session moves on");

    // A card marked Missed goes back to box 1, due the next day. The time
    // spent on the session counts as study.
    let next = app.review_item().expect("another flashcard");
    app.wait(20);
    app.tap("flip", &["review-card"]);
    app.tap("missed", &["review-card"]);
    let missed = app.review(&next).unwrap();
    assert_eq!((missed.level, missed.due), (1, Some(day(2))));
    let studied = app.store(|s| s.study_days().unwrap());
    assert!(
        studied.get(&day(1)).is_some_and(|&s| s >= 20),
        "{studied:?}"
    );

    // Finishing returns to the mistake book, which shows the new box.
    app.click("stop-review");
    assert!(app.shows("mistake-book"));
    app.shot("revision");

    drop(app);
    std::fs::remove_dir_all(&dir).unwrap();
}
fn highlighting() {
    let dir = std::env::temp_dir().join(format!("civil-mentor-marks-{}", std::process::id()));
    let db = dir.join("civil-mentor.db");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the temp folder can be made");
    let mut app = Harness::new(Store::open(&db).expect("the data file opens"), None);
    app.click("decline-diagnostic");

    // A passage of a note, highlighted with a comment.
    let rights = app.note_ix("gs2-polity-fundamental-rights");
    app.click("0-0-1");
    app.click(("note", rights));
    app.select(("note-section", 0usize));
    app.click("highlight");
    assert!(app.shows("highlight-bar"), "the comment bar opens");
    app.act(|window, cx| window.input("Where is this article quoted?", cx));
    app.shot("note-highlight-bar");
    app.click("highlight-save");
    let marks = app.marks();
    assert_eq!(marks.len(), 1, "one highlight was saved: {marks:?}");
    assert_eq!(marks[0].kind, MarkKind::Highlight);
    assert_eq!(
        marks[0].comment.as_deref(),
        Some("Where is this article quoted?")
    );
    assert_eq!(marks[0].item, "gs2-polity-fundamental-rights");
    assert!(marks[0].quote.trim().len() > 8, "{:?}", marks[0].quote);
    assert_eq!(app.painted(), 1, "the highlight paints on the note");
    app.shot("note-highlight");

    // A clip keeps a passage with no comment.
    app.select(("note-section", 1usize));
    app.click("clip");
    let marks = app.marks();
    assert_eq!(marks.len(), 2, "the clip was saved: {marks:?}");
    assert_eq!(marks[1].kind, MarkKind::Clip);
    assert!(marks[1].comment.is_none());
    assert!(marks[1].quote.trim().len() > 8, "{:?}", marks[1].quote);
    app.shot("note-clip");

    // The highlight is still there after a restart.
    drop(app);
    let mut app = Harness::new(Store::open(&db).expect("the data file reopens"), None);
    let rights = app.note_ix("gs2-polity-fundamental-rights");
    app.click("0-0-1");
    app.click(("note", rights));
    assert_eq!(app.marks().len(), 2, "both marks were saved");
    assert_eq!(
        app.painted(),
        1,
        "the highlight paints again on the reopened note"
    );
    app.shot("note-reopened");

    // And a story can be marked the same way.
    app.click("0-0-3");
    app.click(("story", 0usize));
    app.select("story-context");
    app.click("story-highlight");
    assert!(
        app.shows("highlight-bar"),
        "the story highlight waits for its comment"
    );
    app.click("highlight-save");
    let marks = app.marks();
    assert_eq!(marks.len(), 3, "the story highlight was saved: {marks:?}");
    assert!(marks[2].item.starts_with("story:"), "{}", marks[2].item);
    assert_eq!(marks[2].part, "context");
    assert_eq!(app.painted(), 2, "the story highlight paints");
    app.shot("story-highlight");

    // The Notebook lists what was kept, filters it, and opens its source.
    app.click("1-0-3");
    assert_eq!(app.screen(), Screen::Notebook);
    let highlight = app
        .marks()
        .into_iter()
        .find(|mark| mark.kind == MarkKind::Highlight)
        .expect("a highlight to list");
    let clip = app
        .marks()
        .into_iter()
        .find(|mark| mark.kind == MarkKind::Clip)
        .expect("a clip to list");
    assert!(
        app.shows(("mark", highlight.id as usize)) && app.shows(("mark", clip.id as usize)),
        "both marks are listed"
    );
    app.shot("notebook-all");
    app.click("nb-highlights");
    assert!(
        app.shows(("mark", highlight.id as usize)) && !app.shows(("mark", clip.id as usize)),
        "clips are filtered out of the highlights"
    );
    app.click(("mark", highlight.id as usize));
    app.shot("notebook-highlight");
    app.click("open-mark");
    assert_eq!(app.screen(), Screen::Notes, "a highlight opens its note");
    assert_eq!(
        app.note(),
        Some(app.note_ix("gs2-polity-fundamental-rights"))
    );

    // A note of the student's own, saved and found by search.
    app.click("1-0-3");
    app.click("new-note");
    app.act(|window, cx| window.input("Drainage of the Ganga plain", cx));
    app.click("note-body");
    app.act(|window, cx| window.input("svadharma2026: the plain is khadar and bhangar.", cx));
    app.shot("notebook-editor");
    app.click("save-note");
    let mine = app.cx.update(|cx| AppState::get(cx).my_notes.clone());
    assert_eq!(mine.len(), 1, "the note was saved: {mine:?}");
    assert!(mine[0].title.contains("Drainage of the Ganga plain"));
    assert!(mine[0].body.contains("svadharma2026"));
    assert!(app.shows(("my-note", mine[0].id as usize)));
    app.shot("notebook-own-note");
    app.press("secondary-k");
    app.act(|window, cx| window.input("svadharma2026", cx));
    app.shot("notebook-search");
    app.press("enter");
    assert_eq!(
        app.screen(),
        Screen::Notebook,
        "search finds the student's own note"
    );

    // The focus timer counts a 25-minute block down and records it as study.
    let before = app.store(|s| s.study_days().unwrap().values().sum::<u32>());
    app.click("sidebar-footer-0");
    app.wait(1500);
    let after = app.store(|s| s.study_days().unwrap().values().sum::<u32>());
    assert_eq!(
        after,
        before + 25 * 60,
        "the block is a quarter of an hour of focus study: {before} -> {after}"
    );
    app.shot("focus-done");

    std::fs::remove_dir_all(&dir).expect("the temp folder can go");
}

/// Phase 6: Today plans a Mains answer, which is written against the clock,
/// survives a restart, runs past its time, is submitted, scored and found in
/// the history with its trend; the next day has no Mains task and the day
/// after has one again.
fn mains() {
    let today = chrono::Local::now().date_naive();
    let day = |n: i64| today + chrono::Duration::days(n);
    let dir = std::env::temp_dir().join(format!("civil-mentor-mains-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = dir.join("civil-mentor.db");
    let store = Store::open(&db).expect("the data file opens");
    store
        .save_settings(&Settings {
            daily_minutes: 240,
            ..Settings::default()
        })
        .unwrap();
    let mut app = Harness::new(store, None);
    app.click("decline-diagnostic");
    let (ghats, governor) = app.cx.update(|cx| {
        let library = &AppState::get(cx).library;
        (
            library.mains_ix("gs1-western-ghats-rainfall").unwrap(),
            library.mains_ix("gs2-governor-discretion").unwrap(),
        )
    });
    let today_page: Vec<ElementId> = ["plan", "mentor", "this-week", "question-of-the-day"]
        .into_iter()
        .map(ElementId::from)
        .collect();

    // Nothing written yet, so today's plan ends with a Mains answer: 7
    // minutes for a 10-mark question and 5 to score it.
    let plan = app
        .store(|s| s.plan(today).unwrap())
        .expect("today's plan is saved");
    let task = plan
        .iter()
        .position(|t| t.kind == "mains")
        .unwrap_or_else(|| panic!("a Mains task in {plan:?}"));
    assert_eq!(task, plan.len() - 1, "the Mains answer comes last");
    assert_eq!(
        (plan[task].title.as_str(), plan[task].minutes),
        ("Write one Mains answer: GS-1, 10 marks", 12)
    );
    app.reveal(("task", task), &today_page);
    app.shot("today-mains-task");

    // The task opens its question, with the model answer hidden.
    app.click(("task", task));
    assert_eq!(app.screen(), Screen::Mains);
    assert!(app.shows("mains-question-card") && !app.shows("model-answer"));
    app.shot("mains-question");

    // The list, by paper and topic, filtered by paper and by answered.
    app.click("mains-back");
    assert!(app.shows(("mains-question", ghats)) && app.shows(("mains-question", governor)));
    app.shot("mains-list");
    app.click("paper-gs2");
    assert!(
        app.shows(("mains-question", governor)) && !app.shows(("mains-question", ghats)),
        "only GS-2"
    );
    app.shot("mains-list-gs2");
    app.click("status-answered");
    assert!(
        !app.shows(("mains-question", governor)),
        "nothing answered yet"
    );
    app.click("status-unanswered");
    assert!(app.shows(("mains-question", governor)));
    app.click("status-all");

    // Writing: the clock counts down from 11 minutes for 15 marks, and every
    // keystroke is saved.
    let studied = |app: &mut Harness| app.store(|s| s.study_days().unwrap().values().sum::<u32>());
    let before = studied(&mut app);
    app.click(("mains-question", governor));
    app.click("start-writing");
    assert!(app.shows("mains-editor"));
    app.click("mains-editor");
    let opening = "The Governor is the constitutional head of a state under Article 153, \
                   appointed by the President.";
    app.act(|window, cx| window.input(opening, cx));
    let draft = app.mains_draft();
    assert!(
        draft.id > 0 && draft.text.contains("Article 153"),
        "{draft:?}"
    );
    assert_eq!(draft.submitted, None);
    app.wait(90);
    assert_eq!(app.mains_draft().seconds, 90);
    assert!(app.shows("mains-clock") && !app.shows("time-up"));
    app.shot("mains-editor");

    // The app is closed mid-answer and opened again: the draft is there, with
    // its words and its clock.
    drop(app);
    let mut app = Harness::new(Store::open(&db).expect("the data file reopens"), None);
    app.click("1-0-1");
    assert_eq!(app.screen(), Screen::Mains);
    app.click("paper-gs2");
    app.click(("mains-question", governor));
    app.click("start-writing");
    let resumed = app.mains_draft();
    assert_eq!(resumed.id, draft.id, "the same answer, not a new one");
    assert!(resumed.text.contains(opening), "{:?}", resumed.text);
    assert_eq!(resumed.seconds, 90, "the clock stands where it stopped");

    // Time runs out. Left alone, the clock stops at zero; typing on runs it
    // into the overrun, which is kept.
    app.click("mains-editor");
    app.act(|window, cx| {
        window.input(
            " It acts on ministerial advice except under Article 163.",
            cx,
        )
    });
    app.wait(600);
    assert_eq!(
        app.mains_draft().seconds,
        660,
        "an idle overrun is not counted"
    );
    assert!(app.shows("time-up"));
    app.act(|window, cx| window.input(" Sarkaria and Punchhi asked for restraint.", cx));
    app.wait(45);
    let written = app.mains_draft();
    assert_eq!(written.seconds, 705, "45 seconds over");
    app.shot("mains-time-up");

    // Submitted, the answer sits beside the model answer and its points.
    // The sheet is taller than the window, so every click scrolls to its
    // button first, by whichever small element is in view.
    app.click("mains-submit");
    assert!(app.shows("mains-scoring") && app.shows("your-answer") && app.shows("model-answer"));
    let sheet: Vec<ElementId> = ["mains-back", "mains-total", "scoring-done"]
        .into_iter()
        .map(ElementId::from)
        .chain((0..8usize).map(|point| ElementId::from(("point", point))))
        .chain((0..7).map(|d| ElementId::from(SharedString::from(format!("score-{d}-0")))))
        .collect();
    let press = |app: &mut Harness, id: ElementId| {
        app.centre(id.clone(), &sheet);
        app.click(id);
    };
    app.shot("mains-scoring");
    press(&mut app, ("point", 0usize).into());
    press(&mut app, ("point", 4usize).into());
    app.shot("mains-scoring-points");
    for (dimension, score) in [3, 2, 3, 2, 1, 3, 4].into_iter().enumerate() {
        press(
            &mut app,
            SharedString::from(format!("score-{dimension}-{score}")).into(),
        );
    }
    app.shot("mains-scored");
    let scored = app.store(|s| s.mains_answers().unwrap())[0].clone();
    assert_eq!(scored.id, written.id);
    assert!(scored.submitted.is_some_and(|s| s.date() == today));
    assert_eq!(scored.points, [0, 4]);
    assert_eq!(scored.total(), Some(18), "{:?}", scored.scores);
    assert_eq!(
        studied(&mut app) - before,
        705,
        "the writing time, before and after the restart, is study time"
    );

    // Done goes to the history; one scored answer is not yet a trend.
    press(&mut app, "scoring-done".into());
    assert!(app.shows(("history", scored.id as usize)));
    assert!(!app.shows(("trend-chart", 0usize)));

    // The next day has no Mains task; the day after has one, from the
    // paper with no answer yet.
    app.days_ahead(1);
    app.press("secondary-1");
    let plan = app
        .store(|s| s.plan(day(1)).unwrap())
        .expect("the plan is saved");
    assert!(plan.iter().all(|t| t.kind != "mains"), "{plan:?}");
    app.days_ahead(2);
    app.press("secondary-1");
    let plan = app
        .store(|s| s.plan(day(2)).unwrap())
        .expect("the plan is saved");
    let task = plan
        .iter()
        .position(|t| t.kind == "mains")
        .unwrap_or_else(|| panic!("a Mains task in {plan:?}"));
    assert_eq!(plan[task].target, "gs1-western-ghats-rainfall");
    app.reveal(("task", task), &today_page);
    app.click(("task", task));

    // Asked for, the model answer shows before writing.
    let question_page: Vec<ElementId> = ["mains-back", "start-writing", "show-model"]
        .into_iter()
        .map(ElementId::from)
        .collect();
    app.reveal("show-model", &question_page);
    app.click("show-model");
    assert!(app.shows("model-answer"));
    app.reveal("mains-back", &question_page);
    app.shot("mains-model-shown");
    app.reveal("start-writing", &question_page);
    app.click("start-writing");
    assert!(app.shows("mains-editor"), "the editor opens");
    app.click("mains-editor");
    app.act(|window, cx| window.input("The Western Ghats catch the south-west monsoon.", cx));
    app.wait(200);
    let second = app.mains_draft();
    app.shot("mains-second");
    assert!(second.text.contains("Western Ghats"), "{second:?}");
    assert_eq!(second.seconds, 200);
    app.click("mains-submit");
    assert!(app.shows("mains-scoring"), "submitted");
    for (dimension, score) in [4, 3, 3, 3, 2, 3, 4].into_iter().enumerate() {
        press(
            &mut app,
            SharedString::from(format!("score-{dimension}-{score}")).into(),
        );
    }
    press(&mut app, "scoring-done".into());
    let plan = app.store(|s| s.plan(day(2)).unwrap()).unwrap();
    assert!(
        app.cx.update(|cx| {
            let state = AppState::get(cx);
            mentor::task_done(
                &state.library,
                &plan[task],
                day(2),
                &state.reading,
                &[],
                &state.reviews,
                &state.mains,
            )
        }),
        "the Mains task is done once an answer is submitted"
    );

    // The history lists both, newest first, with a chart per dimension.
    let answers = app.store(|s| s.mains_answers().unwrap());
    assert_eq!(answers.len(), 2);
    assert!(answers.iter().all(|a| a.total().is_some()));
    let history: Vec<ElementId> = ["mains-trend", "mains-history"]
        .into_iter()
        .map(ElementId::from)
        .chain((0..7usize).map(|d| ElementId::from(("trend-chart", d))))
        .chain(
            answers
                .iter()
                .map(|a| ElementId::from(("history", a.id as usize))),
        )
        .collect();
    for answer in &answers {
        assert!(app.shows(("history", answer.id as usize)));
    }
    app.reveal("mains-trend", &history);
    for dimension in 0..7usize {
        app.reveal(("trend-chart", dimension), &history);
        let drawn = app.coloured_pixels(("trend-chart", dimension));
        assert!(
            drawn > 20,
            "the chart for dimension {dimension} drew its line: {drawn} pixels"
        );
    }
    app.reveal("mains-trend", &history);
    app.shot("mains-history");

    // A history entry opens its scoring sheet again.
    let first = ("history", answers[0].id as usize);
    app.reveal(first, &history);
    app.click(first);
    assert!(app.shows("mains-scoring"));

    drop(app);
    std::fs::remove_dir_all(&dir).unwrap();
}

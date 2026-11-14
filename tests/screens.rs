//! Drives the real shell in an offscreen window: clicks every sidebar item,
//! uses the shortcuts, searches, changes settings, and saves a PNG of each
//! state to `target/shots/` so the screens can be looked at.
//!
//! Run with `cargo test --test screens`. It needs the main thread and a GPU
//! renderer (Metal on macOS), so it does not use the default test harness.

use std::path::PathBuf;
use std::sync::Arc;

use civil_mentor::store::{Settings, Store, ThemeChoice};
use civil_mentor::{AppAssets, AppState, Screen, Shell};
use gpui_kit::component::{Theme, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, Bounds, Entity, HeadlessAppContext, SharedString, Window, WindowBounds,
    WindowOptions, px, size,
};

struct Harness {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Entity<Shell>,
    shots: PathBuf,
}

impl Harness {
    fn new(store_error: Option<&str>) -> Harness {
        let store_error = store_error.map(str::to_string);
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(AppAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            civil_mentor::init(Store::in_memory(), store_error, cx);
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

    fn click(&mut self, id: impl Into<SharedString>) {
        let id = id.into();
        self.act(|window, cx| window.click(id, cx));
    }

    fn press(&mut self, key: &'static str) {
        self.act(|window, cx| window.press(key, cx));
    }

    fn screen(&mut self) -> Screen {
        let shell = self.shell.clone();
        self.cx.update(|cx| shell.read(cx).screen())
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
    let mut app = Harness::new(None);
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
    let footer = [("sidebar-footer-0".to_string(), Screen::Settings)];
    let items: Vec<(String, Screen)> = primary.chain(secondary).chain(footer).collect();
    assert_eq!(
        items.len(),
        Screen::ALL.len(),
        "every screen is in the sidebar"
    );
    for (id, screen) in items.into_iter().rev() {
        app.click(id.clone());
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
    assert_eq!(
        app.screen(),
        Screen::Notes,
        "the first hit for `monsoon` is a note"
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

    // When the data file cannot be opened the app still starts, on a
    // throwaway store, and says so on every screen.
    let mut unsaved = Harness::new(Some("could not open civil-mentor.db: disk is read-only"));
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

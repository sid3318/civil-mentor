//! The window shell: title bar, sidebar, search palette and the active screen.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::sidebar::{
    Sidebar, SidebarGroup, SidebarItem as _, SidebarMenu, SidebarMenuItem, SidebarToggleButton,
};
use gpui_kit::component::{
    ActiveTheme as _, Collapsible as _, Disableable as _, Icon, Sizable as _, StyledExt as _,
    Theme, TitleBar, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::*;

use crate::AppState;
use crate::analysis::AnalysisScreen;
use crate::content::Target;
use crate::current_affairs::CurrentAffairsScreen;
use crate::exam::ExamScreen;
use crate::mentor::TestPlan;
use crate::notebook::NotebookScreen;
use crate::notes::NotesScreen;
use crate::practice::PracticeScreen;
use crate::review::ReviewScreen;
use crate::revision::RevisionScreen;
use crate::screens;
use crate::settings::SettingsScreen;
use crate::store::Study;
use crate::store::ThemeChoice;
use crate::syllabus::SyllabusScreen;
use crate::today::{TodayScreen, streak_label};

actions!(
    civil_mentor,
    [
        Quit,
        OpenSearch,
        OpenSettings,
        ToggleSidebar,
        ToggleTheme,
        GoBack,
        GoForward,
        GoToday,
        GoNotes,
        GoPractice,
        GoCurrentAffairs,
        GoAnalysis,
    ]
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Today,
    Notes,
    Practice,
    CurrentAffairs,
    Analysis,
    Revision,
    Mains,
    Syllabus,
    Notebook,
    Settings,
}

impl Screen {
    /// The four pillars and the daily plan: the top group of the sidebar.
    pub const PRIMARY: [Screen; 5] = [
        Screen::Today,
        Screen::Notes,
        Screen::Practice,
        Screen::CurrentAffairs,
        Screen::Analysis,
    ];
    pub const SECONDARY: [Screen; 4] = [
        Screen::Revision,
        Screen::Mains,
        Screen::Syllabus,
        Screen::Notebook,
    ];
    pub const ALL: [Screen; 10] = [
        Screen::Today,
        Screen::Notes,
        Screen::Practice,
        Screen::CurrentAffairs,
        Screen::Analysis,
        Screen::Revision,
        Screen::Mains,
        Screen::Syllabus,
        Screen::Notebook,
        Screen::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Today => "Today",
            Screen::Notes => "Notes",
            Screen::Practice => "Practice",
            Screen::CurrentAffairs => "Current affairs",
            Screen::Analysis => "Analysis",
            Screen::Revision => "Revision",
            Screen::Mains => "Mains",
            Screen::Syllabus => "Syllabus",
            Screen::Notebook => "Notebook",
            Screen::Settings => "Settings",
        }
    }

    pub(crate) fn icon(self) -> IconName {
        match self {
            Screen::Today => IconName::Sun,
            Screen::Notes => IconName::BookOpen,
            Screen::Practice => IconName::SquarePen,
            Screen::CurrentAffairs => IconName::Newspaper,
            Screen::Analysis => IconName::ChartColumn,
            Screen::Revision => IconName::Repeat,
            Screen::Mains => IconName::PenLine,
            Screen::Syllabus => IconName::ListTree,
            Screen::Notebook => IconName::NotebookText,
            Screen::Settings => IconName::Settings,
        }
    }
}

/// Where the student is: a section, a note open in the reader, or one of
/// the practice places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Screen(Screen),
    Note(usize),
    /// One question from the bank, by index into the library.
    Question(usize),
    /// A past paper, read with its answers.
    Paper(u16),
    /// The exam room, with the sitting [`ExamScreen`] holds.
    Exam,
    /// The result of a sitting, by its id in the store.
    Result(i64),
    /// A story or editorial in the current-affairs reader.
    Story(Target),
}

impl Place {
    fn screen(self) -> Screen {
        match self {
            Place::Screen(screen) => screen,
            Place::Note(_) => Screen::Notes,
            Place::Story(_) => Screen::CurrentAffairs,
            Place::Question(_) | Place::Paper(_) | Place::Exam | Place::Result(_) => {
                Screen::Practice
            }
        }
    }
}

const SEARCH_KEYS: &str = "secondary-k";

/// Key bindings, menus and the actions that need no window.
pub(crate) fn init(cx: &mut App) {
    // `secondary` is Cmd on macOS and Ctrl on Windows and Linux. Only
    // `KeyBinding::load` resolves it, so the bindings go through that.
    fn key(cx: &App, keys: &str, action: impl Action) -> KeyBinding {
        KeyBinding::load(keys, Box::new(action), None, false, None, cx)
            .expect("the shortcut is spelled correctly")
    }
    let bindings = [
        key(cx, "secondary-q", Quit),
        key(cx, SEARCH_KEYS, OpenSearch),
        key(cx, "secondary-,", OpenSettings),
        key(cx, "secondary-\\", ToggleSidebar),
        key(cx, "secondary-[", GoBack),
        key(cx, "secondary-]", GoForward),
        key(cx, "secondary-1", GoToday),
        key(cx, "secondary-2", GoNotes),
        key(cx, "secondary-3", GoPractice),
        key(cx, "secondary-4", GoCurrentAffairs),
        key(cx, "secondary-5", GoAnalysis),
    ];
    cx.bind_keys(bindings);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &ToggleTheme, cx| toggle_theme(cx));
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    cx.set_menus([
        Menu::new("Civil Mentor").items([
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Quit Civil Mentor", Quit),
        ]),
        Menu::new("View").items([
            MenuItem::action("Search", OpenSearch),
            MenuItem::action("Back", GoBack),
            MenuItem::action("Forward", GoForward),
            MenuItem::separator(),
            MenuItem::action("Today", GoToday),
            MenuItem::action("Notes", GoNotes),
            MenuItem::action("Practice", GoPractice),
            MenuItem::action("Current Affairs", GoCurrentAffairs),
            MenuItem::action("Analysis", GoAnalysis),
            MenuItem::separator(),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Toggle Theme", ToggleTheme),
        ]),
    ]);
}

/// Sends the app-level shortcuts to `shell`. They are handled here, not on an
/// element, so they work whatever has the keyboard focus.
pub(crate) fn route_actions(window: AnyWindowHandle, shell: &Entity<Shell>, cx: &mut App) {
    fn route<A: Action>(
        cx: &mut App,
        window: AnyWindowHandle,
        shell: &Entity<Shell>,
        run: impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    ) {
        let shell = shell.downgrade();
        let run = Rc::new(run);
        cx.on_action(move |_: &A, cx: &mut App| {
            let (shell, run) = (shell.clone(), run.clone());
            // A shortcut arrives while its window is being updated, and a
            // window cannot be updated twice at once. Run after that update.
            cx.defer(move |cx| {
                // Both updates fail only once the window is gone; nothing to do then.
                let _ = window.update(cx, |_, window, cx| {
                    let _ = shell.update(cx, |shell, cx| run(shell, window, cx));
                });
            });
        });
    }

    route::<OpenSearch>(cx, window, shell, |shell, window, cx| {
        shell.open_search(window, cx)
    });
    route::<OpenSettings>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::Settings, cx)
    });
    route::<ToggleSidebar>(cx, window, shell, |shell, _, cx| shell.toggle_sidebar(cx));
    route::<GoBack>(cx, window, shell, |shell, _, cx| shell.back(cx));
    route::<GoForward>(cx, window, shell, |shell, _, cx| shell.forward(cx));
    route::<GoToday>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::Today, cx)
    });
    route::<GoNotes>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::Notes, cx)
    });
    route::<GoPractice>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::Practice, cx)
    });
    route::<GoCurrentAffairs>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::CurrentAffairs, cx)
    });
    route::<GoAnalysis>(cx, window, shell, |shell, _, cx| {
        shell.go(Screen::Analysis, cx)
    });
}

/// "191 days to Prelims", "Prelims is tomorrow".
pub(crate) fn countdown(days: i64) -> String {
    match days {
        2.. => format!("{days} days to Prelims"),
        1 => "Prelims is tomorrow".to_string(),
        0 => "Prelims is today".to_string(),
        _ => "Set your exam date".to_string(),
    }
}

fn toggle_theme(cx: &mut App) {
    let dark = cx.theme().is_dark();
    AppState::update_settings(cx, |settings| {
        settings.theme = if dark {
            ThemeChoice::Light
        } else {
            ThemeChoice::Dark
        };
    });
}

/// How long a focus block is, in minutes.
const FOCUS_MINUTES: u32 = 25;

/// A focus block, counted down in the sidebar one second at a time.
struct Focus {
    left: Duration,
    /// The day's study seconds when the block started, so it adds only the
    /// minutes nothing else counted.
    studied: u32,
}

pub struct Shell {
    /// Every place visited, oldest first, and which one is current. Back and
    /// forward move `at`; going somewhere new drops the places ahead of it.
    history: Vec<Place>,
    at: usize,
    sidebar_collapsed: bool,
    /// Set when the sidebar folded itself for reading, so that it unfolds
    /// again when the student leaves the notes.
    unfold_sidebar: bool,
    palette: Entity<SearchPalette>,
    today: Entity<TodayScreen>,
    analysis: Entity<AnalysisScreen>,
    notes: Entity<NotesScreen>,
    current_affairs: Entity<CurrentAffairsScreen>,
    practice: Entity<PracticeScreen>,
    exam: Entity<ExamScreen>,
    review: Entity<ReviewScreen>,
    revision: Entity<RevisionScreen>,
    notebook: Entity<NotebookScreen>,
    syllabus: Entity<SyllabusScreen>,
    /// The focus block the student is sitting, if any.
    focus: Option<Focus>,
    settings: Entity<SettingsScreen>,
    _appearance: Subscription,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shell = cx.weak_entity();
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            if AppState::get(cx).settings.theme == ThemeChoice::System {
                Theme::sync_system_appearance(Some(window), cx);
            }
        });
        Shell {
            history: vec![Place::Screen(Screen::Today)],
            at: 0,
            sidebar_collapsed: false,
            unfold_sidebar: false,
            palette: cx.new(|cx| SearchPalette::new(shell.clone(), window, cx)),
            today: cx.new(|_| TodayScreen::new(shell.clone())),
            analysis: cx.new(|_| AnalysisScreen::new(shell.clone())),
            notes: cx.new(|_| NotesScreen::new(shell.clone())),
            current_affairs: cx.new(|cx| CurrentAffairsScreen::new(shell.clone(), cx)),
            practice: cx.new(|_| PracticeScreen::new(shell.clone())),
            exam: cx.new(|cx| ExamScreen::new(shell.clone(), cx)),
            review: cx.new(|_| ReviewScreen::new(shell.clone())),
            revision: cx.new(|_| RevisionScreen::new(shell.clone())),
            notebook: cx.new(|_| NotebookScreen::new(shell.clone())),
            syllabus: cx.new(|_| SyllabusScreen::new(shell)),
            focus: None,
            settings: cx.new(|cx| SettingsScreen::new(window, cx)),
            _appearance: appearance,
        }
    }

    pub fn screen(&self) -> Screen {
        self.history[self.at].screen()
    }

    /// The note open in the reader, if any.
    pub fn note(&self, cx: &App) -> Option<usize> {
        self.notes.read(cx).note()
    }

    /// The brief on the Current affairs screen, by index into the library.
    pub fn brief(&self, cx: &App) -> Option<usize> {
        self.current_affairs.read(cx).brief()
    }

    /// The story or editorial open in the current-affairs reader.
    pub fn story(&self, cx: &App) -> Option<Target> {
        self.current_affairs.read(cx).selected()
    }

    /// Whether the current-affairs reader is in Rapid mode.
    pub fn rapid(&self, cx: &App) -> bool {
        self.current_affairs.read(cx).rapid()
    }

    /// True while the exam room is showing. It takes the whole window, and
    /// nothing navigates away from it except leaving or submitting.
    pub fn in_exam(&self) -> bool {
        self.history[self.at] == Place::Exam
    }

    /// The sitting in the exam room, if any.
    pub fn exam_attempt(&self, cx: &App) -> Option<crate::store::Attempt> {
        self.exam.read(cx).attempt().cloned()
    }

    /// The tab open on the Analysis screen.
    pub fn analysis_tab(&self, cx: &App) -> usize {
        self.analysis.read(cx).tab()
    }

    /// The id of the sitting whose result is showing.
    pub fn result(&self) -> Option<i64> {
        match self.history[self.at] {
            Place::Result(id) => Some(id),
            _ => None,
        }
    }

    pub fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.visit(Place::Screen(screen), cx);
    }

    /// Opens what a link or search result points at.
    pub fn open(&mut self, target: Target, cx: &mut Context<Self>) {
        let place = match target {
            Target::Note(ix) => Place::Note(ix),
            Target::Question(ix) => Place::Question(ix),
            Target::Story { .. } | Target::Editorial { .. } => Place::Story(target),
        };
        self.visit(place, cx);
    }

    /// Opens Current affairs at a day's brief, by index into the library.
    pub fn open_brief(&mut self, brief: usize, cx: &mut Context<Self>) {
        self.current_affairs
            .update(cx, |screen, cx| screen.set_brief(brief, cx));
        self.go(Screen::CurrentAffairs, cx);
    }

    /// Opens Revision and starts today's review session.
    pub fn start_review(&mut self, cx: &mut Context<Self>) {
        self.revision.update(cx, |revision, cx| revision.start(cx));
        self.go(Screen::Revision, cx);
    }

    /// How many highlights are painted in the reader on screen, which the UI
    /// test checks.
    pub fn painted(&self, cx: &App) -> usize {
        self.notes.read(cx).painted() + self.current_affairs.read(cx).painted()
    }

    /// The review key on screen in the review session, if one is under way.
    pub fn review_item(&self, cx: &App) -> Option<String> {
        self.revision.read(cx).current().map(str::to_string)
    }

    pub fn open_paper(&mut self, year: u16, cx: &mut Context<Self>) {
        self.visit(Place::Paper(year), cx);
    }

    pub fn open_result(&mut self, id: i64, cx: &mut Context<Self>) {
        self.visit(Place::Result(id), cx);
    }

    /// Opens the Notebook at one of the student's own notes.
    pub fn open_my_note(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.notebook
            .update(cx, |notebook, cx| notebook.open_note(id, window, cx));
        self.go(Screen::Notebook, cx);
    }

    /// Starts a new test in the exam room.
    pub fn sit(&mut self, plan: &TestPlan, cx: &mut Context<Self>) {
        if !self.in_exam() && self.exam.update(cx, |exam, cx| exam.start(plan, cx)) {
            self.visit(Place::Exam, cx);
        }
    }

    /// Reopens an unfinished test in the exam room.
    pub fn resume(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.in_exam() && self.exam.update(cx, |exam, cx| exam.resume(id, cx)) {
            self.visit(Place::Exam, cx);
        }
    }

    /// Called by the exam room when the student leaves a test unfinished.
    pub(crate) fn leave_exam(&mut self, cx: &mut Context<Self>) {
        self.replace(Place::Screen(Screen::Practice), cx);
    }

    /// Called by the exam room when a test is submitted.
    pub(crate) fn show_result(&mut self, id: i64, cx: &mut Context<Self>) {
        self.replace(Place::Result(id), cx);
    }

    /// Puts `place` where the exam room was, so back never returns to it.
    fn replace(&mut self, place: Place, cx: &mut Context<Self>) {
        self.history.truncate(self.at + 1);
        self.history[self.at] = place;
        self.show(cx);
    }

    fn visit(&mut self, mut place: Place, cx: &mut Context<Self>) {
        if self.in_exam() {
            return;
        }
        // The notes screen is wherever the reader was left, so that going
        // back returns to that note.
        if place == Place::Screen(Screen::Notes)
            && let Some(ix) = self.note(cx)
        {
            place = Place::Note(ix);
        }
        if place == Place::Screen(Screen::CurrentAffairs)
            && let Some(target) = self.story(cx)
        {
            place = Place::Story(target);
        }
        if self.history[self.at] != place {
            self.history.truncate(self.at + 1);
            self.history.push(place);
            self.at += 1;
        }
        self.show(cx);
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.at > 0 && !self.in_exam() {
            self.at -= 1;
            self.show(cx);
        }
    }

    pub fn forward(&mut self, cx: &mut Context<Self>) {
        if self.at + 1 < self.history.len() && !self.in_exam() {
            self.at += 1;
            self.show(cx);
        }
    }

    /// Brings up the current place.
    fn show(&mut self, cx: &mut Context<Self>) {
        let place = self.history[self.at];
        if !matches!(place, Place::Note(_)) {
            self.notes.update(cx, |notes, cx| notes.stop_clock(cx));
        }
        if !matches!(place, Place::Story(_)) {
            self.current_affairs
                .update(cx, |screen, cx| screen.stop_clock(cx));
        }
        match place {
            Place::Note(ix) => self.notes.update(cx, |notes, cx| notes.show(ix, cx)),
            Place::Story(target) => self
                .current_affairs
                .update(cx, |screen, cx| screen.show(target, cx)),
            Place::Screen(Screen::CurrentAffairs) => self
                .current_affairs
                .update(cx, |screen, cx| screen.show_day(cx)),
            Place::Question(ix) => self
                .review
                .update(cx, |review, cx| review.show_question(ix, cx)),
            Place::Paper(year) => self
                .review
                .update(cx, |review, cx| review.show_paper(year, cx)),
            Place::Result(id) => self
                .review
                .update(cx, |review, cx| review.show_result(id, cx)),
            Place::Screen(_) | Place::Exam => {}
        }
        // The sidebar folds to a rail while reading, and unfolds after.
        let reading = place.screen() == Screen::Notes;
        if reading && !self.sidebar_collapsed {
            self.sidebar_collapsed = true;
            self.unfold_sidebar = true;
        } else if !reading && self.unfold_sidebar {
            self.sidebar_collapsed = false;
            self.unfold_sidebar = false;
        }
        cx.notify();
    }

    /// The focus item in the sidebar: the block's time left, or its length.
    fn focus_label(&self) -> String {
        let left = match &self.focus {
            Some(focus) => focus.left.as_secs(),
            None => u64::from(FOCUS_MINUTES) * 60,
        };
        format!("Focus {:02}:{:02}", left / 60, left % 60)
    }

    /// Starts a focus block, or ends the one running.
    fn toggle_focus(&mut self, cx: &mut Context<Self>) {
        if self.focus.is_some() {
            self.stop_focus(cx);
            return;
        }
        let today = AppState::today(cx);
        let studied = AppState::get(cx)
            .store
            .study_days()
            .ok()
            .and_then(|days| days.get(&today).copied())
            .unwrap_or(0);
        self.focus = Some(Focus {
            left: Duration::from_secs(u64::from(FOCUS_MINUTES) * 60),
            studied,
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let running = this
                    .update(cx, |this, cx| this.tick_focus(cx))
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Counts one second off the block, and records it when it runs out.
    /// Returns whether the block is still running.
    fn tick_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(focus) = &mut self.focus else {
            return false;
        };
        focus.left = focus.left.saturating_sub(Duration::from_secs(1));
        if !focus.left.is_zero() {
            cx.notify();
            return true;
        }
        let focus = self.focus.take().expect("it was running");
        let today = AppState::today(cx);
        let studied = AppState::get(cx)
            .store
            .study_days()
            .ok()
            .and_then(|days| days.get(&today).copied())
            .unwrap_or(0);
        let counted = studied.saturating_sub(focus.studied);
        let seconds = (FOCUS_MINUTES * 60).saturating_sub(counted);
        if seconds > 0 {
            let mut block = None;
            AppState::record_study(
                cx,
                Study::Focus,
                &FOCUS_MINUTES.to_string(),
                &mut block,
                seconds,
            );
        }
        cx.notify();
        false
    }

    /// Ends the running block without recording anything.
    fn stop_focus(&mut self, cx: &mut Context<Self>) {
        self.focus = None;
        cx.notify();
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        // The student's own choice stays as they set it.
        self.unfold_sidebar = false;
        cx.notify();
    }

    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) || self.in_exam() {
            return;
        }
        let palette = self.palette.clone();
        palette.update(cx, |palette, cx| palette.reset(window, cx));
        // The dialog builder runs on every rebuild; focus only the first time.
        let focus = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if focus.replace(false) {
                let state = palette.read(cx).state.clone();
                state.update(cx, |state, cx| state.focus(window, cx));
            }
            let palette = palette.clone();
            dialog
                .close_button(false)
                .p_0()
                .w(px(580.))
                .content(move |content, _, _| content.child(palette.clone()))
        });
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let today = AppState::today(cx);
        let state = AppState::get(cx);
        let days = (state.settings.exam_date - today).num_days();
        let study = state.store.study_days().unwrap_or_default();
        let countdown = format!(
            "{} · {}",
            countdown(days),
            streak_label(crate::mentor::streak(&study, today))
        );
        let dark = cx.theme().is_dark();
        let search_keys = Keystroke::parse_with_os(SEARCH_KEYS, cx.platform_info().os)
            .expect("the shortcut is spelled correctly");

        TitleBar::new()
            .child(
                h_flex()
                    .flex_1()
                    .gap_2()
                    .child(
                        SidebarToggleButton::new()
                            .collapsed(self.sidebar_collapsed)
                            .accessibility_label("Toggle sidebar")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
                    )
                    .child(
                        Button::new("back")
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::ChevronLeft))
                            .tooltip("Back")
                            .disabled(self.at == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                    )
                    .child(
                        Button::new("forward")
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::ChevronRight))
                            .tooltip("Forward")
                            .disabled(self.at + 1 == self.history.len())
                            .on_click(cx.listener(|this, _, _, cx| this.forward(cx))),
                    )
                    .child(div().text_sm().font_semibold().child("Civil Mentor")),
            )
            .child(
                h_flex()
                    .id("search-trigger")
                    .test_support()
                    .w(px(380.))
                    .h(px(24.))
                    .px_2()
                    .gap_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_pointer()
                    .hover(|this| this.border_color(cx.theme().ring))
                    .child(Icon::new(IconName::Search).xsmall())
                    .child(div().flex_1().child("Search notes, questions and news"))
                    .child(Kbd::new(search_keys))
                    .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
            )
            .child(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .gap_1()
                    .pr_2()
                    .child(
                        Button::new("countdown")
                            .ghost()
                            .xsmall()
                            .label(countdown)
                            .tooltip("Change your exam date")
                            .on_click(cx.listener(|this, _, _, cx| this.go(Screen::Settings, cx))),
                    )
                    .child(
                        Button::new("theme-toggle")
                            .ghost()
                            .small()
                            .icon(Icon::new(if dark { IconName::Sun } else { IconName::Moon }))
                            .tooltip(if dark {
                                "Switch to the light theme"
                            } else {
                                "Switch to the dark theme"
                            })
                            .on_click(|_, _, cx| toggle_theme(cx)),
                    ),
            )
    }

    fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.screen();
        let item = |screen: Screen, cx: &mut Context<Self>| {
            SidebarMenuItem::new(screen.title())
                .icon(Icon::new(screen.icon()))
                .active(screen == current)
                .on_click(cx.listener(move |this, _, _, cx| this.go(screen, cx)))
        };
        let primary = Screen::PRIMARY.map(|screen| item(screen, cx));
        let secondary = Screen::SECONDARY.map(|screen| item(screen, cx));
        let footer = SidebarMenu::new()
            .w_full()
            .child(
                SidebarMenuItem::new(self.focus_label())
                    .icon(Icon::new(IconName::Timer))
                    .active(self.focus.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_focus(cx))),
            )
            .child(item(Screen::Settings, cx))
            .collapsed(self.sidebar_collapsed)
            .render("sidebar-footer", window, cx);

        Sidebar::new("sidebar")
            .w(px(220.))
            .collapsed(self.sidebar_collapsed)
            .child(SidebarGroup::new("Study").child(SidebarMenu::new().children(primary)))
            .child(SidebarGroup::new("More").child(SidebarMenu::new().children(secondary)))
            .footer(footer)
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store_error = AppState::get(cx).store_error.clone();
        let place = self.history[self.at];
        let banner = store_error.map(|error| {
            h_flex()
                .px_4()
                .py_2()
                .gap_2()
                .bg(cx.theme().danger)
                .text_color(cx.theme().danger_foreground)
                .text_sm()
                .child(Icon::new(IconName::TriangleAlert))
                .child(format!("Your progress is not being saved. {error}"))
        });
        // The exam room takes the whole window: no title bar, no sidebar.
        if place == Place::Exam {
            return v_flex()
                .size_full()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .children(banner)
                .child(div().flex_1().min_h_0().child(self.exam.clone()))
                .into_any_element();
        }
        // Notes and Current affairs have panes that scroll on their own;
        // every other place is one column that scrolls as a whole, with its
        // own scroll position.
        let content = match place {
            Place::Note(_) | Place::Screen(Screen::Notes) => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.notes.clone())
                .into_any_element(),
            Place::Story(_) | Place::Screen(Screen::CurrentAffairs) => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.current_affairs.clone())
                .into_any_element(),
            place => {
                let (page, scroll) = match place {
                    Place::Screen(Screen::Settings) => {
                        (self.settings.clone().into_any_element(), 0)
                    }
                    Place::Screen(Screen::Syllabus) => {
                        (self.syllabus.clone().into_any_element(), 1)
                    }
                    Place::Screen(Screen::Practice) => {
                        (self.practice.clone().into_any_element(), 2)
                    }
                    Place::Screen(Screen::Today) => (self.today.clone().into_any_element(), 6),
                    Place::Screen(Screen::Analysis) => {
                        (self.analysis.clone().into_any_element(), 7)
                    }
                    Place::Screen(Screen::Revision) => {
                        (self.revision.clone().into_any_element(), 8)
                    }
                    Place::Screen(Screen::Notebook) => {
                        (self.notebook.clone().into_any_element(), 9)
                    }
                    Place::Result(_) => (self.review.clone().into_any_element(), 3),
                    Place::Paper(_) => (self.review.clone().into_any_element(), 4),
                    Place::Question(_) => (self.review.clone().into_any_element(), 5),
                    place => {
                        let screen = place.screen();
                        let page = screens::placeholder(screen, cx).into_any_element();
                        (page, 10 + screen as usize)
                    }
                };
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scrollbar()
                    .id(("screen", scroll))
                    .child(page)
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.title_bar(cx))
            .children(banner)
            .child(
                // Not `h_flex`: that centres its children vertically, and the
                // sidebar and the screen must fill the height.
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(window, cx))
                    .child(content),
            )
            .into_any_element()
    }
}

/// The Cmd+K palette. Search runs in [`crate::content::Library::search`]; the
/// palette only shows the hits it is handed.
pub struct SearchPalette {
    shell: WeakEntity<Shell>,
    state: Entity<CommandState>,
    /// What the palette is showing. Empty groups are left out, so a group's
    /// position here is its section in the palette.
    groups: Vec<(&'static str, Vec<Entry>)>,
}

#[derive(Clone)]
struct Entry {
    title: SharedString,
    detail: SharedString,
    icon: IconName,
    go: Go,
}

#[derive(Clone, Copy)]
enum Go {
    Screen(Screen),
    Target(Target),
    /// A note the student wrote, by its id.
    MyNote(i64),
}

impl SearchPalette {
    fn new(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        SearchPalette {
            shell,
            state: cx.new(|cx| CommandState::new(window, cx)),
            groups: results("", cx),
        }
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_query("", window, cx));
        self.set_query("", cx);
    }

    fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.groups = results(query, cx);
        cx.notify();
    }
}

/// The sections whose name matches, then the content hits grouped by kind.
fn results(query: &str, cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
    let needle = query.trim().to_lowercase();
    let screens = Screen::ALL
        .into_iter()
        .filter(|screen| screen.title().to_lowercase().contains(&needle))
        .map(|screen| Entry {
            title: screen.title().into(),
            detail: SharedString::default(),
            icon: screen.icon(),
            go: Go::Screen(screen),
        })
        .collect();

    let mine: Vec<Entry> = AppState::get(cx)
        .my_notes
        .iter()
        .filter(|note| {
            note.title.to_lowercase().contains(&needle)
                || note.body.to_lowercase().contains(&needle)
        })
        .map(|note| Entry {
            title: note.label().into(),
            detail: crate::content::one_line(&note.body, 60).into(),
            icon: Screen::Notebook.icon(),
            go: Go::MyNote(note.id),
        })
        .collect();

    let (mut notes, mut stories, mut questions) = (Vec::new(), Vec::new(), Vec::new());
    for hit in AppState::get(cx).library.search(&needle, 30) {
        let (group, screen) = match hit.target {
            Target::Note(_) => (&mut notes, Screen::Notes),
            Target::Story { .. } | Target::Editorial { .. } => {
                (&mut stories, Screen::CurrentAffairs)
            }
            Target::Question(_) => (&mut questions, Screen::Practice),
        };
        group.push(Entry {
            title: hit.title.into(),
            detail: hit.detail.into(),
            icon: screen.icon(),
            go: Go::Target(hit.target),
        });
    }

    [
        ("Go to", screens),
        ("Your notes", mine),
        ("Notes", notes),
        ("Current affairs", stories),
        ("Questions", questions),
    ]
    .into_iter()
    .filter(|(_, entries)| !entries.is_empty())
    .collect()
}

impl Render for SearchPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.weak_entity();
        let shell = self.shell.clone();
        let groups = self.groups.clone();

        let mut command = Command::new(&self.state)
            .bordered(false)
            // The library has already matched the query; do not filter again.
            .filterable(false)
            .placeholder("Search notes, questions and news")
            .on_query(move |query, _, cx| {
                let _ = palette.update(cx, |palette, cx| palette.set_query(query, cx));
            })
            .on_confirm(move |index, window, cx| {
                let Some(entry) = groups
                    .get(index.section)
                    .and_then(|group| group.1.get(index.row))
                else {
                    return;
                };
                window.close_dialog(cx);
                let _ = shell.update(cx, |shell, cx| match entry.go {
                    Go::Screen(screen) => shell.go(screen, cx),
                    Go::Target(target) => shell.open(target, cx),
                    Go::MyNote(id) => shell.open_my_note(id, window, cx),
                });
            });

        for (label, entries) in &self.groups {
            let items = entries.iter().map(|entry| {
                let (title, detail, icon) = (entry.title.clone(), entry.detail.clone(), entry.icon);
                CommandItem::new().label(title.clone()).child(move |_, cx| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(Icon::new(icon).small())
                        .child(div().flex_1().truncate().child(title.clone()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(detail.clone()),
                        )
                })
            });
            command = command.group(CommandGroup::new().label(*label).items(items));
        }
        command
    }
}

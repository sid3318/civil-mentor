//! The window shell: title bar, sidebar, search palette and the active screen.

use std::cell::Cell;
use std::rc::Rc;

use chrono::Local;
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
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::content::Target;
use crate::notes::NotesScreen;
use crate::screens;
use crate::settings::SettingsScreen;
use crate::store::ThemeChoice;
use crate::syllabus::SyllabusScreen;

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

/// Where the student is: a section, or a note open in the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Screen(Screen),
    Note(usize),
}

impl Place {
    fn screen(self) -> Screen {
        match self {
            Place::Screen(screen) => screen,
            Place::Note(_) => Screen::Notes,
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
    notes: Entity<NotesScreen>,
    syllabus: Entity<SyllabusScreen>,
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
            notes: cx.new(|_| NotesScreen::new(shell.clone())),
            syllabus: cx.new(|_| SyllabusScreen::new(shell)),
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

    pub fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.visit(Place::Screen(screen), cx);
    }

    /// Opens what a link or search result points at. A question or a story
    /// opens its section until the practice and current-affairs phases.
    pub fn open(&mut self, target: Target, cx: &mut Context<Self>) {
        let place = match target {
            Target::Note(ix) => Place::Note(ix),
            Target::Question(_) => Place::Screen(Screen::Practice),
            Target::Story { .. } => Place::Screen(Screen::CurrentAffairs),
        };
        self.visit(place, cx);
    }

    fn visit(&mut self, mut place: Place, cx: &mut Context<Self>) {
        // The notes screen is wherever the reader was left, so that going
        // back returns to that note.
        if place == Place::Screen(Screen::Notes)
            && let Some(ix) = self.note(cx)
        {
            place = Place::Note(ix);
        }
        if self.history[self.at] != place {
            self.history.truncate(self.at + 1);
            self.history.push(place);
            self.at += 1;
        }
        self.show(cx);
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.at > 0 {
            self.at -= 1;
            self.show(cx);
        }
    }

    pub fn forward(&mut self, cx: &mut Context<Self>) {
        if self.at + 1 < self.history.len() {
            self.at += 1;
            self.show(cx);
        }
    }

    /// Brings up the current place.
    fn show(&mut self, cx: &mut Context<Self>) {
        let place = self.history[self.at];
        if let Place::Note(ix) = place {
            self.notes.update(cx, |notes, cx| notes.show(ix, cx));
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

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        // The student's own choice stays as they set it.
        self.unfold_sidebar = false;
        cx.notify();
    }

    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
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
        let days = (AppState::get(cx).settings.exam_date - Local::now().date_naive()).num_days();
        let countdown = match days {
            2.. => format!("{days} days to Prelims"),
            1 => "Prelims is tomorrow".to_string(),
            0 => "Prelims is today".to_string(),
            _ => "Set your exam date".to_string(),
        };
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
        let screen = self.screen();
        // Notes has panes that scroll on their own; every other screen is one
        // column that scrolls as a whole.
        let content = match screen {
            Screen::Notes => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.notes.clone())
                .into_any_element(),
            screen => {
                let page = match screen {
                    Screen::Settings => self.settings.clone().into_any_element(),
                    Screen::Syllabus => self.syllabus.clone().into_any_element(),
                    screen => screens::placeholder(screen, cx).into_any_element(),
                };
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scrollbar()
                    // One scroll position per screen.
                    .id(("screen", screen as usize))
                    .child(page)
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.title_bar(cx))
            .when_some(store_error, |this, error| {
                this.child(
                    h_flex()
                        .px_4()
                        .py_2()
                        .gap_2()
                        .bg(cx.theme().danger)
                        .text_color(cx.theme().danger_foreground)
                        .text_sm()
                        .child(Icon::new(IconName::TriangleAlert))
                        .child(format!("Your progress is not being saved. {error}")),
                )
            })
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

    let (mut notes, mut stories, mut questions) = (Vec::new(), Vec::new(), Vec::new());
    for hit in AppState::get(cx).library.search(&needle, 30) {
        let (group, screen) = match hit.target {
            Target::Note(_) => (&mut notes, Screen::Notes),
            Target::Story { .. } => (&mut stories, Screen::CurrentAffairs),
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

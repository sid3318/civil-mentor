//! The Settings screen: exam date, daily study time, theme, and where things live.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::date_picker::{DatePicker, DatePickerEvent, DatePickerState};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::store::ThemeChoice;
use crate::ui;

const STEP_MINUTES: u32 = 15;
const MIN_MINUTES: u32 = 15;
const MAX_MINUTES: u32 = 12 * 60;

pub struct SettingsScreen {
    exam_date: Entity<DatePickerState>,
    _exam_date_changed: Subscription,
}

impl SettingsScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let date = AppState::get(cx).settings.exam_date;
        let exam_date = cx.new(|cx| {
            let mut picker = DatePickerState::new(window, cx).date_format("%-d %B %Y");
            picker.set_date(date, window, cx);
            picker
        });
        let changed = cx.subscribe(&exam_date, |_, _, event: &DatePickerEvent, cx| {
            let DatePickerEvent::Change(value) = event;
            // Clearing the picker sends no date; the saved one then stays.
            if let Some(picked) = value.start() {
                AppState::update_settings(cx, |settings| settings.exam_date = picked.date());
            }
        });
        SettingsScreen {
            exam_date,
            _exam_date_changed: changed,
        }
    }
}

/// "1 h 30 min", "45 min", "2 h".
fn duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

/// One setting: its name and what it does on the left, its control on the right.
fn setting(
    title: &'static str,
    help: impl Into<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    h_flex()
        .justify_between()
        .gap_6()
        .px(px(16.))
        .py(px(12.))
        .child(
            v_flex()
                .min_w_0()
                .gap(px(2.))
                .child(div().text_size(px(13.5)).font_medium().child(title))
                .child(ui::meta(help, cx).text_size(px(12.5))),
        )
        .child(div().flex_shrink_0().child(control))
}

/// A titled group of settings in one card, with hairlines between them.
fn group(title: &'static str, rows: Vec<Div>, cx: &App) -> Div {
    let t = ui::tokens(cx);
    v_flex()
        .gap(px(8.))
        .child(div().px_1().text_size(px(14.)).font_medium().child(title))
        .child(
            ui::card(cx).children(
                rows.into_iter().enumerate().map(|(ix, row)| {
                    row.when(ix > 0, |row| row.border_t_1().border_color(t.border))
                }),
            ),
        )
}

impl SettingsScreen {
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let settings = state.settings.clone();
        let library = &state.library;
        let contents = format!(
            "{} topics, {} notes, {} questions, {} daily briefs",
            library.topics.len(),
            library.notes.len(),
            library.questions.len(),
            library.briefs.len()
        );
        let problems = library.problems.clone();
        let demo = state.store.has_demo().unwrap_or(false);
        let data_file = match state.store.path() {
            Some(path) => path.display().to_string(),
            None => "Nothing is being saved in this session".to_string(),
        };

        let study_time = h_flex()
            .gap_1()
            .child(
                Button::new("less-time")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Minus))
                    .accessibility_label("Study less each day")
                    .disabled(settings.daily_minutes <= MIN_MINUTES)
                    .on_click(|_, _, cx| {
                        AppState::update_settings(cx, |settings| {
                            settings.daily_minutes = settings
                                .daily_minutes
                                .saturating_sub(STEP_MINUTES)
                                .max(MIN_MINUTES);
                        });
                    }),
            )
            .child(
                div()
                    .w(px(96.))
                    .text_center()
                    .text_size(px(13.5))
                    .font_medium()
                    .child(duration(settings.daily_minutes)),
            )
            .child(
                Button::new("more-time")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Plus))
                    .accessibility_label("Study more each day")
                    .disabled(settings.daily_minutes >= MAX_MINUTES)
                    .on_click(|_, _, cx| {
                        AppState::update_settings(cx, |settings| {
                            settings.daily_minutes =
                                (settings.daily_minutes + STEP_MINUTES).min(MAX_MINUTES);
                        });
                    }),
            );

        let theme = ui::segmented(cx).children(ThemeChoice::ALL.map(|choice| {
            ui::segment(choice.name(), choice.name(), choice == settings.theme, cx).on_click(
                move |_, _, cx| AppState::update_settings(cx, |settings| settings.theme = choice),
            )
        }));

        let demo_button = if demo {
            Button::new("remove-demo")
                .small()
                .label("Remove demo history")
                .on_click(|_, _, cx| AppState::set_demo(cx, false))
        } else {
            Button::new("add-demo")
                .small()
                .label("Add demo history")
                .on_click(|_, _, cx| AppState::set_demo(cx, true))
        };
        let mut data = vec![
            setting("Content", contents, div(), cx),
            setting("Your progress file", data_file, div(), cx),
            setting(
                "Demo history",
                if demo {
                    "Made-up tests, reading and study time are showing in Today and Analysis. \
                     Removing them leaves your own data as it is."
                } else {
                    "Adds 30 days of made-up tests, reading and study time, so Today and Analysis \
                     have something to show. Each made-up row is marked, so removing them later \
                     leaves your own data as it is."
                },
                demo_button,
                cx,
            ),
        ];
        if !problems.is_empty() {
            data.push(
                v_flex()
                    .gap_1()
                    .px(px(16.))
                    .py(px(12.))
                    .text_size(px(12.5))
                    .text_color(t.danger)
                    .child(format!("{} problems in the content files:", problems.len()))
                    .children(problems.iter().map(|problem| div().child(problem.clone()))),
            );
        }

        ui::column(680.)
            .gap(px(28.))
            .child(ui::meta("These are saved on this computer.", cx).text_size(px(13.5)))
            .child(group(
                "Exam",
                vec![
                    setting(
                        "Prelims date",
                        "The countdown and your daily plan work back from this day.",
                        DatePicker::new(&self.exam_date).w(px(180.)),
                        cx,
                    ),
                    setting(
                        "Daily study time",
                        "Today's plan is sized to fit this.",
                        study_time,
                        cx,
                    ),
                ],
                cx,
            ))
            .child(group(
                "Appearance",
                vec![setting(
                    "Theme",
                    "System follows your computer's light or dark setting.",
                    theme,
                    cx,
                )],
                cx,
            ))
            .child(group("Library and data", data, cx))
    }
}

impl Render for SettingsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = ui::header(cx).child(ui::title(IconName::Settings, "Settings", cx));
        ui::screen(header, ("screen", 0usize), self.body(cx))
    }
}

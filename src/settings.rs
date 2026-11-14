//! The Settings screen: exam date, daily study time, theme, and where things live.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::date_picker::{DatePicker, DatePickerEvent, DatePickerState};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Screen;
use crate::screens::page;
use crate::store::ThemeChoice;

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

fn setting(
    title: &'static str,
    help: impl Into<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    h_flex()
        .justify_between()
        .gap_6()
        .py_2()
        .child(
            v_flex()
                .min_w_0()
                .child(div().font_medium().child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(help.into()),
                ),
        )
        .child(div().flex_shrink_0().child(control))
}

impl Render for SettingsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let data_file = match state.store.path() {
            Some(path) => path.display().to_string(),
            None => "Nothing is being saved in this session".to_string(),
        };

        let study_time = h_flex()
            .gap_2()
            .child(
                Button::new("less-time")
                    .outline()
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
                    .w(px(100.))
                    .text_center()
                    .child(duration(settings.daily_minutes)),
            )
            .child(
                Button::new("more-time")
                    .outline()
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

        let theme = ButtonGroup::new("theme")
            .outline()
            .small()
            .children(ThemeChoice::ALL.map(|choice| {
                Button::new(choice.name())
                    .label(choice.name())
                    .selected(choice == settings.theme)
            }))
            .on_click(|clicked: &Vec<usize>, _, cx| {
                if let Some(choice) = clicked.first().and_then(|&ix| ThemeChoice::ALL.get(ix)) {
                    AppState::update_settings(cx, |settings| settings.theme = *choice);
                }
            });

        page(
            Screen::Settings.title(),
            "These are saved on this computer",
            cx,
        )
        .child(
            GroupBox::new()
                .outline()
                .title("Exam")
                .child(setting(
                    "Prelims date",
                    "The countdown and your daily plan work back from this day.",
                    DatePicker::new(&self.exam_date).w(px(180.)),
                    cx,
                ))
                .child(setting(
                    "Daily study time",
                    "Today's plan is sized to fit this.",
                    study_time,
                    cx,
                )),
        )
        .child(GroupBox::new().outline().title("Appearance").child(setting(
            "Theme",
            "System follows your computer's light or dark setting.",
            theme,
            cx,
        )))
        .child(
            GroupBox::new()
                .outline()
                .title("Library and data")
                .child(setting("Content", contents, div(), cx))
                .child(setting("Your progress file", data_file, div(), cx))
                .when(!problems.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .gap_1()
                            .pt_2()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(format!("{} problems in the content files:", problems.len()))
                            .children(problems.iter().map(|problem| div().child(problem.clone()))),
                    )
                }),
        )
    }
}

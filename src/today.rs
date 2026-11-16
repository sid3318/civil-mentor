//! The Today screen: the date, the days to Prelims and the streak, today's
//! plan, the note in progress, a question of the day, this week's study and
//! the mentor's advice. Until the student takes or declines it, it also
//! offers the first-launch diagnostic.

use chrono::{Datelike as _, Duration, Local, NaiveDate};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::analysis::heat;
use crate::app::{Shell, countdown};
use crate::content::{Target, one_line};
use crate::exam::LETTERS;
use crate::mentor::{self, DRILL_QUESTIONS, Plan, RECENT_TESTS, TaskKind, TestKind, TestPlan};

/// The mentor cards Today shows; Analysis has the rest.
const CARDS: usize = 3;

pub struct TodayScreen {
    shell: WeakEntity<Shell>,
    /// The day whose question of the day has its answer showing.
    answer_shown: Option<NaiveDate>,
}

/// Today's plan: the one saved for the day, or else a new one, saved so
/// that it stays as it is for the rest of the day.
pub(crate) fn todays_plan(cx: &mut App, today: NaiveDate) -> Plan {
    let state = AppState::get(cx);
    match state.store.plan(today) {
        Ok(Some(tasks)) => Plan { day: today, tasks },
        Ok(None) => {
            let history = state.store.answered().unwrap_or_default();
            let plan = mentor::today_plan(
                &state.library,
                &state.reading,
                &history,
                state.settings.daily_minutes,
                today,
            );
            if let Err(err) = state.store.save_plan(today, &plan.tasks) {
                AppState::report(cx, err);
            }
            plan
        }
        Err(err) => {
            AppState::report(cx, err);
            Plan {
                day: today,
                tasks: Vec::new(),
            }
        }
    }
}

/// "No streak yet", "1-day streak", "12-day streak".
pub(crate) fn streak_label(days: u32) -> String {
    match days {
        0 => "No streak yet".to_string(),
        days => format!("{days}-day streak"),
    }
}

fn card(title: impl Into<SharedString>, cx: &App) -> Div {
    v_flex()
        .min_w_0()
        .p_4()
        .gap_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(div().font_semibold().child(title.into()))
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

impl TodayScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        TodayScreen {
            shell,
            answer_shown: None,
        }
    }

    fn sit(&self, plan: TestPlan, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.sit(&plan, cx));
    }

    fn open(&self, target: Target, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.open(target, cx));
    }

    fn diagnostic(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        card("Start with a diagnostic test", cx)
            .id("diagnostic-offer")
            .test_support()
            .bg(cx.theme().accent)
            .child(div().text_sm().child(
                "Twenty questions across the seven subjects, 24 minutes, marked like Prelims. \
                 Your answers show the mentor where you are weak, so your plan can start there.",
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("take-diagnostic")
                            .primary()
                            .small()
                            .label("Take the diagnostic")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Err(err) = AppState::get(cx).store.settle_diagnostic(true) {
                                    AppState::report(cx, err);
                                }
                                let plan = TestPlan::standard(
                                    TestKind::Diagnostic,
                                    &AppState::get(cx).library,
                                );
                                this.sit(plan, cx);
                            })),
                    )
                    .child(
                        Button::new("decline-diagnostic")
                            .ghost()
                            .small()
                            .label("No thanks")
                            .on_click(|_, _, cx| {
                                if let Err(err) = AppState::get(cx).store.settle_diagnostic(false) {
                                    AppState::report(cx, err);
                                }
                                cx.refresh_windows();
                            }),
                    )
                    .child(muted(
                        "It is offered once. Practice has every test whenever you want one.",
                        cx,
                    )),
            )
    }

    fn plan(&self, plan: &Plan, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let library = &state.library;
        let tests = state.store.finished(RECENT_TESTS).unwrap_or_default();
        let done: Vec<bool> = plan
            .tasks
            .iter()
            .map(|task| mentor::task_done(task, plan.day, &state.reading, &tests))
            .collect();
        let title = format!(
            "Today's plan · {} of {} done · {} min",
            done.iter().filter(|&&d| d).count(),
            plan.tasks.len(),
            plan.minutes()
        );
        let day = plan.day;
        let rows = plan.tasks.iter().enumerate().map(|(ix, task)| {
            let finished = done[ix];
            // Done another way (the note marked read, the drill sat): the tick stays.
            let earned = finished && !task.done;
            let kind = TaskKind::parse(&task.kind);
            let note = library.notes.iter().position(|note| note.id == task.target);
            let drill = mentor::task_drill(library, task);
            h_flex()
                .gap_3()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    Checkbox::new(("task-done", ix))
                        .checked(finished)
                        .disabled(earned)
                        .on_click(move |checked, _, cx| {
                            if let Err(err) =
                                AppState::get(cx).store.set_task_done(day, ix, *checked)
                            {
                                AppState::report(cx, err);
                            }
                            cx.refresh_windows();
                        }),
                )
                .child(
                    div()
                        .id(("task", ix))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .cursor_pointer()
                        .hover(|this| this.underline())
                        .when(finished, |this| {
                            this.line_through().text_color(cx.theme().muted_foreground)
                        })
                        .child(task.title.clone())
                        .on_click(cx.listener(move |this, _, _, cx| match kind {
                            Some(TaskKind::Note) => {
                                if let Some(note) = note {
                                    this.open(Target::Note(note), cx);
                                }
                            }
                            Some(TaskKind::Drill) => this.sit(drill.clone(), cx),
                            None => {}
                        })),
                )
                .child(muted(format!("{} min", task.minutes), cx))
        });
        card(title, cx)
            .id("plan")
            .test_support()
            .flex_1()
            .children(rows)
            .when(plan.tasks.is_empty(), |this| {
                this.child(muted("Nothing to plan: every note is read and no topic has enough answers for a drill yet.", cx))
            })
            .child(muted(
                "Made the first time you open the app each day, from what you have read and how your tests went.",
                cx,
            ))
    }

    fn continue_reading(&self, cx: &mut Context<Self>) -> Div {
        let state = AppState::get(cx);
        let library = &state.library;
        let card = card("Continue reading", cx).flex_1();
        let Some(ix) = mentor::in_progress(library, &state.reading) else {
            return card.child(muted(
                "Nothing in progress. Open a note from Notes and it waits here.",
                cx,
            ));
        };
        let note = &library.notes[ix];
        let seconds = state.reading.get(&note.id).map_or(0, |r| r.seconds);
        let share = (mentor::percent(seconds as usize, note.minutes as usize * 60)).min(99);
        let left = note.minutes.saturating_sub(seconds / 60).max(1);
        card.child(div().font_medium().child(note.title.clone()))
            .child(muted(format!("{share}% · {left} min left"), cx))
            .child(
                h_flex().child(
                    Button::new("continue-reading")
                        .outline()
                        .small()
                        .label("Continue")
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open(Target::Note(ix), cx)),
                        ),
                ),
            )
    }

    fn question(&self, today: NaiveDate, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let library = &AppState::get(cx).library;
        let card = card("Question of the day", cx)
            .id("question-of-the-day")
            .test_support()
            .flex_1();
        let Some(ix) = mentor::question_of_the_day(library, today) else {
            return card.child(muted("The question bank is empty.", cx));
        };
        let question = &library.questions[ix];
        let shown = self.answer_shown == Some(today);
        let theme = cx.theme();
        card.child(div().text_sm().child(question.stem.clone()))
            .children(question.options.iter().enumerate().map(|(option, text)| {
                let right = shown && question.answer == Some(option);
                h_flex()
                    .gap_2()
                    .text_sm()
                    .px_1()
                    .rounded(theme.radius)
                    .when(right, |this| {
                        this.bg(theme.success.opacity(0.12))
                            .text_color(theme.success)
                            .font_medium()
                    })
                    .child(
                        div()
                            .w(px(24.))
                            .flex_shrink_0()
                            .child(format!("({})", LETTERS[option])),
                    )
                    .child(div().flex_1().min_w_0().child(text.clone()))
            }))
            .when(shown, |this| {
                this.child(muted(one_line(&question.explanation, 240), cx))
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("qotd-answer")
                            .outline()
                            .small()
                            .label(if shown {
                                "Hide the answer"
                            } else {
                                "Show the answer"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.answer_shown = if shown { None } else { Some(today) };
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("qotd-open")
                            .ghost()
                            .small()
                            .label("Open")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open(Target::Question(ix), cx)
                            })),
                    ),
            )
    }

    fn mentor(&self, today: NaiveDate, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let library = &state.library;
        let tests = state.store.finished(RECENT_TESTS).unwrap_or_default();
        let history = state.store.answered().unwrap_or_default();
        let study = state.store.study_days().unwrap_or_default();
        let insights = mentor::insights(
            library,
            &state.reading,
            &history,
            &tests,
            &study,
            state.settings.daily_minutes,
            today,
        );
        let cards = insights.cards(library);
        let theme = cx.theme();
        let advice = cards.into_iter().take(CARDS).enumerate().map(|(ix, card)| {
            let note = card
                .topic
                .as_deref()
                .and_then(|t| mentor::note_for_topic(library, t));
            let drill = card
                .topic
                .as_deref()
                .map(|t| mentor::topic_drill(library, t, DRILL_QUESTIONS))
                .filter(|plan| plan.count > 0);
            v_flex()
                .id(("insight", ix))
                .test_support()
                .gap_2()
                .py_2()
                .when(ix > 0, |this| this.border_t_1().border_color(theme.border))
                .child(div().text_sm().child(card.text))
                .when(note.is_some() || drill.is_some(), |this| {
                    this.child(
                        h_flex()
                            .gap_2()
                            .when_some(note, |this, note| {
                                this.child(
                                    Button::new(("insight-note", ix))
                                        .outline()
                                        .xsmall()
                                        .label("Open note")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open(Target::Note(note), cx)
                                        })),
                                )
                            })
                            .when_some(drill, |this, drill| {
                                this.child(
                                    Button::new(("insight-drill", ix))
                                        .outline()
                                        .xsmall()
                                        .label("Drill")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.sit(drill.clone(), cx)
                                        })),
                                )
                            }),
                    )
                })
        });
        let advice: Vec<_> = advice.collect();
        card("Mentor", cx)
            .id("mentor")
            .test_support()
            .when(advice.is_empty(), |this| {
                this.child(muted(
                    "Take a test or two and the mentor's advice appears here.",
                    cx,
                ))
            })
            .children(advice)
    }

    fn week(&self, today: NaiveDate, cx: &App) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let study = state.store.study_days().unwrap_or_default();
        let goal = state.settings.daily_minutes * 60;
        let monday = today - Duration::days(today.weekday().num_days_from_monday().into());
        let mut total = 0;
        let days =
            ["M", "T", "W", "T", "F", "S", "S"]
                .into_iter()
                .enumerate()
                .map(|(n, label)| {
                    let day = monday + Duration::days(n as i64);
                    let seconds = study.get(&day).copied().unwrap_or(0);
                    total += seconds;
                    let minutes = seconds / 60;
                    v_flex()
                        .flex_1()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .child(
                            div()
                                .when(day == today, |this| this.font_semibold())
                                .text_color(cx.theme().muted_foreground)
                                .child(label),
                        )
                        .child(
                            div()
                                .size(px(28.))
                                .rounded(px(4.))
                                .when(day <= today, |this| this.bg(heat(seconds, goal, cx)))
                                .when(day > today, |this| {
                                    this.border_1().border_color(cx.theme().border)
                                }),
                        )
                        .child(div().text_color(cx.theme().muted_foreground).child(
                            if minutes > 0 {
                                format!("{minutes}m")
                            } else {
                                " ".into()
                            },
                        ))
                })
                .collect::<Vec<_>>();
        card("This week", cx)
            .id("this-week")
            .test_support()
            .child(h_flex().gap_1().children(days))
            .child(muted(
                format!(
                    "{} so far, against {} a day",
                    mentor::hours(total),
                    mentor::hours(goal)
                ),
                cx,
            ))
    }
}

impl Render for TodayScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = Local::now().date_naive();
        let plan = todays_plan(cx, today);
        let state = AppState::get(cx);
        let days = (state.settings.exam_date - today).num_days();
        let study = state.store.study_days().unwrap_or_default();
        let streak = mentor::streak(&study, today);
        let offer =
            !state.store.diagnostic_settled() && !state.store.has_own_tests().unwrap_or(true);

        v_flex()
            .w_full()
            .max_w(px(1080.))
            .mx_auto()
            .p_8()
            .gap_5()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_2xl()
                            .font_semibold()
                            .child(today.format("%A, %-d %B").to_string()),
                    )
                    .child(
                        div()
                            .id("today-summary")
                            .test_support()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} · {}", countdown(days), streak_label(streak))),
                    ),
            )
            .when(offer, |this| this.child(self.diagnostic(cx)))
            .child(
                h_flex()
                    .items_start()
                    .gap_5()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_5()
                            .child(self.plan(&plan, cx))
                            .child(
                                h_flex()
                                    .items_start()
                                    .gap_5()
                                    .child(self.continue_reading(cx))
                                    .child(self.question(today, cx)),
                            ),
                    )
                    .child(
                        v_flex()
                            .w(px(330.))
                            .flex_shrink_0()
                            .gap_5()
                            .child(self.mentor(today, cx))
                            .child(self.week(today, cx)),
                    ),
            )
            .child(div().h(px(24.)))
    }
}

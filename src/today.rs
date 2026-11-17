//! The Today screen: the date, the days to Prelims and the streak, today's
//! plan, the note in progress, a question of the day, this week's study and
//! the mentor's advice. Until the student takes or declines it, it also
//! offers the first-launch diagnostic. Laid out as in DESIGN.md, "Today".

use chrono::{Datelike as _, Duration, Local, NaiveDate, Timelike as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell, countdown};
use crate::content::{Target, one_line};
use crate::current_affairs::{quiz_score, sittings};
use crate::exam::LETTERS;
use crate::mentor::{
    self, DRILL_QUESTIONS, Insight, Plan, RECENT_TESTS, TaskKind, TestKind, TestPlan,
};
use crate::ui;

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
                &state.reviews,
                &state.mains,
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

/// How many of today's tasks are done, out of how many, once today's plan
/// has been made. Does not make the plan.
pub(crate) fn plan_progress(cx: &App, today: NaiveDate) -> Option<(usize, usize)> {
    let state = AppState::get(cx);
    let tasks = state.store.plan(today).ok().flatten()?;
    let tests = state.store.finished(RECENT_TESTS).unwrap_or_default();
    let done = tasks
        .iter()
        .filter(|task| {
            mentor::task_done(
                &state.library,
                task,
                today,
                &state.reading,
                &tests,
                &state.reviews,
                &state.mains,
            )
        })
        .count();
    Some((done, tasks.len()))
}

/// "No streak yet", "1-day streak", "12-day streak".
pub(crate) fn streak_label(days: u32) -> String {
    match days {
        0 => "No streak yet".to_string(),
        days => format!("{days}-day streak"),
    }
}

/// Splits a mentor insight into a headline and what follows: at the first
/// colon when it comes before the first full stop, else at that stop.
fn headline(text: &str) -> (String, String) {
    let colon = text.find(": ");
    let stop = text.find(". ");
    match (colon, stop) {
        (Some(c), s) if s.is_none_or(|s| c < s) => (text[..c].into(), text[c + 2..].into()),
        (_, Some(s)) => (text[..s].into(), text[s + 2..].into()),
        _ => (text.trim_end_matches('.').into(), String::new()),
    }
}

/// What a task in the plan opens.
#[derive(Clone)]
enum Go {
    /// The brief, by index into the library.
    Brief(Option<usize>),
    Review,
    Note(Option<usize>),
    Drill(TestPlan),
    /// The Mains question, by index into the library.
    Mains(Option<usize>),
    Nothing,
}

/// One task of the plan, ready to draw.
struct Row {
    title: String,
    meta: String,
    minutes: u32,
    icon: IconName,
    color: Hsla,
    finished: bool,
    /// Done another way (the note marked read, the drill sat): the tick stays.
    earned: bool,
    go: Go,
    /// The brief's quiz, and the last time it was sat.
    quiz: Option<(TestPlan, Option<crate::store::Attempt>)>,
    /// What the row's button says, when it has one.
    action: Option<&'static str>,
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

    fn go(&self, go: &Go, cx: &mut App) {
        match go {
            Go::Brief(Some(ix)) => {
                let ix = *ix;
                let _ = self.shell.update(cx, |shell, cx| shell.open_brief(ix, cx));
            }
            Go::Review => {
                let _ = self.shell.update(cx, |shell, cx| shell.start_review(cx));
            }
            Go::Note(Some(note)) => self.open(Target::Note(*note), cx),
            Go::Drill(plan) => self.sit(plan.clone(), cx),
            Go::Mains(Some(question)) => {
                let question = *question;
                let _ = self
                    .shell
                    .update(cx, |shell, cx| shell.open_mains(question, cx));
            }
            Go::Brief(None) | Go::Note(None) | Go::Mains(None) | Go::Nothing => {}
        }
    }

    /// Each task of the plan with what it opens and whether it is done.
    fn rows(&self, plan: &Plan, cx: &App) -> Vec<Row> {
        let state = AppState::get(cx);
        let library = &state.library;
        let t = ui::tokens(cx);
        let tests = state.store.finished(RECENT_TESTS).unwrap_or_default();
        let current = ui::subject_id_color("current", cx);
        plan.tasks
            .iter()
            .map(|task| {
                let finished = mentor::task_done(
                    library,
                    task,
                    plan.day,
                    &state.reading,
                    &tests,
                    &state.reviews,
                    &state.mains,
                );
                let (icon, color, meta, go, action, quiz) = match TaskKind::parse(&task.kind) {
                    Some(TaskKind::Brief) => {
                        let date: Option<NaiveDate> = task.target.parse().ok();
                        let quiz = date.map(|date| {
                            let kind = TestKind::Quiz(date);
                            let last = sittings(&kind, cx).into_iter().next();
                            (TestPlan::standard(kind, library), last)
                        });
                        let brief = date.and_then(|date| library.brief_on(date));
                        (
                            IconName::Newspaper,
                            current,
                            "Current affairs".to_string(),
                            Go::Brief(brief),
                            None,
                            quiz,
                        )
                    }
                    Some(TaskKind::Review) => (
                        IconName::Repeat,
                        t.accent,
                        "Revision · due today".to_string(),
                        Go::Review,
                        Some("Review"),
                        None,
                    ),
                    Some(TaskKind::Note) => {
                        let note = library.notes.iter().position(|note| note.id == task.target);
                        let topic = note
                            .and_then(|ix| library.notes[ix].topics.first().cloned())
                            .unwrap_or_default();
                        (
                            IconName::BookOpen,
                            ui::subject_color(&topic, cx),
                            library.subject_label(&topic),
                            Go::Note(note),
                            Some("Read"),
                            None,
                        )
                    }
                    Some(TaskKind::Drill) => (
                        IconName::SquarePen,
                        ui::subject_color(&task.target, cx),
                        format!(
                            "{} · your weakest topic",
                            mentor::subject_of_topic(&task.target)
                                .map_or("Prelims", |subject| subject.name)
                        ),
                        Go::Drill(mentor::task_drill(library, task)),
                        Some("Start"),
                        None,
                    ),
                    Some(TaskKind::Mains) => {
                        let question = library.mains_ix(&task.target);
                        let topic = question
                            .and_then(|ix| library.mains[ix].topics.first().cloned())
                            .unwrap_or_default();
                        (
                            IconName::PenLine,
                            ui::subject_color(&topic, cx),
                            "Mains answer writing".to_string(),
                            Go::Mains(question),
                            Some("Write"),
                            None,
                        )
                    }
                    None => (
                        IconName::Circle,
                        t.text_3,
                        String::new(),
                        Go::Nothing,
                        None,
                        None,
                    ),
                };
                Row {
                    title: task.title.clone(),
                    meta,
                    minutes: task.minutes,
                    icon,
                    color,
                    finished,
                    earned: finished && !task.done,
                    go,
                    quiz,
                    action,
                }
            })
            .collect()
    }

    /// The date and greeting, the streak, the countdown and the day's study,
    /// with the plan's progress as a ring.
    fn hero(&self, today: NaiveDate, rows: &[Row], cx: &App) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let study = state.store.study_days().unwrap_or_default();
        let streak = mentor::streak(&study, today);
        let days = (state.settings.exam_date - today).num_days();
        let studied = study.get(&today).copied().unwrap_or(0);
        let goal = state.settings.daily_minutes * 60;
        let greeting = match Local::now().hour() {
            0..12 => "Good morning",
            12..17 => "Good afternoon",
            _ => "Good evening",
        };
        let done = rows.iter().filter(|row| row.finished).count();
        let left: u32 = rows
            .iter()
            .filter(|row| !row.finished)
            .map(|row| row.minutes)
            .sum();
        let share = if rows.is_empty() {
            0.
        } else {
            done as f32 / rows.len() as f32
        };
        let chip = |icon: IconName, color: Hsla, text: String| {
            ui::chip(cx)
                .child(Icon::new(icon).size(px(13.)).text_color(color))
                .child(text)
        };
        h_flex()
            .justify_between()
            .gap_6()
            .pt_1()
            .pb_2()
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        ui::caption(today.format("%A, %-d %B").to_string(), cx).text_size(px(12.)),
                    )
                    .child(
                        div()
                            .font_family(ui::DISPLAY)
                            .text_size(px(28.))
                            .line_height(px(34.))
                            .font_semibold()
                            .child(greeting),
                    )
                    .child(
                        h_flex()
                            .id("today-summary")
                            .test_support()
                            .flex_wrap()
                            .gap(px(6.))
                            .pt(px(10.))
                            .child(chip(IconName::Flame, t.attention, streak_label(streak)))
                            .child(chip(IconName::CalendarDays, t.text_3, countdown(days)))
                            .child(chip(
                                IconName::Clock,
                                t.text_3,
                                format!(
                                    "{} of {} today",
                                    mentor::hours(studied),
                                    mentor::hours(goal)
                                ),
                            )),
                    ),
            )
            .when(!rows.is_empty(), |this| {
                this.child(
                    h_flex()
                        .gap(px(14.))
                        .pr_1()
                        .child(ui::ring(
                            share,
                            92.,
                            8.,
                            t.accent,
                            div()
                                .text_size(px(20.))
                                .font_semibold()
                                .child(format!("{}%", (share * 100.).round())),
                            cx,
                        ))
                        .child(
                            v_flex()
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(t.text_3)
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_medium()
                                        .text_color(t.text)
                                        .child("Today's plan"),
                                )
                                .child(format!("{done} of {} tasks done", rows.len()))
                                .child(if left > 0 {
                                    format!("about {left} min to go")
                                } else {
                                    "all done for today".to_string()
                                }),
                        ),
                )
            })
    }

    fn diagnostic(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        ui::card(cx)
            .id("diagnostic-offer")
            .test_support()
            .relative()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(ui::dot_field(t.accent)),
            )
            .child(
                v_flex()
                    .p_4()
                    .gap(px(10.))
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .child(icon_tile(IconName::Target, t.accent))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_semibold()
                                    .child("Start with a diagnostic test"),
                            ),
                    )
                    .child(
                        div()
                            .max_w(px(640.))
                            .text_size(px(13.5))
                            .line_height(px(20.))
                            .text_color(t.text_2)
                            .child(
                                "Twenty questions across the seven subjects, 24 minutes, marked \
                                 like Prelims. Your answers show the mentor where you are weak, \
                                 so your plan can start there.",
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("take-diagnostic")
                                    .primary()
                                    .small()
                                    .label("Take the diagnostic")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Err(err) =
                                            AppState::get(cx).store.settle_diagnostic(true)
                                        {
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
                                        if let Err(err) =
                                            AppState::get(cx).store.settle_diagnostic(false)
                                        {
                                            AppState::report(cx, err);
                                        }
                                        cx.refresh_windows();
                                    }),
                            )
                            .child(ui::meta(
                                "Offered once. Practice has every test whenever you want one.",
                                cx,
                            )),
                    ),
            )
    }

    fn plan(&self, plan: &Plan, rows: &[Row], cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let day = plan.day;
        let done = rows.iter().filter(|row| row.finished).count();
        let segments = rows.iter().map(|row| {
            div().flex_1().h(px(4.)).rounded_full().bg(if row.finished {
                t.accent
            } else {
                t.selected
            })
        });
        let lines: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(ix, row)| {
                let go = row.go.clone();
                let action = row.quiz.clone().map(|(plan, last)| match last {
                    Some(test) => ui::meta(format!("Quiz {}", quiz_score(&test)), cx)
                        .id(("brief-quiz-score", ix))
                        .test_support()
                        .into_any_element(),
                    None => Button::new(("brief-quiz", ix))
                        .small()
                        .label("Take quiz")
                        .disabled(!plan.fits(library))
                        .on_click(cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx)))
                        .into_any_element(),
                });
                let action = action.or_else(|| {
                    row.action.filter(|_| !row.finished).map(|label| {
                        let go = go.clone();
                        Button::new(("task-go", ix))
                            .small()
                            .label(label)
                            .on_click(cx.listener(move |this, _, _, cx| this.go(&go, cx)))
                            .into_any_element()
                    })
                });
                h_flex()
                    .gap(px(12.))
                    .h(px(48.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(t.border)
                    .child(
                        Checkbox::new(("task-done", ix))
                            .checked(row.finished)
                            .disabled(row.earned)
                            .on_click(move |checked, _, cx| {
                                if let Err(err) =
                                    AppState::get(cx).store.set_task_done(day, ix, *checked)
                                {
                                    AppState::report(cx, err);
                                }
                                cx.refresh_windows();
                            }),
                    )
                    .child(icon_tile(
                        row.icon,
                        if row.finished { t.text_4 } else { row.color },
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .id(("task", ix))
                                    .test_support()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_medium()
                                    .cursor_pointer()
                                    .hover(|this| this.text_color(t.accent))
                                    .when(row.finished, |this| {
                                        this.line_through().text_color(t.text_3)
                                    })
                                    .child(row.title.clone())
                                    .on_click(cx.listener(move |this, _, _, cx| this.go(&go, cx))),
                            )
                            .child(ui::meta(row.meta.clone(), cx).truncate()),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .flex_shrink_0()
                            .text_right()
                            .text_size(px(12.))
                            .text_color(t.text_3)
                            .child(format!("{} min", row.minutes)),
                    )
                    .child(
                        h_flex()
                            .w(px(92.))
                            .flex_shrink_0()
                            .justify_end()
                            .children(action),
                    )
            })
            .collect();
        ui::card(cx)
            .id("plan")
            .test_support()
            .flex_1()
            .child(
                ui::card_head("Today's plan", cx)
                    .child(ui::meta(
                        format!("{done} of {} done · {} min", rows.len(), plan.minutes()),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(h_flex().w(px(130.)).gap(px(3.)).children(segments)),
            )
            .children(lines)
            .when(rows.is_empty(), |this| {
                this.child(
                    ui::meta(
                        "Nothing to plan: the brief and every note are read, and no topic has \
                         enough answers for a drill yet.",
                        cx,
                    )
                    .px(px(14.))
                    .pb(px(12.)),
                )
            })
            .child(
                ui::meta(
                    "Made the first time you open the app each day, from what you have read and \
                     how your tests went.",
                    cx,
                )
                .text_size(px(11.5))
                .px(px(14.))
                .py(px(10.))
                .border_t_1()
                .border_color(t.border),
            )
    }

    fn continue_reading(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let library = &state.library;
        let t = ui::tokens(cx);
        let card = ui::card(cx)
            .flex_1()
            .child(ui::card_head("Continue reading", cx));
        let Some(ix) = mentor::in_progress(library, &state.reading) else {
            return card.child(
                ui::meta(
                    "Nothing in progress. Open a note from Notes and it waits here.",
                    cx,
                )
                .px(px(14.))
                .pb(px(14.)),
            );
        };
        let note = &library.notes[ix];
        let seconds = state.reading.get(&note.id).map_or(0, |r| r.seconds);
        let share = (mentor::percent(seconds as usize, note.minutes as usize * 60)).min(99);
        let left = note.minutes.saturating_sub(seconds / 60).max(1);
        let topic = note.topics.first().cloned().unwrap_or_default();
        card.child(
            v_flex()
                .px(px(14.))
                .pb(px(14.))
                .gap(px(10.))
                .items_start()
                .child(ui::subject_chip(
                    library.subject_label(&topic),
                    ui::subject_color(&topic, cx),
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(20.))
                        .font_semibold()
                        .child(note.title.clone()),
                )
                .child(ui::progress(share as f32 / 100., t.accent, cx))
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .child(ui::meta(format!("{share}% read · {left} min left"), cx))
                        .child(
                            Button::new("continue-reading")
                                .small()
                                .label("Continue")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open(Target::Note(ix), cx)
                                })),
                        ),
                ),
        )
    }

    fn question(&self, today: NaiveDate, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let library = &AppState::get(cx).library;
        let t = ui::tokens(cx);
        let card = ui::card(cx)
            .id("question-of-the-day")
            .test_support()
            .flex_1();
        let Some(ix) = mentor::question_of_the_day(library, today) else {
            return card.child(ui::card_head("Question of the day", cx)).child(
                ui::meta("The question bank is empty.", cx)
                    .px(px(14.))
                    .pb(px(14.)),
            );
        };
        let question = &library.questions[ix];
        let shown = self.answer_shown == Some(today);
        let topic = question.topics.first().cloned().unwrap_or_default();
        let subject = mentor::subject_of_topic(&topic).map_or("Prelims", |subject| subject.name);
        let option = |option: usize, text: &String| {
            let right = shown && question.answer == Some(option);
            h_flex()
                .flex_1()
                .min_w_0()
                .min_h(px(30.))
                .py_1()
                .px(px(8.))
                .gap(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(if right { t.success } else { t.border_2 })
                .when(right, |this| this.bg(t.success.opacity(0.1)))
                .text_size(px(12.5))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(18.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.5))
                        .font_semibold()
                        .bg(if right { t.success } else { t.sunken })
                        .text_color(if right { t.on_accent } else { t.text_2 })
                        .child(LETTERS[option]),
                )
                .child(
                    div()
                        .min_w_0()
                        .when(right, |this| this.text_color(t.success).font_medium())
                        .child(text.clone()),
                )
        };
        let mut options = question
            .options
            .iter()
            .enumerate()
            .map(|(ix, text)| option(ix, text));
        let mut pairs = Vec::new();
        while let Some(first) = options.next() {
            pairs.push(h_flex().gap(px(6.)).child(first).children(options.next()));
        }
        card.child(ui::card_head("Question of the day", cx))
            .child(h_flex().px(px(14.)).pb(px(8.)).child(ui::subject_chip(
                subject,
                ui::subject_color(&topic, cx),
                cx,
            )))
            .child(
                div()
                    .px(px(14.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .child(TextView::markdown("qotd-stem", question.stem.clone())),
            )
            .child(v_flex().px(px(14.)).gap(px(6.)).children(pairs))
            .when(shown, |this| {
                this.child(
                    ui::meta(one_line(&question.explanation, 240), cx)
                        .px(px(14.))
                        .pt(px(10.)),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .px(px(14.))
                    .pt(px(12.))
                    .pb(px(14.))
                    .child(
                        Button::new("qotd-answer")
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
        let t = ui::tokens(cx);
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
        let cards: Vec<mentor::Card> = insights.cards(library).into_iter().take(CARDS).collect();
        let count = cards.len();
        let advice: Vec<_> = cards
            .into_iter()
            .enumerate()
            .map(|(ix, card)| {
                let note = card
                    .topic
                    .as_deref()
                    .and_then(|t| mentor::note_for_topic(library, t));
                let drill = card
                    .topic
                    .as_deref()
                    .map(|t| mentor::topic_drill(library, t, DRILL_QUESTIONS))
                    .filter(|plan| plan.count > 0);
                let (icon, color) = match card.kind {
                    Insight::Weak => (IconName::TrendingDown, t.danger),
                    Insight::Guesses => (IconName::Dices, t.attention),
                    Insight::Attempts => (IconName::TrendingUp, t.success),
                    Insight::Pace => (IconName::Timer, t.accent),
                    Insight::Week => (IconName::Clock, t.accent),
                    Insight::HighYield => (IconName::Lightbulb, t.attention),
                };
                let (head, rest) = headline(&card.text);
                h_flex()
                    .id(("insight", ix))
                    .test_support()
                    .items_start()
                    .gap(px(11.))
                    .px(px(14.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(t.border)
                    .child(icon_tile(icon, color))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .font_medium()
                                    .child(head),
                            )
                            .when(!rest.is_empty(), |this| this.child(ui::meta(rest, cx)))
                            .when(note.is_some() || drill.is_some(), |this| {
                                this.child(
                                    h_flex()
                                        .gap(px(6.))
                                        .pt(px(8.))
                                        .when_some(note, |this, note| {
                                            this.child(
                                                Button::new(("insight-note", ix))
                                                    .xsmall()
                                                    .label("Open note")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.open(Target::Note(note), cx)
                                                        },
                                                    )),
                                            )
                                        })
                                        .when_some(drill, |this, drill| {
                                            this.child(
                                                Button::new(("insight-drill", ix))
                                                    .primary()
                                                    .outline()
                                                    .xsmall()
                                                    .label(format!("Drill {} Q", drill.count))
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.sit(drill.clone(), cx)
                                                        },
                                                    )),
                                            )
                                        }),
                                )
                            }),
                    )
            })
            .collect();
        ui::card(cx)
            .id("mentor")
            .test_support()
            .child(ui::card_head("Mentor", cx).when(count > 0, |this| {
                this.child(ui::meta(
                    if count == 1 {
                        "1 insight".to_string()
                    } else {
                        format!("{count} insights")
                    },
                    cx,
                ))
            }))
            .when(advice.is_empty(), |this| {
                this.child(
                    ui::meta(
                        "Take a test or two and the mentor's advice appears here.",
                        cx,
                    )
                    .px(px(14.))
                    .pb(px(14.)),
                )
            })
            .children(advice)
    }

    fn week(&self, today: NaiveDate, cx: &App) -> impl IntoElement + use<> {
        // The goal line sits this high; a day's bar reaches it at the goal.
        const GOAL: f32 = 64.;
        const TALLEST: f32 = 84.;
        let state = AppState::get(cx);
        let t = ui::tokens(cx);
        let study = state.store.study_days().unwrap_or_default();
        let goal = state.settings.daily_minutes * 60;
        let monday = today - Duration::days(today.weekday().num_days_from_monday().into());
        let days: Vec<(NaiveDate, u32)> = (0..7)
            .map(|n| {
                let day = monday + Duration::days(n);
                (day, study.get(&day).copied().unwrap_or(0))
            })
            .collect();
        let total: u32 = days.iter().map(|(_, seconds)| seconds).sum();
        let bars = days.iter().map(|&(day, seconds)| {
            let height = (seconds as f32 / goal.max(1) as f32 * GOAL).min(TALLEST);
            let color = if day > today || seconds == 0 {
                t.selected
            } else if day == today {
                t.accent.opacity(0.55)
            } else {
                t.accent
            };
            div()
                .flex_1()
                .h(px(height.max(4.)))
                .rounded(px(4.))
                .bg(color)
        });
        let labels = ["M", "T", "W", "T", "F", "S", "S"]
            .into_iter()
            .zip(&days)
            .map(|(label, &(day, _))| {
                div()
                    .flex_1()
                    .text_center()
                    .when(day == today, |this| this.font_semibold().text_color(t.text))
                    .child(label)
            });
        ui::card(cx)
            .id("this-week")
            .test_support()
            .flex_1()
            .child(
                ui::card_head("This week", cx)
                    .child(div().flex_1())
                    .child(ui::meta(mentor::hours(total), cx)),
            )
            .child(
                h_flex()
                    .relative()
                    .items_end()
                    .gap(px(9.))
                    .h(px(TALLEST))
                    .mx(px(14.))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(GOAL))
                            .border_t_1()
                            .border_dashed()
                            .border_color(t.border_3),
                    )
                    .children(bars),
            )
            .child(
                h_flex()
                    .gap(px(9.))
                    .mx(px(14.))
                    .pt(px(6.))
                    .text_size(px(11.))
                    .text_color(t.text_3)
                    .children(labels),
            )
            .child(
                ui::meta(format!("Goal {} a day", mentor::hours(goal)), cx)
                    .px(px(14.))
                    .pt(px(8.))
                    .pb(px(14.)),
            )
    }
}

/// A 26 px rounded square tinted with `color`, with an icon in it.
fn icon_tile(icon: IconName, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .size(px(26.))
        .rounded(px(7.))
        .flex()
        .items_center()
        .justify_center()
        .bg(color.opacity(0.14))
        .child(Icon::new(icon).size(px(14.)).text_color(color))
}

impl Render for TodayScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = AppState::today(cx);
        let plan = todays_plan(cx, today);
        let rows = self.rows(&plan, cx);
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let offer =
            !state.store.diagnostic_settled() && !state.store.has_own_tests().unwrap_or(true);
        let next = rows
            .iter()
            .find(|row| !row.finished)
            .map(|row| row.go.clone());

        let header = ui::header(cx).child(
            h_flex()
                .flex_1()
                .gap_1()
                .child(ui::title(IconName::Sun, "Today", cx))
                .child(div().flex_1())
                .child(
                    Button::new("plan-settings")
                        .ghost()
                        .small()
                        .icon(Icon::new(IconName::SlidersHorizontal))
                        .label("Plan settings")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let _ = this
                                .shell
                                .update(cx, |shell, cx| shell.go(Screen::Settings, cx));
                        })),
                )
                .when_some(next, |this, next| {
                    this.child(
                        Button::new("start-next")
                            .primary()
                            .small()
                            .icon(Icon::new(IconName::Play))
                            .label("Start next task")
                            .on_click(cx.listener(move |this, _, _, cx| this.go(&next, cx))),
                    )
                }),
        );
        let body = ui::column(1040.)
            .relative()
            .gap(px(14.))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(200.))
                    .child(ui::dot_field(t.accent)),
            )
            .child(self.hero(today, &rows, cx))
            .when(offer, |this| this.child(self.diagnostic(cx)))
            .child(
                h_flex()
                    .items_start()
                    .gap(px(14.))
                    .child(self.plan(&plan, &rows, cx))
                    .child(
                        div()
                            .w(px(330.))
                            .flex_shrink_0()
                            .child(self.mentor(today, cx)),
                    ),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap(px(14.))
                    .child(self.continue_reading(cx))
                    .child(self.question(today, cx))
                    .child(self.week(today, cx)),
            );
        ui::screen(header, ("screen", 6usize), body)
    }
}

#[cfg(test)]
mod tests {
    use super::headline;

    #[test]
    fn an_insight_splits_into_a_headline_and_the_rest() {
        assert_eq!(
            headline("Money and banking: 33% right over 18 questions."),
            (
                "Money and banking".to_string(),
                "33% right over 18 questions.".to_string()
            )
        );
        assert_eq!(
            headline("Guesses cost you 0.7 marks per test. Skip unless you can rule out two."),
            (
                "Guesses cost you 0.7 marks per test".to_string(),
                "Skip unless you can rule out two.".to_string()
            )
        );
        assert_eq!(
            headline("You studied 3 h in the last 7 days, 30% of your goal."),
            (
                "You studied 3 h in the last 7 days, 30% of your goal".to_string(),
                String::new()
            )
        );
    }
}

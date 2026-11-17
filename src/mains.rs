//! Mains: answer writing against the clock. The questions by paper and
//! topic, each with a model answer and the points a good answer covers; a
//! timed editor that saves every keystroke; self-evaluation on seven
//! dimensions; and the history of every answer with the trend on each
//! dimension.

use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, Timelike as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::chart::LineChart;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{InputEvent, Position, Textarea, TextareaState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::content::{MAINS_PAPERS, MainsQuestion, one_line, word_count};
use crate::exam::clock;
use crate::mentor::{self, DIMENSIONS, MAINS_SCORING_MINUTES};
use crate::notes::IDLE;
use crate::screens::{count, page};
use crate::store::{MainsAnswer, SCORES, Study};

/// The tabs of the list.
const TABS: [&str; 2] = ["Questions", "History"];
/// The clock turns red with this much time left.
const LOW_TIME: i64 = 60;
/// The widest the editor and the scoring sheet grow.
const WIDE: f32 = 1080.;

/// Which questions the list shows by whether they have been answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    All,
    Unanswered,
    Answered,
}

/// What the screen shows.
enum View {
    /// The questions or the history, as the tab says.
    List,
    /// One question, by its place in the library. Its model answer shows
    /// once the student has answered it or asked to see it.
    Question {
        ix: usize,
        model_shown: bool,
    },
    Writing(Box<Writing>),
    /// The scoring sheet of a submitted answer, by its id.
    Scoring(i64),
}

/// An answer being written.
struct Writing {
    answer: MainsAnswer,
    question: usize,
    editor: Entity<TextareaState>,
    /// Up to when the clock has been counted.
    counted: Instant,
    /// The last keystroke. Once time is up the clock runs only while the
    /// student is typing, so an editor left open does not count for hours.
    typed: Instant,
    /// The block of study time the writing is counted in.
    block: Option<i64>,
    /// Ticks once a second while the editor is on screen.
    clock: Option<Task<()>>,
    _changed: Subscription,
}

pub struct MainsScreen {
    tab: usize,
    /// The paper the list is narrowed to, if any.
    paper: Option<&'static str>,
    status: Status,
    view: View,
}

/// The local time to the second on the app's day, as the store keeps it.
fn now(cx: &App) -> NaiveDateTime {
    let now = Local::now().naive_local() + chrono::Duration::days(AppState::get(cx).days_ahead);
    now.with_nanosecond(0).unwrap_or(now)
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn card(cx: &App) -> Div {
    v_flex()
        .min_w_0()
        .p_4()
        .gap_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
}

/// A column wider than a page, for the editor and the scoring sheet.
fn wide(cx: &App) -> Div {
    v_flex()
        .w_full()
        .max_w(px(WIDE))
        .mx_auto()
        .p_8()
        .gap_5()
        .text_color(cx.theme().foreground)
}

/// "GS-2 · Constitution and polity", from the question's first topic.
fn place(question: &MainsQuestion, cx: &App) -> String {
    let library = &AppState::get(cx).library;
    let subject = question
        .topics
        .first()
        .map(|topic| mentor::topic_title(library, mentor::subject_node(topic)));
    match subject {
        Some(subject) => format!("{} · {subject}", question.paper_title()),
        None => question.paper_title(),
    }
}

/// "15 marks · 250 words · 11 minutes".
fn terms(question: &MainsQuestion) -> String {
    format!(
        "{} marks · {} words · {} minutes",
        question.marks,
        question.words,
        question.minutes()
    )
}

/// "9:12", or "12:40, 1:40 over" for an answer that ran past its clock.
fn time_taken(question: Option<&MainsQuestion>, seconds: u32) -> String {
    match question.map(|question| mentor::seconds_left(question, seconds)) {
        Some(left) if left < 0 => format!("{}, {} over", clock(seconds), clock(-left as u32)),
        _ => clock(seconds),
    }
}

/// "up 0.7", "down 0.3" or "steady".
fn direction(change: f64) -> String {
    if change >= 0.05 {
        format!("up {change:.1}")
    } else if change <= -0.05 {
        format!("down {:.1}", -change)
    } else {
        "steady".to_string()
    }
}

impl MainsScreen {
    pub fn new() -> Self {
        MainsScreen {
            tab: 0,
            paper: None,
            status: Status::All,
            view: View::List,
        }
    }

    /// The answer in the editor, if one is being written.
    pub fn writing(&self) -> Option<&MainsAnswer> {
        match &self.view {
            View::Writing(writing) => Some(&writing.answer),
            _ => None,
        }
    }

    /// Opens one question, as today's Mains task asks. An answer being
    /// written is saved and put aside first.
    pub fn open_question(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.stop_clock(cx);
        self.tab = 0;
        self.view = View::Question {
            ix,
            model_shown: false,
        };
        cx.notify();
    }

    /// Called when the screen comes up: the clock of an answer being
    /// written goes on.
    pub fn show(&mut self, cx: &mut Context<Self>) {
        let View::Writing(writing) = &mut self.view else {
            return;
        };
        if writing.clock.is_some() {
            return;
        }
        let now = cx.background_executor().now();
        writing.counted = now;
        writing.typed = now;
        writing.clock = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// Called when the student goes to another screen: the clock stops
    /// where it stands, and the draft waits on screen for their return.
    pub fn stop_clock(&mut self, cx: &mut Context<Self>) {
        if matches!(&self.view, View::Writing(writing) if writing.clock.is_some()) {
            self.tick(cx);
        }
        if let View::Writing(writing) = &mut self.view {
            writing.clock = None;
        }
    }

    /// Starts an answer to the question at `ix`, or resumes its draft.
    fn start(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let state = AppState::get(cx);
        let question = &state.library.mains[ix];
        let draft = state
            .mains
            .iter()
            .rev()
            .find(|answer| answer.question == question.id && answer.submitted.is_none())
            .cloned();
        let answer = match draft {
            Some(draft) => draft,
            None => {
                let begun = MainsAnswer::new(&question.id, now(cx));
                let Some(saved) = AppState::save_mains_answer(cx, begun) else {
                    return;
                };
                saved
            }
        };
        let text = answer.text.clone();
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write your answer here. It is saved as you type.")
        });
        editor.update(cx, |state, cx| {
            state.set_value(text.clone(), window, cx);
            let line = text.matches('\n').count() as u32;
            state.set_cursor_position(Position::new(line, u32::MAX), window, cx);
        });
        let changed = cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.typed(cx);
            }
        });
        let now = cx.background_executor().now();
        self.view = View::Writing(Box::new(Writing {
            answer,
            question: ix,
            editor,
            counted: now,
            typed: now,
            block: None,
            clock: None,
            _changed: changed,
        }));
        self.show(cx);
        cx.notify();
    }

    /// Saves the text as it stands after a keystroke.
    fn typed(&mut self, cx: &mut Context<Self>) {
        let View::Writing(writing) = &mut self.view else {
            return;
        };
        writing.answer.text = writing.editor.read(cx).value().to_string();
        writing.typed = cx.background_executor().now();
        self.save(cx);
    }

    /// Counts the seconds since the last tick on the answer's clock and as
    /// study, and saves them.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let View::Writing(writing) = &mut self.view else {
            return;
        };
        let seconds = now.saturating_duration_since(writing.counted).as_secs() as u32;
        if seconds == 0 {
            return;
        }
        writing.counted += Duration::from_secs(u64::from(seconds));
        let question = &AppState::get(cx).library.mains[writing.question];
        let running = mentor::seconds_left(question, writing.answer.seconds) > 0
            || now.saturating_duration_since(writing.typed) < IDLE;
        if !running {
            return;
        }
        writing.answer.seconds += seconds;
        let item = writing.answer.id.to_string();
        AppState::record_study(cx, Study::Mains, &item, &mut writing.block, seconds);
        self.save(cx);
    }

    /// Saves the answer being written.
    fn save(&mut self, cx: &mut Context<Self>) {
        let View::Writing(writing) = &mut self.view else {
            return;
        };
        if let Some(saved) = AppState::save_mains_answer(cx, writing.answer.clone()) {
            writing.answer = saved;
        }
        cx.notify();
    }

    /// Leaves the editor for the question. The draft is saved and resumes
    /// where it stopped.
    fn leave(&mut self, cx: &mut Context<Self>) {
        self.stop_clock(cx);
        if let View::Writing(writing) = &self.view {
            self.view = View::Question {
                ix: writing.question,
                model_shown: false,
            };
        }
        cx.notify();
    }

    /// Hands the answer in and opens its scoring sheet.
    fn submit(&mut self, cx: &mut Context<Self>) {
        self.stop_clock(cx);
        let submitted = now(cx);
        let View::Writing(writing) = &mut self.view else {
            return;
        };
        writing.answer.submitted = Some(submitted);
        if let Some(saved) = AppState::save_mains_answer(cx, writing.answer.clone()) {
            self.view = View::Scoring(saved.id);
        }
        cx.notify();
    }

    /// Changes a submitted answer's ticks or scores and saves it.
    fn score(&mut self, id: i64, change: impl FnOnce(&mut MainsAnswer), cx: &mut Context<Self>) {
        let answer = AppState::get(cx).mains.iter().find(|a| a.id == id).cloned();
        if let Some(mut answer) = answer {
            change(&mut answer);
            AppState::save_mains_answer(cx, answer);
        }
        cx.notify();
    }

    fn back_to_list(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.tab = tab;
        self.view = View::List;
        cx.notify();
    }

    fn back_button(&self, label: &'static str, tab: usize, cx: &Context<Self>) -> Button {
        Button::new("mains-back")
            .ghost()
            .small()
            .icon(Icon::new(IconName::ChevronLeft))
            .label(label)
            .on_click(cx.listener(move |this, _, _, cx| this.back_to_list(tab, cx)))
    }

    // ---- the list ----------------------------------------------------------

    fn list(&self, cx: &Context<Self>) -> Div {
        let state = AppState::get(cx);
        let answered = state
            .library
            .mains
            .iter()
            .filter(|q| {
                state
                    .mains
                    .iter()
                    .any(|a| a.question == q.id && a.submitted.is_some())
            })
            .count();
        let summary = format!(
            "{} · {answered} answered · write against the clock, then score yourself",
            count(state.library.mains.len(), "question", "questions"),
        );
        let tabs = TabBar::new("mains-tabs")
            .segmented()
            .selected_index(self.tab)
            .children(TABS.map(|title| Tab::new().label(title)))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.tab = *ix;
                cx.notify();
            }));
        let body = if self.tab == 1 {
            self.history(cx).into_any_element()
        } else {
            self.questions(cx).into_any_element()
        };
        page("Mains", summary, cx)
            .child(h_flex().child(tabs))
            .child(body)
    }

    fn questions(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let papers = ButtonGroup::new("mains-paper")
            .outline()
            .small()
            .child(
                Button::new("paper-all")
                    .label("All papers")
                    .selected(self.paper.is_none()),
            )
            .children(MAINS_PAPERS.map(|paper| {
                Button::new(SharedString::from(format!("paper-{paper}")))
                    .label(format!("GS-{}", paper.trim_start_matches("gs")))
                    .selected(self.paper == Some(paper))
            }))
            .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                if let Some(&ix) = clicked.first() {
                    this.paper = ix.checked_sub(1).map(|paper| MAINS_PAPERS[paper]);
                    cx.notify();
                }
            }));
        let statuses = [
            (Status::All, "status-all", "All"),
            (Status::Unanswered, "status-unanswered", "Not answered"),
            (Status::Answered, "status-answered", "Answered"),
        ];
        let status = ButtonGroup::new("mains-status")
            .outline()
            .small()
            .children(statuses.map(|(status, id, label)| {
                Button::new(id).label(label).selected(self.status == status)
            }))
            .on_click(cx.listener(move |this, clicked: &Vec<usize>, _, cx| {
                if let Some(&ix) = clicked.first() {
                    this.status = statuses[ix].0;
                    cx.notify();
                }
            }));

        let state = AppState::get(cx);
        let library = &state.library;
        let mut groups: Vec<AnyElement> = Vec::new();
        for paper in MAINS_PAPERS {
            if self.paper.is_some_and(|chosen| chosen != paper) {
                continue;
            }
            // In file order, grouped by subject in the order they first appear.
            let mut subjects: Vec<(&str, Vec<usize>)> = Vec::new();
            for (ix, question) in library.mains.iter().enumerate() {
                if question.paper != paper {
                    continue;
                }
                let answers: Vec<&MainsAnswer> = state
                    .mains
                    .iter()
                    .filter(|answer| answer.question == question.id)
                    .collect();
                let answered = answers.iter().any(|answer| answer.submitted.is_some());
                let shown = match self.status {
                    Status::All => true,
                    Status::Unanswered => !answered,
                    Status::Answered => answered,
                };
                if !shown {
                    continue;
                }
                let subject = question
                    .topics
                    .first()
                    .map_or("", |t| mentor::subject_node(t));
                match subjects.iter_mut().find(|(s, _)| *s == subject) {
                    Some((_, list)) => list.push(ix),
                    None => subjects.push((subject, vec![ix])),
                }
            }
            if subjects.is_empty() {
                continue;
            }
            let mut group = v_flex().gap_2().child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child(format!("GS-{}", paper.trim_start_matches("gs"))),
            );
            for (subject, list) in subjects {
                group = group.child(
                    div()
                        .pt_1()
                        .text_sm()
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .child(mentor::topic_title(library, subject)),
                );
                for ix in list {
                    group = group.child(self.question_row(ix, cx));
                }
            }
            groups.push(group.into_any_element());
        }

        v_flex()
            .id("mains-questions")
            .test_support()
            .gap_5()
            .child(h_flex().flex_wrap().gap_3().child(papers).child(status))
            .when(groups.is_empty(), |this| {
                this.child(muted("No question matches these filters.", cx))
            })
            .children(groups)
    }

    fn question_row(&self, ix: usize, cx: &Context<Self>) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let question = &state.library.mains[ix];
        let answers: Vec<&MainsAnswer> = state
            .mains
            .iter()
            .filter(|answer| answer.question == question.id)
            .collect();
        let draft = answers
            .iter()
            .rev()
            .find(|answer| answer.submitted.is_none());
        let last = answers
            .iter()
            .rev()
            .find(|answer| answer.submitted.is_some());
        let (status, colour) = match (draft, last) {
            (Some(draft), _) => (
                format!("Draft · {} written", clock(draft.seconds)),
                cx.theme().warning,
            ),
            (None, Some(last)) => (
                match last.total() {
                    Some(total) => format!("Answered · {total}/35"),
                    None => "Answered · not scored".to_string(),
                },
                cx.theme().success,
            ),
            (None, None) => ("Not answered".to_string(), cx.theme().muted_foreground),
        };
        v_flex()
            .id(("mains-question", ix))
            .test_support()
            .gap_1()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().accent))
            .child(div().child(question.question.clone()))
            .child(
                h_flex()
                    .gap_3()
                    .text_sm()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(terms(question)),
                    )
                    .child(div().text_color(colour).child(status)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_question(ix, cx)))
    }

    fn history(&self, cx: &Context<Self>) -> Div {
        let state = AppState::get(cx);
        let library = &state.library;
        let mut submitted: Vec<MainsAnswer> = state
            .mains
            .iter()
            .filter(|answer| answer.submitted.is_some())
            .cloned()
            .collect();
        submitted.sort_by_key(|answer| std::cmp::Reverse((answer.submitted, answer.id)));
        let rows = submitted.iter().map(|answer| {
            let question = library
                .mains_ix(&answer.question)
                .map(|ix| &library.mains[ix]);
            let overran = question.is_some_and(|q| mentor::seconds_left(q, answer.seconds) < 0);
            let words = word_count(&answer.text);
            let over_words = question.is_some_and(|q| words > q.words as usize);
            let id = answer.id;
            h_flex()
                .id(("history", id as usize))
                .test_support()
                .gap_3()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_sm()
                .cursor_pointer()
                .hover(|this| this.bg(cx.theme().accent))
                .child(
                    div()
                        .w(px(56.))
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            answer
                                .submitted
                                .map(|s| s.format("%-d %b").to_string())
                                .unwrap_or_default(),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(match question {
                            Some(q) => one_line(&q.question, 90),
                            None => "A question no longer in the content".to_string(),
                        }))
                        .child(
                            h_flex()
                                .gap_2()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(question.map(|q| q.paper_title()).unwrap_or_default())
                                .child(
                                    div()
                                        .when(overran, |this| this.text_color(cx.theme().danger))
                                        .child(time_taken(question, answer.seconds)),
                                )
                                .child(
                                    div()
                                        .when(over_words, |this| this.text_color(cx.theme().danger))
                                        .child(match question {
                                            Some(q) => format!("{words} of {} words", q.words),
                                            None => count(words, "word", "words"),
                                        }),
                                )
                                .when_some(question, |this, q| {
                                    this.child(format!(
                                        "{} of {} points",
                                        answer.points.len(),
                                        q.points.len()
                                    ))
                                }),
                        ),
                )
                .child(
                    div()
                        .w(px(84.))
                        .flex_shrink_0()
                        .text_right()
                        .font_medium()
                        .child(match answer.total() {
                            Some(total) => format!("{total}/35"),
                            None => "Not scored".to_string(),
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.view = View::Scoring(id);
                    cx.notify();
                }))
        });
        let rows: Vec<_> = rows.collect();
        v_flex().gap_5().child(self.trend(&submitted, cx)).child(
            card(cx)
                .id("mains-history")
                .test_support()
                .child(
                    div()
                        .font_semibold()
                        .child(format!("Your answers · {}", submitted.len())),
                )
                .when(rows.is_empty(), |this| {
                    this.child(muted(
                        "Answers you submit appear here, with their time, length and scores.",
                        cx,
                    ))
                })
                .children(rows),
        )
    }

    /// One small chart per dimension: the student's scores over time.
    fn trend(&self, answers: &[MainsAnswer], cx: &App) -> impl IntoElement + use<> {
        let trends = mentor::mains_trend(answers);
        let scored = trends[0].scores.len();
        let panel = card(cx).id("mains-trend").test_support().child(
            v_flex()
                .child(div().font_semibold().child("Trend per dimension"))
                .child(muted(
                    "Your own scores, 0 to 5, over the answers you have scored",
                    cx,
                )),
        );
        if scored < 2 {
            return panel.child(muted(
                "The trend appears once you have scored two answers.",
                cx,
            ));
        }
        let charts = trends.into_iter().enumerate().map(|(dimension, trend)| {
            let summary = match (trend.average(), trend.change) {
                (Some(average), Some(change)) => {
                    format!("average {average:.1} · {}", direction(change))
                }
                _ => String::new(),
            };
            v_flex()
                .min_w_0()
                .gap_1()
                .child(div().text_sm().font_medium().child(DIMENSIONS[dimension].0))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(summary),
                )
                .child(
                    div()
                        .id(("trend-chart", dimension))
                        .test_support()
                        .h(px(80.))
                        .child(
                            LineChart::new(trend.scores)
                                .id(("trend-line", dimension))
                                .x(|point: &(chrono::NaiveDate, u8)| {
                                    SharedString::from(point.0.format("%-d %b").to_string())
                                })
                                .y(|point| f64::from(point.1))
                                .y_domain(0., 5.)
                                .dot()
                                .x_axis(false)
                                .tooltip_value(|_, value| format!("{value:.0} of 5").into())
                                .name(DIMENSIONS[dimension].0),
                        ),
                )
        });
        panel.child(div().grid().grid_cols(4).gap_4().children(charts))
    }

    // ---- one question --------------------------------------------------------

    fn question(&self, ix: usize, model_shown: bool, cx: &Context<Self>) -> Div {
        let state = AppState::get(cx);
        let question = state.library.mains[ix].clone();
        let answers: Vec<MainsAnswer> = state
            .mains
            .iter()
            .filter(|answer| answer.question == question.id)
            .cloned()
            .collect();
        let draft = answers
            .iter()
            .rev()
            .find(|answer| answer.submitted.is_none());
        let submitted: Vec<&MainsAnswer> = answers
            .iter()
            .filter(|answer| answer.submitted.is_some())
            .collect();
        let model_open = model_shown || !submitted.is_empty();
        let start_label = match draft {
            Some(draft) => format!("Resume your draft ({} written)", clock(draft.seconds)),
            None if submitted.is_empty() => "Start writing".to_string(),
            None => "Write it again".to_string(),
        };

        let past = submitted.iter().rev().map(|answer| {
            let id = answer.id;
            h_flex()
                .id(("past-answer", id as usize))
                .test_support()
                .gap_3()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_sm()
                .cursor_pointer()
                .hover(|this| this.bg(cx.theme().accent))
                .child(div().flex_1().child(format!(
                    "{} · {} · {}",
                    answer
                        .submitted
                        .map(|s| s.format("%-d %b %Y").to_string())
                        .unwrap_or_default(),
                    time_taken(Some(&question), answer.seconds),
                    count(word_count(&answer.text), "word", "words"),
                )))
                .child(div().font_medium().child(match answer.total() {
                    Some(total) => format!("{total}/35"),
                    None => "Not scored".to_string(),
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.view = View::Scoring(id);
                    cx.notify();
                }))
        });
        let past: Vec<_> = past.collect();

        page("Mains", place(&question, cx), cx)
            .child(h_flex().child(self.back_button("All questions", 0, cx)))
            .child(
                card(cx)
                    .id("mains-question-card")
                    .test_support()
                    .child(muted(terms(&question), cx))
                    .child(
                        div()
                            .text_size(px(17.))
                            .child(TextView::markdown("mains-question-text", question.question.clone())),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("start-writing")
                                    .primary()
                                    .small()
                                    .icon(Icon::new(IconName::PenLine))
                                    .label(start_label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start(ix, window, cx)
                                    })),
                            )
                            .when(!model_open, |this| {
                                this.child(
                                    Button::new("show-model")
                                        .ghost()
                                        .small()
                                        .label("Show the model answer")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.view = View::Question {
                                                ix,
                                                model_shown: true,
                                            };
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(muted(
                        format!(
                            "The clock gives {} minutes for {} marks. Your plan allows {} more to score yourself.",
                            question.minutes(),
                            question.marks,
                            MAINS_SCORING_MINUTES
                        ),
                        cx,
                    )),
            )
            .when(!model_open, |this| {
                this.child(muted(
                    "The model answer stays hidden until you have written your own, so it does not shape your answer.",
                    cx,
                ))
            })
            .when(model_open, |this| {
                this.child(self.model(&question, None, cx))
            })
            .when(!past.is_empty(), |this| {
                this.child(
                    card(cx)
                        .child(div().font_semibold().child("Your answers to this question"))
                        .children(past),
                )
            })
    }

    /// The model answer and the points to cover, with ticks when `scoring`
    /// names the answer being scored.
    fn model(
        &self,
        question: &MainsQuestion,
        scoring: Option<&MainsAnswer>,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let points = question
            .points
            .iter()
            .enumerate()
            .map(|(point, text)| match scoring {
                Some(answer) => {
                    let id = answer.id;
                    let made = answer.points.contains(&point);
                    Checkbox::new(("point", point))
                        .checked(made)
                        .label(text.clone())
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.score(
                                id,
                                |answer| {
                                    answer.points.retain(|&p| p != point);
                                    if checked {
                                        answer.points.push(point);
                                        answer.points.sort_unstable();
                                    }
                                },
                                cx,
                            );
                        }))
                        .into_any_element()
                }
                None => h_flex()
                    .gap_2()
                    .items_start()
                    .text_sm()
                    .child("•")
                    .child(div().flex_1().child(text.clone()))
                    .into_any_element(),
            });
        card(cx)
            .flex_1()
            .id("model-answer")
            .test_support()
            .child(div().font_semibold().child("Model answer"))
            .child(TextView::markdown(
                SharedString::from(format!("mains-model-{}", question.id)),
                question.model.clone(),
            ))
            .child(
                v_flex()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().child("Points to cover"))
                    .when(scoring.is_some(), |this| {
                        this.child(muted("Tick the points your answer made.", cx))
                    })
                    .children(points),
            )
    }

    // ---- writing -------------------------------------------------------------

    fn writing_view(&self, writing: &Writing, cx: &Context<Self>) -> impl IntoElement + use<> {
        let question = AppState::get(cx).library.mains[writing.question].clone();
        let answer = &writing.answer;
        let left = mentor::seconds_left(&question, answer.seconds);
        let words = word_count(&answer.text);
        let theme = cx.theme();
        let clock_text = if left >= 0 {
            format!("{} left", clock(left as u32))
        } else {
            format!("{} over", clock(-left as u32))
        };
        wide(cx)
            .id("mains-writing")
            .test_support()
            .child(
                v_flex()
                    .gap_2()
                    .child(muted(format!("{} · {}", place(&question, cx), terms(&question)), cx))
                    .child(
                        div()
                            .text_size(px(17.))
                            .child(TextView::markdown("mains-writing-question", question.question.clone())),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .id("mains-clock")
                            .test_support()
                            .gap_1()
                            .font_semibold()
                            .when(left <= LOW_TIME, |this| this.text_color(theme.danger))
                            .child(Icon::new(IconName::Timer).small())
                            .child(clock_text),
                    )
                    .child(
                        div()
                            .id("mains-words")
                            .test_support()
                            .font_medium()
                            .when(words > question.words as usize, |this| this.text_color(theme.danger))
                            .child(format!("{words} / {} words", question.words)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("mains-leave")
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::LogOut))
                            .label("Save and leave")
                            .tooltip("Your answer and its clock are saved. Resume from the question.")
                            .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
                    )
                    .child(
                        Button::new("mains-submit")
                            .primary()
                            .small()
                            .label("Submit and score it")
                            .disabled(words == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
            .when(left <= 0, |this| {
                this.child(
                    div()
                        .id("time-up")
                        .test_support()
                        .px_4()
                        .py_2()
                        .rounded(theme.radius)
                        .bg(theme.warning)
                        .text_color(theme.warning_foreground)
                        .text_sm()
                        .child(
                            "Time is up. In the exam you would move on now: finish your sentence and submit. \
                             The overrun is kept with your answer.",
                        ),
                )
            })
            .child(
                div()
                    .id("mains-editor")
                    .test_support()
                    .child(Textarea::new(&writing.editor).h(px(400.))),
            )
            .child(muted(
                "Saved as you type. If you leave or the app closes, the draft and its clock wait for you.",
                cx,
            ))
    }

    // ---- scoring -------------------------------------------------------------

    fn scoring(&self, id: i64, cx: &Context<Self>) -> AnyElement {
        let state = AppState::get(cx);
        let Some(answer) = state.mains.iter().find(|a| a.id == id).cloned() else {
            return page("Mains", "This answer is not in your data.", cx).into_any_element();
        };
        let question = state
            .library
            .mains_ix(&answer.question)
            .map(|ix| state.library.mains[ix].clone());
        let words = word_count(&answer.text);
        let summary = format!(
            "Submitted {} · took {} · {}",
            answer
                .submitted
                .map(|s| s.format("%-d %b %Y, %H:%M").to_string())
                .unwrap_or_default(),
            time_taken(question.as_ref(), answer.seconds),
            match &question {
                Some(q) => format!("{words} of {} words", q.words),
                None => count(words, "word", "words"),
            },
        );

        let scores = DIMENSIONS
            .iter()
            .enumerate()
            .map(|(dimension, (name, hint))| {
                let given = answer.scores[dimension];
                h_flex()
                    .gap_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().font_medium().child(*name))
                            .child(muted(*hint, cx)),
                    )
                    .child(
                        ButtonGroup::new(("scores", dimension))
                            .outline()
                            .small()
                            .children((0..=5u8).map(|score| {
                                Button::new(SharedString::from(format!(
                                    "score-{dimension}-{score}"
                                )))
                                .label(score.to_string())
                                .selected(given == Some(score))
                            }))
                            .on_click(cx.listener(move |this, clicked: &Vec<usize>, _, cx| {
                                if let Some(&score) = clicked.first() {
                                    this.score(
                                        id,
                                        |answer| answer.scores[dimension] = Some(score as u8),
                                        cx,
                                    );
                                }
                            })),
                    )
            });
        let given = answer.scores.iter().filter(|s| s.is_some()).count();
        let total = match answer.total() {
            Some(total) => format!("Total {total} / 35"),
            None => format!("{given} of {SCORES} scored"),
        };

        wide(cx)
            .id("mains-scoring")
            .test_support()
            .child(h_flex().child(self.back_button("History", 1, cx)))
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_2xl().font_semibold().child("Score your answer"))
                    .when_some(question.as_ref(), |this, q| {
                        this.child(muted(format!("{} · {}", place(q, cx), terms(q)), cx))
                            .child(div().text_size(px(17.)).child(q.question.clone()))
                    })
                    .child(muted(summary, cx)),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(
                        card(cx)
                            .flex_1()
                            .id("your-answer")
                            .test_support()
                            .child(div().font_semibold().child("Your answer"))
                            .child(TextView::markdown(
                                ("mains-yours", id as usize),
                                answer.text.clone(),
                            )),
                    )
                    .when_some(question.as_ref(), |this, q| {
                        this.child(self.model(q, Some(&answer), cx))
                    }),
            )
            .child(
                card(cx)
                    .id("self-scores")
                    .test_support()
                    .child(
                        h_flex()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .child(div().font_semibold().child("Score yourself, 0 to 5"))
                                    .child(muted(
                                        "Be strict: 3 is a fair answer, 5 one you would not change.",
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .id("mains-total")
                                    .test_support()
                                    .text_lg()
                                    .font_semibold()
                                    .child(total),
                            ),
                    )
                    .children(scores)
                    .child(
                        h_flex().pt_2().child(
                            Button::new("scoring-done")
                                .primary()
                                .small()
                                .label("Done")
                                .on_click(cx.listener(|this, _, _, cx| this.back_to_list(1, cx))),
                        ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for MainsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.view {
            View::List => self.list(cx).into_any_element(),
            View::Question { ix, model_shown } => {
                self.question(*ix, *model_shown, cx).into_any_element()
            }
            View::Writing(writing) => self.writing_view(writing, cx).into_any_element(),
            View::Scoring(id) => self.scoring(*id, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::direction;

    #[test]
    fn a_change_in_score_reads_as_a_direction() {
        assert_eq!(direction(0.7), "up 0.7");
        assert_eq!(direction(-0.3), "down 0.3");
        assert_eq!(direction(0.04), "steady");
    }
}

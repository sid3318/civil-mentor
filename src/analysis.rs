//! The Analysis screen: where the marks come from and where they go. Five
//! tabs over the student's record: an overview, subjects, tests, study time
//! and syllabus coverage. The numbers come from [`crate::mentor::insights`]
//! and the store; the charts are gpui-kit's, except the 12-week calendar,
//! which is plain cells.

use std::collections::BTreeMap;

use chrono::{Datelike as _, Duration, Local, NaiveDate};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::chart::{BarChart, LineChart};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell};
use crate::content::Target;
use crate::mentor::{
    self, DRILL_QUESTIONS, Insights, RECENT_TESTS, SECONDS_PER_QUESTION, SUBJECTS, TopicScore,
    hours, percent,
};
use crate::review::marks;
use crate::store::{Attempt, Confidence, Past};
use crate::syllabus::{coverage, statuses};

pub const TABS: [&str; 5] = ["Overview", "Subjects", "Tests", "Time", "Coverage"];
/// The study calendar covers this many weeks, ending with this one.
const WEEKS: i64 = 12;

pub struct AnalysisScreen {
    shell: WeakEntity<Shell>,
    tab: usize,
}

/// Everything the tabs show, read once per frame.
struct Record {
    today: NaiveDate,
    /// The last tests, newest first.
    tests: Vec<Attempt>,
    history: Vec<Past>,
    study: BTreeMap<NaiveDate, u32>,
    insights: Insights,
    daily_minutes: u32,
    demo: bool,
}

impl AnalysisScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        AnalysisScreen { shell, tab: 0 }
    }

    pub fn tab(&self) -> usize {
        self.tab
    }

    fn drill(&self, topic: &str, cx: &mut App) {
        let plan = mentor::topic_drill(&AppState::get(cx).library, topic, DRILL_QUESTIONS);
        let _ = self.shell.update(cx, |shell, cx| shell.sit(&plan, cx));
    }

    fn open(&self, target: Target, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.open(target, cx));
    }
}

fn panel(title: impl Into<SharedString>, note: impl Into<SharedString>, cx: &App) -> Div {
    let note = note.into();
    v_flex()
        .flex_1()
        .min_w_0()
        .p_4()
        .gap_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .child(div().font_semibold().child(title.into()))
                .when(!note.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(note),
                    )
                }),
        )
}

fn empty(text: &'static str, cx: &App) -> Div {
    div()
        .py_6()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn row(cx: &App) -> Div {
    h_flex()
        .gap_3()
        .py_1p5()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_sm()
}

fn cell(width: f32, text: impl Into<SharedString>) -> Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_right()
        .whitespace_nowrap()
        .child(text.into())
}

/// "+4.67", "-2", "0".
fn signed(value: f64) -> String {
    if value > 0.0 {
        format!("+{}", marks(value))
    } else {
        marks(value)
    }
}

/// The net score of each recent test as a share of its full marks, oldest
/// first, labelled by day. A second test on a day gets its number.
fn score_points(tests: &[Attempt]) -> Vec<(SharedString, f64)> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    tests
        .iter()
        .rev()
        .map(|test| {
            let day = test
                .finished
                .unwrap_or(test.started)
                .format("%-d %b")
                .to_string();
            let n = seen.entry(day.clone()).or_default();
            *n += 1;
            let label = if *n == 1 { day } else { format!("{day} ({n})") };
            let full = mentor::full_marks(test);
            let share = if full > 0.0 {
                mentor::score(test).marks * 100.0 / full
            } else {
                0.0
            };
            (SharedString::from(label), share)
        })
        .collect()
}

/// Right and total answers for each subject, in [`SUBJECTS`] order.
fn subject_scores(history: &[Past]) -> Vec<(usize, usize)> {
    SUBJECTS
        .iter()
        .map(|subject| {
            history
                .iter()
                .filter(|past| mentor::subject_of_topic(&past.topic) == Some(subject))
                .fold((0, 0), |(right, total), past| {
                    (right + usize::from(past.correct), total + 1)
                })
        })
        .collect()
}

/// A short subject name that fits under a bar.
fn short(id: &str) -> &'static str {
    match id {
        "history" => "History",
        "geography" => "Geography",
        "polity" => "Polity",
        "economy" => "Economy",
        "environment" => "Environment",
        "science" => "Sci and tech",
        _ => "Current",
    }
}

/// The colour of a day in the study calendar: empty, or darker the nearer
/// the day came to the daily goal.
pub(crate) fn heat(seconds: u32, goal: u32, cx: &App) -> Hsla {
    let theme = cx.theme();
    if seconds == 0 {
        return theme.muted;
    }
    let share = seconds as f32 / goal.max(1) as f32;
    let opacity = match share {
        s if s < 0.25 => 0.3,
        s if s < 0.5 => 0.5,
        s if s < 1.0 => 0.75,
        _ => 1.0,
    };
    theme.primary.opacity(opacity)
}

/// The last 12 weeks of study, a column a week and a row a weekday,
/// ending with this week.
fn calendar(record: &Record, cx: &App) -> impl IntoElement {
    let goal = record.daily_minutes * 60;
    let monday =
        record.today - Duration::days(record.today.weekday().num_days_from_monday().into());
    let start = monday - Duration::weeks(WEEKS - 1);
    let size = px(16.);
    let labels = v_flex()
        .gap(px(3.))
        .pr_1()
        .text_size(px(11.))
        .text_color(cx.theme().muted_foreground)
        .children(["M", "", "W", "", "F", "", "S"].map(|label| div().h(size).child(label)));
    let weeks = (0..WEEKS).map(|week| {
        v_flex().gap(px(3.)).children((0..7).map(|weekday| {
            let day = start + Duration::days(week * 7 + weekday);
            let seconds = record.study.get(&day).copied().unwrap_or(0);
            div()
                .size(size)
                .rounded(px(3.))
                .when(day <= record.today, |this| this.bg(heat(seconds, goal, cx)))
        }))
    });
    h_flex()
        .id("heatmap")
        .test_support()
        .items_start()
        .gap(px(3.))
        .child(labels)
        .children(weeks)
}

impl AnalysisScreen {
    /// The net score chart, with at most `labels` dates under it.
    fn score_chart(&self, record: &Record, labels: usize, cx: &App) -> Div {
        let panel = panel(
            format!("Net score, {}", recent(record.tests.len())),
            "As a share of full marks",
            cx,
        );
        if record.tests.is_empty() {
            return panel.child(empty(
                "Your net score appears here after your first test.",
                cx,
            ));
        }
        let points = score_points(&record.tests);
        panel.child(
            div().id("score-chart").test_support().h(px(180.)).child(
                LineChart::new(points)
                    .x(|point: &(SharedString, f64)| point.0.clone())
                    .y(|point| point.1)
                    .dot()
                    .x_tick_count(labels)
                    .y_axis(true)
                    .y_tick_format(|value| format!("{value:.0}%"))
                    .tooltip_value(|_, value| format!("{value:.0}%").into())
                    .name("Net score"),
            ),
        )
    }

    fn subject_chart(&self, record: &Record, height: f32, cx: &App) -> Div {
        let panel = panel(
            "Accuracy by subject",
            "Share of answers right, all tests",
            cx,
        );
        let bars: Vec<(SharedString, f64)> = SUBJECTS
            .iter()
            .zip(subject_scores(&record.history))
            .filter(|(_, (_, total))| *total > 0)
            .map(|(subject, (right, total))| {
                (
                    SharedString::from(short(subject.id)),
                    right as f64 * 100.0 / total as f64,
                )
            })
            .collect();
        if bars.is_empty() {
            return panel.child(empty("Answer a few questions to see your accuracy.", cx));
        }
        panel.child(
            div()
                .id("subject-chart")
                .test_support()
                .h(px(height))
                .child(
                    BarChart::new(bars)
                        .band(|bar: &(SharedString, f64)| bar.0.clone())
                        .value(|bar| bar.1)
                        .label(|bar| format!("{:.0}%", bar.1))
                        .tooltip_value(|_, value| format!("{value:.0}% right").into())
                        .name("Accuracy"),
                ),
        )
    }

    fn weak_topics(&self, record: &Record, cx: &mut Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let panel = panel(
            "Weakest topics",
            "Under 50% right over 8 or more answers",
            cx,
        );
        if record.insights.weak.is_empty() {
            return panel.child(empty("No topic is that weak yet.", cx));
        }
        panel.children(
            record
                .insights
                .weak
                .iter()
                .take(5)
                .enumerate()
                .map(|(ix, score)| {
                    let topic = score.topic.clone();
                    row(cx)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(mentor::topic_title(library, &score.topic)),
                        )
                        .child(cell(44., format!("{}%", percent(score.right, score.total))))
                        .child(cell(44., format!("{} Q", score.total)))
                        .child(
                            Button::new(("drill-topic", ix))
                                .outline()
                                .xsmall()
                                .label("Drill")
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.drill(&topic, cx)),
                                ),
                        )
                }),
        )
    }

    fn guesses(&self, record: &Record, cx: &App) -> Div {
        let panel = panel(
            "Guess analysis",
            format!(
                "Marks won or lost at each confidence level, {}",
                recent(record.insights.tests)
            ),
            cx,
        );
        if record.insights.confidence.is_empty() {
            return panel.child(empty(
                "Tag your answers Sure, 50-50 or Guess in the exam room.",
                cx,
            ));
        }
        let theme = cx.theme();
        panel.children(record.insights.confidence.iter().map(|(level, score)| {
            let label = level.map_or("Not tagged", Confidence::label);
            let color = match score.marks {
                marks if marks < 0.0 => theme.danger,
                marks if marks > 0.0 => theme.success,
                _ => theme.muted_foreground,
            };
            row(cx)
                .id(SharedString::from(format!("confidence-{label}")))
                .test_support()
                .child(div().flex_1().child(label))
                .child(cell(
                    90.,
                    format!("{}% right", percent(score.right, score.answered())),
                ))
                .child(cell(100., format!("{} marks", signed(score.marks))).text_color(color))
        }))
    }

    fn papers(&self, cx: &App) -> Vec<(String, (usize, usize))> {
        let state = AppState::get(cx);
        let library = &state.library;
        let statuses = statuses(library, &state.reading);
        library
            .topics
            .iter()
            .filter(|topic| topic.depth == 1)
            .map(|paper| (paper.title.clone(), coverage(library, &statuses, &paper.id)))
            .collect()
    }

    fn overview(&self, record: &Record, cx: &mut Context<Self>) -> Div {
        let read: Vec<String> = self
            .papers(cx)
            .into_iter()
            .filter(|(title, _)| !title.contains("CSAT") && !title.starts_with("Paper"))
            .map(|(title, counts)| format!("{title} {}%", percent(counts.0, counts.1)))
            .collect();
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(self.score_chart(record, 5, cx))
                    .child(self.subject_chart(record, 180., cx)),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(self.weak_topics(record, cx))
                    .child(self.guesses(record, cx)),
            )
            .child(
                panel(
                    format!("Study time, last {WEEKS} weeks"),
                    format!("Syllabus read: {}", read.join(" · ")),
                    cx,
                )
                .child(calendar(record, cx)),
            )
    }

    fn subjects(&self, record: &Record, cx: &mut Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let table = panel("By subject", "Every answer, at Prelims marking", cx).children(
            SUBJECTS.iter().zip(subject_scores(&record.history)).map(
                |(subject, (right, total))| {
                    let wrong = total - right;
                    row(cx)
                        .child(div().flex_1().child(subject.name))
                        .child(cell(90., format!("{total} answered")))
                        .child(cell(70., format!("{right} right")))
                        .child(cell(70., format!("{wrong} wrong")))
                        .child(cell(60., format!("{}%", percent(right, total))))
                        .child(cell(
                            110.,
                            format!(
                                "{} marks",
                                signed(
                                    right as f64 * mentor::GS_RIGHT
                                        - wrong as f64 * mentor::GS_WRONG
                                )
                            ),
                        ))
                },
            ),
        );
        let mut topics: Vec<TopicScore> = mentor::topic_scores(&record.history);
        topics.sort_by(|a, b| {
            (a.right * b.total)
                .cmp(&(b.right * a.total))
                .then(b.total.cmp(&a.total))
        });
        let topic_rows = topics.iter().take(15).enumerate().map(|(ix, score)| {
            let topic = score.topic.clone();
            let subject = mentor::subject_of_topic(&score.topic).map_or("", |s| s.name);
            row(cx)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(mentor::topic_title(library, &score.topic)),
                )
                .child(
                    div()
                        .w(px(150.))
                        .text_color(cx.theme().muted_foreground)
                        .child(subject),
                )
                .child(cell(60., format!("{} of {}", score.right, score.total)))
                .child(cell(48., format!("{}%", percent(score.right, score.total))))
                .child(
                    Button::new(("topic-drill", ix))
                        .ghost()
                        .xsmall()
                        .label("Drill")
                        .on_click(cx.listener(move |this, _, _, cx| this.drill(&topic, cx))),
                )
        });
        v_flex()
            .gap_4()
            .child(self.subject_chart(record, 220., cx))
            .child(table)
            .child(if topics.is_empty() {
                panel("Topics, weakest first", "", cx).child(empty("No answers yet.", cx))
            } else {
                panel(
                    "Topics, weakest first",
                    "Every topic you have answered on",
                    cx,
                )
                .children(topic_rows)
            })
    }

    fn tests_tab(&self, record: &Record, cx: &mut Context<Self>) -> Div {
        let insights = &record.insights;
        let overall = &insights.overall;
        let attempts = match insights.marks_per_attempt() {
            Some(per_attempt) => format!(
                "In your last {} you attempted {} of {} questions ({}%) and got {}% of those right. Each attempt earned {} marks on average: at Prelims marking an attempt pays once more than a quarter of attempts are right.",
                mentor_count(insights.tests),
                overall.answered(),
                overall.answered() + overall.blank,
                percent(overall.answered(), overall.answered() + overall.blank),
                percent(overall.right, overall.answered()),
                signed(per_attempt),
            ),
            None => "Attempts and marks appear here after your first test.".to_string(),
        };
        let rows = record.tests.iter().map(|test| {
            let id = test.id;
            let score = mentor::score(test);
            let per_question = test.seconds_used / test.answers.len().max(1) as u32;
            row(cx)
                .id(("analysis-test", id as u64))
                .cursor_pointer()
                .hover(|this| this.bg(cx.theme().accent))
                .child(
                    div().w(px(56.)).child(
                        test.finished
                            .unwrap_or(test.started)
                            .format("%-d %b")
                            .to_string(),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(test.title.clone()),
                )
                .child(cell(
                    80.,
                    format!("{} of {}", score.answered(), test.answers.len()),
                ))
                .child(cell(56., format!("{} right", score.right)))
                .child(cell(64., format!("{} wrong", score.wrong)))
                .child(cell(
                    96.,
                    format!(
                        "{} of {}",
                        marks(score.marks),
                        marks(mentor::full_marks(test))
                    ),
                ))
                .child(cell(56., format!("{per_question} s/Q")))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let _ = this.shell.update(cx, |shell, cx| shell.open_result(id, cx));
                }))
        });
        v_flex()
            .gap_4()
            .child(self.score_chart(record, RECENT_TESTS, cx))
            .child(panel("Attempts against net score", "", cx).child(div().text_sm().child(attempts)))
            .child(if record.tests.is_empty() {
                panel("Recent tests", "", cx).child(empty("No tests yet.", cx))
            } else {
                panel("Recent tests", "Attempted, right, wrong, net score and time per question. Open one to review it.", cx)
                    .children(rows)
            })
    }

    fn time(&self, record: &Record, cx: &App) -> Div {
        let insights = &record.insights;
        let monday =
            record.today - Duration::days(record.today.weekday().num_days_from_monday().into());
        let weeks: Vec<(SharedString, f64)> = (0..WEEKS)
            .rev()
            .map(|back| {
                let start = monday - Duration::weeks(back);
                let seconds: u32 = record
                    .study
                    .range(start..start + Duration::days(7))
                    .map(|(_, s)| s)
                    .sum();
                (
                    SharedString::from(start.format("%-d %b").to_string()),
                    seconds as f64 / 3600.0,
                )
            })
            .collect();
        let studied = weeks.iter().any(|week| week.1 > 0.0);
        let pace = match insights.seconds_per_question {
            Some(pace) => format!(
                "{:.0} seconds a question in your last {}, against the {SECONDS_PER_QUESTION}-second budget.",
                pace,
                mentor_count(insights.tests)
            ),
            None => "Your pace appears here after your first test.".to_string(),
        };
        let facts = [
            pace,
            format!(
                "{} in the last 7 days, against a goal of {} ({}%).",
                hours(insights.week_seconds),
                hours(insights.week_goal),
                percent(insights.week_seconds as usize, insights.week_goal as usize)
            ),
            format!(
                "{} today, against {} a day.",
                hours(insights.today_seconds),
                hours(record.daily_minutes * 60)
            ),
            format!(
                "A streak of {}.",
                days(mentor::streak(&record.study, record.today))
            ),
        ];
        v_flex()
            .gap_4()
            .child(
                panel(
                    format!("Study time, last {WEEKS} weeks"),
                    "Time reading notes and in tests. Darker days came nearer your daily goal.",
                    cx,
                )
                .child(calendar(record, cx)),
            )
            .child(panel("Hours a week", "", cx).child(if studied {
                div()
                    .id("week-chart")
                    .test_support()
                    .h(px(180.))
                    .child(
                        BarChart::new(weeks)
                            .band(|week: &(SharedString, f64)| week.0.clone())
                            .value(|week| week.1)
                            .label(|week| match week.1 {
                                0.0 => String::new(),
                                hours => format!("{hours:.1}"),
                            })
                            .tooltip_value(|_, value| format!("{value:.1} h").into())
                            .name("Hours"),
                    )
                    .into_any_element()
            } else {
                empty("Time spent reading and in tests appears here.", cx).into_any_element()
            }))
            .child(panel("Pace and goal", "", cx).children(facts.map(|fact| row(cx).child(fact))))
    }

    fn coverage_tab(&self, record: &Record, cx: &mut Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let papers = self
            .papers(cx)
            .into_iter()
            .enumerate()
            .map(|(ix, (title, counts))| {
                row(cx)
                    .child(div().w(px(200.)).child(title))
                    .child(div().flex_1().child(
                        Progress::new(("paper-read", ix)).value(percent(counts.0, counts.1) as f32),
                    ))
                    .child(cell(
                        150.,
                        format!("{} of {} topics read", counts.0, counts.1),
                    ))
            });
        let high_yield =
            record
                .insights
                .high_yield
                .iter()
                .enumerate()
                .map(|(ix, (topic, asked))| {
                    let note = mentor::note_for_topic(library, topic);
                    let drill = topic.clone();
                    row(cx)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(mentor::topic_title(library, topic)),
                        )
                        .child(cell(130., format!("{asked} past questions")))
                        .child(
                            Button::new(("high-yield-note", ix))
                                .ghost()
                                .xsmall()
                                .label(if note.is_some() {
                                    "Open note"
                                } else {
                                    "No note yet"
                                })
                                .disabled(note.is_none())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(note) = note {
                                        this.open(Target::Note(note), cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new(("high-yield-drill", ix))
                                .outline()
                                .xsmall()
                                .label("Drill")
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.drill(&drill, cx)),
                                ),
                        )
                });
        v_flex()
            .gap_4()
            .child(panel("Syllabus read", "Bottom-level topics whose notes you have all marked read", cx).children(papers))
            .child(if record.insights.high_yield.is_empty() {
                panel("High-yield and unread", "", cx).child(empty("Every topic UPSC asks often is read.", cx))
            } else {
                panel(
                    "High-yield and unread",
                    "Prelims topics with the most past UPSC questions that you have not finished reading",
                    cx,
                )
                .children(high_yield)
            })
    }
}

/// "10 tests", "1 test".
fn mentor_count(n: usize) -> String {
    crate::screens::count(n, "test", "tests")
}

/// "last 10 tests", or "recent tests" before there are any.
fn recent(n: usize) -> String {
    match n {
        0 => "recent tests".to_string(),
        n => format!("last {}", mentor_count(n)),
    }
}

fn days(n: u32) -> String {
    crate::screens::count(n as usize, "day", "days")
}

impl Render for AnalysisScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = Local::now().date_naive();
        let state = AppState::get(cx);
        let tests = state.store.finished(RECENT_TESTS).unwrap_or_default();
        let history = state.store.answered().unwrap_or_default();
        let study = state.store.study_days().unwrap_or_default();
        let insights = mentor::insights(
            &state.library,
            &state.reading,
            &history,
            &tests,
            &study,
            state.settings.daily_minutes,
            today,
        );
        let record = Record {
            today,
            tests,
            history,
            study,
            insights,
            daily_minutes: state.settings.daily_minutes,
            demo: state.store.has_demo().unwrap_or(false),
        };
        let tabs = TabBar::new("analysis-tabs")
            .segmented()
            .selected_index(self.tab)
            .children(TABS.map(|title| Tab::new().label(title)))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.tab = *ix;
                cx.notify();
            }));
        let body = match self.tab {
            1 => self.subjects(&record, cx),
            2 => self.tests_tab(&record, cx),
            3 => self.time(&record, cx),
            4 => self.coverage_tab(&record, cx),
            _ => self.overview(&record, cx),
        };
        v_flex()
            .w_full()
            .max_w(px(1040.))
            .mx_auto()
            .p_8()
            .gap_5()
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_2xl().font_semibold().child(Screen::Analysis.title()))
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Where your marks come from and where they go"),
                            ),
                    )
                    .child(tabs),
            )
            .when(record.demo, |this| {
                this.child(
                    div()
                        .id("demo-banner")
                        .test_support()
                        .px_3()
                        .py_2()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().muted)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("This includes made-up demo history. Remove it in Settings when you are done looking."),
                )
            })
            .child(body)
            .child(div().h(px(24.)))
    }
}

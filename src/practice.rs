//! The Practice hub: every kind of test, unfinished tests to resume, past
//! papers and recent results. Tests run in the exam room ([`crate::exam`]).

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::exam::clock;
use crate::mentor::{self, SECONDS_PER_QUESTION, SUBJECTS, TestKind, TestPlan};
use crate::review::marks;
use crate::screens::count;
use crate::ui;

const CUSTOM_STEP: usize = 5;
const CUSTOM_MAX: usize = 100;
const MINUTE_STEP: u32 = 5;
const MAX_MINUTES: u32 = 180;

/// "12 min", "2 h", "1 h 30 min".
fn duration(seconds: u32) -> String {
    let minutes = seconds.div_ceil(60);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

pub struct PracticeScreen {
    shell: WeakEntity<Shell>,
    /// The custom drill being set up: subject ids, question count, and the
    /// time in minutes once the student sets it (72 seconds a question until then).
    subjects: Vec<String>,
    count: usize,
    minutes: Option<u32>,
}

impl PracticeScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        PracticeScreen {
            shell,
            subjects: Vec::new(),
            count: 20,
            minutes: None,
        }
    }

    fn custom_plan(&self) -> TestPlan {
        TestPlan {
            kind: TestKind::Custom,
            subjects: self.subjects.clone(),
            count: self.count,
            seconds: match self.minutes {
                Some(minutes) => minutes * 60,
                None => self.count as u32 * SECONDS_PER_QUESTION,
            },
        }
    }

    fn sit(&self, plan: TestPlan, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.sit(&plan, cx));
    }
}

/// A titled group of the hub, with one line saying what it is.
fn section(title: &'static str, note: impl Into<SharedString>, cx: &App) -> Div {
    v_flex().gap(px(10.)).child(
        v_flex()
            .gap(px(2.))
            .child(div().text_size(px(15.)).font_semibold().child(title))
            .child(ui::meta(note, cx)),
    )
}

/// A row inside a card, under a hairline.
fn row(cx: &App) -> Div {
    h_flex()
        .gap_4()
        .min_h(px(44.))
        .px(px(14.))
        .py(px(8.))
        .border_t_1()
        .border_color(ui::tokens(cx).border)
}

/// A 30 px rounded square tinted with `color`, with an icon in it.
fn icon_tile(icon: IconName, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .size(px(30.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .bg(color.opacity(0.14))
        .child(Icon::new(icon).size(px(15.)).text_color(color))
}

impl PracticeScreen {
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let unfinished = state.store.unfinished().unwrap_or_default();
        let finished = state.store.finished(8).unwrap_or_default();
        let history = state.store.answered().unwrap_or_default();
        let scored = library
            .questions
            .iter()
            .filter(|q| q.answer.is_some())
            .count();

        // Unfinished tests, newest first.
        let resume = unfinished.iter().map(|attempt| {
            let id = attempt.id;
            let total = attempt.answers.len().max(1);
            ui::card(cx).child(
                h_flex()
                    .p(px(14.))
                    .gap(px(14.))
                    .child(icon_tile(IconName::Timer, t.accent))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_semibold()
                                    .child(attempt.title.clone()),
                            )
                            .child(ui::progress(
                                attempt.answered() as f32 / total as f32,
                                t.accent,
                                cx,
                            ))
                            .child(ui::meta(
                                format!(
                                    "{} of {} answered · {} left · started {}",
                                    attempt.answered(),
                                    attempt.answers.len(),
                                    clock(attempt.seconds_left()),
                                    attempt.started.format("%-d %b, %H:%M")
                                ),
                                cx,
                            )),
                    )
                    .child(
                        Button::new(("resume", id as u64))
                            .primary()
                            .small()
                            .icon(Icon::new(IconName::Play))
                            .label("Resume")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let _ = this.shell.update(cx, |shell, cx| shell.resume(id, cx));
                            })),
                    ),
            )
        });

        // Micro, mini, mock and the personalised mix, as tiles.
        let quick = [
            (
                TestKind::Micro,
                "start-micro",
                IconName::Zap,
                "A quick check between reading sessions.",
            ),
            (
                TestKind::Mini,
                "start-mini",
                IconName::Timer,
                "A focused half hour.",
            ),
            (
                TestKind::Mock,
                "start-mock",
                IconName::FileText,
                "The full paper: 100 questions in 2 hours.",
            ),
            (
                TestKind::Mix,
                "start-mix",
                IconName::Shuffle,
                "More of what you got wrong and your weak topics.",
            ),
        ]
        .map(|(kind, id, icon, blurb)| {
            let plan = TestPlan::standard(kind, library);
            let fits = plan.fits(library);
            let title = plan.kind.title();
            let size = format!(
                "{} · {}",
                count(plan.count, "question", "questions"),
                duration(plan.seconds)
            );
            ui::card(cx)
                .flex_1()
                .min_w_0()
                .p(px(14.))
                .gap(px(8.))
                .when(!fits, |this| this.opacity(0.6))
                .child(
                    h_flex()
                        .justify_between()
                        .child(icon_tile(icon, t.accent))
                        .child(
                            Button::new(id)
                                .small()
                                .label("Start")
                                .disabled(!fits)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx)),
                                ),
                        ),
                )
                .child(div().pt_1().text_size(px(14.)).font_semibold().child(title))
                .child(ui::meta(size, cx))
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.))
                        .text_color(t.text_2)
                        .child(if fits {
                            blurb.to_string()
                        } else {
                            format!("The bank has {scored} questions, too few for this.")
                        }),
                )
        });

        // One 20-question drill per subject, with how the student does in it.
        let drills: Vec<AnyElement> = SUBJECTS
            .iter()
            .enumerate()
            .map(|(ix, subject)| {
                let plan = TestPlan::standard(TestKind::Subject(subject.id.to_string()), library);
                let available = mentor::pool(library, &plan.kind, &[]).len();
                let fits = plan.fits(library);
                let needed = plan.count;
                let color = ui::subject_id_color(subject.id, cx);
                let (right, answered) = history
                    .iter()
                    .filter(|past| {
                        mentor::subject_of_topic(&past.topic).is_some_and(|s| s.id == subject.id)
                    })
                    .fold((0, 0), |(right, total), past| {
                        (right + usize::from(past.correct), total + 1)
                    });
                ui::card(cx)
                    .flex_1()
                    .min_w_0()
                    .when(!fits, |this| this.opacity(0.6))
                    .child(div().h(px(3.)).rounded_t(px(8.)).bg(color))
                    .child(
                        v_flex()
                            .p(px(14.))
                            .gap(px(8.))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(ui::dot(color))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(13.5))
                                            .font_semibold()
                                            .child(subject.name),
                                    )
                                    .child(
                                        Button::new(("start-subject", ix))
                                            .small()
                                            .label("Drill")
                                            .disabled(!fits)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.sit(plan.clone(), cx)
                                            })),
                                    ),
                            )
                            .child(ui::meta(
                                if fits {
                                    format!(
                                        "{} in the bank",
                                        count(available, "question", "questions")
                                    )
                                } else {
                                    format!("{available} of the {needed} needed")
                                },
                                cx,
                            ))
                            .child(if answered == 0 {
                                ui::meta("Not tried yet", cx)
                            } else {
                                h_flex()
                                    .gap(px(10.))
                                    .child(div().flex_1().child(ui::progress(
                                        right as f32 / answered as f32,
                                        color,
                                        cx,
                                    )))
                                    .child(ui::meta(
                                        format!("{}% right", mentor::percent(right, answered)),
                                        cx,
                                    ))
                            }),
                    )
                    .into_any_element()
            })
            .collect();
        let mut drill_rows = Vec::new();
        let mut drills = drills.into_iter();
        loop {
            let three: Vec<AnyElement> = drills.by_ref().take(3).collect();
            if three.is_empty() {
                break;
            }
            let gaps = 3 - three.len();
            drill_rows.push(
                h_flex()
                    .items_stretch()
                    .gap(px(12.))
                    .children(three)
                    .children((0..gaps).map(|_| div().flex_1())),
            );
        }

        // The custom drill.
        let custom = self.custom_plan();
        let custom_fits = custom.fits(library);
        let available = mentor::pool(library, &custom.kind, &custom.subjects).len();
        let toggles = SUBJECTS.iter().enumerate().map(|(ix, subject)| {
            let on = self.subjects.iter().any(|id| id == subject.id);
            let id = subject.id;
            Button::new(("custom-subject", ix))
                .outline()
                .small()
                .label(subject.name)
                .selected(on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if on {
                        this.subjects.retain(|s| s != id);
                    } else {
                        this.subjects.push(id.to_string());
                    }
                    cx.notify();
                }))
        });
        let step = |id: &'static str, icon: IconName, enabled: bool| {
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(Icon::new(icon))
                .disabled(!enabled)
        };
        let minutes = custom.seconds.div_ceil(60);
        let custom_note = if self.subjects.is_empty() {
            "Pick one or more subjects.".to_string()
        } else if custom_fits {
            format!(
                "{} to draw from.",
                count(available, "question", "questions")
            )
        } else {
            format!("Only {available} questions in these subjects. Ask for fewer or add a subject.")
        };
        let stepper = |label: &'static str| {
            h_flex().gap(px(6.)).child(ui::meta(label, cx)).child(
                h_flex()
                    .gap_1()
                    .px_1()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(t.border_2),
            )
        };
        let questions = stepper("Questions");
        let time = stepper("Time");
        let custom_section = section(
            "Custom drill",
            "Choose the subjects, the number of questions and the time.",
            cx,
        )
        .child(
            ui::card(cx)
                .p(px(14.))
                .gap(px(12.))
                .child(h_flex().flex_wrap().gap_2().children(toggles))
                .child(
                    h_flex()
                        .gap_6()
                        .child(
                            questions
                                .child(
                                    step(
                                        "fewer-questions",
                                        IconName::Minus,
                                        self.count > CUSTOM_STEP,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.count = this
                                                .count
                                                .saturating_sub(CUSTOM_STEP)
                                                .max(CUSTOM_STEP);
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    div()
                                        .w(px(40.))
                                        .text_center()
                                        .text_size(px(13.))
                                        .font_medium()
                                        .child(self.count.to_string()),
                                )
                                .child(
                                    step("more-questions", IconName::Plus, self.count < CUSTOM_MAX)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.count = (this.count + CUSTOM_STEP).min(CUSTOM_MAX);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            time.child(
                                step("less-test-time", IconName::Minus, minutes > MINUTE_STEP)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.minutes = Some(
                                            minutes.saturating_sub(MINUTE_STEP).max(MINUTE_STEP),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .w(px(64.))
                                    .text_center()
                                    .text_size(px(13.))
                                    .font_medium()
                                    .child(format!("{minutes} min")),
                            )
                            .child(
                                step("more-test-time", IconName::Plus, minutes < MAX_MINUTES)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.minutes =
                                            Some((minutes + MINUTE_STEP).min(MAX_MINUTES));
                                        cx.notify();
                                    })),
                            ),
                        )
                        .when(self.minutes.is_none(), |this| {
                            this.child(ui::meta("72 s a question, as in Prelims", cx))
                        }),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            Button::new("start-custom")
                                .when(custom_fits, |this| this.primary())
                                .small()
                                .label("Start the drill")
                                .disabled(!custom_fits)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.sit(this.custom_plan(), cx)
                                })),
                        )
                        .child(ui::meta(custom_note, cx)),
                ),
        );

        // Past papers by year.
        let papers = library.papers();
        let paper_rows: Vec<_> = papers
            .iter()
            .enumerate()
            .map(|(ix, (year, questions))| {
                let year = *year;
                let plan = TestPlan::standard(TestKind::Paper(year), library);
                row(cx)
                    .when(ix == 0, |this| this.border_t_0())
                    .child(icon_tile(IconName::FileText, t.text_3))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_medium()
                                    .child(TestKind::Paper(year).title()),
                            )
                            .child(ui::meta(
                                format!(
                                    "{} · {} · UPSC's answer key",
                                    count(questions.len(), "question", "questions"),
                                    duration(plan.seconds)
                                ),
                                cx,
                            )),
                    )
                    .child(
                        Button::new(("read-paper", year as u64))
                            .ghost()
                            .small()
                            .label("Read with answers")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let _ = this
                                    .shell
                                    .update(cx, |shell, cx| shell.open_paper(year, cx));
                            })),
                    )
                    .child(
                        Button::new(("sit-paper", year as u64))
                            .small()
                            .label("Sit as a mock")
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx)),
                            ),
                    )
            })
            .collect();

        // Recent results.
        let results: Vec<_> = finished
            .iter()
            .enumerate()
            .map(|(ix, attempt)| {
                let id = attempt.id;
                let score = mentor::score(attempt);
                let full = mentor::full_marks(attempt);
                let share = if full > 0. {
                    (score.marks / full).max(0.) as f32
                } else {
                    0.
                };
                row(cx)
                    .id(("result", id as u64))
                    .test_support()
                    .when(ix == 0, |this| this.border_t_0())
                    .cursor_pointer()
                    .hover(|this| this.bg(t.hover))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.5))
                                    .font_medium()
                                    .child(attempt.title.clone()),
                            )
                            .child(ui::meta(
                                format!(
                                    "{} right · {} wrong · {} blank",
                                    score.right, score.wrong, score.blank
                                ),
                                cx,
                            )),
                    )
                    .child(div().w(px(96.)).child(ui::progress(share, t.accent, cx)))
                    .child(
                        div()
                            .w(px(84.))
                            .text_right()
                            .text_size(px(13.))
                            .font_semibold()
                            .child(format!("{} / {}", marks(score.marks), marks(full))),
                    )
                    .child(
                        div()
                            .w(px(96.))
                            .text_right()
                            .text_size(px(12.))
                            .text_color(t.text_3)
                            .child(
                                attempt
                                    .finished
                                    .map(|t| t.format("%-d %b, %H:%M").to_string())
                                    .unwrap_or_default(),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let _ = this.shell.update(cx, |shell, cx| shell.open_result(id, cx));
                    }))
            })
            .collect();

        ui::column(1040.)
            .gap(px(28.))
            .child(div().text_size(px(13.5)).text_color(t.text_2).child(
                "Every test runs in the exam room with UPSC marking: +2 for a right \
                         answer, −0.67 for a wrong one, nothing for a blank.",
            ))
            .when(!unfinished.is_empty(), |this| {
                this.child(
                    section(
                        "Unfinished",
                        "Tests you left or that closed early. The clock waits where you stopped.",
                        cx,
                    )
                    .children(resume),
                )
            })
            .child(
                section(
                    "Tests",
                    "Drawn from the whole bank. Questions you got wrong come back more often.",
                    cx,
                )
                .child(h_flex().items_stretch().gap(px(12.)).children(quick)),
            )
            .child(
                section(
                    "Subject drills",
                    format!("{} on one subject.", count(20, "question", "questions")),
                    cx,
                )
                .children(drill_rows),
            )
            .child(custom_section)
            .when(!papers.is_empty(), |this| {
                this.child(
                    section(
                        "Past papers",
                        "Real Prelims papers with UPSC's official answer keys. Read one with its \
                         answers or sit it against the clock.",
                        cx,
                    )
                    .child(ui::card(cx).children(paper_rows)),
                )
            })
            .when(!finished.is_empty(), |this| {
                this.child(
                    section("Recent results", "Open one to review every question.", cx)
                        .child(ui::card(cx).children(results)),
                )
            })
    }
}

impl Render for PracticeScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let library = &AppState::get(cx).library;
        let scored = library
            .questions
            .iter()
            .filter(|q| q.answer.is_some())
            .count();
        let header = ui::header(cx).child(
            h_flex()
                .flex_1()
                .gap_3()
                .child(ui::title(IconName::SquarePen, "Practice", cx))
                .child(ui::chip(cx).child(format!(
                    "{} in the bank",
                    count(scored, "question", "questions")
                ))),
        );
        ui::screen(header, ("screen", 2usize), self.body(cx))
    }
}

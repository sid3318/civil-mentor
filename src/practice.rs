//! The Practice hub: every kind of test, unfinished tests to resume, past
//! papers and recent results. Tests run in the exam room ([`crate::exam`]).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, StyledExt as _, h_flex,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell};
use crate::exam::clock;
use crate::mentor::{self, SECONDS_PER_QUESTION, SUBJECTS, TestKind, TestPlan};
use crate::review::marks;
use crate::screens::{count, page};

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

fn section(title: &'static str, note: impl Into<SharedString>, cx: &App) -> Div {
    v_flex().gap_2().child(
        v_flex()
            .child(div().text_lg().font_semibold().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(note.into()),
            ),
    )
}

fn row(cx: &App) -> Div {
    h_flex()
        .gap_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
}

impl Render for PracticeScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let unfinished = state.store.unfinished().unwrap_or_default();
        let finished = state.store.finished(8).unwrap_or_default();
        let scored = library
            .questions
            .iter()
            .filter(|q| q.answer.is_some())
            .count();

        let summary = format!(
            "{} in the bank. Every test runs in the exam room with UPSC marking: \
             +2 for a right answer, -0.67 for a wrong one, nothing for a blank.",
            count(scored, "question", "questions")
        );

        // Unfinished tests, newest first.
        let resume = (!unfinished.is_empty()).then(|| {
            section(
                "Unfinished",
                "Tests you left or that closed early. The clock waits where you stopped.",
                cx,
            )
            .children(unfinished.iter().map(|attempt| {
                let id = attempt.id;
                row(cx)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().font_medium().child(attempt.title.clone()))
                            .child(div().text_sm().text_color(muted).child(format!(
                                "{} of {} answered · {} left · started {}",
                                attempt.answered(),
                                attempt.answers.len(),
                                clock(attempt.seconds_left()),
                                attempt.started.format("%-d %b, %H:%M")
                            ))),
                    )
                    .child(
                        Button::new(("resume", id as u64))
                            .primary()
                            .small()
                            .label("Resume")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let _ = this.shell.update(cx, |shell, cx| shell.resume(id, cx));
                            })),
                    )
            }))
        });

        // Micro, mini, mock and the personalised mix, as cards.
        let quick = [
            (
                TestKind::Micro,
                "start-micro",
                "A quick check between reading sessions.",
            ),
            (TestKind::Mini, "start-mini", "A focused half hour."),
            (
                TestKind::Mock,
                "start-mock",
                "The full paper: 100 questions in 2 hours.",
            ),
            (
                TestKind::Mix,
                "start-mix",
                "More of what you got wrong and your weak topics.",
            ),
        ]
        .map(|(kind, id, blurb)| {
            let plan = TestPlan::standard(kind, library);
            let fits = plan.fits(library);
            let label = format!(
                "{} · {}",
                count(plan.count, "question", "questions"),
                duration(plan.seconds)
            );
            v_flex()
                .flex_1()
                .min_w_0()
                .p_4()
                .gap_2()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .child(div().font_semibold().child(plan.kind.title()))
                .child(div().text_sm().child(label))
                .child(div().flex_1().text_sm().text_color(muted).child(if fits {
                    blurb.to_string()
                } else {
                    format!("The bank has {scored} questions, too few for this.")
                }))
                .child(
                    h_flex().child(
                        Button::new(id)
                            .outline()
                            .small()
                            .label("Start")
                            .disabled(!fits)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx)),
                            ),
                    ),
                )
        });

        // One 20-question drill per subject.
        let drills = SUBJECTS.iter().enumerate().map(|(ix, subject)| {
            let plan = TestPlan::standard(TestKind::Subject(subject.id.to_string()), library);
            let available = mentor::pool(library, &plan.kind, &[]).len();
            let fits = plan.fits(library);
            row(cx)
                .child(div().flex_1().min_w_0().child(subject.name))
                .child(div().text_sm().text_color(muted).child(if fits {
                    count(available, "question", "questions")
                } else {
                    format!("{available} of the {} needed", plan.count)
                }))
                .child(
                    Button::new(("start-subject", ix))
                        .outline()
                        .small()
                        .label("Start")
                        .disabled(!fits)
                        .on_click(cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx))),
                )
        });

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
        let step = |id: &'static str, label: &'static str, enabled: bool| {
            Button::new(id)
                .outline()
                .small()
                .label(label)
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
        let custom_section = section(
            "Custom drill",
            "Choose the subjects, the number of questions and the time.",
            cx,
        )
        .child(h_flex().flex_wrap().gap_2().children(toggles))
        .child(
            h_flex()
                .gap_6()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().text_sm().child("Questions"))
                        .child(
                            step("fewer-questions", "-", self.count > CUSTOM_STEP).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.count =
                                        this.count.saturating_sub(CUSTOM_STEP).max(CUSTOM_STEP);
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(div().w(px(48.)).text_center().child(self.count.to_string()))
                        .child(
                            step("more-questions", "+", self.count < CUSTOM_MAX).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.count = (this.count + CUSTOM_STEP).min(CUSTOM_MAX);
                                    cx.notify();
                                }),
                            ),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().text_sm().child("Time"))
                        .child(step("less-test-time", "-", minutes > MINUTE_STEP).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.minutes =
                                    Some(minutes.saturating_sub(MINUTE_STEP).max(MINUTE_STEP));
                                cx.notify();
                            }),
                        ))
                        .child(
                            div()
                                .w(px(72.))
                                .text_center()
                                .child(format!("{minutes} min")),
                        )
                        .child(step("more-test-time", "+", minutes < MAX_MINUTES).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.minutes = Some((minutes + MINUTE_STEP).min(MAX_MINUTES));
                                cx.notify();
                            }),
                        ))
                        .when(self.minutes.is_none(), |this| {
                            this.child(div().text_sm().text_color(muted).child("72 s a question"))
                        }),
                ),
        )
        .child(
            h_flex()
                .gap_3()
                .child(
                    Button::new("start-custom")
                        .outline()
                        .small()
                        .label("Start")
                        .disabled(!custom_fits)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.sit(this.custom_plan(), cx)),
                        ),
                )
                .child(div().text_sm().text_color(muted).child(custom_note)),
        );

        // Past papers by year.
        let papers = library.papers();
        let paper_rows = papers.iter().map(|(year, questions)| {
            let year = *year;
            let plan = TestPlan::standard(TestKind::Paper(year), library);
            row(cx)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(TestKind::Paper(year).title()),
                )
                .child(div().text_sm().text_color(muted).child(format!(
                    "{} · {}",
                    count(questions.len(), "question", "questions"),
                    duration(plan.seconds)
                )))
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
                        .outline()
                        .small()
                        .label("Sit as a mock")
                        .on_click(cx.listener(move |this, _, _, cx| this.sit(plan.clone(), cx))),
                )
        });

        // Recent results.
        let results = finished.iter().map(|attempt| {
            let id = attempt.id;
            let score = mentor::score(attempt);
            row(cx)
                .id(("result", id as u64))
                .test_support()
                .cursor_pointer()
                .hover(|this| this.bg(theme.accent))
                .child(div().flex_1().min_w_0().child(attempt.title.clone()))
                .child(div().text_sm().text_color(muted).child(format!(
                    "{} right, {} wrong, {} blank",
                    score.right, score.wrong, score.blank
                )))
                .child(div().w(px(120.)).text_right().font_medium().child(format!(
                    "{} of {}",
                    marks(score.marks),
                    marks(mentor::full_marks(attempt))
                )))
                .child(
                    div()
                        .w(px(110.))
                        .text_right()
                        .text_sm()
                        .text_color(muted)
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
        });

        page(Screen::Practice.title(), summary, cx)
            .children(resume)
            .child(
                section(
                    "Tests",
                    "Questions are drawn from the whole bank. Ones you got wrong come back more often.",
                    cx,
                )
                .child(h_flex().items_stretch().gap_3().children(quick)),
            )
            .child(
                section(
                    "Subject drills",
                    format!("{} on one subject.", count(20, "question", "questions")),
                    cx,
                )
                .children(drills),
            )
            .child(custom_section)
            .when(!papers.is_empty(), |page| {
                page.child(
                    section(
                        "Past papers",
                        "Real Prelims papers with UPSC's official answer keys. Read one with its answers or sit it against the clock.",
                        cx,
                    )
                    .children(paper_rows),
                )
            })
            .when(!finished.is_empty(), |page| {
                page.child(section("Recent results", "Open one to review every question.", cx).children(results))
            })
            .child(div().h(px(32.)))
    }
}

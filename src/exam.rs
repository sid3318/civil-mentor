//! The exam room: one question at a time against the clock, filling the
//! whole window. Every change is saved the moment it happens, so a sitting
//! survives quitting or a crash and Practice can resume it.

use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, Timelike as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::mentor::{self, TestPlan};
use crate::store::{Attempt, Confidence};

actions!(
    exam,
    [
        ChooseA,
        ChooseB,
        ChooseC,
        ChooseD,
        ToggleMark,
        NextQuestion,
        PreviousQuestion
    ]
);

/// The keys work only while the exam room has the focus, so they never
/// reach a text field elsewhere.
const KEYS: &str = "ExamRoom";
pub(crate) const LETTERS: [&str; 4] = ["A", "B", "C", "D"];
/// The clock turns red with this much time left.
const LOW_TIME: u32 = 5 * 60;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("a", ChooseA, Some(KEYS)),
        KeyBinding::new("b", ChooseB, Some(KEYS)),
        KeyBinding::new("c", ChooseC, Some(KEYS)),
        KeyBinding::new("d", ChooseD, Some(KEYS)),
        KeyBinding::new("m", ToggleMark, Some(KEYS)),
        KeyBinding::new("right", NextQuestion, Some(KEYS)),
        KeyBinding::new("down", NextQuestion, Some(KEYS)),
        KeyBinding::new("left", PreviousQuestion, Some(KEYS)),
        KeyBinding::new("up", PreviousQuestion, Some(KEYS)),
    ]);
}

/// The local time to the second, as the store keeps it.
fn now() -> NaiveDateTime {
    let now = Local::now().naive_local();
    now.with_nanosecond(0).unwrap_or(now)
}

/// "21:48", or "1:05:00" past an hour.
pub(crate) fn clock(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub struct ExamScreen {
    shell: WeakEntity<Shell>,
    focus: FocusHandle,
    session: Option<Session>,
}

struct Session {
    attempt: Attempt,
    /// Each question's place in the library, or `None` when the content no
    /// longer has it.
    questions: Vec<Option<usize>>,
    /// Asking "submit with questions unanswered?".
    confirm: bool,
    /// Up to when the clock has been counted.
    counted: Instant,
    /// Take the keyboard on the next frame, so the letter keys work at once.
    take_focus: bool,
    _clock: Task<()>,
}

impl ExamScreen {
    pub fn new(shell: WeakEntity<Shell>, cx: &mut Context<Self>) -> Self {
        ExamScreen {
            shell,
            focus: cx.focus_handle(),
            session: None,
        }
    }

    /// The sitting on screen, if any.
    pub fn attempt(&self) -> Option<&Attempt> {
        self.session.as_ref().map(|session| &session.attempt)
    }

    /// Chooses the questions for `plan`, saves the new sitting and opens it.
    /// Returns false when it could not be saved.
    pub fn start(&mut self, plan: &TestPlan, cx: &mut Context<Self>) -> bool {
        let now = now();
        let seed = now.and_utc().timestamp_micros() as u64;
        let state = AppState::get(cx);
        let history = state.store.answered().unwrap_or_default();
        let questions = mentor::pick(&state.library, plan, &history, seed);
        let mut attempt = mentor::new_attempt(&state.library, plan, &questions, now);
        let started = state.store.start_attempt(&attempt);
        match started {
            Ok(id) => attempt.id = id,
            Err(err) => {
                AppState::report(cx, err);
                return false;
            }
        }
        self.open(attempt, cx);
        true
    }

    /// Reopens an unfinished sitting where the student left it.
    pub fn resume(&mut self, id: i64, cx: &mut Context<Self>) -> bool {
        match AppState::get(cx).store.attempt(id) {
            Ok(Some(attempt)) if attempt.finished.is_none() => {
                self.open(attempt, cx);
                true
            }
            Ok(_) => false,
            Err(err) => {
                AppState::report(cx, err);
                false
            }
        }
    }

    fn open(&mut self, mut attempt: Attempt, cx: &mut Context<Self>) {
        let library = &AppState::get(cx).library;
        let questions = attempt
            .answers
            .iter()
            .map(|answer| library.question_ix(&answer.question))
            .collect();
        attempt.current = attempt.current.min(attempt.answers.len().saturating_sub(1));
        if let Some(answer) = attempt.answers.get_mut(attempt.current) {
            answer.visited = true;
        }
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        self.session = Some(Session {
            attempt,
            questions,
            confirm: false,
            counted: cx.background_executor().now(),
            take_focus: true,
            _clock: clock,
        });
        self.save(cx);
        cx.notify();
    }

    /// Counts the seconds since the last tick against the test and the
    /// question on screen, and submits the test when time is up.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let Some(session) = &mut self.session else {
            return;
        };
        let seconds = now.saturating_duration_since(session.counted).as_secs();
        if seconds == 0 {
            return;
        }
        session.counted += Duration::from_secs(seconds);
        let attempt = &mut session.attempt;
        let spent = (seconds as u32).min(attempt.seconds_left());
        attempt.seconds_used += spent;
        let current = attempt.current;
        if let Some(answer) = attempt.answers.get_mut(current) {
            answer.seconds += spent;
        }
        let out_of_time = attempt.seconds_left() == 0;
        self.save(cx);
        if out_of_time {
            self.submit(cx);
        } else {
            cx.notify();
        }
    }

    /// Saves the question on screen and the clock.
    fn save(&self, cx: &mut App) {
        if let Some(session) = &self.session {
            let attempt = &session.attempt;
            if let Err(err) = AppState::get(cx)
                .store
                .save_answer(attempt, attempt.current)
            {
                AppState::report(cx, err);
            }
        }
    }

    /// Changes the question on screen, saves it and redraws.
    fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Session, &App)) {
        let Some(session) = &mut self.session else {
            return;
        };
        change(session, cx);
        session.confirm = false;
        self.save(cx);
        cx.notify();
    }

    fn choose(&mut self, option: usize, cx: &mut Context<Self>) {
        self.change(cx, |session, cx| {
            let current = session.attempt.current;
            let right = session.questions[current]
                .and_then(|ix| AppState::get(cx).library.questions[ix].answer);
            let answer = &mut session.attempt.answers[current];
            answer.chosen = Some(option);
            answer.correct = right.map(|right| right == option);
            if answer.is_struck(option) {
                answer.toggle_strike(option);
            }
        });
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.change(cx, |session, _| {
            let answer = &mut session.attempt.answers[session.attempt.current];
            answer.chosen = None;
            answer.correct = None;
            answer.confidence = None;
        });
    }

    fn strike(&mut self, option: usize, cx: &mut Context<Self>) {
        self.change(cx, |session, _| {
            let answer = &mut session.attempt.answers[session.attempt.current];
            answer.toggle_strike(option);
            // Striking out the chosen option takes the answer back.
            if answer.is_struck(option) && answer.chosen == Some(option) {
                answer.chosen = None;
                answer.correct = None;
            }
        });
    }

    fn toggle_mark(&mut self, cx: &mut Context<Self>) {
        self.change(cx, |session, _| {
            let answer = &mut session.attempt.answers[session.attempt.current];
            answer.marked = !answer.marked;
        });
    }

    fn set_confidence(&mut self, confidence: Confidence, cx: &mut Context<Self>) {
        self.change(cx, |session, _| {
            let answer = &mut session.attempt.answers[session.attempt.current];
            answer.confidence = (answer.confidence != Some(confidence)).then_some(confidence);
        });
    }

    fn go_to(&mut self, position: usize, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        if position >= session.attempt.answers.len() || position == session.attempt.current {
            return;
        }
        // The question being left keeps its time; the clock goes on with the next.
        self.save(cx);
        self.change(cx, |session, _| {
            session.attempt.current = position;
            session.attempt.answers[position].visited = true;
        });
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            let position = session.attempt.current as isize + by;
            if position >= 0 {
                self.go_to(position as usize, cx);
            }
        }
    }

    fn submit_clicked(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let unanswered = session.attempt.answers.len() - session.attempt.answered();
        let marked = session.attempt.answers.iter().any(|a| a.marked);
        if (unanswered > 0 || marked) && !session.confirm {
            session.confirm = true;
            cx.notify();
        } else {
            self.submit(cx);
        }
    }

    /// Closes the sitting and shows its result.
    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        session.attempt.finished = Some(now());
        if let Err(err) = AppState::get(cx).store.finish_attempt(&session.attempt) {
            AppState::report(cx, err);
        }
        let id = session.attempt.id;
        let _ = self.shell.update(cx, |shell, cx| shell.show_result(id, cx));
        cx.notify();
    }

    /// Leaves the room. Everything is already saved, so Practice can resume it.
    fn leave(&mut self, cx: &mut Context<Self>) {
        self.save(cx);
        self.session = None;
        let _ = self.shell.update(cx, |shell, cx| shell.leave_exam(cx));
        cx.notify();
    }

    fn header(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let attempt = &session.attempt;
        let left = attempt.seconds_left();
        h_flex()
            .flex_shrink_0()
            .h(px(52.))
            .px_4()
            // The window's own buttons sit at the top left on macOS.
            .when(cfg!(target_os = "macos"), |this| this.pl(px(84.)))
            .gap_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_semibold()
                    .child(attempt.title.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Question {} of {}",
                        attempt.current + 1,
                        attempt.answers.len()
                    )),
            )
            .child(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .gap_3()
                    .child(
                        h_flex()
                            .id("clock")
                            .gap_1()
                            .font_semibold()
                            .when(left <= LOW_TIME, |this| this.text_color(cx.theme().danger))
                            .child(Icon::new(IconName::Timer).small())
                            .child(format!("{} left", clock(left))),
                    )
                    .child(
                        Button::new("leave")
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::LogOut))
                            .label("Save and leave")
                            .tooltip("Your answers and time are saved. Resume from Practice.")
                            .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
                    )
                    .child(
                        Button::new("submit")
                            .primary()
                            .small()
                            .label("Submit")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_clicked(cx))),
                    ),
            )
    }

    fn question(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let library = &AppState::get(cx).library;
        let position = session.attempt.current;
        let answer = &session.attempt.answers[position];
        let Some(question) = session.questions[position].map(|ix| &library.questions[ix]) else {
            return v_flex()
                .flex_1()
                .p_8()
                .text_color(cx.theme().muted_foreground)
                .child("This question is no longer in the question bank. Move on to the next.")
                .into_any_element();
        };
        let subject = mentor::subject_of(question).map_or("", |s| s.name);
        let meta = [subject, question.source.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");

        let options = question.options.iter().enumerate().map(|(option, text)| {
            let chosen = answer.chosen == Some(option);
            let struck = answer.is_struck(option);
            h_flex()
                .gap_2()
                .child(
                    h_flex()
                        .id(("option", option))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .items_start()
                        .gap_3()
                        .p_3()
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(if chosen {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .when(chosen, |this| this.bg(cx.theme().accent))
                        .cursor_pointer()
                        .hover(|this| this.bg(cx.theme().accent))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .font_medium()
                                .when(chosen, |this| {
                                    this.bg(cx.theme().primary)
                                        .border_color(cx.theme().primary)
                                        .text_color(cx.theme().primary_foreground)
                                })
                                .child(LETTERS[option]),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .pt(px(2.))
                                .when(struck, |this| {
                                    this.line_through().text_color(cx.theme().muted_foreground)
                                })
                                .child(text.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(option, cx))),
                )
                .child(
                    Button::new(("strike", option))
                        .ghost()
                        .small()
                        .icon(Icon::new(IconName::Strikethrough))
                        .selected(struck)
                        .tooltip(if struck {
                            "Bring this option back"
                        } else {
                            "Strike this option out"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.strike(option, cx))),
                )
        });

        v_flex()
            .id("exam-question")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(780.))
                    .mx_auto()
                    .px_8()
                    .py_6()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(meta),
                    )
                    .child(
                        div()
                            .text_size(px(17.))
                            .child(TextView::markdown(
                                ("exam-stem", position),
                                question.stem.clone(),
                            )),
                    )
                    .child(v_flex().gap_2().children(options))
                    .when(question.answer.is_none(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("UPSC dropped this question from its answer key, so it carries no marks."),
                        )
                    }),
            )
            .into_any_element()
    }

    fn panel(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let attempt = &session.attempt;
        let current = &attempt.answers[attempt.current];
        let theme = cx.theme();
        let cells = attempt
            .answers
            .iter()
            .enumerate()
            .map(|(position, answer)| {
                let (bg, fg, border) = if answer.marked {
                    (theme.warning, theme.warning_foreground, theme.warning)
                } else if answer.chosen.is_some() {
                    (theme.primary, theme.primary_foreground, theme.primary)
                } else if answer.visited {
                    (theme.muted, theme.foreground, theme.muted)
                } else {
                    (theme.background, theme.muted_foreground, theme.border)
                };
                div()
                    .id(("cell", position))
                    .test_support()
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radius)
                    .bg(bg)
                    .text_color(fg)
                    .border_1()
                    .border_color(border)
                    .text_sm()
                    .cursor_pointer()
                    .when(position == attempt.current, |this| {
                        this.border_2().border_color(theme.ring).font_semibold()
                    })
                    .child((position + 1).to_string())
                    .on_click(cx.listener(move |this, _, _, cx| this.go_to(position, cx)))
            });
        let count = |test: &dyn Fn(&crate::store::Answer) -> bool| {
            attempt.answers.iter().filter(|a| test(a)).count()
        };
        let answered = count(&|a| a.chosen.is_some());
        let marked = count(&|a| a.marked);
        let unseen = count(&|a| !a.visited);
        let seen = attempt.answers.len() - answered - unseen;
        let key = |bg: Hsla, border: Hsla, label: String| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .size(px(10.))
                        .rounded(px(2.))
                        .bg(bg)
                        .border_1()
                        .border_color(border),
                )
                .child(label)
        };

        let confidence = ButtonGroup::new("confidence")
            .outline()
            .small()
            .disabled(current.chosen.is_none())
            .children(Confidence::ALL.map(|level| {
                Button::new(match level {
                    Confidence::Sure => "sure",
                    Confidence::FiftyFifty => "fifty-fifty",
                    Confidence::Guess => "guess",
                })
                .label(level.label())
                .selected(current.confidence == Some(level))
            }))
            .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                if let Some(&ix) = clicked.first() {
                    this.set_confidence(Confidence::ALL[ix], cx);
                }
            }));

        v_flex()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            // The grid scrolls on its own, so the confidence tag stays in
            // view even in a 100-question paper.
            .child(
                v_flex()
                    .id("exam-grid")
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .gap_4()
                    .overflow_y_scrollbar()
                    .child(div().flex().flex_wrap().gap_1().children(cells))
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap_x_3()
                            .gap_y_1()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(key(
                                theme.primary,
                                theme.primary,
                                format!("answered {answered}"),
                            ))
                            .child(key(
                                theme.warning,
                                theme.warning,
                                format!("marked {marked}"),
                            ))
                            .child(key(
                                theme.muted,
                                theme.muted,
                                format!("not answered {seen}"),
                            ))
                            .child(key(
                                theme.background,
                                theme.border,
                                format!("not visited {unseen}"),
                            )),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .p_4()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(div().text_sm().font_medium().child("How sure are you?"))
                    .child(confidence)
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Tagging each answer shows later whether guessing wins or loses you marks.",
                    ))
                    .child(
                        div()
                            .pt_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Keys: A to D answer, M marks, arrow keys move."),
                    ),
            )
    }

    fn footer(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let attempt = &session.attempt;
        let current = &attempt.answers[attempt.current];
        h_flex()
            .flex_shrink_0()
            .h(px(56.))
            .px_4()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("mark")
                    .outline()
                    .small()
                    .icon(Icon::new(IconName::Flag))
                    .label(if current.marked {
                        "Marked for review"
                    } else {
                        "Mark for review"
                    })
                    .selected(current.marked)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_mark(cx))),
            )
            .child(
                Button::new("clear")
                    .ghost()
                    .small()
                    .label("Clear")
                    .disabled(current.chosen.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new("previous")
                    .outline()
                    .small()
                    .icon(Icon::new(IconName::ChevronLeft))
                    .label("Previous")
                    .disabled(attempt.current == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
            )
            .child(
                Button::new("next")
                    .outline()
                    .small()
                    .label("Next")
                    .child(Icon::new(IconName::ChevronRight))
                    .disabled(attempt.current + 1 == attempt.answers.len())
                    .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
            )
    }
}

impl Render for ExamScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let Some(session) = &mut self.session else {
            return div().into_any_element();
        };
        if std::mem::take(&mut session.take_focus) {
            window.focus(&focus, cx);
        }
        let Some(session) = &self.session else {
            return div().into_any_element();
        };
        let attempt = &session.attempt;
        let unanswered = attempt.answers.len() - attempt.answered();
        let marked = attempt.answers.iter().filter(|a| a.marked).count();
        let confirm = session.confirm.then(|| {
            h_flex()
                .flex_shrink_0()
                .px_4()
                .py_2()
                .gap_3()
                .bg(cx.theme().warning)
                .text_color(cx.theme().warning_foreground)
                .text_sm()
                .child(div().flex_1().child(format!(
                    "{} unanswered and {} marked for review. Submit now?",
                    crate::screens::count(unanswered, "question", "questions"),
                    marked
                )))
                .child(
                    Button::new("keep-going")
                        .ghost()
                        .small()
                        .label("Keep going")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(session) = &mut this.session {
                                session.confirm = false;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("submit-anyway")
                        .primary()
                        .small()
                        .label("Submit anyway")
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                )
        });

        v_flex()
            .id("exam-room")
            .key_context(KEYS)
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &ChooseA, _, cx| this.choose(0, cx)))
            .on_action(cx.listener(|this, _: &ChooseB, _, cx| this.choose(1, cx)))
            .on_action(cx.listener(|this, _: &ChooseC, _, cx| this.choose(2, cx)))
            .on_action(cx.listener(|this, _: &ChooseD, _, cx| this.choose(3, cx)))
            .on_action(cx.listener(|this, _: &ToggleMark, _, cx| this.toggle_mark(cx)))
            .on_action(cx.listener(|this, _: &NextQuestion, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousQuestion, _, cx| this.step(-1, cx)))
            .child(self.header(session, cx))
            .children(confirm)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.question(session, cx))
                    .child(self.panel(session, cx)),
            )
            .child(self.footer(session, cx))
            .into_any_element()
    }
}

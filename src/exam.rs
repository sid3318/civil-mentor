//! The exam room: one question at a time against the clock, filling the
//! whole window. Every change is saved the moment it happens, so a sitting
//! survives quitting or a crash and Practice can resume it.

use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, Timelike as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::mentor::{self, TestPlan};
use crate::store::{Attempt, Confidence, Study};
use crate::ui;

actions!(
    exam,
    [
        ChooseA,
        ChooseB,
        ChooseC,
        ChooseD,
        ToggleMark,
        NextQuestion,
        PreviousQuestion,
        TagSure,
        TagFiftyFifty,
        TagGuess,
        SubmitTest
    ]
);

/// The keys work only while the exam room has the focus, so they never
/// reach a text field elsewhere.
const KEYS: &str = "ExamRoom";
pub(crate) const LETTERS: [&str; 4] = ["A", "B", "C", "D"];
/// How the submit shortcut is shown.
const SUBMIT_KEY: &str = if cfg!(target_os = "macos") {
    "⌘↵"
} else {
    "Ctrl ↵"
};
/// The clock turns amber with this much time left, and red in the last minute.
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
        KeyBinding::new("1", TagSure, Some(KEYS)),
        KeyBinding::new("2", TagFiftyFifty, Some(KEYS)),
        KeyBinding::new("3", TagGuess, Some(KEYS)),
    ]);
    // `secondary` is Cmd on macOS and Ctrl elsewhere; only `load` resolves it.
    let submit = KeyBinding::load(
        "secondary-enter",
        Box::new(SubmitTest),
        Some(std::rc::Rc::new(
            KeyBindingContextPredicate::parse(KEYS).expect("the context is spelled correctly"),
        )),
        false,
        None,
        cx,
    )
    .expect("the shortcut is spelled correctly");
    cx.bind_keys([submit]);
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
    /// The block of study time this visit to the room is counted in.
    study: Option<i64>,
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
            study: None,
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
        if spent > 0 {
            let item = attempt.id.to_string();
            AppState::record_study(cx, Study::Test, &item, &mut session.study, spent);
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
        let state = AppState::get(cx);
        let mistakes = mentor::mistakes(&session.attempt, &state.reviews, AppState::today(cx));
        AppState::save_reviews(cx, mistakes);
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
        let t = ui::tokens(cx);
        let attempt = &session.attempt;
        let left = attempt.seconds_left();
        let warn = if left <= 60 {
            Some(t.danger)
        } else if left <= LOW_TIME {
            Some(t.attention)
        } else {
            None
        };
        ui::header(cx)
            .h(px(52.))
            // The window's own buttons sit at the top left on macOS.
            .when(cfg!(target_os = "macos"), |this| this.pl(px(84.)))
            .child(
                h_flex()
                    .flex_1()
                    .gap_3()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_3()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.5))
                                    .font_semibold()
                                    .child(attempt.title.clone()),
                            )
                            .child(ui::meta(
                                format!(
                                    "Question {} of {}",
                                    attempt.current + 1,
                                    attempt.answers.len()
                                ),
                                cx,
                            )),
                    )
                    .child(
                        h_flex()
                            .id("clock")
                            .test_support()
                            .h(px(30.))
                            .px(px(13.))
                            .gap(px(7.))
                            .rounded_full()
                            .bg(t.sunken)
                            .border_1()
                            .border_color(warn.unwrap_or(t.border))
                            .text_size(px(14.5))
                            .font_semibold()
                            .text_color(warn.unwrap_or(t.text))
                            .child(
                                Icon::new(IconName::Timer)
                                    .size(px(15.))
                                    .text_color(warn.unwrap_or(t.text_3)),
                            )
                            .child(clock(left)),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .justify_end()
                            .gap_1()
                            .child(
                                Button::new("leave")
                                    .ghost()
                                    .small()
                                    .icon(Icon::new(IconName::LogOut))
                                    .label("Save and leave")
                                    .tooltip(
                                        "Your answers and time are saved. Resume from Practice.",
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
                            )
                            .child(
                                Button::new("submit")
                                    .primary()
                                    .small()
                                    .label("Submit")
                                    .child(ui::key(SUBMIT_KEY, true, cx))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.submit_clicked(cx)),
                                    ),
                            ),
                    ),
            )
    }

    fn question(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let position = session.attempt.current;
        let answer = &session.attempt.answers[position];
        let Some(question) = session.questions[position].map(|ix| &library.questions[ix]) else {
            return v_flex()
                .flex_1()
                .p_8()
                .child(ui::meta(
                    "This question is no longer in the question bank. Move on to the next.",
                    cx,
                ))
                .into_any_element();
        };
        let topic = question.topics.first().cloned().unwrap_or_default();
        let subject = mentor::subject_of(question).map(|s| s.name);

        let options = question.options.iter().enumerate().map(|(option, text)| {
            let chosen = answer.chosen == Some(option);
            let struck = answer.is_struck(option);
            h_flex()
                .gap(px(6.))
                .child(
                    h_flex()
                        .id(("option", option))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .min_h(px(52.))
                        .gap(px(14.))
                        .px(px(12.))
                        .py(px(10.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(if chosen { t.accent } else { t.border_2 })
                        .bg(if chosen { t.accent_soft } else { t.card })
                        .when(struck, |this| this.opacity(0.45))
                        .cursor_pointer()
                        .hover(|this| this.border_color(if chosen { t.accent } else { t.border_3 }))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_1()
                                .border_color(if chosen { t.accent } else { t.border_2 })
                                .when(chosen, |this| this.bg(t.accent).text_color(t.on_accent))
                                .when(!chosen, |this| this.text_color(t.text_2))
                                .text_size(px(12.5))
                                .font_semibold()
                                .child(LETTERS[option]),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(15.))
                                .when(struck, |this| this.line_through().text_color(t.text_3))
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

        let answered = answer.chosen.is_some();
        let tags = Confidence::ALL.into_iter().enumerate().map(|(ix, level)| {
            let id = match level {
                Confidence::Sure => "sure",
                Confidence::FiftyFifty => "fifty-fifty",
                Confidence::Guess => "guess",
            };
            ui::segment(id, level.label(), answer.confidence == Some(level), cx)
                .child(
                    ui::key((ix + 1).to_string(), false, cx)
                        .h(px(16.))
                        .min_w(px(16.)),
                )
                .when(!answered, |this| this.opacity(0.5))
                .when(answered, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| this.set_confidence(level, cx)))
                })
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
                    .max_w(px(760.))
                    .mx_auto()
                    .px_8()
                    .py(px(28.))
                    .gap(px(14.))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .when_some(subject, |this, subject| {
                                this.child(ui::subject_chip(
                                    subject,
                                    ui::subject_color(&topic, cx),
                                    cx,
                                ))
                            })
                            .when(!question.source.is_empty(), |this| {
                                this.child(ui::chip(cx).child(question.source.clone()))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(17.))
                            .line_height(px(27.))
                            .child(TextView::markdown(
                                ("exam-stem", position),
                                question.stem.clone(),
                            )),
                    )
                    .child(v_flex().gap(px(8.)).children(options))
                    .when(question.answer.is_none(), |this| {
                        this.child(ui::meta(
                            "UPSC dropped this question from its answer key, so it carries no marks.",
                            cx,
                        ))
                    })
                    .child(
                        h_flex()
                            .gap_3()
                            .pt(px(6.))
                            .child(ui::meta("How sure are you?", cx))
                            .child(ui::segmented(cx).children(tags))
                            .when(!answered, |this| {
                                this.child(ui::meta("Answer first, then tag it.", cx))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn panel(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let attempt = &session.attempt;
        let cells = attempt
            .answers
            .iter()
            .enumerate()
            .map(|(position, answer)| {
                let answered = answer.chosen.is_some();
                let (bg, fg, border) = if answered {
                    (t.accent, t.on_accent, t.accent)
                } else if answer.marked {
                    (t.attention, rgb(0x1e1d1a).into(), t.attention)
                } else if answer.visited {
                    (transparent_black(), t.text_2, t.border_3)
                } else {
                    (transparent_black(), t.text_4, t.border)
                };
                div()
                    .id(("cell", position))
                    .test_support()
                    .relative()
                    .size(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(bg)
                    .text_color(fg)
                    .border_1()
                    .border_color(border)
                    .text_size(px(12.5))
                    .font_medium()
                    .cursor_pointer()
                    .when(position == attempt.current, |this| {
                        this.border_2().border_color(t.text).font_semibold()
                    })
                    .when(answered && answer.marked, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-4.))
                                .right(px(-4.))
                                .size(px(11.))
                                .rounded_full()
                                .bg(t.attention)
                                .border_2()
                                .border_color(t.canvas),
                        )
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
        let key = |fill: Option<Hsla>, edge: Hsla, label: String| {
            h_flex()
                .gap(px(7.))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(10.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(edge)
                        .when_some(fill, |this, fill| this.bg(fill)),
                )
                .child(label)
        };
        let keys = [
            (vec!["A", "D"], "answer"),
            (vec!["M"], "mark for review"),
            (vec!["←", "→"], "move"),
            (vec!["1", "2", "3"], "how sure you are"),
            (vec![SUBMIT_KEY], "submit"),
        ]
        .map(|(caps, what)| {
            let range = caps.len() == 2 && caps[0] == "A";
            h_flex()
                .gap(px(4.))
                .children(caps.iter().enumerate().map(|(ix, cap)| {
                    h_flex()
                        .gap(px(4.))
                        .when(range && ix == 1, |this| this.child("–"))
                        .child(ui::key(*cap, false, cx))
                }))
                .child(div().pl(px(4.)).child(what))
        });

        v_flex()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(t.border)
            // The grid scrolls on its own, so the keys stay in view even in a
            // 100-question paper.
            .child(
                v_flex()
                    .id("exam-grid")
                    .flex_1()
                    .min_h_0()
                    .p(px(16.))
                    .gap(px(14.))
                    .overflow_y_scrollbar()
                    .child(
                        h_flex()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.5))
                                    .font_semibold()
                                    .child("Questions"),
                            )
                            .child(ui::meta(
                                format!("{answered} of {} answered", attempt.answers.len()),
                                cx,
                            )),
                    )
                    .child(div().flex().flex_wrap().gap(px(9.)).children(cells))
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap_x_3()
                            .gap_y(px(8.))
                            .text_size(px(12.))
                            .text_color(t.text_3)
                            .child(key(
                                Some(t.accent),
                                t.accent,
                                format!("Answered {answered}"),
                            ))
                            .child(key(
                                Some(t.attention),
                                t.attention,
                                format!("Marked {marked}"),
                            ))
                            .child(key(None, t.border_3, format!("Skipped {seen}")))
                            .child(key(None, t.border, format!("Not seen {unseen}"))),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .m(px(16.))
                    .p(px(12.))
                    .gap(px(8.))
                    .rounded(px(8.))
                    .bg(t.sunken)
                    .border_1()
                    .border_color(t.border)
                    .text_size(px(12.))
                    .text_color(t.text_3)
                    .child(div().font_medium().text_color(t.text_2).child("Keys"))
                    .children(keys),
            )
    }

    fn footer(&self, session: &Session, cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let attempt = &session.attempt;
        let current = &attempt.answers[attempt.current];
        h_flex()
            .flex_shrink_0()
            .h(px(56.))
            .px(px(12.))
            .gap_2()
            .border_t_1()
            .border_color(t.border)
            .child(
                Button::new("mark")
                    .small()
                    .icon(Icon::new(IconName::Flag))
                    .label(if current.marked {
                        "Marked for review"
                    } else {
                        "Mark for review"
                    })
                    .selected(current.marked)
                    .child(ui::key("M", false, cx))
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
                    .small()
                    .icon(Icon::new(IconName::ArrowLeft))
                    .label("Previous")
                    .disabled(attempt.current == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
            )
            .child(
                Button::new("next")
                    .small()
                    .label("Next")
                    .child(Icon::new(IconName::ArrowRight).size(px(14.)))
                    .child(ui::key("→", false, cx))
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
        let t = ui::tokens(cx);
        let confirm = session.confirm.then(|| {
            h_flex()
                .flex_shrink_0()
                .mx(px(12.))
                .mt(px(10.))
                .px(px(14.))
                .py(px(8.))
                .gap_3()
                .rounded(px(8.))
                .bg(t.attention.opacity(0.14))
                .border_1()
                .border_color(t.attention.opacity(0.5))
                .text_size(px(13.))
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .size_4()
                        .text_color(t.attention),
                )
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
            .on_action(cx.listener(|this, _: &ChooseA, _, cx| this.choose(0, cx)))
            .on_action(cx.listener(|this, _: &ChooseB, _, cx| this.choose(1, cx)))
            .on_action(cx.listener(|this, _: &ChooseC, _, cx| this.choose(2, cx)))
            .on_action(cx.listener(|this, _: &ChooseD, _, cx| this.choose(3, cx)))
            .on_action(cx.listener(|this, _: &ToggleMark, _, cx| this.toggle_mark(cx)))
            .on_action(cx.listener(|this, _: &NextQuestion, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousQuestion, _, cx| this.step(-1, cx)))
            .on_action(
                cx.listener(|this, _: &TagSure, _, cx| this.set_confidence(Confidence::Sure, cx)),
            )
            .on_action(cx.listener(|this, _: &TagFiftyFifty, _, cx| {
                this.set_confidence(Confidence::FiftyFifty, cx)
            }))
            .on_action(
                cx.listener(|this, _: &TagGuess, _, cx| this.set_confidence(Confidence::Guess, cx)),
            )
            .on_action(cx.listener(|this, _: &SubmitTest, _, cx| this.submit_clicked(cx)))
            .child(self.header(session, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(3.))
                    .w_full()
                    .bg(t.selected)
                    .child(div().h_full().bg(t.accent).w(relative(
                        attempt.answered() as f32 / attempt.answers.len().max(1) as f32,
                    ))),
            )
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

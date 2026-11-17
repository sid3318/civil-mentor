//! After a test: the result and the review of every question. The same
//! question cards show one question on its own (where search lands) and a
//! past paper read with its answers.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell};
use crate::content::Target;
use crate::exam::{LETTERS, clock};
use crate::mentor::{self, SECONDS_PER_QUESTION, Score, TestKind, TestPlan};
use crate::screens::{count, page};
use crate::store::{Answer, Attempt, Confidence};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Nothing,
    Result,
    Question(usize),
    Paper(u16),
}

pub struct ReviewScreen {
    shell: WeakEntity<Shell>,
    view: View,
    /// The sitting whose result is showing.
    attempt: Option<Attempt>,
    /// The questions whose answers are showing, by index into the library.
    shown: HashSet<usize>,
}

/// "10.67", "-2", "12".
pub(crate) fn marks(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl ReviewScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        ReviewScreen {
            shell,
            view: View::Nothing,
            attempt: None,
            shown: HashSet::new(),
        }
    }

    pub fn show_result(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.view != View::Result || self.attempt.as_ref().map(|a| a.id) != Some(id) {
            self.shown.clear();
        }
        self.view = View::Result;
        self.attempt = match AppState::get(cx).store.attempt(id) {
            Ok(attempt) => attempt,
            Err(err) => {
                AppState::report(cx, err);
                None
            }
        };
        cx.notify();
    }

    pub fn show_question(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.view != View::Question(ix) {
            self.shown.clear();
        }
        self.view = View::Question(ix);
        cx.notify();
    }

    pub fn show_paper(&mut self, year: u16, cx: &mut Context<Self>) {
        if self.view != View::Paper(year) {
            self.shown.clear();
        }
        self.view = View::Paper(year);
        cx.notify();
    }

    fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.shown.remove(&ix) {
            self.shown.insert(ix);
        }
        cx.notify();
    }

    fn open(&self, target: Target, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.open(target, cx));
    }

    /// One question with its options. Once revealed, the right option is
    /// green, a wrong choice red, and the explanation, trap and note follow.
    fn card(
        &self,
        ix: usize,
        heading: String,
        chosen: Option<usize>,
        revealed: bool,
        toggle: bool,
        cx: &Context<Self>,
    ) -> Div {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let question = &library.questions[ix];
        let options = question.options.iter().enumerate().map(|(option, text)| {
            let right = revealed && question.answer == Some(option);
            let wrong = revealed && chosen == Some(option) && !right;
            let tone = if right {
                Some(t.success)
            } else if wrong {
                Some(t.danger)
            } else {
                None
            };
            h_flex()
                .items_start()
                .gap(px(10.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .text_size(px(13.5))
                .when_some(tone, |this, tone| {
                    this.bg(tone.opacity(0.1)).text_color(tone).font_medium()
                })
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(22.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.))
                        .font_semibold()
                        .border_1()
                        .border_color(tone.unwrap_or(t.border_2))
                        .when_some(tone, |this, tone| this.bg(tone).text_color(t.on_accent))
                        .when(tone.is_none(), |this| this.text_color(t.text_2))
                        .child(LETTERS[option]),
                )
                .child(div().flex_1().min_w_0().pt(px(2.)).child(text.clone()))
                .when(chosen == Some(option), |this| {
                    this.child(ui::chip(cx).child("your answer"))
                })
                .when(right, |this| {
                    this.child(Icon::new(IconName::Check).size_4().mt(px(3.)))
                })
                .when(wrong, |this| {
                    this.child(Icon::new(IconName::Close).size_4().mt(px(3.)))
                })
        });
        let note = question
            .note
            .as_deref()
            .and_then(|id| library.notes.iter().position(|note| note.id == id));
        ui::card(cx)
            .p_4()
            .gap(px(12.))
            .child(ui::meta(heading, cx))
            .child(
                div()
                    .text_size(px(14.5))
                    .line_height(px(22.))
                    .child(TextView::markdown(
                        ("review-stem", ix),
                        question.stem.clone(),
                    )),
            )
            .child(v_flex().gap(px(4.)).children(options))
            .when(revealed, |card| {
                let explanation = match question.answer {
                    Some(_) => question.explanation.clone(),
                    None => format!(
                        "UPSC dropped this question from its answer key. {}",
                        question.explanation
                    ),
                };
                card.child(
                    div()
                        .text_size(px(13.5))
                        .line_height(px(20.))
                        .text_color(t.text_2)
                        .child(explanation),
                )
                .when_some(question.trap.clone(), |card, trap| {
                    card.child(
                        h_flex()
                            .items_start()
                            .gap(px(8.))
                            .px_3()
                            .py(px(10.))
                            .rounded(px(8.))
                            .bg(t.attention.opacity(0.1))
                            .border_l_2()
                            .border_color(t.attention)
                            .text_size(px(13.))
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .size(px(14.))
                                    .mt(px(2.))
                                    .text_color(t.attention),
                            )
                            .child(div().flex_1().child(format!("The trap: {trap}"))),
                    )
                })
            })
            .child(
                h_flex()
                    .gap_2()
                    .when(toggle, |row| {
                        row.child(
                            Button::new(("answer", ix))
                                .small()
                                .label(if revealed {
                                    "Hide the answer"
                                } else {
                                    "Show the answer"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| this.toggle(ix, cx))),
                        )
                    })
                    .when_some(note.filter(|_| revealed), |row, note| {
                        row.child(
                            Button::new(("open-note", ix))
                                .ghost()
                                .small()
                                .icon(Icon::new(IconName::BookOpen))
                                .label(format!("Read the note: {}", library.notes[note].title))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open(Target::Note(note), cx)
                                })),
                        )
                    }),
            )
    }

    fn result(&self, cx: &Context<Self>) -> Div {
        let Some(attempt) = &self.attempt else {
            return page("This test could not be found.", cx);
        };
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let score = mentor::score(attempt);
        let full = mentor::full_marks(attempt);
        let seconds_each = attempt.seconds_used / attempt.answers.len().max(1) as u32;
        let submitted = attempt
            .finished
            .unwrap_or(attempt.started)
            .format("%-d %B %Y, %H:%M");
        let share = if full > 0. {
            (score.marks / full).clamp(0., 1.) as f32
        } else {
            0.
        };
        let answered = score.right + score.wrong;
        let accuracy = mentor::percent(score.right, answered);

        let stat = |value: String, label: &'static str, tone: Option<Hsla>| {
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(ui::caption(label, cx).text_size(px(12.)))
                .child(
                    div()
                        .text_size(px(22.))
                        .line_height(px(28.))
                        .font_semibold()
                        .when_some(tone, |this, tone| this.text_color(tone))
                        .child(value),
                )
        };
        let hero = ui::card(cx).id("score").test_support().child(
            h_flex()
                .p(px(18.))
                .gap(px(24.))
                .child(ui::ring(
                    share,
                    104.,
                    9.,
                    t.accent,
                    v_flex()
                        .items_center()
                        .child(
                            div()
                                .font_family(ui::DISPLAY)
                                .text_size(px(24.))
                                .font_semibold()
                                .child(marks(score.marks)),
                        )
                        .child(ui::meta(format!("of {}", marks(full)), cx)),
                    cx,
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(14.))
                        .child(
                            v_flex()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .font_family(ui::DISPLAY)
                                        .text_size(px(20.))
                                        .font_semibold()
                                        .child(attempt.title.clone()),
                                )
                                .child(ui::meta(
                                    format!(
                                        "Submitted {submitted} · used {} of {}",
                                        clock(attempt.seconds_used),
                                        clock(attempt.seconds_allowed)
                                    ),
                                    cx,
                                )),
                        )
                        .child(
                            h_flex()
                                .gap(px(18.))
                                .child(stat(score.right.to_string(), "Right", Some(t.success)))
                                .child(stat(score.wrong.to_string(), "Wrong", Some(t.danger)))
                                .child(stat(score.blank.to_string(), "Left blank", None))
                                .child(stat(format!("{accuracy}%"), "Accuracy", None))
                                .child(stat(
                                    format!("{seconds_each} s"),
                                    "A question",
                                    (seconds_each > SECONDS_PER_QUESTION).then_some(t.attention),
                                )),
                        ),
                ),
        );

        let by_subject: Vec<(String, Hsla, Score)> =
            mentor::score_by(attempt, |a| a.subject.clone())
                .into_iter()
                .map(|(id, score)| {
                    let name = mentor::subject(&id).map_or("Other", |s| s.name);
                    (name.to_string(), ui::subject_id_color(&id, cx), score)
                })
                .collect();
        let by_confidence: Vec<(String, Hsla, Score)> = mentor::score_by(attempt, |a| a.confidence)
            .into_iter()
            .map(|(level, score)| {
                let label = level.map_or("Not tagged", Confidence::label);
                let color = match level {
                    Some(Confidence::Sure) => t.success,
                    Some(Confidence::FiftyFifty) => t.accent,
                    Some(Confidence::Guess) => t.attention,
                    None => t.text_4,
                };
                (label.to_string(), color, score)
            })
            .collect();
        let breakdown =
            |title: &'static str, note: &'static str, rows: Vec<(String, Hsla, Score)>| {
                let lines = rows.into_iter().map(|(label, color, score)| {
                    let asked = score.right + score.wrong + score.blank;
                    let right = score.right as f32 / asked.max(1) as f32;
                    h_flex()
                        .gap(px(10.))
                        .h(px(34.))
                        .px(px(14.))
                        .border_t_1()
                        .border_color(t.border)
                        .text_size(px(13.))
                        .child(
                            h_flex()
                                .w(px(150.))
                                .flex_shrink_0()
                                .gap(px(8.))
                                .child(ui::dot(color))
                                .child(div().truncate().child(label)),
                        )
                        .child(div().flex_1().child(ui::progress(right, color, cx)))
                        .child(
                            div()
                                .w(px(76.))
                                .text_right()
                                .text_size(px(12.))
                                .text_color(t.text_3)
                                .child(format!("{} of {asked}", score.right)),
                        )
                        .child(
                            div()
                                .w(px(56.))
                                .text_right()
                                .font_semibold()
                                .text_color(if score.marks < 0. { t.danger } else { t.text })
                                .child(if score.marks > 0. {
                                    format!("+{}", marks(score.marks))
                                } else {
                                    marks(score.marks)
                                }),
                        )
                });
                ui::card(cx)
                    .flex_1()
                    .min_w_0()
                    .child(ui::card_head(title, cx).child(ui::meta(note, cx)))
                    .children(lines)
            };

        let cards = attempt
            .answers
            .iter()
            .enumerate()
            .map(|(position, answer)| {
                let Some(ix) = library.question_ix(&answer.question) else {
                    return ui::meta(
                        format!("Question {} is no longer in the bank.", position + 1),
                        cx,
                    );
                };
                self.card(
                    ix,
                    heading(position, answer, attempt),
                    answer.chosen,
                    true,
                    false,
                    cx,
                )
            });

        let shell = self.shell.clone();
        ui::column(900.)
            .gap(px(18.))
            .child(hero)
            .child(
                h_flex()
                    .items_start()
                    .gap(px(14.))
                    .child(breakdown(
                        "By subject",
                        "right answers and marks",
                        by_subject,
                    ))
                    .child(breakdown(
                        "By how sure you were",
                        "what guessing won or lost",
                        by_confidence,
                    )),
            )
            .child(
                v_flex()
                    .gap(px(12.))
                    .pt_2()
                    .child(
                        div()
                            .font_family(ui::DISPLAY)
                            .text_size(px(20.))
                            .font_semibold()
                            .child("Every question"),
                    )
                    .children(cards),
            )
            .child(
                h_flex().child(
                    Button::new("back-to-practice")
                        .label("Back to Practice")
                        .on_click(move |_, _, cx| {
                            let _ = shell.update(cx, |shell, cx| shell.go(Screen::Practice, cx));
                        }),
                ),
            )
    }

    fn question(&self, ix: usize, cx: &Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let question = &library.questions[ix];
        let subject = mentor::subject_of(question).map_or("Question", |s| s.name);
        let summary = if question.source.is_empty() {
            "From the question bank".to_string()
        } else {
            question.source.clone()
        };
        page(summary, cx).child(self.card(
            ix,
            subject.to_string(),
            None,
            self.shown.contains(&ix),
            true,
            cx,
        ))
    }

    fn paper(&self, year: u16, cx: &Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let questions = library
            .papers()
            .into_iter()
            .find(|(y, _)| *y == year)
            .map(|(_, questions)| questions)
            .unwrap_or_default();
        let dropped = questions
            .iter()
            .filter(|&&ix| library.questions[ix].answer.is_none())
            .count();
        let mut summary = format!(
            "{}, as UPSC set them, with answers from UPSC's official key.",
            count(questions.len(), "question", "questions")
        );
        if dropped > 0 {
            summary.push_str(&format!(
                " UPSC dropped {} from its key.",
                count(dropped, "question", "questions")
            ));
        }
        let all_shown = !questions.is_empty() && questions.iter().all(|ix| self.shown.contains(ix));
        let shell = self.shell.clone();
        let cards = questions.iter().enumerate().map(|(n, &ix)| {
            let subject = mentor::subject_of(&library.questions[ix]).map_or("", |s| s.name);
            self.card(
                ix,
                format!("Question {} · {subject}", n + 1),
                None,
                self.shown.contains(&ix),
                true,
                cx,
            )
        });
        page(summary, cx)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_family(ui::DISPLAY)
                            .text_size(px(22.))
                            .font_semibold()
                            .child(TestKind::Paper(year).title()),
                    )
                    .child(
                        Button::new("show-all")
                            .small()
                            .label(if all_shown {
                                "Hide all answers"
                            } else {
                                "Show all answers"
                            })
                            .on_click(cx.listener({
                                let questions = questions.clone();
                                move |this, _, _, cx| {
                                    if all_shown {
                                        this.shown.clear();
                                    } else {
                                        this.shown.extend(questions.iter().copied());
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("sit-this-paper")
                            .primary()
                            .small()
                            .icon(Icon::new(IconName::Play))
                            .label("Sit it as a mock")
                            .on_click(move |_, _, cx| {
                                let _ = shell.update(cx, |shell, cx| {
                                    let plan = TestPlan::standard(
                                        TestKind::Paper(year),
                                        &AppState::get(cx).library,
                                    );
                                    shell.sit(&plan, cx)
                                });
                            }),
                    ),
            )
            .child(v_flex().gap(px(12.)).children(cards))
    }
}

/// "Question 3 · Polity · 41 s · Sure · wrong, -0.67".
fn heading(position: usize, answer: &Answer, attempt: &Attempt) -> String {
    let outcome = match (answer.chosen, answer.correct) {
        (None, _) => "left blank".to_string(),
        (Some(_), Some(true)) => format!("right, +{}", marks(attempt.right_marks)),
        (Some(_), Some(false)) => format!("wrong, -{}", marks(attempt.wrong_marks)),
        (Some(_), None) => "dropped by UPSC, no marks".to_string(),
    };
    let mut parts = vec![format!("Question {}", position + 1)];
    if let Some(subject) = mentor::subject(&answer.subject) {
        parts.push(subject.name.to_string());
    }
    parts.push(format!("{} s", answer.seconds));
    if let Some(level) = answer.confidence {
        parts.push(level.label().to_string());
    }
    if answer.marked {
        parts.push("marked".to_string());
    }
    parts.push(outcome);
    parts.join(" · ")
}

impl Render for ReviewScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, scroll) = match self.view {
            View::Nothing | View::Result => ("Result", 3usize),
            View::Paper(_) => ("Past paper", 4),
            View::Question(_) => ("Question", 5),
        };
        let header = ui::header(cx).child(ui::title(IconName::SquarePen, title, cx));
        let body = match self.view {
            View::Nothing => page("", cx),
            View::Result => self.result(cx),
            View::Question(ix) => self.question(ix, cx),
            View::Paper(year) => self.paper(year, cx),
        };
        ui::screen(header, ("screen", scroll), body)
    }
}

#[cfg(test)]
mod tests {
    use super::marks;

    #[test]
    fn marks_drop_needless_decimals() {
        assert_eq!(marks(10.0), "10");
        assert_eq!(marks(10.0 + 2.0 / 3.0), "10.67");
        assert_eq!(marks(-2.0 / 3.0), "-0.67");
        assert_eq!(marks(1.5), "1.5");
    }
}

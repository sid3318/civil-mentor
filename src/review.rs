//! After a test: the result and the review of every question. The same
//! question cards show one question on its own (where search lands) and a
//! past paper read with its answers.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell};
use crate::content::Target;
use crate::exam::{LETTERS, clock};
use crate::mentor::{self, SECONDS_PER_QUESTION, Score, TestKind, TestPlan};
use crate::screens::{count, page};
use crate::store::{Answer, Attempt, Confidence};

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
        let library = &AppState::get(cx).library;
        let question = &library.questions[ix];
        let theme = cx.theme();
        let options = question.options.iter().enumerate().map(|(option, text)| {
            let right = revealed && question.answer == Some(option);
            let wrong = revealed && chosen == Some(option) && !right;
            h_flex()
                .items_start()
                .gap_2()
                .px_2()
                .py_1()
                .rounded(theme.radius)
                .when(right, |this| {
                    this.bg(theme.success.opacity(0.12))
                        .text_color(theme.success)
                        .font_medium()
                })
                .when(wrong, |this| {
                    this.bg(theme.danger.opacity(0.12)).text_color(theme.danger)
                })
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(28.))
                        .child(format!("({})", LETTERS[option])),
                )
                .child(div().flex_1().min_w_0().child(text.clone()))
                .when(chosen == Some(option), |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("your answer"),
                    )
                })
                .when(right, |this| this.child(Icon::new(IconName::Check)))
                .when(wrong, |this| this.child(Icon::new(IconName::Close)))
        });
        let note = question
            .note
            .as_deref()
            .and_then(|id| library.notes.iter().position(|note| note.id == id));
        v_flex()
            .p_4()
            .gap_3()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(heading),
            )
            .child(TextView::markdown(
                ("review-stem", ix),
                question.stem.clone(),
            ))
            .child(v_flex().gap_1().children(options))
            .when(revealed, |card| {
                let explanation = match question.answer {
                    Some(_) => question.explanation.clone(),
                    None => format!(
                        "UPSC dropped this question from its answer key. {}",
                        question.explanation
                    ),
                };
                card.child(div().child(explanation)).when_some(
                    question.trap.clone(),
                    |card, trap| {
                        card.child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(format!("The trap: {trap}")),
                        )
                    },
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .when(toggle, |row| {
                        row.child(
                            Button::new(("answer", ix))
                                .outline()
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
            return page("Result", "This test could not be found.", cx);
        };
        let library = &AppState::get(cx).library;
        let theme = cx.theme();
        let score = mentor::score(attempt);
        let seconds_each = attempt.seconds_used / attempt.answers.len().max(1) as u32;
        let submitted = attempt
            .finished
            .unwrap_or(attempt.started)
            .format("%-d %B %Y, %H:%M");
        let summary = format!(
            "Submitted {submitted}. You used {} of {}.",
            clock(attempt.seconds_used),
            clock(attempt.seconds_allowed)
        );

        let tile = |value: String, label: String| {
            v_flex()
                .flex_1()
                .p_4()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .child(div().text_2xl().font_semibold().child(value))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(label),
                )
        };
        let tiles = h_flex()
            .id("score")
            .test_support()
            .gap_3()
            .child(tile(
                marks(score.marks),
                format!("net score of {}", marks(mentor::full_marks(attempt))),
            ))
            .child(tile(score.right.to_string(), "right".into()))
            .child(tile(score.wrong.to_string(), "wrong".into()))
            .child(tile(score.blank.to_string(), "left blank".into()))
            .child(tile(
                format!("{seconds_each} s"),
                format!("a question, against {SECONDS_PER_QUESTION} s"),
            ));

        let table = |title: &'static str, rows: Vec<(String, Score)>| {
            let row = |cells: [String; 5], strong: bool| {
                let [label, right, wrong, blank, marks] = cells;
                let cell = |text: String| div().w(px(56.)).text_right().child(text);
                h_flex()
                    .py_1()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_sm()
                    .when(strong, |this| {
                        this.font_medium().text_color(theme.muted_foreground)
                    })
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .child(cell(right))
                    .child(cell(wrong))
                    .child(cell(blank))
                    .child(cell(marks))
            };
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().pb_1().font_medium().child(title))
                .child(row(
                    [
                        String::new(),
                        "right".into(),
                        "wrong".into(),
                        "blank".into(),
                        "marks".into(),
                    ],
                    true,
                ))
                .children(rows.into_iter().map(|(label, score)| {
                    row(
                        [
                            label,
                            score.right.to_string(),
                            score.wrong.to_string(),
                            score.blank.to_string(),
                            marks(score.marks),
                        ],
                        false,
                    )
                }))
        };
        let by_subject = mentor::score_by(attempt, |a| a.subject.clone())
            .into_iter()
            .map(|(id, score)| {
                let name = mentor::subject(&id).map_or("Other", |s| s.name);
                (name.to_string(), score)
            })
            .collect();
        let by_confidence = mentor::score_by(attempt, |a| a.confidence)
            .into_iter()
            .map(|(level, score)| {
                let label = level.map_or("Not tagged", Confidence::label);
                (label.to_string(), score)
            })
            .collect();

        let cards = attempt
            .answers
            .iter()
            .enumerate()
            .map(|(position, answer)| {
                let Some(ix) = library.question_ix(&answer.question) else {
                    return div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "Question {} is no longer in the bank.",
                            position + 1
                        ));
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
        page("Result", summary, cx)
            .child(div().text_lg().font_semibold().child(attempt.title.clone()))
            .child(tiles)
            .child(
                h_flex()
                    .items_start()
                    .gap_8()
                    .child(table("Marks by subject", by_subject))
                    .child(table("Marks by how sure you were", by_confidence)),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(div().text_lg().font_semibold().child("Every question"))
                    .children(cards),
            )
            .child(
                h_flex().pb_8().child(
                    Button::new("back-to-practice")
                        .outline()
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
        page("Question", summary, cx).child(self.card(
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
        page("Past paper", summary, cx)
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child(TestKind::Paper(year).title()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("sit-this-paper")
                            .primary()
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
                    )
                    .child(
                        Button::new("show-all")
                            .outline()
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
                    ),
            )
            .child(v_flex().gap_3().pb_8().children(cards))
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
        match self.view {
            View::Nothing => page("Practice", "", cx),
            View::Result => self.result(cx),
            View::Question(ix) => self.question(ix, cx),
            View::Paper(year) => self.paper(year, cx),
        }
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

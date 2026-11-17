//! Revision: the mistake book, and today's review session, where mistakes
//! are asked again as questions and flashcards are flipped. Each answer
//! moves its item through the review boxes (see [`mentor::review`]).

use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Flashcard, Question, Target, one_line};
use crate::exam::LETTERS;
use crate::mentor::{self, ReviewItem, SUBJECTS};
use crate::notes::IDLE;
use crate::screens::{count, page};
use crate::store::{Review, Study};

pub struct RevisionScreen {
    shell: WeakEntity<Shell>,
    session: Option<Session>,
}

/// Today's review session.
struct Session {
    /// The review keys to go through, fixed when the session starts.
    items: Vec<String>,
    /// The item on screen, or `items.len()` once every one is answered.
    at: usize,
    /// The option chosen for the mistake on screen, once it is answered.
    chosen: Option<usize>,
    /// Whether the flashcard on screen shows its back.
    flipped: bool,
    right: usize,
    /// Since when the time on the session is not yet counted.
    counted: Instant,
    /// The block of study time in the store, once any is saved.
    block: Option<i64>,
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn card(cx: &App) -> Div {
    v_flex()
        .p_4()
        .gap_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
}

/// "19 Nov".
fn short_date(date: chrono::NaiveDate) -> String {
    date.format("%-d %b").to_string()
}

/// Where an item stands, as the mistake book shows it.
fn standing(review: &Review) -> String {
    match review.due {
        Some(due) => format!("Box {} · due {}", review.level, short_date(due)),
        None => "Learnt".to_string(),
    }
}

impl RevisionScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        RevisionScreen {
            shell,
            session: None,
        }
    }

    /// The review key on screen in the session, if one is under way.
    pub fn current(&self) -> Option<&str> {
        let session = self.session.as_ref()?;
        session.items.get(session.at).map(String::as_str)
    }

    /// Starts today's session with what is due. With nothing due, the
    /// mistake book stays on screen.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        let today = AppState::today(cx);
        let state = AppState::get(cx);
        let items = mentor::review_queue(&state.library, &state.reviews, today);
        if items.is_empty() {
            return;
        }
        self.session = Some(Session {
            items,
            at: 0,
            chosen: None,
            flipped: false,
            right: 0,
            counted: cx.background_executor().now(),
            block: None,
        });
        cx.notify();
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.session = None;
        cx.notify();
    }

    /// Records the answer to the item on screen: it moves through the
    /// boxes, and the time since the last answer, up to [`IDLE`], counts as
    /// study.
    fn answer(&mut self, right: bool, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let Some(key) = session.items.get(session.at).cloned() else {
            return;
        };
        let now = cx.background_executor().now();
        let seconds = now.saturating_duration_since(session.counted).min(IDLE);
        session.counted = now;
        session.right += usize::from(right);
        AppState::record_study(
            cx,
            Study::Review,
            "session",
            &mut session.block,
            seconds.as_secs() as u32,
        );
        let today = AppState::today(cx);
        let entry = mentor::review(AppState::get(cx).reviews.get(&key), right, today);
        AppState::save_reviews(cx, vec![(key, entry)]);
        cx.notify();
    }

    fn choose(&mut self, option: usize, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        if session.chosen.is_some() {
            return;
        }
        let library = &AppState::get(cx).library;
        let Some(ReviewItem::Mistake(question)) = session
            .items
            .get(session.at)
            .and_then(|key| mentor::review_item(library, key))
        else {
            return;
        };
        let right = question.answer == Some(option);
        session.chosen = Some(option);
        self.answer(right, cx);
    }

    fn flip(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            session.flipped = true;
            cx.notify();
        }
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            session.at += 1;
            session.chosen = None;
            session.flipped = false;
            cx.notify();
        }
    }

    fn mark_card(&mut self, right: bool, cx: &mut Context<Self>) {
        self.answer(right, cx);
        self.next(cx);
    }

    fn open(&self, target: Target, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| shell.open(target, cx));
    }

    /// A mistake asked again. Once answered, the right option is green, a
    /// wrong choice red, and the explanation and when it comes back follow.
    fn mistake(
        &self,
        question: &Question,
        chosen: Option<usize>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let theme = cx.theme();
        let revealed = chosen.is_some();
        let options = question.options.iter().enumerate().map(|(option, text)| {
            let right = revealed && question.answer == Some(option);
            let wrong = revealed && chosen == Some(option) && !right;
            h_flex()
                .id(("review-option", option))
                .test_support()
                .items_start()
                .gap_2()
                .px_2()
                .py_1()
                .rounded(theme.radius)
                .when(!revealed, |this| {
                    this.cursor_pointer()
                        .hover(|this| this.bg(theme.accent))
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(option, cx)))
                })
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
                .when(right, |this| this.child(Icon::new(IconName::Check)))
                .when(wrong, |this| this.child(Icon::new(IconName::Close)))
        });
        let subject = mentor::subject_of(question).map_or("Question", |s| s.name);
        let note = question
            .note
            .as_deref()
            .and_then(|id| library.notes.iter().position(|note| note.id == id));
        let comes_back = state
            .reviews
            .get(&mentor::mistake_key(&question.id))
            .map(|review| match review.due {
                Some(due) => format!("It comes back on {}.", short_date(due)),
                None => "Learnt: it leaves the reviews.".to_string(),
            })
            .unwrap_or_default();
        card(cx)
            .id("review-mistake")
            .test_support()
            .child(muted(
                format!("From your mistake book · {subject}. Pick the answer."),
                cx,
            ))
            .child(TextView::markdown(
                SharedString::from(format!("revision-stem-{}", question.id)),
                question.stem.clone(),
            ))
            .child(v_flex().gap_1().children(options))
            .when(revealed, |card| {
                let right = chosen == question.answer;
                card.child(
                    div()
                        .font_medium()
                        .text_color(if right { theme.success } else { theme.danger })
                        .child(if right {
                            format!("Right. {comes_back}")
                        } else {
                            format!("Not this time. {comes_back}")
                        }),
                )
                .child(div().child(question.explanation.clone()))
                .when_some(question.trap.clone(), |card, trap| {
                    card.child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("The trap: {trap}")),
                    )
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("review-next")
                                .primary()
                                .small()
                                .label("Next")
                                .icon(Icon::new(IconName::ChevronRight))
                                .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                        )
                        .when_some(note, |row, note| {
                            row.child(
                                Button::new("review-note")
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
            })
            .into_any_element()
    }

    /// A flashcard: its front, then its back once flipped, marked "Got it"
    /// or "Missed".
    fn flashcard(&self, flashcard: &Flashcard, flipped: bool, cx: &Context<Self>) -> AnyElement {
        let library = &AppState::get(cx).library;
        let theme = cx.theme();
        let subject = flashcard
            .topics
            .first()
            .map(|topic| library.subject_label(topic))
            .unwrap_or_default();
        card(cx)
            .id("review-card")
            .test_support()
            .p_6()
            .gap_4()
            .child(muted(format!("Flashcard · {subject}"), cx))
            .child(
                div()
                    .text_xl()
                    .font_medium()
                    .line_height(relative(1.35))
                    .child(flashcard.front.clone()),
            )
            .when(flipped, |card| {
                card.child(
                    div()
                        .id("card-back")
                        .test_support()
                        .pt_4()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_lg()
                        .child(flashcard.back.clone()),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .when(!flipped, |row| {
                        row.child(
                            Button::new("flip")
                                .primary()
                                .small()
                                .label("Show the answer")
                                .on_click(cx.listener(|this, _, _, cx| this.flip(cx))),
                        )
                    })
                    .when(flipped, |row| {
                        row.child(
                            Button::new("got-it")
                                .primary()
                                .small()
                                .icon(Icon::new(IconName::Check))
                                .label("Got it")
                                .on_click(cx.listener(|this, _, _, cx| this.mark_card(true, cx))),
                        )
                        .child(
                            Button::new("missed")
                                .outline()
                                .small()
                                .icon(Icon::new(IconName::Close))
                                .label("Missed")
                                .on_click(cx.listener(|this, _, _, cx| this.mark_card(false, cx))),
                        )
                    }),
            )
            .into_any_element()
    }

    fn session_page(&self, session: &Session, cx: &Context<Self>) -> Div {
        let library = &AppState::get(cx).library;
        let total = session.items.len();
        let finish = cx.listener(|this, _, _, cx| this.finish(cx));
        let Some(key) = session.items.get(session.at) else {
            return page(
                "Review done",
                format!("{} of {total} right", session.right),
                cx,
            )
            .child(muted(
                "Right answers come back after a longer gap each time; missed ones come back tomorrow.",
                cx,
            ))
            .child(
                h_flex().child(
                    Button::new("end-review")
                        .primary()
                        .label("Back to the mistake book")
                        .on_click(finish),
                ),
            );
        };
        let body = match mentor::review_item(library, key) {
            Some(ReviewItem::Mistake(question)) => self.mistake(question, session.chosen, cx),
            Some(ReviewItem::Card(flashcard)) => self.flashcard(flashcard, session.flipped, cx),
            None => muted("This item has left the content.", cx).into_any_element(),
        };
        page(
            "Today's review",
            format!(
                "{} of {total} · {} right so far",
                session.at + 1,
                session.right
            ),
            cx,
        )
        .child(body)
        .child(
            h_flex().child(
                Button::new("stop-review")
                    .ghost()
                    .small()
                    .label("Finish for now")
                    .on_click(finish),
            ),
        )
    }

    /// What is due today, then the mistake book by subject and where the
    /// flashcards stand.
    fn overview(&self, cx: &Context<Self>) -> Div {
        let today = AppState::today(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let theme = cx.theme();
        let queue = mentor::review_queue(library, &state.reviews, today);
        let mistakes_due = queue.iter().filter(|k| k.starts_with("question:")).count();
        let new_cards = queue
            .iter()
            .filter(|key| !state.reviews.contains_key(*key))
            .count();
        let cards_due = queue.len() - mistakes_due - new_cards;

        let mut book: Vec<(usize, usize, &Review)> = state
            .reviews
            .iter()
            .filter_map(|(key, review)| {
                let ix = library.question_ix(key.strip_prefix("question:")?)?;
                let subject = mentor::subject_of(&library.questions[ix])
                    .and_then(|s| SUBJECTS.iter().position(|x| x.id == s.id))
                    .unwrap_or(SUBJECTS.len());
                Some((subject, ix, review))
            })
            .collect();
        book.sort_by_key(|&(subject, ix, review)| (subject, review.due.is_none(), review.due, ix));

        let today_card = if queue.is_empty() {
            let next = state
                .reviews
                .values()
                .filter_map(|review| review.due)
                .filter(|&due| due > today)
                .min();
            card(cx)
                .child(div().font_medium().child("Nothing is due today."))
                .child(muted(
                    match next {
                        Some(due) => format!("The next review is on {}.", short_date(due)),
                        None => {
                            "Get a question wrong in a test and it comes back here the next day."
                                .to_string()
                        }
                    },
                    cx,
                ))
        } else {
            let mut parts = Vec::new();
            if mistakes_due > 0 {
                parts.push(count(mistakes_due, "mistake", "mistakes"));
            }
            if cards_due > 0 {
                parts.push(count(cards_due, "flashcard", "flashcards"));
            }
            if new_cards > 0 {
                parts.push(count(new_cards, "new flashcard", "new flashcards"));
            }
            card(cx)
                .child(
                    div()
                        .font_medium()
                        .child(format!("Due today: {}", parts.join(", "))),
                )
                .child(muted(
                    "Mistakes are asked again as questions and flashcards are flipped. Each answer sets when the item comes back.",
                    cx,
                ))
                .child(
                    h_flex().child(
                        Button::new("start-review")
                            .primary()
                            .icon(Icon::new(IconName::Repeat))
                            .label("Start today's review")
                            .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    ),
                )
        };

        let mut subjects: Vec<Div> = Vec::new();
        for group in book.chunk_by(|a, b| a.0 == b.0) {
            let name = SUBJECTS.get(group[0].0).map_or("Other", |s| s.name);
            let rows = group.iter().map(|&(_, ix, review)| {
                let question = &library.questions[ix];
                h_flex()
                    .id(("mistake", ix))
                    .test_support()
                    .gap_3()
                    .px_1()
                    .py_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .hover(|this| this.bg(theme.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(one_line(&question.stem, 110)),
                    )
                    .child(muted(standing(review), cx).flex_shrink_0())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open(Target::Question(ix), cx)),
                    )
            });
            subjects.push(
                v_flex()
                    .pt_2()
                    .child(
                        div()
                            .pb_1()
                            .text_sm()
                            .font_semibold()
                            .child(format!("{name} · {}", group.len())),
                    )
                    .children(rows),
            );
        }
        let mistake_book = card(cx)
            .id("mistake-book")
            .test_support()
            .child(div().font_semibold().child(format!(
                "Mistake book · {}",
                count(book.len(), "question", "questions")
            )))
            .when(book.is_empty(), |this| {
                this.child(muted(
                    "Every question you get wrong in a test lands here, due the next day.",
                    cx,
                ))
            })
            .children(subjects);

        let (mut started, mut learnt) = (0, 0);
        for flashcard in &library.cards {
            match state.reviews.get(&mentor::card_key(&flashcard.id)) {
                Some(review) if review.due.is_none() => learnt += 1,
                Some(_) => started += 1,
                None => {}
            }
        }
        let flashcards = card(cx)
            .child(
                div()
                    .font_semibold()
                    .child(format!("Flashcards · {}", library.cards.len())),
            )
            .child(muted(
                format!(
                    "{started} in the boxes, {learnt} learnt, {} not started. Up to {} new cards join the review each day.",
                    library.cards.len() - started - learnt,
                    mentor::NEW_CARDS
                ),
                cx,
            ));

        let summary = if queue.is_empty() {
            "Nothing is due today".to_string()
        } else {
            format!("{} due today", count(queue.len(), "item", "items"))
        };
        page("Revision", summary, cx)
            .child(today_card)
            .child(mistake_book)
            .child(flashcards)
    }
}

impl Render for RevisionScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.session {
            Some(session) => self.session_page(session, cx),
            None => self.overview(cx),
        }
    }
}

//! The Notes screen: the list of notes, the reader, and an outline beside it.

use std::collections::HashSet;

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
use crate::content::{Target, sections};
use crate::screens::count;
use crate::syllabus::Status;

/// Reader text sizes, in pixels.
const MIN_SIZE: u32 = 13;
const MAX_SIZE: u32 = 24;
/// Widths of the reading column. The narrow one keeps lines near 75
/// characters at the default size; the wide one suits tables.
const NARROW: f32 = 720.;
const WIDE: f32 = 960.;

pub struct NotesScreen {
    shell: WeakEntity<Shell>,
    note: Option<usize>,
    distilled: bool,
    /// The questions whose answers are showing, by index into the library.
    answers: HashSet<usize>,
    /// The reader's children are the header, one per section, the footer and
    /// the questions, so the outline scrolls to a section by its position.
    scroll: ScrollHandle,
}

impl NotesScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        NotesScreen {
            shell,
            note: None,
            distilled: false,
            answers: HashSet::new(),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn note(&self) -> Option<usize> {
        self.note
    }

    /// Opens a note in the reader and records that the student opened it.
    /// Called by the shell, which keeps the back and forward history.
    pub fn show(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.note != Some(ix) {
            self.note = Some(ix);
            self.answers.clear();
            self.scroll.set_offset(Point::default());
        }
        let id = AppState::get(cx).library.notes[ix].id.clone();
        AppState::update_reading(cx, &id, |reading, today| reading.last = today);
        cx.notify();
    }

    fn set_distilled(&mut self, distilled: bool, cx: &mut Context<Self>) {
        if self.distilled != distilled {
            self.distilled = distilled;
            self.scroll.set_offset(Point::default());
            cx.notify();
        }
    }

    /// Every note, bookmarked ones first, then grouped by paper and subject.
    fn list(&self, cx: &App) -> impl IntoElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let bookmarked: Vec<usize> = (0..library.notes.len())
            .filter(|&ix| {
                state
                    .reading
                    .get(&library.notes[ix].id)
                    .is_some_and(|reading| reading.bookmarked)
            })
            .collect();
        // In syllabus order, so that each subject's notes come together.
        let mut order: Vec<usize> = (0..library.notes.len()).collect();
        order.sort_by_key(|&ix| {
            let first = library.notes[ix].topics.first();
            library
                .topics
                .iter()
                .position(|topic| Some(&topic.id) == first)
        });
        let mut subjects: Vec<(String, Vec<usize>)> = Vec::new();
        for ix in order {
            let label = library.notes[ix]
                .topics
                .first()
                .map(|topic| library.subject_label(topic))
                .unwrap_or_default();
            match subjects.last_mut() {
                Some((last, notes)) if *last == label => notes.push(ix),
                _ => subjects.push((label, vec![ix])),
            }
        }

        let header = |label: String| {
            div()
                .px_2()
                .pt_3()
                .pb_1()
                .text_xs()
                .font_medium()
                .text_color(cx.theme().muted_foreground)
                .child(label)
        };
        // ponytail: one plain list. Fold subjects away once there are
        // hundreds of notes.
        v_flex()
            .w(px(250.))
            .flex_shrink_0()
            .h_full()
            .p_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_y_scrollbar()
            .id("notes-list")
            .when(!bookmarked.is_empty(), |list| {
                list.child(header("Bookmarked".into())).children(
                    bookmarked
                        .iter()
                        .map(|&ix| self.list_row(("bookmarked", ix), ix, cx)),
                )
            })
            .children(subjects.into_iter().map(|(label, notes)| {
                v_flex().child(header(label)).children(
                    notes
                        .into_iter()
                        .map(|ix| self.list_row(("note", ix), ix, cx)),
                )
            }))
    }

    fn list_row(&self, id: impl Into<ElementId>, ix: usize, cx: &App) -> impl IntoElement {
        let state = AppState::get(cx);
        let note = &state.library.notes[ix];
        let shell = self.shell.clone();
        h_flex()
            .id(id)
            .test_support()
            .items_start()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .text_sm()
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().accent))
            .when(self.note == Some(ix), |this| {
                this.bg(cx.theme().accent).font_medium()
            })
            .child(
                div()
                    .pt(px(2.))
                    .child(Status::of(state.reading.get(&note.id)).icon(cx).small()),
            )
            .child(div().flex_1().min_w_0().child(note.title.clone()))
            // Not a listener: the shell updates this screen when it opens the note.
            .on_click(move |_, _, cx| {
                let _ = shell.update(cx, |shell, cx| shell.open(Target::Note(ix), cx));
            })
    }

    /// The open note with its toolbar, and the outline beside it.
    fn reader(&self, cx: &Context<Self>) -> AnyElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let Some(ix) = self.note else {
            let read = library
                .notes
                .iter()
                .filter(|note| Status::of(state.reading.get(&note.id)) == Status::Done)
                .count();
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap_1()
                .child(div().text_lg().font_medium().child("Pick a note to read"))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "You have read {read} of {}.",
                            count(library.notes.len(), "note", "notes")
                        )),
                )
                .into_any_element();
        };

        let note = &library.notes[ix];
        let reading = state.reading.get(&note.id).copied();
        let bookmarked = reading.is_some_and(|reading| reading.bookmarked);
        let size = state.settings.reader_size.clamp(MIN_SIZE, MAX_SIZE);
        let wide = state.settings.reader_wide;
        let distilled = self.distilled && !note.distilled.is_empty();
        let text = if distilled {
            &note.distilled
        } else {
            &note.body
        };
        let sections = sections(text);
        let questions = library.questions_for(note);
        let muted = cx.theme().muted_foreground;

        let toolbar = h_flex()
            .flex_shrink_0()
            .h(px(44.))
            .px_3()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                ButtonGroup::new("mode")
                    .outline()
                    .small()
                    .child(Button::new("read-mode").label("Read").selected(!distilled))
                    .child(
                        Button::new("distilled-mode")
                            .label("Distilled")
                            .selected(distilled)
                            .disabled(note.distilled.is_empty()),
                    )
                    .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                        if let Some(&button) = clicked.first() {
                            this.set_distilled(button == 1, cx);
                        }
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new("smaller-text")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::AArrowDown))
                    .tooltip("Smaller text")
                    .disabled(size <= MIN_SIZE)
                    .on_click(move |_, _, cx| {
                        AppState::update_settings(cx, |settings| settings.reader_size = size - 1);
                    }),
            )
            .child(
                Button::new("larger-text")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::AArrowUp))
                    .tooltip("Larger text")
                    .disabled(size >= MAX_SIZE)
                    .on_click(move |_, _, cx| {
                        AppState::update_settings(cx, |settings| settings.reader_size = size + 1);
                    }),
            )
            .child(
                Button::new("wide")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::ChevronsLeftRight))
                    .selected(wide)
                    .tooltip(if wide { "Narrow column" } else { "Wide column" })
                    .on_click(|_, _, cx| {
                        AppState::update_settings(cx, |settings| {
                            settings.reader_wide = !settings.reader_wide
                        });
                    }),
            )
            .child(
                Button::new("bookmark")
                    .ghost()
                    .small()
                    .icon(Icon::new(if bookmarked {
                        IconName::BookmarkCheck
                    } else {
                        IconName::Bookmark
                    }))
                    .selected(bookmarked)
                    .tooltip(if bookmarked {
                        "Remove the bookmark"
                    } else {
                        "Bookmark this note"
                    })
                    .on_click({
                        let id = note.id.clone();
                        move |_, _, cx| {
                            AppState::update_reading(cx, &id, |reading, _| {
                                reading.bookmarked = !reading.bookmarked
                            });
                        }
                    }),
            );

        // Every child of the reader is one of these, so the outline can find
        // a section by its position.
        let column = |child: AnyElement| {
            div()
                .w_full()
                .max_w(px(if wide { WIDE } else { NARROW }))
                .mx_auto()
                .px_8()
                .child(child)
        };

        let mut facts = vec![format!("{} min read", note.minutes)];
        if !questions.is_empty() {
            facts.push(count(questions.len(), "question", "questions"));
        }
        let years: Vec<String> = library
            .years_asked(note)
            .iter()
            .map(u16::to_string)
            .collect();
        if !years.is_empty() {
            facts.push(format!("asked in {}", years.join(", ")));
        }
        if let Some(updated) = note.updated {
            facts.push(format!("updated {}", updated.format("%-d %b %Y")));
        }
        let subject = note
            .topics
            .first()
            .map(|topic| library.subject_label(topic))
            .unwrap_or_default();
        let header = v_flex()
            .pt_8()
            .pb_4()
            .gap_2()
            .child(div().text_sm().text_color(muted).child(subject))
            .child(
                div()
                    .text_size(px(size as f32 * 1.75))
                    .line_height(relative(1.2))
                    .font_semibold()
                    .child(note.title.clone()),
            )
            .child(div().text_sm().text_color(muted).child(facts.join(" · ")));

        let done = reading.and_then(|reading| reading.done);
        let mark = {
            let id = note.id.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                AppState::update_reading(cx, &id, |reading, today| {
                    reading.done = match reading.done {
                        Some(_) => None,
                        None => Some(today),
                    };
                });
            }
        };
        let footer = match done {
            None => h_flex()
                .py_8()
                .gap_3()
                .child(
                    Button::new("mark-read")
                        .primary()
                        .icon(Icon::new(IconName::Check))
                        .label("Mark as read")
                        .on_click(mark),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("It then counts towards your syllabus coverage."),
                ),
            Some(day) => h_flex()
                .py_8()
                .gap_2()
                .child(Icon::new(IconName::CircleCheck).text_color(cx.theme().success))
                .child(format!(
                    "You marked this read on {}.",
                    day.format("%-d %B %Y")
                ))
                .child(
                    Button::new("mark-read")
                        .ghost()
                        .small()
                        .label("Mark as unread")
                        .on_click(mark),
                ),
        };

        let section_views = sections.iter().enumerate().map(|(k, section)| {
            let id = SharedString::from(format!("{}-{}", note.id, distilled));
            column(
                div()
                    .when(section.heading.is_some(), |this| this.pt_6())
                    .child(TextView::markdown((id, k), section.text.to_string()))
                    .into_any_element(),
            )
        });
        let reader = div()
            .id("reader")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .text_size(px(size as f32))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(column(header.into_any_element()))
            .children(section_views)
            .child(column(footer.into_any_element()))
            .when(!questions.is_empty(), |reader| {
                reader.child(column(self.questions(&questions, cx).into_any_element()))
            });

        // The outline: each heading, then the questions, as reader positions.
        let mut outline: Vec<(SharedString, usize)> = sections
            .iter()
            .enumerate()
            .filter_map(|(k, section)| Some((section.heading?.to_string().into(), k + 1)))
            .collect();
        if !questions.is_empty() {
            outline.push((
                format!("Questions ({})", questions.len()).into(),
                sections.len() + 2,
            ));
        }
        let panel = v_flex()
            .w(px(220.))
            .flex_shrink_0()
            .h_full()
            .p_3()
            .gap_1()
            .border_l_1()
            .border_color(cx.theme().border)
            .text_sm()
            .overflow_y_scrollbar()
            .id("outline")
            .when(!outline.is_empty(), |panel| {
                panel.child(
                    div()
                        .pb_1()
                        .text_xs()
                        .font_medium()
                        .text_color(muted)
                        .child("On this page"),
                )
            })
            .children(
                outline
                    .into_iter()
                    .enumerate()
                    .map(|(entry, (title, child))| {
                        div()
                            .id(("outline", entry))
                            .test_support()
                            .px_2()
                            .py_1()
                            .rounded(cx.theme().radius)
                            .cursor_pointer()
                            .hover(|this| this.bg(cx.theme().accent))
                            .child(title)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.scroll.scroll_to_top_of_item(child);
                                cx.notify();
                            }))
                    }),
            );

        div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                v_flex().flex_1().min_w_0().h_full().child(toolbar).child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(reader)
                        .vertical_scrollbar(&self.scroll),
                ),
            )
            .child(panel)
            .into_any_element()
    }

    /// The questions that go with the note, each with its answer behind a button.
    fn questions(&self, questions: &[usize], cx: &Context<Self>) -> impl IntoElement {
        let library = &AppState::get(cx).library;
        let cards = questions.iter().map(|&ix| {
            let question = &library.questions[ix];
            let shown = self.answers.contains(&ix);
            let options = question.options.iter().enumerate().map(|(option, text)| {
                let right = shown && Some(option) == question.answer;
                h_flex()
                    .items_start()
                    .gap_2()
                    .when(right, |this| {
                        this.font_medium().text_color(cx.theme().success)
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(28.))
                            .child(format!("({})", ["A", "B", "C", "D"][option])),
                    )
                    .child(div().min_w_0().child(text.clone()))
                    .when(right, |this| this.child(Icon::new(IconName::Check)))
            });
            v_flex()
                .p_4()
                .gap_3()
                .rounded(cx.theme().radius_lg)
                .border_1()
                .border_color(cx.theme().border)
                .when(!question.source.is_empty(), |card| {
                    card.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(question.source.clone()),
                    )
                })
                .child(TextView::markdown(("stem", ix), question.stem.clone()))
                .child(v_flex().gap_1().children(options))
                .when(shown, |card| {
                    card.child(div().child(question.explanation.clone()))
                        .when_some(question.trap.clone(), |card, trap| {
                            card.child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("The trap: {trap}")),
                            )
                        })
                })
                .child(
                    h_flex().child(
                        Button::new(("answer", ix))
                            .outline()
                            .small()
                            .label(if shown {
                                "Hide the answer"
                            } else {
                                "Show the answer"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.answers.remove(&ix) {
                                    this.answers.insert(ix);
                                }
                                cx.notify();
                            })),
                    ),
                )
        });
        v_flex()
            .pb_16()
            .gap_3()
            .child(
                div()
                    .text_size(rems(1.25))
                    .font_semibold()
                    .child("Questions on this note"),
            )
            .children(cards)
    }
}

impl Render for NotesScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .child(self.list(cx))
            .child(self.reader(cx))
    }
}

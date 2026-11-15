//! Stand-in screens. Each one shows what the content loader found for its
//! section and says which phase builds the real thing. They are replaced one
//! by one as the phases land.

use chrono::Local;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Screen;
use crate::content::{Library, Story};

/// A centred column with a title and a one-line summary. Screens add to it.
pub(crate) fn page(title: &'static str, summary: impl Into<SharedString>, cx: &App) -> Div {
    v_flex()
        .w_full()
        .max_w(px(860.))
        .mx_auto()
        .p_8()
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_2xl().font_semibold().child(title))
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(summary.into()),
                ),
        )
}

/// A titled list of rows.
fn section(title: impl Into<SharedString>, rows: Vec<Div>, cx: &App) -> Div {
    v_flex()
        .child(
            div()
                .pb_1()
                .text_sm()
                .font_medium()
                .text_color(cx.theme().muted_foreground)
                .child(title.into()),
        )
        .children(rows)
}

fn row(left: impl IntoElement, right: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .justify_between()
        .gap_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(div().min_w_0().child(left))
        .child(
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(right.into()),
        )
}

fn coming(phase: u8, what: &'static str, cx: &App) -> Div {
    div()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .bg(cx.theme().muted)
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(format!("Phase {phase} builds this screen: {what}."))
}

pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn first_subject(library: &Library, topics: &[String]) -> String {
    topics
        .first()
        .map(|topic| library.subject_label(topic))
        .unwrap_or_default()
}

fn story_rows(library: &Library, stories: &[Story], cx: &App) -> Vec<Div> {
    stories
        .iter()
        .map(|story| {
            row(
                story.title.clone(),
                first_subject(library, &story.topics),
                cx,
            )
        })
        .collect()
}

pub(crate) fn placeholder(screen: Screen, cx: &App) -> impl IntoElement {
    let library = &AppState::get(cx).library;

    match screen {
        Screen::Today => {
            let today = Local::now().date_naive();
            let tile = |value: usize, label: &'static str| {
                v_flex()
                    .flex_1()
                    .p_4()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(div().text_2xl().font_semibold().child(value.to_string()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
            };
            page(
                screen.title(),
                today.format("%A, %-d %B %Y").to_string(),
                cx,
            )
            .child(coming(
                3,
                "your plan for the day, your streak and the mentor's advice",
                cx,
            ))
            .child(
                h_flex()
                    .gap_4()
                    .child(tile(library.notes.len(), "notes"))
                    .child(tile(library.questions.len(), "questions"))
                    .child(tile(library.briefs.len(), "daily briefs")),
            )
            .when_some(library.briefs.first(), |page, brief| {
                page.child(section(
                    format!("Latest brief, {}", brief.date.format("%-d %B")),
                    story_rows(library, &brief.stories, cx),
                    cx,
                ))
            })
        }
        Screen::CurrentAffairs => page(
            screen.title(),
            count(library.briefs.len(), "daily brief", "daily briefs"),
            cx,
        )
        .child(coming(
            4,
            "the daily brief reader, editorials and the daily quiz",
            cx,
        ))
        .children(library.briefs.iter().map(|brief| {
            section(
                brief.date.format("%A, %-d %B %Y").to_string(),
                story_rows(library, &brief.stories, cx),
                cx,
            )
        })),
        Screen::Analysis => page(
            screen.title(),
            "Where your marks come from and where they go",
            cx,
        )
        .child(coming(
            3,
            "score trends, accuracy by subject and topic, and guess analysis",
            cx,
        )),
        Screen::Revision => {
            page(screen.title(), "What is due for another look today", cx).child(coming(
                5,
                "the mistake book and flashcards on a spaced schedule",
                cx,
            ))
        }
        Screen::Mains => page(screen.title(), "Answer writing against the clock", cx).child(
            coming(6, "model answers, a timed editor and self-evaluation", cx),
        ),
        Screen::Notebook => page(screen.title(), "Your highlights, clips and own notes", cx).child(
            coming(5, "highlights, clips, bookmarks and your own notes", cx),
        ),
        // The shell renders these itself; this arm only keeps the match total.
        Screen::Notes | Screen::Practice | Screen::Syllabus | Screen::Settings => {
            page(screen.title(), "", cx)
        }
    }
}

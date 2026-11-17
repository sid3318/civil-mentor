//! Stand-in screens. Each one shows what the content loader found for its
//! section and says which phase builds the real thing. They are replaced one
//! by one as the phases land.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, v_flex};
use gpui_kit::*;

use crate::app::Screen;

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

pub(crate) fn placeholder(screen: Screen, cx: &App) -> impl IntoElement {
    match screen {
        Screen::Mains => page(screen.title(), "Answer writing against the clock", cx).child(
            coming(6, "model answers, a timed editor and self-evaluation", cx),
        ),
        Screen::Notebook => page(screen.title(), "Your highlights, clips and own notes", cx).child(
            coming(5, "highlights, clips, bookmarks and your own notes", cx),
        ),
        // The shell renders these itself; this arm only keeps the match total.
        Screen::Today
        | Screen::Revision
        | Screen::Notes
        | Screen::CurrentAffairs
        | Screen::Practice
        | Screen::Analysis
        | Screen::Syllabus
        | Screen::Settings => page(screen.title(), "", cx),
    }
}

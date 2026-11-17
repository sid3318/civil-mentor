//! Pieces every screen uses: the page column and counted nouns. The
//! stand-in screens that were here until each phase built its screen are
//! gone; Phase 6 replaced the last one, Mains.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, v_flex};
use gpui_kit::*;

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

pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

//! Pieces every screen uses: the page column and counted nouns. The
//! stand-in screens that were here until each phase built its screen are
//! gone; Phase 6 replaced the last one, Mains.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::ui;

/// A centred column under the screen's header, starting with a one-line
/// summary. Screens add to it.
pub(crate) fn page(summary: impl Into<SharedString>, cx: &App) -> Div {
    let summary = summary.into();
    v_flex()
        .w_full()
        .max_w(px(860.))
        .mx_auto()
        .px_8()
        .pt_6()
        .pb_8()
        .gap_6()
        .when(!summary.is_empty(), |page| {
            page.child(
                div()
                    .text_size(px(14.))
                    .text_color(ui::tokens(cx).text_2)
                    .child(summary),
            )
        })
}

pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

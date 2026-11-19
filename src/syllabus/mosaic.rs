//! Mosaic: the whole exam on one page, one square per topic, paper by paper
//! and subject by subject, each chapter's squares in a run of their own. A
//! square fills in with its subject's colour as its topic is read, so the
//! page shows at a glance how much of the syllabus is done, and where.
//! Pointing at a square names it in the strip above; a square with a note
//! opens it.

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Status, Tree, View, describe, facts, status_bar};
use crate::screens::count;
use crate::ui;

const TILE: f32 = 13.;
const LABEL: f32 = 216.;

/// A topic's square: its subject's colour, as strong as its status.
fn tile(tree: &Tree, leaf: usize, color: Hsla, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let status = tree.statuses[leaf];
    let note = tree.note(leaf);
    let pointed = view.hovered == Some(leaf);
    let hover = view.clone();
    let open = view.clone();
    div()
        .id(("tile", leaf))
        .test_support()
        .flex_shrink_0()
        .size(px(TILE))
        .rounded(px(3.))
        .bg(color.opacity(status.fill()))
        .when(status == Status::NotStarted, |this| {
            this.border_1().border_color(color.opacity(0.75))
        })
        .when(pointed, |this| this.border_2().border_color(t.text))
        .on_hover(move |on, _, cx| hover.pointed(leaf, *on, cx))
        .when_some(note, |this, note| {
            this.cursor_pointer()
                .on_click(move |_, _, cx| open.open_note(note, cx))
        })
        .into_any_element()
}

/// A subject's row: its name and count, then its squares.
fn row(tree: &Tree, subject: usize, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let topic = tree.topic(subject);
    let color = tree.color(subject, cx);
    let tally = tree.tally[subject];
    let groups: Vec<Vec<usize>> = if topic.leaf {
        vec![vec![subject]]
    } else {
        tree.kids(subject)
            .iter()
            .map(|&chapter| tree.leaves(chapter))
            .collect()
    };
    let mut tiles = Vec::new();
    for (n, group) in groups.iter().enumerate() {
        if n > 0 {
            // A little more room before the next chapter's run.
            tiles.push(div().w(px(4.)).h(px(TILE)).into_any_element());
        }
        tiles.extend(group.iter().map(|&leaf| tile(tree, leaf, color, view, cx)));
    }
    h_flex()
        .items_start()
        .gap(px(16.))
        .py(px(6.))
        .child(
            h_flex()
                .w(px(LABEL))
                .flex_shrink_0()
                .h(px(TILE + 2.))
                .gap(px(8.))
                .child(ui::dot(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.5))
                        .text_color(t.text)
                        .child(topic.title.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.5))
                        .text_color(t.text_3)
                        .child(format!("{}/{}", tally.done, tally.total())),
                ),
        )
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .pt(px(1.))
                .flex_wrap()
                .gap(px(3.))
                .children(tiles),
        )
        .into_any_element()
}

/// A paper: its name, stage and marks, how much of it is read, then a row
/// per subject.
fn block(tree: &Tree, paper: usize, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let topic = tree.topic(paper);
    let tally = tree.tally[paper];
    let about = [
        tree.parent[paper]
            .map(|stage| tree.topic(stage).title.clone())
            .unwrap_or_default(),
        facts(topic.official.as_deref().unwrap_or_default()),
        count(tally.total(), "topic", "topics"),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    v_flex()
        .w_full()
        .max_w(px(1120.))
        .mx_auto()
        .px(px(28.))
        .pt(px(24.))
        .child(
            h_flex()
                .items_end()
                .gap(px(16.))
                .pb(px(10.))
                .mb(px(6.))
                .border_b_1()
                .border_color(t.border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .font_family(ui::DISPLAY)
                                .text_size(px(18.))
                                .line_height(px(24.))
                                .font_semibold()
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .child(ui::meta(about, cx)),
                )
                .child(
                    v_flex()
                        .flex_shrink_0()
                        .items_end()
                        .gap(px(6.))
                        .child(ui::meta(
                            format!("{} of {} read", tally.done, tally.total()),
                            cx,
                        ))
                        .child(div().w(px(132.)).child(status_bar(tally, 4., &t))),
                ),
        )
        .children(
            tree.kids(paper)
                .iter()
                .map(|&subject| row(tree, subject, view, cx)),
        )
        .into_any_element()
}

/// The strip above the mosaic: the square under the pointer, or how to read
/// the page, and what each strength of colour means.
fn lens(tree: &Tree, view: &View, cx: &App) -> impl IntoElement + use<> {
    let t = ui::tokens(cx);
    let pointed = view
        .hovered
        .filter(|&ix| ix < tree.statuses.len() && tree.topic(ix).leaf);
    let what = match pointed {
        Some(ix) => {
            let topic = tree.topic(ix);
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(t.text_3)
                        .child(tree.trail(ix)),
                )
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(tree.statuses[ix].icon(cx).size(px(14.)))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(14.))
                                .font_medium()
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .child(
                            ui::meta(describe(tree.library, tree.reading, &topic.id), cx)
                                .flex_shrink_0(),
                        ),
                )
        }
        None => {
            let all: usize = tree
                .stages()
                .iter()
                .map(|&stage| tree.tally[stage].total())
                .sum();
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_medium()
                        .text_color(t.text)
                        .child(format!("The whole syllabus: {all} topics, one square each")),
                )
                .child(ui::meta(
                    "Point at a square to see its topic. A square with a note opens it.",
                    cx,
                ))
        }
    };
    let sample = |status: Status| {
        h_flex()
            .gap(px(6.))
            .child(
                div()
                    .size(px(TILE))
                    .rounded(px(3.))
                    .bg(t.accent.opacity(status.fill()))
                    .when(status == Status::NotStarted, |this| {
                        this.border_1().border_color(t.accent.opacity(0.75))
                    }),
            )
            .child(status.label())
    };
    h_flex()
        .flex_shrink_0()
        .h(px(64.))
        .px(px(28.))
        .gap(px(24.))
        .border_b_1()
        .border_color(t.border)
        .child(what)
        .child(
            h_flex()
                .flex_shrink_0()
                .gap(px(14.))
                .text_size(px(12.))
                .text_color(t.text_3)
                .children(
                    [
                        Status::Done,
                        Status::Reading,
                        Status::NotStarted,
                        Status::NoNotes,
                    ]
                    .map(sample),
                ),
        )
}

pub(super) fn view(tree: &Tree, view: View, cx: &App) -> AnyElement {
    let papers: Vec<usize> = tree
        .stages()
        .into_iter()
        .flat_map(|stage| tree.kids(stage).to_vec())
        .collect();
    let blocks: Vec<AnyElement> = papers
        .iter()
        .map(|&paper| block(tree, paper, &view, cx))
        .collect();
    let scroll = view.scroll.clone();
    v_flex()
        .size_full()
        .child(lens(tree, &view, cx))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("mosaic")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .children(blocks)
                        .child(div().h(px(40.))),
                )
                .vertical_scrollbar(&scroll),
        )
        .into_any_element()
}

//! Mind map: the focus in the middle, its parts branching out on both sides
//! and each part's own parts fanning out beyond it, the way students draw a
//! syllabus on paper. Clicking a part puts it in the middle; clicking the
//! middle, or a step of the path above, goes back up.

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Colorize as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Status, Tree, View, focus_bar, tip, topic_tip};
use crate::ui;

const PAD: f32 = 24.;
const MIDDLE_W: f32 = 172.;
const MIDDLE_H: f32 = 78.;
/// A part's pill.
const PART_W: f32 = 164.;
const PART_H: f32 = 42.;
/// From the middle to a part's pill.
const REACH: f32 = 44.;
/// From a part's pill to its own parts.
const TWIG: f32 = 28.;
/// The line one of a part's own parts sits on.
const LINE: f32 = 25.;
/// Between parts on one side.
const SPACE: f32 = 16.;
const LABEL_MIN: f32 = 140.;
/// With everything on one side there is room to read more of each label.
const ONE_SIDE_MIN: f32 = 200.;
const LABEL_MAX: f32 = 290.;

/// A branch from the middle to a point, or from a pill to a twig.
struct Curve {
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
    color: Hsla,
}

/// A part around the middle, placed: its pill's corner, which side it is
/// on, and where each of its own parts sits.
struct Branch {
    ix: usize,
    right: bool,
    x: f32,
    y: f32,
    twigs: Vec<(usize, f32)>,
}

/// A twig's mark: a dot filled in by its topic's status, or a small ring of
/// how much of a group is read.
fn mark(tree: &Tree, ix: usize, color: Hsla, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    if !tree.topic(ix).leaf {
        return ui::ring(tree.tally[ix].share(), 11., 2.2, color, div(), cx).into_any_element();
    }
    div()
        .flex_shrink_0()
        .size(px(9.))
        .rounded_full()
        .map(|this| match tree.statuses[ix] {
            Status::Done => this.bg(color),
            Status::Reading => this.bg(color.opacity(0.55)),
            Status::NotStarted => this.border_2().border_color(color),
            Status::NoNotes => this.border_1().border_color(t.text_4),
        })
        .into_any_element()
}

/// The id a node takes: topics are `topic`, groups are `part`.
fn id_of(tree: &Tree, ix: usize) -> ElementId {
    if tree.topic(ix).leaf {
        ("topic", ix).into()
    } else {
        ("part", ix).into()
    }
}

/// The map, laid out for the room it has: on both sides of the middle when
/// it fits, otherwise all to the right.
fn map(view: View, room: Size<Pixels>, cx: &mut App) -> impl IntoElement + use<> {
    let tree = Tree::new(cx);
    let t = ui::tokens(cx);
    let parts = tree.kids(view.focus);
    let (room_w, room_h) = (f32::from(room.width), f32::from(room.height));
    let side = |label: f32| REACH + PART_W + TWIG + label;
    let both = room_w >= 2. * PAD + MIDDLE_W + 2. * side(LABEL_MIN);
    let label = if both {
        ((room_w - 2. * PAD - MIDDLE_W) / 2. - side(0.)).clamp(LABEL_MIN, LABEL_MAX)
    } else {
        (room_w - 2. * PAD - MIDDLE_W - side(0.)).clamp(ONE_SIDE_MIN, LABEL_MAX)
    };
    let block = |ix: usize| (tree.kids(ix).len() as f32 * LINE).max(PART_H);

    // Split the parts where both sides come out closest in height.
    let heights: Vec<f32> = parts.iter().map(|&ix| block(ix) + SPACE).collect();
    let total: f32 = heights.iter().sum();
    let mut split = parts.len();
    if both && parts.len() > 1 {
        let mut run = 0.;
        let mut best = f32::MAX;
        for (n, h) in heights.iter().enumerate().take(parts.len() - 1) {
            run += h;
            let off = (2. * run - total).abs();
            if off < best {
                best = off;
                split = n + 1;
            }
        }
    }
    let (right, left) = parts.split_at(split);
    let tall = |side: &[usize]| side.iter().map(|&ix| block(ix) + SPACE).sum::<f32>() - SPACE;
    let height = (tall(right).max(tall(left)).max(MIDDLE_H) + 2. * PAD).max(room_h);
    let width = if both {
        2. * PAD + MIDDLE_W + 2. * side(label)
    } else {
        2. * PAD + MIDDLE_W + side(label)
    }
    .max(room_w);
    let mid_x = if both {
        width / 2.
    } else {
        PAD + MIDDLE_W / 2.
    };
    // The middle stays on the first screen when the map is taller than it.
    let mid_y = (height / 2.).min(room_h / 2.);

    let lay = |side: &[usize], right: bool| -> Vec<Branch> {
        let mut y = (mid_y - tall(side) / 2.).max(PAD);
        side.iter()
            .map(|&ix| {
                let h = block(ix);
                let kids = tree.kids(ix);
                let first = y + (h - kids.len() as f32 * LINE) / 2.;
                let branch = Branch {
                    ix,
                    right,
                    x: if right {
                        mid_x + MIDDLE_W / 2. + REACH
                    } else {
                        mid_x - MIDDLE_W / 2. - REACH - PART_W
                    },
                    y: y + h / 2. - PART_H / 2.,
                    twigs: kids
                        .iter()
                        .enumerate()
                        .map(|(n, &kid)| (kid, first + n as f32 * LINE))
                        .collect(),
                };
                y += h + SPACE;
                branch
            })
            .collect()
    };
    let branches: Vec<Branch> = lay(right, true)
        .into_iter()
        .chain(lay(left, false))
        .collect();

    let ground = t.canvas;
    let mut curves = Vec::new();
    let mut nodes: Vec<AnyElement> = Vec::new();
    for branch in &branches {
        let color = tree.color(branch.ix, cx);
        let twig_color = color.mix_oklab(ground, 0.5);
        let (inner, outer) = if branch.right {
            (branch.x, branch.x + PART_W)
        } else {
            (branch.x + PART_W, branch.x)
        };
        let pill_y = branch.y + PART_H / 2.;
        let edge = if branch.right {
            mid_x + MIDDLE_W / 2.
        } else {
            mid_x - MIDDLE_W / 2.
        };
        curves.push(Curve {
            from: (edge, mid_y),
            to: (inner, pill_y),
            width: 2.5,
            color,
        });
        let label_x = if branch.right {
            outer + TWIG
        } else {
            outer - TWIG - label
        };
        for &(kid, y) in &branch.twigs {
            let mark_x = if branch.right {
                label_x + 5.
            } else {
                label_x + label - 5.
            };
            curves.push(Curve {
                from: (outer, pill_y),
                to: (mark_x, y + LINE / 2.),
                width: 1.5,
                color: twig_color,
            });
            nodes.push(twig(
                &tree,
                kid,
                (label_x, y),
                label,
                branch.right,
                color,
                &view,
                cx,
            ));
        }
        nodes.push(pill(&tree, branch, color, &view, cx));
    }

    div()
        .id("mind-map")
        .size_full()
        .overflow_scroll()
        .track_scroll(&view.scroll)
        .child(
            div()
                .relative()
                .w(px(width))
                .h(px(height))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let at = |(x, y): (f32, f32)| bounds.origin + point(px(x), px(y));
                            for curve in &curves {
                                let bend = (curve.to.0 - curve.from.0) / 2.;
                                let mut path = PathBuilder::stroke(px(curve.width));
                                path.move_to(at(curve.from));
                                path.cubic_bezier_to(
                                    at(curve.to),
                                    at((curve.from.0 + bend, curve.from.1)),
                                    at((curve.to.0 - bend, curve.to.1)),
                                );
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, curve.color);
                                }
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .children(nodes)
                .child(middle(&tree, &view, (mid_x, mid_y), cx)),
        )
}

/// The focus, in the middle. Clicking it goes up a level.
fn middle(tree: &Tree, view: &View, (x, y): (f32, f32), cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let focus = view.focus;
    let tally = tree.tally[focus];
    let color = if focus == view.paper {
        t.accent
    } else {
        tree.color(focus, cx)
    };
    let up = (focus != view.paper).then(|| tree.parent[focus]).flatten();
    v_flex()
        .id("mind-middle")
        .test_support()
        .absolute()
        .left(px(x - MIDDLE_W / 2.))
        .top(px(y - MIDDLE_H / 2.))
        .w(px(MIDDLE_W))
        .h(px(MIDDLE_H))
        .px(px(14.))
        .gap(px(6.))
        .justify_center()
        .items_center()
        .rounded(px(16.))
        .bg(t.card)
        .border_2()
        .border_color(color)
        .shadow(ui::lift(cx))
        .child(
            div()
                .text_size(px(14.))
                .line_height(px(18.))
                .font_semibold()
                .text_color(t.text)
                .text_center()
                .line_clamp(2)
                .text_ellipsis()
                .child(tree.topic(focus).title.clone()),
        )
        .child(
            h_flex()
                .gap(px(6.))
                .child(ui::ring(tally.share(), 13., 2.5, color, div(), cx))
                .child(ui::meta(
                    format!("{} of {} read", tally.done, tally.total()),
                    cx,
                )),
        )
        .when_some(up, |this, up| {
            let view = view.clone();
            this.cursor_pointer()
                .hover(|this| this.bg(t.popover))
                .tooltip(tip(format!("Back to {}", tree.topic(up).title)))
                .on_click(move |_, _, cx| view.update(cx, |screen| screen.focus_on(up)))
        })
        .into_any_element()
}

/// A part's pill, in its subject's colour. A group opens in the middle; a
/// topic opens its note.
fn pill(tree: &Tree, branch: &Branch, color: Hsla, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let ix = branch.ix;
    let topic = tree.topic(ix);
    let tally = tree.tally[ix];
    let action = view.open(ix, tree);
    h_flex()
        .id(id_of(tree, ix))
        .test_support()
        .absolute()
        .left(px(branch.x))
        .top(px(branch.y))
        .w(px(PART_W))
        .h(px(PART_H))
        .px(px(12.))
        .gap(px(8.))
        .rounded(px(12.))
        .bg(t.card)
        .border_1()
        .border_color(color.opacity(0.55))
        .when(!branch.right, |this| this.flex_row_reverse())
        .child(mark(tree, ix, color, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.5))
                .line_height(px(15.))
                .font_medium()
                .text_color(color.mix_oklab(t.text, 0.35))
                .line_clamp(2)
                .text_ellipsis()
                .when(!branch.right, |this| this.text_right())
                .child(topic.title.clone()),
        )
        .map(|this| {
            if topic.leaf {
                this.tooltip(topic_tip(topic.id.clone()))
            } else {
                this.tooltip(tip(format!(
                    "{} · {} of {} topics read",
                    topic.title,
                    tally.done,
                    tally.total()
                )))
            }
        })
        .when_some(action, |this, action| {
            this.cursor_pointer()
                .hover(|this| this.bg(color.opacity(0.1)).border_color(color))
                .on_click(move |_, _, cx| action(cx))
        })
        .into_any_element()
}

/// One of a part's own parts, on its line beyond the pill.
#[allow(clippy::too_many_arguments)]
fn twig(
    tree: &Tree,
    ix: usize,
    (x, y): (f32, f32),
    width: f32,
    right: bool,
    color: Hsla,
    view: &View,
    cx: &App,
) -> AnyElement {
    let t = ui::tokens(cx);
    let topic = tree.topic(ix);
    let action = view.open(ix, tree);
    let parts = tree.kids(ix).len();
    h_flex()
        .id(id_of(tree, ix))
        .test_support()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .h(px(LINE))
        .px(px(1.))
        .gap(px(7.))
        .rounded(px(5.))
        .when(!right, |this| this.flex_row_reverse())
        .child(mark(tree, ix, color, cx))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(12.5))
                .text_color(if topic.leaf { t.text_2 } else { t.text })
                .child(topic.title.clone()),
        )
        .when(parts > 0, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .px(px(5.))
                    .rounded_full()
                    .bg(color.opacity(0.12))
                    .text_size(px(10.5))
                    .font_medium()
                    .text_color(color.mix_oklab(t.text, 0.5))
                    .child(parts.to_string()),
            )
        })
        .map(|this| {
            if topic.leaf {
                this.tooltip(topic_tip(topic.id.clone()))
            } else {
                this.tooltip(tip(topic.title.clone()))
            }
        })
        .when_some(action, |this, action| {
            this.cursor_pointer()
                .hover(|this| this.bg(t.hover))
                .on_click(move |_, _, cx| action(cx))
        })
        .into_any_element()
}

pub(super) fn view(tree: &Tree, view: View, cx: &App) -> AnyElement {
    let scroll = view.scroll.clone();
    v_flex()
        .size_full()
        .child(focus_bar(tree, &view, cx))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(container_query(move |room, _, cx| map(view, room, cx)))
                .vertical_scrollbar(&scroll)
                .horizontal_scrollbar(&scroll),
        )
        .into_any_element()
}

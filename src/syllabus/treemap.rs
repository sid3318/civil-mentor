//! Treemap: the focus split into rectangles, one per part, each sized by
//! how many topics it holds, with the part's own parts nested inside, so the
//! weight of a paper shows at a glance. A rectangle deepens in its subject's
//! colour as its topics are read. Clicking a part zooms into it; a topic
//! opens its note.

use gpui_kit::component::{StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Status, Tree, View, focus_bar, tip, topic_tip};
use crate::ui;

/// A part's title strip.
const HEAD: f32 = 28.;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn inset(self, by: f32) -> Rect {
        Rect {
            x: self.x + by,
            y: self.y + by,
            w: (self.w - 2. * by).max(0.),
            h: (self.h - 2. * by).max(0.),
        }
    }
}

/// The worst ratio of long side to short among a row of areas laid along a
/// side of length `side`.
fn worst(row: &[f32], side: f32) -> f32 {
    let sum: f32 = row.iter().sum();
    let (least, most) = row.iter().fold((f32::MAX, 0f32), |(lo, hi), &area| {
        (lo.min(area), hi.max(area))
    });
    let (side, sum) = (side * side, sum * sum);
    (side * most / sum).max(sum / (side * least))
}

/// Splits `area` into rectangles in proportion to `weights`, in order, as
/// near square as it can: the squarified treemap of Bruls, Huizing and van
/// Wijk, without its sort, so the syllabus keeps its order.
fn squarify(weights: &[f32], area: Rect) -> Vec<Rect> {
    let total: f32 = weights.iter().sum();
    if total <= 0. || area.w <= 0. || area.h <= 0. {
        return vec![
            Rect {
                w: 0.,
                h: 0.,
                ..area
            };
            weights.len()
        ];
    }
    let areas: Vec<f32> = weights
        .iter()
        .map(|weight| weight * area.w * area.h / total)
        .collect();
    let mut rects = Vec::with_capacity(areas.len());
    let mut rest = area;
    let mut start = 0;
    while start < areas.len() {
        let side = rest.w.min(rest.h);
        // Grow the row while that makes its worst rectangle squarer.
        let mut end = start + 1;
        while end < areas.len()
            && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
        {
            end += 1;
        }
        let row = &areas[start..end];
        let thick = row.iter().sum::<f32>() / side;
        let mut along = 0.;
        for &part in row {
            let len = part / thick;
            rects.push(if rest.w >= rest.h {
                Rect {
                    x: rest.x,
                    y: rest.y + along,
                    w: thick,
                    h: len,
                }
            } else {
                Rect {
                    x: rest.x + along,
                    y: rest.y,
                    w: len,
                    h: thick,
                }
            });
            along += len;
        }
        rest = if rest.w >= rest.h {
            Rect {
                x: rest.x + thick,
                w: rest.w - thick,
                ..rest
            }
        } else {
            Rect {
                y: rest.y + thick,
                h: rest.h - thick,
                ..rest
            }
        };
        start = end;
    }
    rects
}

/// Each part's weight: how many topics it holds.
fn weights(tree: &Tree, parts: &[usize]) -> Vec<f32> {
    parts
        .iter()
        .map(|&ix| tree.tally[ix].total().max(1) as f32)
        .collect()
}

/// How strongly a cell takes its colour. Capped, so that its text stays
/// readable on it in both themes.
fn strength(tree: &Tree, ix: usize) -> f32 {
    let fill = if tree.topic(ix).leaf {
        tree.statuses[ix].fill()
    } else {
        tree.tally[ix].fill()
    };
    0.07 + 0.5 * fill
}

/// Places a cell inside its parent.
fn place<E: Styled>(element: E, rect: Rect) -> E {
    element
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.w))
        .h(px(rect.h))
}

/// A topic's cell, opening its note.
fn leaf(tree: &Tree, ix: usize, rect: Rect, size: f32, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let topic = tree.topic(ix);
    let color = tree.color(ix, cx);
    let note = tree.note(ix);
    let lines = ((rect.h - 8.) / (size + 3.)).floor().max(1.) as usize;
    let view = view.clone();
    place(div().id(("topic", ix)).test_support(), rect)
        .rounded(px(5.))
        .overflow_hidden()
        .bg(color.opacity(strength(tree, ix)))
        .when(tree.statuses[ix] == Status::NotStarted, |this| {
            this.border_1().border_color(color.opacity(0.7))
        })
        .when(rect.w >= 64. && rect.h >= size + 8., |this| {
            this.p(px(4.)).child(
                div()
                    .text_size(px(size))
                    .line_height(px(size + 3.))
                    .text_color(t.text)
                    .line_clamp(lines)
                    .text_ellipsis()
                    .child(topic.title.clone()),
            )
        })
        .tooltip(topic_tip(topic.id.clone()))
        .when_some(note, |this, note| {
            this.cursor_pointer()
                .hover(|this| this.border_1().border_color(color))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    view.open_note(note, cx);
                })
        })
        .into_any_element()
}

/// A part's own part, inside it: a coloured cell with its title when it
/// fits. Clicking it zooms straight into it.
fn inner(tree: &Tree, ix: usize, rect: Rect, view: &View, cx: &App) -> AnyElement {
    let rect = rect.inset(1.5);
    if tree.topic(ix).leaf {
        return leaf(tree, ix, rect, 11., view, cx);
    }
    let t = ui::tokens(cx);
    let topic = tree.topic(ix);
    let tally = tree.tally[ix];
    let color = tree.color(ix, cx);
    // Room for the count under the title, when the cell is tall enough.
    let counted = rect.w >= 64. && rect.h >= 48.;
    let lines = ((rect.h - if counted { 26. } else { 8. }) / 14.)
        .floor()
        .max(1.) as usize;
    let view = view.clone();
    place(v_flex().id(("cell", ix)).test_support(), rect)
        .rounded(px(5.))
        .overflow_hidden()
        .bg(color.opacity(strength(tree, ix)))
        .cursor_pointer()
        .hover(|this| this.border_1().border_color(color))
        .when(rect.w >= 64. && rect.h >= 20., |this| {
            this.p(px(4.)).child(
                div()
                    .flex_1()
                    .text_size(px(11.))
                    .line_height(px(14.))
                    .text_color(t.text)
                    .line_clamp(lines)
                    .text_ellipsis()
                    .child(topic.title.clone()),
            )
        })
        .when(counted, |this| {
            this.child(
                div()
                    .text_size(px(10.5))
                    .text_color(t.text_3)
                    .child(format!("{}/{}", tally.done, tally.total())),
            )
        })
        .tooltip(tip(format!(
            "{} · {} of {} topics read",
            topic.title,
            tally.done,
            tally.total()
        )))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            view.update(cx, |screen| screen.focus_on(ix));
        })
        .into_any_element()
}

/// One of the focus's parts: a frame in its subject's colour with its title
/// and count along the top and its own parts nested below.
fn part(tree: &Tree, ix: usize, rect: Rect, view: &View, cx: &App) -> AnyElement {
    let rect = rect.inset(2.5);
    if tree.topic(ix).leaf {
        return leaf(tree, ix, rect, 13., view, cx);
    }
    let t = ui::tokens(cx);
    let topic = tree.topic(ix);
    let tally = tree.tally[ix];
    let color = tree.color(ix, cx);
    let kids = tree.kids(ix);
    let body = Rect {
        x: 4.,
        y: HEAD,
        w: rect.w - 8.,
        h: rect.h - HEAD - 4.,
    };
    let roomy = body.w >= 60. && body.h >= 24.;
    let view = view.clone();
    place(div().id(("cell", ix)).test_support(), rect)
        .rounded(px(8.))
        .overflow_hidden()
        .bg(color.opacity(0.05))
        .border_1()
        .border_color(color.opacity(0.35))
        .cursor_pointer()
        .hover(|this| this.border_color(color))
        .child(
            h_flex()
                .h(px(HEAD))
                .px(px(9.))
                .gap(px(8.))
                .child(div().flex_shrink_0().size(px(7.)).rounded_full().bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.5))
                        .font_semibold()
                        .text_color(t.text)
                        .child(topic.title.clone()),
                )
                .when(rect.w >= 140., |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(t.text_3)
                            .child(format!("{}/{}", tally.done, tally.total())),
                    )
                }),
        )
        .when(roomy, |this| {
            this.children(
                kids.iter()
                    .zip(squarify(&weights(tree, kids), body))
                    .map(|(&kid, rect)| inner(tree, kid, rect, &view, cx)),
            )
        })
        .tooltip(tip(format!(
            "{} · {} of {} topics read",
            topic.title,
            tally.done,
            tally.total()
        )))
        .on_click(move |_, _, cx| view.update(cx, |screen| screen.focus_on(ix)))
        .into_any_element()
}

fn map(view: View, room: Size<Pixels>, cx: &mut App) -> impl IntoElement + use<> {
    let tree = Tree::new(cx);
    let parts = tree.kids(view.focus);
    let area = Rect {
        x: 0.,
        y: 0.,
        w: f32::from(room.width),
        h: f32::from(room.height),
    };
    div().relative().size_full().children(
        parts
            .iter()
            .zip(squarify(&weights(&tree, parts), area))
            .map(|(&ix, rect)| part(&tree, ix, rect, &view, cx)),
    )
}

pub(super) fn view(tree: &Tree, view: View, cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .child(focus_bar(tree, &view, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(12.))
                .child(container_query(move |room, _, cx| map(view, room, cx))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Rect, squarify};

    #[test]
    fn a_treemap_fills_its_area_in_proportion() {
        let area = Rect {
            x: 10.,
            y: 20.,
            w: 600.,
            h: 400.,
        };
        let weights = [97., 64., 61., 47., 40., 36., 35., 20., 5., 1.];
        let rects = squarify(&weights, area);
        assert_eq!(rects.len(), weights.len());
        let total: f32 = weights.iter().sum();
        for (rect, weight) in rects.iter().zip(weights) {
            let share = rect.w * rect.h / (area.w * area.h);
            assert!(
                (share - weight / total).abs() < 1e-4,
                "{rect:?} for {weight}"
            );
            assert!(rect.x >= area.x - 1e-3 && rect.y >= area.y - 1e-3);
            assert!(rect.x + rect.w <= area.x + area.w + 1e-3);
            assert!(rect.y + rect.h <= area.y + area.h + 1e-3);
        }
        assert_eq!(squarify(&[], area), Vec::<Rect>::new());
        let flat = squarify(&[1., 1.], Rect { w: 0., ..area });
        assert!(flat.iter().all(|rect| rect.w == 0.), "no room, no cells");
    }
}

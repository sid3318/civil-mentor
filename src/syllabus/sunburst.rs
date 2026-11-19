//! Sunburst: the focus as a disc with its parts in a ring around it, each
//! part's arc sized by how many topics it holds, and every ring beyond split
//! into the parts of the one inside it, down to single topics at the edge.
//! An arc deepens in its subject's colour as its topics are read. Pointing
//! at an arc shows it beside the chart; clicking zooms into it, or opens a
//! topic's note, and the disc in the middle zooms back out.

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Colorize as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Status, Tree, View, describe, focus_bar, quote, status_bar, status_legend, tip};
use crate::content::within;
use crate::ui;

const PANEL: f32 = 300.;
/// The rings' widths relative to each other, from the inside out.
const RINGS: [f32; 4] = [1., 0.82, 0.68, 0.42];
/// How strong an untouched ring is, from the inside out: the inner rings
/// carry the structure, the outer ones the texture.
const BASE: [f32; 4] = [0.36, 0.26, 0.2, 0.2];

/// A part's arc, in radians clockwise from the right and pixels from the
/// middle.
#[derive(Clone, Copy)]
struct Arc {
    ix: usize,
    start: f32,
    end: f32,
    inner: f32,
    outer: f32,
    color: Hsla,
    /// What clicking it does: a topic's note, or zooming in.
    leaf: bool,
    note: Option<usize>,
}

/// How many levels lie under `ix`.
fn depth_below(tree: &Tree, ix: usize) -> usize {
    tree.kids(ix)
        .iter()
        .map(|&kid| 1 + depth_below(tree, kid))
        .max()
        .unwrap_or(0)
}

/// Lays out `ix`'s parts between `start` and `end` on ring `ring`, and their
/// parts on the rings beyond.
#[allow(clippy::too_many_arguments)]
fn lay(
    tree: &Tree,
    ix: usize,
    ring: usize,
    (start, end): (f32, f32),
    radii: &[(f32, f32)],
    arcs: &mut Vec<Arc>,
    cx: &App,
) {
    let Some(&(inner, outer)) = radii.get(ring) else {
        return;
    };
    let total = tree.tally[ix].total().max(1) as f32;
    let mut at = start;
    for &kid in tree.kids(ix) {
        let span = (end - start) * tree.tally[kid].total() as f32 / total;
        let leaf = tree.topic(kid).leaf;
        let fill = if leaf {
            tree.statuses[kid].fill()
        } else {
            tree.tally[kid].fill()
        };
        // From untouched (no notes) to read.
        let progress =
            ((fill - Status::NoNotes.fill()) / (1. - Status::NoNotes.fill())).clamp(0., 1.);
        let base = BASE[ring.min(BASE.len() - 1)];
        arcs.push(Arc {
            ix: kid,
            start: at,
            end: at + span,
            inner,
            outer,
            color: tree.color(kid, cx).opacity(base + (1. - base) * progress),
            leaf,
            note: tree.note(kid),
        });
        lay(tree, kid, ring + 1, (at, at + span), radii, arcs, cx);
        at += span;
    }
}

/// The ring segment an arc covers, a hair short of its neighbours. One of
/// half a turn or more is drawn as two, since an arc cannot end where it
/// starts.
fn shape(arc: &Arc, centre: Point<Pixels>, path: &mut PathBuilder) {
    let span = arc.end - arc.start;
    if span >= PI {
        let mid = arc.start + span / 2.;
        shape(&Arc { end: mid, ..*arc }, centre, path);
        shape(&Arc { start: mid, ..*arc }, centre, path);
        return;
    }
    let at = |r: f32, a: f32| centre + point(px(r * a.cos()), px(r * a.sin()));
    let trim = |r: f32| (0.6 / r).min(span / 4.);
    let (outer, inner) = (arc.outer, arc.inner);
    path.move_to(at(outer, arc.start + trim(outer)));
    path.arc_to(
        point(px(outer), px(outer)),
        px(0.),
        false,
        true,
        at(outer, arc.end - trim(outer)),
    );
    path.line_to(at(inner, arc.end - trim(inner)));
    path.arc_to(
        point(px(inner), px(inner)),
        px(0.),
        false,
        false,
        at(inner, arc.start + trim(inner)),
    );
    path.close();
}

/// The arc under a point, given relative to the middle.
fn hit(arcs: &[Arc], x: f32, y: f32) -> Option<&Arc> {
    let r = (x * x + y * y).sqrt();
    let mut a = y.atan2(x);
    // The rings start at the top, so angles run from there.
    if a < -FRAC_PI_2 {
        a += TAU;
    }
    arcs.iter()
        .find(|arc| r >= arc.inner && r <= arc.outer && a >= arc.start && a < arc.end)
}

fn chart(view: View, room: Size<Pixels>, cx: &mut App) -> impl IntoElement + use<> {
    let tree = Tree::new(cx);
    let t = ui::tokens(cx);
    let (w, h) = (f32::from(room.width), f32::from(room.height));
    let radius = (w.min(h) / 2. - 18.).max(80.);
    let hole = radius * 0.28;
    let levels = depth_below(&tree, view.focus).clamp(1, RINGS.len());
    let unit = (radius - hole) / RINGS[..levels].iter().sum::<f32>();
    let mut radii = Vec::new();
    let mut r = hole;
    for width in &RINGS[..levels] {
        radii.push((r + 1.5, r + width * unit));
        r += width * unit;
    }
    let mut arcs = Vec::new();
    lay(
        &tree,
        view.focus,
        0,
        (-FRAC_PI_2, -FRAC_PI_2 + TAU),
        &radii,
        &mut arcs,
        cx,
    );
    let arcs = Rc::new(arcs);
    let (mid_x, mid_y) = (w / 2., h / 2.);
    // Where the chart is in the window, known once it is painted; the
    // pointer's position comes in window coordinates.
    let origin = Rc::new(Cell::new(Point::<Pixels>::default()));
    let local = {
        let origin = origin.clone();
        move |position: Point<Pixels>| {
            let at = position - origin.get();
            (f32::from(at.x) - mid_x, f32::from(at.y) - mid_y)
        }
    };
    let hovered = view.hovered;
    let outline = t.text;

    let tally = tree.tally[view.focus];
    let up = (view.focus != view.paper)
        .then(|| tree.parent[view.focus])
        .flatten();
    let disc = (hole - 6.).max(30.);
    let middle = v_flex()
        .id("sunburst-middle")
        .test_support()
        .absolute()
        .left(px(mid_x - disc))
        .top(px(mid_y - disc))
        .size(px(2. * disc))
        .justify_center()
        .items_center()
        .gap(px(2.))
        .px(px(10.))
        .rounded_full()
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(15.))
                .font_medium()
                .text_color(t.text_2)
                .text_center()
                .line_clamp(2)
                .text_ellipsis()
                .child(tree.topic(view.focus).title.clone()),
        )
        .child(
            div()
                .font_family(ui::DISPLAY)
                .text_size(px(24.))
                .line_height(px(28.))
                .font_semibold()
                .text_color(t.text)
                .child(format!("{:.0}%", tally.share() * 100.)),
        )
        .child(ui::meta(format!("{} of {}", tally.done, tally.total()), cx))
        .when_some(up, |this, up| {
            let view = view.clone();
            this.cursor_pointer()
                .hover(|this| this.bg(t.hover))
                .tooltip(tip(format!("Back to {}", tree.topic(up).title)))
                .on_click(move |_, _, cx| view.update(cx, |screen| screen.focus_on(up)))
        });

    let paint_arcs = arcs.clone();
    let paint_origin = origin.clone();
    let move_arcs = arcs.clone();
    let move_local = local.clone();
    let move_view = view.clone();
    let click_view = view.clone();
    div()
        .id("sunburst")
        .test_support()
        .relative()
        .size_full()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_origin.set(bounds.origin);
                    let centre = bounds.origin + point(px(mid_x), px(mid_y));
                    for arc in paint_arcs.iter() {
                        let mut path = PathBuilder::fill();
                        shape(arc, centre, &mut path);
                        if let Ok(path) = path.build() {
                            let color = if Some(arc.ix) == hovered {
                                arc.color.opacity(1.)
                            } else {
                                arc.color
                            };
                            window.paint_path(path, color);
                        }
                    }
                    if let Some(arc) = paint_arcs.iter().find(|arc| Some(arc.ix) == hovered) {
                        let mut path = PathBuilder::stroke(px(1.5));
                        shape(arc, centre, &mut path);
                        if let Ok(path) = path.build() {
                            window.paint_path(path, outline);
                        }
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .child(middle)
        .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
            let (x, y) = move_local(event.position);
            // Off the rings, the last arc stays shown, so that the pointer
            // can reach the panel; inside the disc, the focus shows again.
            if let Some(arc) = hit(&move_arcs, x, y) {
                move_view.hover(Some(arc.ix), cx);
            } else if (x * x + y * y).sqrt() < hole {
                move_view.hover(None, cx);
            }
        })
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            let (x, y) = local(event.position);
            let Some(arc) = hit(&arcs, x, y).copied() else {
                return;
            };
            if !arc.leaf {
                click_view.update(cx, |screen| screen.focus_on(arc.ix));
            } else if let Some(note) = arc.note {
                click_view.open_note(note, cx);
            }
        })
}

/// The panel beside the chart: the arc under the pointer, or the focus.
fn detail(tree: &Tree, view: &View, cx: &App) -> impl IntoElement + use<> {
    let t = ui::tokens(cx);
    let focus_id = &tree.topic(view.focus).id;
    let ix = view
        .hovered
        .filter(|&ix| ix < tree.statuses.len() && within(&tree.topic(ix).id, focus_id))
        .unwrap_or(view.focus);
    let topic = tree.topic(ix);
    let tally = tree.tally[ix];
    let color = tree.color(ix, cx);
    let trail = tree.trail(ix);

    let head = v_flex()
        .gap(px(6.))
        .when(!trail.is_empty(), |this| this.child(ui::meta(trail, cx)))
        .child(
            div()
                .font_family(ui::DISPLAY)
                .text_size(px(19.))
                .line_height(px(24.))
                .font_semibold()
                .text_color(t.text)
                .child(topic.title.clone()),
        );
    let words = tree.official(ix).map(|(node, official)| {
        let caption = if node == ix {
            "UPSC's line".to_string()
        } else {
            format!("UPSC's line for {}", tree.topic(node).title)
        };
        v_flex()
            .gap(px(6.))
            .child(ui::caption(caption, cx))
            .child(quote(official, 14., t.text))
    });
    let state = if topic.leaf {
        let note = tree.note(ix);
        let view = view.clone();
        v_flex()
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(tree.statuses[ix].icon(cx).size(px(15.)))
                    .child(div().text_size(px(13.)).text_color(t.text).child(describe(
                        tree.library,
                        tree.reading,
                        &topic.id,
                    ))),
            )
            .when_some(note, |this, note| {
                this.child(
                    h_flex()
                        .id(("open-note", ix))
                        .test_support()
                        .h(px(30.))
                        .px(px(12.))
                        .gap(px(6.))
                        .rounded(px(6.))
                        .bg(t.accent)
                        .text_color(t.on_accent)
                        .text_size(px(13.))
                        .font_medium()
                        .cursor_pointer()
                        .child("Open note")
                        .child(Icon::new(IconName::ArrowUpRight).size(px(14.)))
                        .on_click(move |_, _, cx| view.open_note(note, cx)),
                )
            })
            .into_any_element()
    } else {
        v_flex()
            .gap(px(10.))
            .child(
                h_flex()
                    .child(ui::meta(
                        format!("{} of {} topics read", tally.done, tally.total()),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_medium()
                            .text_color(t.text)
                            .child(format!("{:.0}%", tally.share() * 100.)),
                    ),
            )
            .child(status_bar(tally, 6., &t))
            .child(status_legend(tally, &t))
            .into_any_element()
    };
    let parts = (!topic.leaf).then(|| {
        v_flex().gap(px(8.)).child(ui::caption("Inside", cx)).child(
            h_flex()
                .flex_wrap()
                .gap(px(6.))
                .children(tree.kids(ix).iter().map(|&kid| {
                    let action = view.open(kid, tree);
                    let kid_topic = tree.topic(kid);
                    h_flex()
                        .id(("part", kid))
                        .test_support()
                        .max_w_full()
                        .h(px(24.))
                        .px(px(9.))
                        .gap(px(6.))
                        .rounded_full()
                        .bg(color.opacity(0.1))
                        .text_size(px(12.))
                        .text_color(color.mix_oklab(t.text, 0.35))
                        .child(div().flex_shrink_0().size(px(6.)).rounded_full().bg(color))
                        .child(div().min_w_0().truncate().child(kid_topic.title.clone()))
                        .when_some(action, |this, action| {
                            this.cursor_pointer()
                                .hover(|this| this.bg(color.opacity(0.2)))
                                .on_click(move |_, _, cx| action(cx))
                        })
                })),
        )
    });

    v_flex()
        .w(px(PANEL))
        .flex_shrink_0()
        .h_full()
        .border_l_1()
        .border_color(t.border)
        .child(
            div()
                .size_full()
                .overflow_y_scrollbar()
                .id("sunburst-detail")
                .child(
                    v_flex()
                        .p(px(18.))
                        .gap(px(18.))
                        .child(head)
                        .children(words)
                        .child(state)
                        .children(parts)
                        .child(ui::meta(
                            "Point at an arc to see it here. Click one to open it; the disc in the \
                         middle goes back up.",
                            cx,
                        )),
                ),
        )
}

pub(super) fn view(tree: &Tree, view: View, cx: &App) -> AnyElement {
    let panel = detail(tree, &view, cx);
    h_flex()
        .size_full()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(focus_bar(tree, &view, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(container_query(move |room, _, cx| chart(view, room, cx))),
                ),
        )
        .child(panel)
        .into_any_element()
}

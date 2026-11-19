//! Flow: the open paper as a canvas of cards, the way a workflow tool draws
//! one. The paper is the first card; its subjects hang from it, the open
//! subject's chapters from that and the open chapter's topics from that,
//! each column joined to its parent by connectors over a dotted canvas, with
//! the open path drawn in its subject's colour. Beside the canvas, an
//! overview: how much of the paper is read, UPSC's words for what is open,
//! what to read next, and every subject's progress.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Colorize as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{SyllabusScreen, Tree, View, facts, quote, status_bar, status_legend, tip, topic_tip};
use crate::screens::count;
use crate::ui;

/// Around the canvas's content.
const PAD: f32 = 24.;
/// Left of the subjects, where the paper's connector comes down.
const GUTTER: f32 = 28.;
/// Between columns. A column's connector runs down the middle of it.
const GAP: f32 = 48.;
const ROOT_H: f32 = 82.;
/// A subject's or a chapter's card.
const CARD_H: f32 = 82.;
/// A topic's card, or the head of one with subtopics.
const TOPIC_H: f32 = 46.;
/// A subtopic's line in its topic's card.
const SUB_H: f32 = 24.;
/// Between cards in a column.
const SPACE: f32 = 12.;
/// Above the columns, for the pill that says what a column holds.
const PILL_ROOM: f32 = 40.;
const PANEL: f32 = 272.;
const DOT_STEP: f32 = 18.;

/// A card on the canvas, in canvas coordinates.
#[derive(Clone, Copy)]
struct Card {
    ix: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Card {
    /// Where connectors meet the card: its middle, or the head of a topic
    /// card that lists subtopics.
    fn port(&self) -> f32 {
        let head = if self.h > CARD_H { TOPIC_H } else { self.h };
        self.y + head / 2.
    }
}

/// Stacks cards of the given heights at `x`, centred on `centre` but
/// starting no higher than `top`.
fn column(items: &[(usize, f32)], x: f32, w: f32, centre: f32, top: f32) -> Vec<Card> {
    let height = items.iter().map(|(_, h)| h + SPACE).sum::<f32>() - SPACE;
    let mut y = (centre - height / 2.).max(top);
    items
        .iter()
        .map(|&(ix, h)| {
            let card = Card { ix, x, y, w, h };
            y += h + SPACE;
            card
        })
        .collect()
}

/// How tall a node's card is: a topic's is short, a topic in the last
/// column lists its subtopics, and the rest are full cards.
fn height(tree: &Tree, ix: usize, last: bool) -> f32 {
    if tree.topic(ix).leaf {
        TOPIC_H
    } else if last {
        TOPIC_H + tree.leaves(ix).len() as f32 * SUB_H + 8.
    } else {
        CARD_H
    }
}

/// The open subject and chapter: the student's picks, or the first of each
/// that has parts to open.
fn picks(tree: &Tree, view: &View) -> (Option<usize>, Option<usize>) {
    let open = |kids: &[usize], pick: Option<usize>| {
        let branch = |ix: &usize| !tree.topic(*ix).leaf;
        pick.filter(|ix| kids.contains(ix) && branch(ix))
            .or_else(|| kids.iter().copied().find(branch))
    };
    let subject = open(tree.kids(view.paper), view.subject);
    let chapter = subject.and_then(|subject| open(tree.kids(subject), view.chapter));
    (subject, chapter)
}

/// A piece of a connector, in canvas coordinates. `Curve` goes to its first
/// point, bending towards its second.
#[derive(Clone, Copy)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Curve(f32, f32, f32, f32),
}

struct Stroke {
    segs: Vec<Seg>,
    color: Hsla,
}

/// A connector from a parent's edge at `from`, along to `trunk`, down or up
/// it, and along into a child's edge at `to`, with rounded corners. One that
/// starts on the trunk runs straight down it first.
fn elbow(from: (f32, f32), trunk: f32, to: (f32, f32)) -> Vec<Seg> {
    let ((x0, y0), (x1, y1)) = (from, to);
    let dy = y1 - y0;
    if dy.abs() < 1. {
        return vec![Seg::Move(x0, y0), Seg::Line(x1, y1)];
    }
    let s = dy.signum();
    let r = 8f32.min(dy.abs() / 2.);
    let mut segs = vec![Seg::Move(x0, y0)];
    if x0 < trunk - 0.5 {
        segs.push(Seg::Line(trunk - r, y0));
        segs.push(Seg::Curve(trunk, y0 + s * r, trunk, y0));
    }
    segs.push(Seg::Line(trunk, y1 - s * r));
    segs.push(Seg::Curve(trunk + r, y1, trunk, y1));
    segs.push(Seg::Line(x1, y1));
    segs
}

/// Draws the dotted canvas where it shows, then the connectors, then a dot
/// where each one meets a card.
fn paint(
    bounds: Bounds<Pixels>,
    strokes: &[Stroke],
    ports: &[(f32, f32, Hsla)],
    grid: Hsla,
    ground: Hsla,
    window: &mut Window,
) {
    let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
    let shown = bounds.intersect(&window.content_mask().bounds);
    let left = f32::from(shown.origin.x - bounds.origin.x);
    let top = f32::from(shown.origin.y - bounds.origin.y);
    let right = left + f32::from(shown.size.width);
    let bottom = top + f32::from(shown.size.height);
    let mut y = (top / DOT_STEP).floor() * DOT_STEP + DOT_STEP / 2.;
    while y < bottom {
        let mut x = (left / DOT_STEP).floor() * DOT_STEP + DOT_STEP / 2.;
        while x < right {
            window.paint_quad(
                fill(Bounds::centered_at(at(x, y), size(px(1.6), px(1.6))), grid)
                    .corner_radii(px(0.8)),
            );
            x += DOT_STEP;
        }
        y += DOT_STEP;
    }
    for stroke in strokes {
        let mut path = PathBuilder::stroke(px(1.5));
        for &seg in &stroke.segs {
            match seg {
                Seg::Move(x, y) => path.move_to(at(x, y)),
                Seg::Line(x, y) => path.line_to(at(x, y)),
                Seg::Curve(x, y, bend_x, bend_y) => path.curve_to(at(x, y), at(bend_x, bend_y)),
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, stroke.color);
        }
    }
    for &(x, y, color) in ports {
        window.paint_quad(
            fill(Bounds::centered_at(at(x, y), size(px(8.), px(8.))), color)
                .corner_radii(px(4.))
                .border_widths(px(2.))
                .border_color(ground),
        );
    }
}

/// A count on a card's footer, with its icon: topics read, notes, questions.
fn chip(icon: IconName, label: String, tint: Option<Hsla>, t: &ui::Tokens) -> Div {
    h_flex()
        .flex_shrink_0()
        .h(px(20.))
        .px(px(6.))
        .gap(px(4.))
        .rounded(px(5.))
        .border_1()
        .text_size(px(11.))
        .font_medium()
        .map(|this| match tint {
            Some(color) => this
                .bg(color.opacity(0.1))
                .border_color(color.opacity(0.3))
                .text_color(color.mix_oklab(t.text, 0.7)),
            None => this.border_color(t.border_2).text_color(t.text_3),
        })
        .child(Icon::new(icon).size(px(11.)))
        .child(label)
}

/// A card's footer: how many of its topics are read, and how many notes and
/// questions there are on it.
fn chips(tree: &Tree, ix: usize, t: &ui::Tokens) -> Div {
    let tally = tree.tally[ix];
    let (notes, questions) = tree.material(ix);
    h_flex()
        .gap(px(4.))
        .child(chip(
            IconName::CircleCheck,
            format!("{}/{}", tally.done, tally.total()),
            (tally.done > 0).then_some(t.success),
            t,
        ))
        .child(chip(
            IconName::NotebookText,
            notes.to_string(),
            (notes > 0).then_some(t.accent),
            t,
        ))
        .child(chip(
            IconName::CircleQuestionMark,
            questions.to_string(),
            None,
            t,
        ))
}

/// Places a card on the canvas.
fn place<E: Styled>(element: E, card: Card) -> E {
    element
        .absolute()
        .left(px(card.x))
        .top(px(card.y))
        .w(px(card.w))
        .h(px(card.h))
}

/// The paper's card, at the head of the canvas.
fn root_card(tree: &Tree, card: Card, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let topic = tree.topic(card.ix);
    let about = [
        facts(topic.official.as_deref().unwrap_or_default()),
        count(tree.kids(card.ix).len(), "subject", "subjects"),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    place(v_flex(), card)
        .p(px(10.))
        .gap(px(10.))
        .rounded(px(10.))
        .bg(t.card)
        .border_1()
        .border_color(t.border_2)
        .shadow(ui::lift(cx))
        .child(
            h_flex()
                .gap(px(10.))
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .size(px(32.))
                        .justify_center()
                        .rounded(px(8.))
                        .bg(t.accent_soft)
                        .child(
                            Icon::new(IconName::BookOpen)
                                .size(px(16.))
                                .text_color(t.accent),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.5))
                                .font_semibold()
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(t.text_3)
                                .child(about),
                        ),
                ),
        )
        .child(chips(tree, card.ix, &t))
        .into_any_element()
}

/// A subject's or a chapter's card: its progress as a ring in the subject's
/// colour, its title, and its counts. Clicking it opens it in the next
/// column.
fn node_card(
    tree: &Tree,
    card: Card,
    open: bool,
    view: &View,
    pick: fn(&mut SyllabusScreen, usize),
    cx: &App,
) -> AnyElement {
    let t = ui::tokens(cx);
    let ix = card.ix;
    let topic = tree.topic(ix);
    let color = tree.color(ix, cx);
    let view = view.clone();
    place(v_flex().id(("node", ix)).test_support(), card)
        .p(px(10.))
        .gap(px(10.))
        .rounded(px(10.))
        .bg(t.card)
        .border_1()
        .cursor_pointer()
        .map(|this| {
            if open {
                // The open card's ring is its state, not a shadow.
                this.border_color(color).shadow(vec![BoxShadow {
                    color: color.opacity(0.2),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            } else {
                this.border_color(t.border)
                    .shadow(ui::lift(cx))
                    .hover(|this| this.border_color(t.border_3))
            }
        })
        .child(
            h_flex()
                .items_start()
                .gap(px(10.))
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .size(px(32.))
                        .justify_center()
                        .rounded(px(8.))
                        .bg(color.opacity(0.12))
                        .child(ui::ring(tree.tally[ix].share(), 16., 2.5, color, div(), cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.5))
                        .line_height(px(16.))
                        .font_semibold()
                        .text_color(t.text)
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(topic.title.clone()),
                ),
        )
        .child(chips(tree, ix, &t))
        .tooltip(tip(topic.title.clone()))
        .on_click(move |_, _, cx| view.update(cx, |screen| pick(screen, ix)))
        .into_any_element()
}

/// One topic's line: its status, its title and, when a note covers it, a
/// mark that it opens.
fn topic_line(
    tree: &Tree,
    ix: usize,
    size: f32,
    lines: usize,
    view: &View,
    cx: &App,
) -> impl Styled + IntoElement + use<> {
    let t = ui::tokens(cx);
    let topic = tree.topic(ix);
    let note = tree.note(ix);
    let view = view.clone();
    h_flex()
        .id(("topic", ix))
        .test_support()
        .gap(px(8.))
        .rounded(px(6.))
        .child(
            tree.statuses[ix]
                .icon(cx)
                .size(px(size + 1.5))
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(size))
                .line_height(px(size + 3.))
                .text_color(t.text)
                .map(|this| {
                    if lines == 1 {
                        this.truncate()
                    } else {
                        this.line_clamp(lines).text_ellipsis()
                    }
                })
                .child(topic.title.clone()),
        )
        .when(note.is_some(), |this| {
            this.child(
                Icon::new(IconName::ArrowUpRight)
                    .size(px(13.))
                    .text_color(t.text_3)
                    .flex_shrink_0(),
            )
        })
        .tooltip(topic_tip(topic.id.clone()))
        .when_some(note, |this, note| {
            this.cursor_pointer()
                .hover(|this| this.bg(t.hover))
                .on_click(move |_, _, cx| view.open_note(note, cx))
        })
}

/// A topic's card in the last column. A topic with subtopics lists them
/// under its title, each opening its own note.
fn topic_card(tree: &Tree, card: Card, view: &View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let ix = card.ix;
    let base = place(v_flex(), card)
        .rounded(px(9.))
        .bg(t.card)
        .border_1()
        .border_color(t.border)
        .shadow(ui::lift(cx));
    if tree.topic(ix).leaf {
        return base
            .justify_center()
            .px(px(4.))
            .child(topic_line(tree, ix, 12.5, 2, view, cx).p(px(6.)))
            .into_any_element();
    }
    let color = tree.color(ix, cx);
    let tally = tree.tally[ix];
    base.child(
        h_flex()
            .h(px(TOPIC_H))
            .flex_shrink_0()
            .px(px(10.))
            .gap(px(8.))
            .child(ui::ring(tally.share(), 14., 2.5, color, div(), cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .line_height(px(15.))
                    .font_medium()
                    .text_color(t.text)
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(tree.topic(ix).title.clone()),
            )
            .child(ui::meta(format!("{}/{}", tally.done, tally.total()), cx).flex_shrink_0()),
    )
    .child(
        v_flex()
            .mx(px(6.))
            .pt(px(4.))
            .border_t_1()
            .border_color(t.border)
            .children(tree.leaves(ix).into_iter().map(|leaf| {
                topic_line(tree, leaf, 12., 1, view, cx)
                    .h(px(SUB_H))
                    .px(px(4.))
            })),
    )
    .into_any_element()
}

/// A pill at the head of a column, saying what it holds.
fn pill(x: f32, y: f32, label: String, color: Hsla, t: &ui::Tokens) -> Div {
    h_flex()
        .absolute()
        .left(px(x))
        .top(px(y))
        .h(px(20.))
        .px(px(8.))
        .rounded_full()
        .bg(t.card)
        .border_1()
        .border_color(color)
        .text_size(px(11.))
        .font_medium()
        .text_color(color.mix_oklab(t.text, 0.7))
        .child(label)
}

/// The canvas: the paper's card and the three columns under it, laid out
/// for the width the canvas has.
fn board(view: View, room: Size<Pixels>, cx: &mut App) -> impl IntoElement + use<> {
    let tree = Tree::new(cx);
    let t = ui::tokens(cx);
    let (subject, chapter) = picks(&tree, &view);
    let (room_w, room_h) = (f32::from(room.width), f32::from(room.height));
    let w = ((room_w - 2. * PAD - GUTTER - 2. * GAP) / 3.).clamp(188., 268.);
    let xs = [
        PAD + GUTTER,
        PAD + GUTTER + w + GAP,
        PAD + GUTTER + 2. * (w + GAP),
    ];
    let top = PAD + ROOT_H + PILL_ROOM;
    let root = Card {
        ix: view.paper,
        x: PAD,
        y: PAD,
        w: GUTTER + w,
        h: ROOT_H,
    };
    let items = |parent: usize, last: bool| -> Vec<(usize, f32)> {
        tree.kids(parent)
            .iter()
            .map(|&ix| (ix, height(&tree, ix, last)))
            .collect()
    };
    let find = |cards: &[Card], ix: Option<usize>| {
        ix.and_then(|ix| cards.iter().find(|card| card.ix == ix).copied())
    };
    let subjects = column(&items(view.paper, false), xs[0], w, f32::MIN, top);
    let open_subject = find(&subjects, subject);
    let chapters = open_subject.map_or_else(Vec::new, |card| {
        column(&items(card.ix, false), xs[1], w, card.port(), top)
    });
    let open_chapter = find(&chapters, chapter);
    let topics = open_chapter.map_or_else(Vec::new, |card| {
        column(&items(card.ix, true), xs[2], w, card.port(), top)
    });
    let width = (xs[2] + w + PAD).max(room_w);
    let height = [&subjects, &chapters, &topics]
        .iter()
        .filter_map(|cards| cards.last())
        .map(|card| card.y + card.h + PAD)
        .fold(room_h, f32::max);

    // The paper's connector comes down the gutter in grey, and the one to
    // the open subject is drawn last, in its colour. A column hanging from
    // a card takes that card's colour, lighter, with its open card's
    // connector at full strength.
    let ground = t.sunken;
    let grey = t.text.mix_oklab(ground, 0.24);
    let mut strokes = Vec::new();
    let mut ports = Vec::new();
    let mut pills = Vec::new();
    let trunk = PAD + GUTTER / 2.;
    let mut lit = None;
    for card in &subjects {
        let segs = elbow((trunk, root.y + root.h), trunk, (card.x, card.port()));
        ports.push((card.x, card.port(), grey));
        if Some(card.ix) == subject {
            lit = Some(Stroke {
                segs,
                color: tree.color(card.ix, cx),
            });
        } else {
            strokes.push(Stroke { segs, color: grey });
        }
    }
    ports.push((trunk, root.y + root.h, grey));
    strokes.extend(lit);
    for (parent, cards, open, label) in [
        (open_subject, &chapters, chapter, "chapter"),
        (open_chapter, &topics, None, "topic"),
    ] {
        let (Some(parent), Some(first)) = (parent, cards.first()) else {
            continue;
        };
        let color = tree.color(parent.ix, cx);
        let light = color.mix_oklab(ground, 0.45);
        let trunk = parent.x + parent.w + GAP / 2.;
        let from = (parent.x + parent.w, parent.port());
        // A column of plain topics is a column of topics, wherever it hangs.
        let noun = if cards.iter().all(|card| tree.topic(card.ix).leaf) {
            "topic"
        } else {
            label
        };
        pills.push(pill(
            trunk - 12.,
            first.y - 30.,
            count(cards.len(), noun, &format!("{noun}s")),
            light,
            &t,
        ));
        strokes.push(Stroke {
            segs: vec![
                Seg::Move(trunk, first.y - 10.),
                Seg::Line(trunk, first.port()),
            ],
            color: light,
        });
        let mut lit = None;
        for card in cards {
            let segs = elbow(from, trunk, (card.x, card.port()));
            ports.push((card.x, card.port(), light));
            if Some(card.ix) == open {
                lit = Some(Stroke { segs, color });
            } else {
                strokes.push(Stroke { segs, color: light });
            }
        }
        strokes.extend(lit);
        ports.push((from.0, from.1, color));
    }

    let mut cards: Vec<AnyElement> = vec![root_card(&tree, root, cx)];
    let pick_subject: fn(&mut SyllabusScreen, usize) = |screen, ix| {
        screen.subject = Some(ix);
        screen.chapter = None;
    };
    let pick_chapter: fn(&mut SyllabusScreen, usize) = |screen, ix| screen.chapter = Some(ix);
    for (column, open, pick) in [
        (&subjects, subject, pick_subject),
        (&chapters, chapter, pick_chapter),
    ] {
        for &card in column.iter() {
            cards.push(if tree.topic(card.ix).leaf {
                topic_card(&tree, card, &view, cx)
            } else {
                node_card(&tree, card, Some(card.ix) == open, &view, pick, cx)
            });
        }
    }
    cards.extend(
        topics
            .iter()
            .map(|&card| topic_card(&tree, card, &view, cx)),
    );

    let grid = t.text.mix_oklab(ground, 0.16);
    div()
        .id("flow")
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
                            paint(bounds, &strokes, &ports, grid, ground, window)
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .children(pills)
                .children(cards),
        )
}

/// The overview beside the canvas.
fn overview(tree: &Tree, view: &View, cx: &App) -> impl IntoElement + use<> {
    let t = ui::tokens(cx);
    let paper = tree.topic(view.paper);
    let tally = tree.tally[view.paper];
    let (subject, chapter) = picks(tree, view);
    let open = chapter.or(subject).unwrap_or(view.paper);
    let about = [
        paper.title.clone(),
        facts(paper.official.as_deref().unwrap_or_default()),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");

    let coverage = ui::card(cx)
        .p(px(14.))
        .gap(px(10.))
        .child(
            h_flex()
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_medium()
                                .text_color(t.text)
                                .child("Read so far"),
                        )
                        .child(ui::meta(
                            format!("{} of {} topics", tally.done, tally.total()),
                            cx,
                        )),
                )
                .child(
                    div()
                        .font_family(ui::DISPLAY)
                        .text_size(px(24.))
                        .line_height(px(28.))
                        .font_semibold()
                        .text_color(t.text)
                        .child(format!("{:.0}%", tally.share() * 100.)),
                ),
        )
        .child(status_bar(tally, 6., &t))
        .child(status_legend(tally, &t));

    let words = tree.official(open).map(|(node, official)| {
        v_flex()
            .gap(px(6.))
            .child(ui::caption(
                format!("UPSC's line for {}", tree.topic(node).title),
                cx,
            ))
            .child(quote(official, 14., t.text))
    });

    let reach = subject.unwrap_or(view.paper);
    let next = tree.next(reach, 3);
    let reads = v_flex()
        .gap(px(8.))
        .child(ui::caption("Read next", cx))
        .when(next.is_empty(), |this| {
            this.child(ui::meta(
                format!("No notes cover {} yet.", tree.topic(reach).title),
                cx,
            ))
        })
        .children(next.into_iter().filter_map(|leaf| {
            let note = tree.note(leaf)?;
            let view = view.clone();
            let chapter = tree.parent[leaf].map(|up| tree.topic(up).title.clone());
            Some(
                ui::card(cx)
                    .id(("next", leaf))
                    .test_support()
                    .px(px(10.))
                    .py(px(8.))
                    .gap(px(2.))
                    .cursor_pointer()
                    .hover(|this| this.border_color(t.border_3))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(tree.statuses[leaf].icon(cx).size(px(14.)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .font_medium()
                                    .text_color(t.text)
                                    .child(tree.topic(leaf).title.clone()),
                            )
                            .child(
                                Icon::new(IconName::ArrowUpRight)
                                    .size(px(13.))
                                    .text_color(t.text_3),
                            ),
                    )
                    .when_some(chapter, |this, chapter| {
                        this.child(ui::meta(chapter, cx).truncate().pl(px(22.)))
                    })
                    .on_click(move |_, _, cx| view.open_note(note, cx)),
            )
        }));

    let subjects = v_flex()
        .gap(px(2.))
        .child(ui::caption("Subjects", cx).pb(px(6.)))
        .children(tree.kids(view.paper).iter().map(|&ix| {
            let topic = tree.topic(ix);
            let tally = tree.tally[ix];
            let chosen = Some(ix) == subject;
            let action = view.open(ix, tree);
            let view = view.clone();
            v_flex()
                .id(("overview-subject", ix))
                .test_support()
                .px(px(8.))
                .py(px(7.))
                .gap(px(6.))
                .rounded(px(6.))
                .map(|this| {
                    if chosen {
                        this.bg(t.selected)
                    } else {
                        this.hover(|this| this.bg(t.hover))
                    }
                })
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(ui::dot(tree.color(ix, cx)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .child(ui::meta(format!("{}/{}", tally.done, tally.total()), cx)),
                )
                .child(status_bar(tally, 4., &t))
                .map(|this| {
                    if topic.leaf {
                        match action {
                            Some(action) => {
                                this.cursor_pointer().on_click(move |_, _, cx| action(cx))
                            }
                            None => this,
                        }
                    } else {
                        this.cursor_pointer().on_click(move |_, _, cx| {
                            view.update(cx, |screen| {
                                screen.subject = Some(ix);
                                screen.chapter = None;
                            })
                        })
                    }
                })
        }));

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
                .id("flow-overview")
                .child(
                    v_flex()
                        .p(px(16.))
                        .gap(px(20.))
                        .child(
                            v_flex()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.5))
                                        .font_semibold()
                                        .text_color(t.text)
                                        .child("Overview"),
                                )
                                .child(ui::meta(about, cx)),
                        )
                        .child(coverage)
                        .children(words)
                        .child(reads)
                        .child(subjects),
                ),
        )
}

/// Below this width the canvas gets all of it, and the overview goes.
const ROOMY: Pixels = px(880.);

pub(super) fn view(tree: &Tree, view: View, cx: &App) -> AnyElement {
    let t = ui::tokens(cx);
    let scroll = view.scroll.clone();
    let panel = overview(tree, &view, cx).into_any_element();
    // Only the layout knows the width, so the overview is built beforehand
    // and dropped when there is no room for it.
    container_query(move |room, _, _| {
        canvas_and((room.width >= ROOMY).then_some(panel), view, scroll, t)
    })
    .into_any_element()
}

/// The canvas, with the overview beside it when there is room.
fn canvas_and(panel: Option<AnyElement>, view: View, scroll: ScrollHandle, t: ui::Tokens) -> Div {
    h_flex()
        .size_full()
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .bg(t.sunken)
                // The canvas's own corner: a filled child must not square it off.
                .rounded_bl(px(11.))
                .child(container_query(move |room, _, cx| board(view, room, cx)))
                .vertical_scrollbar(&scroll)
                .horizontal_scrollbar(&scroll),
        )
        .children(panel)
}

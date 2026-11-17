//! The look every screen shares, as set out in DESIGN.md: the colour tokens,
//! the fonts, and the small parts built from them (the canvas header,
//! cards, chips, dots, bars and rings). Screens use these instead of their
//! own colours and sizes.

use std::borrow::Cow;

use gpui_kit::component::chart::PieChart;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Colorize as _, StyledExt as _, TitleBar, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::content::within;
use crate::mentor;

/// The heading face, for text of 20 px and up. GPUI has no letter-spacing,
/// so this is what keeps large headings tight.
pub const DISPLAY: &str = "Inter Display";
/// The reading face for notes, stories and Mains answers.
pub const SERIF: &str = "Source Serif 4";

/// Inter for the interface, Inter Display for headings and Source Serif 4
/// for reading, all under the SIL Open Font License (see `assets/fonts`).
pub(crate) fn load_fonts(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/InterDisplay-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4-It.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4-Semibold.ttf")),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("the bundled fonts are valid");
}

/// The colours of one theme (DESIGN.md, section 3). `assets/theme.json`
/// holds the same values for the gpui-kit controls.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    /// The window behind the sidebar and the canvas.
    pub chrome: Hsla,
    /// The raised panel the work sits on.
    pub canvas: Hsla,
    pub card: Hsla,
    pub popover: Hsla,
    /// Tracks, segmented backgrounds, table heads.
    pub sunken: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    /// Hairlines and card edges.
    pub border: Hsla,
    /// Buttons and inputs.
    pub border_2: Hsla,
    /// Strong dividers.
    pub border_3: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    /// Meta lines, captions and counts.
    pub text_3: Hsla,
    /// Placeholders and disabled text.
    pub text_4: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub on_accent: Hsla,
    /// Marked for review, the streak, the trap. A fill, never small text.
    pub attention: Hsla,
    pub highlight: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    /// One colour per subject, in [`SUBJECT_COLOURS`] order.
    pub subjects: [Hsla; 9],
}

fn hex(value: u32) -> Hsla {
    rgba(value).into()
}

fn paper() -> Tokens {
    Tokens {
        chrome: hex(0xf2f0edff),
        canvas: hex(0xfcfbfaff),
        card: hex(0xffffffff),
        popover: hex(0xffffffff),
        sunken: hex(0xf5f4f2ff),
        hover: hex(0x1e1d1a0b),
        selected: hex(0x1e1d1a12),
        border: hex(0x1e1d1a14),
        border_2: hex(0x1e1d1a1f),
        border_3: hex(0x1e1d1a29),
        text: hex(0x1e1d1aff),
        text_2: hex(0x5c5b57ff),
        text_3: hex(0x6d6c69ff),
        text_4: hex(0xa6a4a2ff),
        accent: hex(0x006d99ff),
        accent_soft: hex(0x006d991a),
        on_accent: hex(0xffffffff),
        attention: hex(0xef9d25ff),
        highlight: hex(0xf5cb708c),
        success: hex(0x2a904bff),
        danger: hex(0xc83a35ff),
        subjects: [
            hex(0xb66c18ff),
            hex(0x00988fff),
            hex(0x9669bbff),
            hex(0x6679cdff),
            hex(0x409551ff),
            hex(0xb85c8bff),
            hex(0xc25e53ff),
            hex(0x8c8300ff),
            hex(0x6d7c8cff),
        ],
    }
}

fn ink() -> Tokens {
    Tokens {
        chrome: hex(0x0d0c0aff),
        canvas: hex(0x151412ff),
        card: hex(0x1d1c1aff),
        popover: hex(0x242321ff),
        sunken: hex(0x191816ff),
        hover: hex(0xffffff0a),
        selected: hex(0xffffff12),
        border: hex(0xffffff14),
        border_2: hex(0xffffff1f),
        border_3: hex(0xffffff29),
        text: hex(0xecebe8ff),
        text_2: hex(0xb2b1aeff),
        text_3: hex(0x8a8986ff),
        text_4: hex(0x5f5d5bff),
        accent: hex(0x42acd2ff),
        accent_soft: hex(0x42acd224),
        on_accent: hex(0x0d0c0aff),
        attention: hex(0xf2a635ff),
        highlight: hex(0x96680059),
        success: hex(0x4cb86aff),
        danger: hex(0xe8605bff),
        subjects: [
            hex(0xd38c49ff),
            hex(0x0fb6acff),
            hex(0xb389d8ff),
            hex(0x8499e9ff),
            hex(0x67b373ff),
            hex(0xd67ea9ff),
            hex(0xdf8074ff),
            hex(0xaaa23fff),
            hex(0x8b9aabff),
        ],
    }
}

/// The colours of the theme on screen.
pub fn tokens(cx: &App) -> Tokens {
    if cx.theme().is_dark() { ink() } else { paper() }
}

/// The subjects that have a colour: the seven in [`mentor::SUBJECTS`], by
/// id, then Ethics and everything else (CSAT, Essay, the language papers).
pub const SUBJECT_COLOURS: [&str; 9] = [
    "history",
    "geography",
    "polity",
    "economy",
    "environment",
    "science",
    "current",
    "ethics",
    "other",
];

/// Mains subjects take the colour of the nearest Prelims subject. The first
/// node a topic falls in wins, so the specific ones come first.
const MAINS_COLOURS: [(&str, &str); 9] = [
    ("mains.gs1.geography", "geography"),
    ("mains.gs1", "history"),
    ("mains.gs2", "polity"),
    ("mains.gs3.security", "polity"),
    ("mains.gs3.science-tech", "science"),
    ("mains.gs3.environment", "environment"),
    ("mains.gs3.disaster", "environment"),
    ("mains.gs3", "economy"),
    ("mains.gs4", "ethics"),
];

/// Which of [`SUBJECT_COLOURS`] a syllabus topic takes.
pub fn subject_slot(topic: &str) -> usize {
    let id = mentor::subject_of_topic(topic)
        .map(|subject| subject.id)
        .or_else(|| {
            MAINS_COLOURS
                .iter()
                .find(|(node, _)| within(topic, node))
                .map(|(_, id)| *id)
        })
        .unwrap_or("other");
    SUBJECT_COLOURS
        .iter()
        .position(|slot| *slot == id)
        .unwrap_or(SUBJECT_COLOURS.len() - 1)
}

/// The colour of a syllabus topic's subject.
pub fn subject_color(topic: &str, cx: &App) -> Hsla {
    tokens(cx).subjects[subject_slot(topic)]
}

/// The colour of a subject in [`mentor::SUBJECTS`], by its id.
pub fn subject_id_color(id: &str, cx: &App) -> Hsla {
    let slot = SUBJECT_COLOURS
        .iter()
        .position(|slot| *slot == id)
        .unwrap_or(SUBJECT_COLOURS.len() - 1);
    tokens(cx).subjects[slot]
}

/// A card's shadow: a soft lift in Paper, nothing in Ink, where the border
/// carries the edge.
pub fn lift(cx: &App) -> Vec<BoxShadow> {
    if cx.theme().is_dark() {
        Vec::new()
    } else {
        vec![BoxShadow {
            color: hsla(0., 0., 0., 0.04),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        }]
    }
}

/// The raised panel the screens sit on, inset from the window by 8 px.
/// Nothing that touches its edge may paint a background: GPUI clips to a
/// rectangle, so a filled child would square off the rounded corners.
pub fn canvas(cx: &App) -> Div {
    let t = tokens(cx);
    let shadow = if cx.theme().is_dark() {
        hsla(0., 0., 0., 0.4)
    } else {
        hsla(0., 0., 0., 0.05)
    };
    gpui_kit::component::v_flex()
        .size_full()
        .bg(t.canvas)
        .rounded(px(12.))
        .border_1()
        .border_color(t.border)
        .shadow(vec![BoxShadow {
            color: shadow,
            offset: point(px(0.), px(1.)),
            blur_radius: px(3.),
            spread_radius: px(0.),
            inset: false,
        }])
        .overflow_hidden()
}

/// The 40 px strip at the top of the canvas. Add one row to it: the
/// screen's title or breadcrumb, then its tabs, then its actions. Its empty
/// space moves the window, as a title bar does.
pub fn header(cx: &App) -> TitleBar {
    TitleBar::new()
        .h(px(40.))
        .pl(px(16.))
        .pr(px(8.))
        .bg(transparent_black())
        .border_color(tokens(cx).border)
}

/// The screen's icon and name, for the left of its header.
pub fn title(
    icon: impl Into<gpui_kit::component::Icon>,
    text: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let t = tokens(cx);
    h_flex()
        .gap_2()
        .flex_shrink_0()
        .child(icon.into().size_4().text_color(t.text_3))
        .child(
            div()
                .text_size(px(13.5))
                .font_semibold()
                .text_color(t.text)
                .child(text.into()),
        )
}

/// A screen inside the canvas: its header, then a body that scrolls on its
/// own and keeps its own scroll position, by `id`.
pub fn screen(header: impl IntoElement, id: impl Into<ElementId>, body: impl IntoElement) -> Div {
    gpui_kit::component::v_flex()
        .size_full()
        .child(header)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scrollbar()
                .id(id)
                .child(body),
        )
}

/// A centred column for a screen's content, at most `width` wide.
pub fn column(width: f32) -> Div {
    gpui_kit::component::v_flex()
        .w_full()
        .max_w(px(width))
        .mx_auto()
        .px_8()
        .pt_6()
        .pb_10()
}

/// A card on the canvas: 8 px corners, a hairline edge and a soft lift.
pub fn card(cx: &App) -> Div {
    let t = tokens(cx);
    gpui_kit::component::v_flex()
        .min_w_0()
        .bg(t.card)
        .border_1()
        .border_color(t.border)
        .rounded(px(8.))
        .shadow(lift(cx))
}

/// A card's title row. Add a meta line, a spacer and actions after it.
pub fn card_head(title: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap_2()
        .min_h(px(40.))
        .px(px(14.))
        .pt(px(10.))
        .pb(px(6.))
        .child(
            div()
                .text_size(px(13.5))
                .font_semibold()
                .text_color(tokens(cx).text)
                .child(title.into()),
        )
}

/// Secondary text: 12 px in the meta colour.
pub fn meta(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(12.))
        .text_color(tokens(cx).text_3)
        .child(text.into())
}

/// A group label: 11 px medium in the meta colour.
pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(11.))
        .font_medium()
        .text_color(tokens(cx).text_3)
        .child(text.into())
}

/// A 22 px pill on the sunken colour. Add an icon and a label.
pub fn chip(cx: &App) -> Div {
    let t = tokens(cx);
    h_flex()
        .flex_shrink_0()
        .h(px(22.))
        .px(px(9.))
        .gap(px(5.))
        .rounded_full()
        .bg(t.sunken)
        .border_1()
        .border_color(t.border)
        .text_size(px(12.))
        .font_medium()
        .text_color(t.text_2)
}

/// A pill tinted with a subject's colour, with its dot.
pub fn subject_chip(label: impl Into<SharedString>, color: Hsla, cx: &App) -> Div {
    h_flex()
        .flex_shrink_0()
        .h(px(22.))
        .px(px(9.))
        .gap(px(6.))
        .rounded_full()
        .bg(color.opacity(0.13))
        .text_size(px(12.))
        .font_medium()
        .text_color(color.mix_oklab(tokens(cx).text, 0.8))
        .child(dot(color))
        .child(label.into())
}

/// The 8 px dot that marks a subject in lists and legends.
pub fn dot(color: Hsla) -> Div {
    div().flex_shrink_0().size(px(8.)).rounded_full().bg(color)
}

/// A 4 px bar filled to `share` (0 to 1).
pub fn progress(share: f32, color: Hsla, cx: &App) -> Div {
    div()
        .w_full()
        .h(px(4.))
        .rounded_full()
        .bg(tokens(cx).selected)
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(color)
                .w(relative(share.clamp(0., 1.))),
        )
}

/// A donut filled to `share` (0 to 1), `size` across, with `label` in the middle.
pub fn ring(
    share: f32,
    size: f32,
    thickness: f32,
    color: Hsla,
    label: impl IntoElement,
    cx: &App,
) -> Div {
    let track = tokens(cx).selected;
    let share = share.clamp(0., 1.);
    // A slice must not be empty, or the chart draws nothing.
    let slices = [
        (share.max(0.0001), color),
        ((1. - share).max(0.0001), track),
    ];
    let radius = size / 2.;
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .child(
            PieChart::new(slices)
                .value(|(value, _)| *value)
                .color(|(_, color)| *color)
                .outer_radius(radius)
                .inner_radius(radius - thickness),
        )
        .child(h_flex().absolute().inset_0().justify_center().child(label))
}

/// A segmented control's track: options side by side on the sunken colour.
/// Fill it with [`segment`]s.
pub fn segmented(cx: &App) -> Div {
    let t = tokens(cx);
    h_flex()
        .flex_shrink_0()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(8.))
        .bg(t.sunken)
        .border_1()
        .border_color(t.border)
}

/// One option of a [`segmented`] control; the chosen one is raised.
pub fn segment<I: Into<ElementId>, L: Into<SharedString>>(
    id: I,
    label: L,
    chosen: bool,
    cx: &App,
) -> impl StatefulInteractiveElement + Styled + ParentElement + IntoElement + use<I, L> {
    let t = tokens(cx);
    let raised = if cx.theme().is_dark() {
        t.popover
    } else {
        t.card
    };
    h_flex()
        .id(id)
        .test_support()
        .h(px(24.))
        .px(px(10.))
        .gap(px(6.))
        .justify_center()
        .rounded(px(6.))
        .text_size(px(12.5))
        .font_medium()
        .cursor_pointer()
        .when(chosen, |this| {
            this.bg(raised)
                .text_color(t.text)
                .border_1()
                .border_color(t.border)
                .shadow(lift(cx))
        })
        .when(!chosen, |this| {
            this.text_color(t.text_2)
                .hover(|this| this.text_color(t.text))
        })
        .child(label.into())
}

/// A key the student can press, shown as a small cap: "M", "⌘↵". On an
/// accent button it takes the button's own colours.
pub fn key(label: impl Into<SharedString>, on_accent: bool, cx: &App) -> Div {
    let t = tokens(cx);
    let cap = div()
        .flex_shrink_0()
        .h(px(18.))
        .min_w(px(18.))
        .px(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .text_size(px(11.))
        .font_medium();
    if on_accent {
        cap.bg(t.on_accent.opacity(0.18)).text_color(t.on_accent)
    } else {
        cap.bg(t.sunken)
            .border_1()
            .border_color(t.border)
            .text_color(t.text_3)
    }
    .child(label.into())
}

/// A field of small dots that fades out from the top-right corner: the one
/// decorative touch, behind Today's greeting and the diagnostic offer. Give
/// it a sized, absolutely placed parent.
pub fn dot_field(color: Hsla) -> impl IntoElement {
    // GPUI's canvas, not the panel above.
    gpui_kit::canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            const STEP: f32 = 11.;
            const DOT: f32 = 2.2;
            let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            // The fade reaches across an ellipse from the corner.
            let (reach_x, reach_y) = (width * 0.55, height);
            let mut y = STEP / 2.;
            while y < height {
                let mut x = STEP / 2.;
                while x < width {
                    let (dx, dy) = ((width - x) / reach_x, y / reach_y);
                    let fade = 1. - (dx * dx + dy * dy).sqrt();
                    if fade > 0. {
                        let origin = bounds.origin + point(px(x - DOT / 2.), px(y - DOT / 2.));
                        window.paint_quad(
                            fill(
                                Bounds::new(origin, size(px(DOT), px(DOT))),
                                color.opacity(0.5 * fade),
                            )
                            .corner_radii(Corners::all(px(DOT / 2.))),
                        );
                    }
                    x += STEP;
                }
                y += STEP;
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in `gpui_kit::test`, which shadows `#[test]`.
    use super::{SUBJECT_COLOURS, subject_slot};
    use crate::content::Library;

    #[test]
    fn every_subject_in_the_syllabus_has_a_colour() {
        let library = Library::embedded();
        let other = SUBJECT_COLOURS.len() - 1;
        for topic in library.topics.iter().filter(|topic| topic.depth == 2) {
            let general = [
                "pre.gs.",
                "mains.gs1.",
                "mains.gs2.",
                "mains.gs3.",
                "mains.gs4.",
            ]
            .iter()
            .any(|paper| topic.id.starts_with(paper));
            let slot = subject_slot(&topic.id);
            assert_eq!(
                slot != other,
                general,
                "{} takes the colour {}",
                topic.id,
                SUBJECT_COLOURS[slot]
            );
        }
        assert_eq!(
            SUBJECT_COLOURS[subject_slot("mains.gs1.geography.x")],
            "geography"
        );
        assert_eq!(
            SUBJECT_COLOURS[subject_slot("mains.gs3.security")],
            "polity"
        );
        assert_eq!(SUBJECT_COLOURS[subject_slot("mains.gs4.cases")], "ethics");
        assert_eq!(
            SUBJECT_COLOURS[subject_slot("pre.gs.modern.gandhian")],
            "history"
        );
    }
}

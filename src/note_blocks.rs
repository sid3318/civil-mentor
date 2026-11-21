//! Reader typography hierarchy and native visual blocks for study notes.
//!
//! Notes remain plain Markdown so search, highlighting and export keep working,
//! while `gpui_kit::base::TextView` renders stepped Inter/Agrave headings,
//! semantic callout surfaces (`> [!prelims]`, `> [!mains]`, …) and structured
//! visual blocks (`timeline`, `compare`, `flow`, `keyfacts`, `cases`,
//! `framework`).

use gpui_kit::assets::IconName;
use gpui_kit::base::TextViewStyle;
use gpui_kit::base::text::{MarkdownExtensions, MarkdownNode, MarkdownParseContext, markdown_ast};
use gpui_kit::component::{ActiveTheme as _, Colorize as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::ui;

const PARSER_REVISION: u64 = 1;

/// Typography and container style for a note's Markdown sections.
///
/// Gives each structural level its own face, size, weight and colour so
/// chapters, topics (`##`), micro-topics (`###`), sub-points (`####`), prose
/// and list points separate at a glance.
pub fn reader_style(size: u32, subject_color: Hsla, cx: &App) -> TextViewStyle {
    let t = ui::tokens(cx);
    let theme = cx.theme();
    let base = size as f32;
    let heading_accent = subject_color.mix_oklab(t.text, 0.72);
    let radius = px(8.);

    let mut table = StyleRefinement::default()
        .font_family("Inter")
        .text_size(px((base * 0.86).max(12.5)))
        .rounded(radius)
        .border_1()
        .border_color(t.border);
    table.overflow.x = Some(Overflow::Scroll);

    let table_head = StyleRefinement::default()
        .bg(t.sunken)
        .text_color(t.text)
        .font_semibold();

    let table_cell = StyleRefinement::default().px(px(11.)).py(px(7.));

    let code_block = StyleRefinement::default()
        .bg(t.sunken)
        .rounded(radius)
        .border_1()
        .border_color(t.border)
        .p(px(14.))
        .text_size(px((base * 0.82).max(12.)));

    let inline_code_style = StyleRefinement::default()
        .font_family("Inter")
        .text_size(px((base * 0.84).max(12.)))
        .font_medium()
        .text_color(t.text)
        .bg(t.sunken)
        .rounded(px(4.))
        .border_1()
        .border_color(t.border)
        .px(px(5.))
        .py(px(1.));

    let list_style = StyleRefinement::default()
        .font_family("Inter")
        .text_size(px((base * 0.84).max(12.)))
        .font_semibold()
        .text_color(heading_accent);

    let blockquote_style = StyleRefinement::default()
        .font_family(ui::SERIF)
        .italic()
        .text_size(px((base * 0.96).max(13.)))
        .text_color(t.text_2)
        .bg(t.sunken)
        .rounded(radius)
        .border_1()
        .border_color(t.border_2)
        .px(px(16.))
        .py(px(12.));

    TextViewStyle::default()
        .with_foreground(t.text)
        .with_muted_foreground(t.text_2)
        .with_link(t.accent)
        .with_selection(theme.selection)
        .with_code_background(t.sunken)
        .with_border(t.border)
        .with_paragraph_gap(rems(0.85))
        .with_heading(move |level| match level {
            1 => StyleRefinement::default()
                .font_family(ui::DISPLAY)
                .text_size(px(base * 1.65))
                .font_semibold()
                .text_color(t.text)
                .pb(px(8.))
                .border_b_1()
                .border_color(t.border_2),
            2 => StyleRefinement::default()
                .font_family(ui::DISPLAY)
                .text_size(px(base * 1.44))
                .font_semibold()
                .text_color(t.text)
                .pb(px(7.))
                .mb(px(6.))
                .border_b_1()
                .border_color(t.border),
            3 => StyleRefinement::default()
                .font_family("Inter")
                .text_size(px(base * 1.16))
                .font_semibold()
                .text_color(t.text)
                .pt(px(6.)),
            4 => StyleRefinement::default()
                .font_family("Inter")
                .text_size(px(base * 0.97))
                .font_semibold()
                .text_color(heading_accent)
                .pt(px(4.)),
            _ => StyleRefinement::default()
                .font_family("Inter")
                .text_size(px(base * 0.88))
                .font_semibold()
                .text_color(t.text_2),
        })
        .with_list_style(list_style)
        .with_blockquote_style(blockquote_style)
        .with_inline_code_style(inline_code_style)
        .with_table(table)
        .with_table_head(table_head)
        .with_table_cell(table_cell)
        .with_table_row_hover(theme.table_hover)
        .with_code_block(code_block)
        .with_dark(theme.is_dark())
}

/// Markdown extensions for notes: semantic callout surfaces and custom visual
/// blocks (`timeline`, `compare`, `flow`, `keyfacts`, `cases`, `framework`).
pub fn extensions(size: u32, subject_color: Hsla) -> MarkdownExtensions {
    let body_size = (size as f32 * 0.94).max(12.5);
    MarkdownExtensions::default()
        .parser_revision(PARSER_REVISION)
        .block_surface("prelims", move |_, cx| {
            callout_surface(
                "Prelims pointer",
                IconName::Target,
                ui::tokens(cx).accent,
                body_size,
                cx,
            )
        })
        .block_surface("mains", move |_, cx| {
            callout_surface(
                "Mains angle",
                IconName::PenLine,
                subject_color,
                body_size,
                cx,
            )
        })
        .block_surface("news", move |_, cx| {
            callout_surface(
                "In the news",
                IconName::Newspaper,
                ui::tokens(cx).success,
                body_size,
                cx,
            )
        })
        .block_surface("case", move |_, cx| {
            callout_surface(
                "Landmark case",
                IconName::Scale,
                subject_color,
                body_size,
                cx,
            )
        })
        .block_surface("remember", move |_, cx| {
            callout_surface(
                "Remember & trap",
                IconName::Lightbulb,
                ui::tokens(cx).attention,
                body_size,
                cx,
            )
        })
        .block_surface("debate", move |_, cx| {
            callout_surface(
                "Constitutional debate",
                IconName::Gavel,
                ui::tokens(cx).danger,
                body_size,
                cx,
            )
        })
        .block_surface("example", move |_, cx| {
            callout_surface(
                "Example",
                IconName::Landmark,
                ui::tokens(cx).text_2,
                body_size,
                cx,
            )
        })
        .block_parser(parse_visual_block)
        .block_renderer("timeline", move |node, _, cx| {
            render_timeline(node, subject_color, cx)
        })
        .block_renderer("compare", move |node, _, cx| {
            render_compare(node, subject_color, cx)
        })
        .block_renderer("flow", move |node, _, cx| {
            render_flow(node, subject_color, cx)
        })
        .block_renderer("keyfacts", move |node, _, cx| {
            render_keyfacts(node, subject_color, cx)
        })
        .block_renderer("cases", move |node, _, cx| {
            render_cases(node, subject_color, cx)
        })
        .block_renderer("framework", move |node, _, cx| {
            render_framework(node, subject_color, cx)
        })
}

fn callout_surface(
    title: &'static str,
    icon: IconName,
    color: Hsla,
    body_size: f32,
    cx: &App,
) -> Div {
    let t = ui::tokens(cx);
    let dark = cx.theme().is_dark();
    let label_color = color.mix_oklab(t.text, 0.72);
    v_flex()
        .rounded(px(10.))
        .border_1()
        .border_color(color.opacity(if dark { 0.34 } else { 0.25 }))
        .bg(color.opacity(if dark { 0.11 } else { 0.055 }))
        .px(px(16.))
        .pt(px(12.))
        .pb(px(13.))
        .text_size(px(body_size))
        .text_color(t.text)
        .child(
            h_flex()
                .gap(px(7.))
                .pb(px(7.))
                .font_family("Inter")
                .text_size(px(12.))
                .font_semibold()
                .text_color(label_color)
                .child(Icon::new(icon).size(px(14.)).text_color(color))
                .child(title),
        )
}

// ---- Custom visual blocks: parsed AST payloads ----------------------------

#[derive(Clone, Debug)]
struct TimelineData {
    title: Option<String>,
    items: Vec<TimelineItem>,
}

#[derive(Clone, Debug)]
struct TimelineItem {
    when: String,
    title: String,
    detail: String,
}

#[derive(Clone, Debug)]
struct CompareData {
    title: Option<String>,
    headers: Vec<String>,
    rows: Vec<CompareRow>,
}

#[derive(Clone, Debug)]
struct CompareRow {
    criterion: Option<String>,
    cells: Vec<String>,
}

#[derive(Clone, Debug)]
struct FlowData {
    title: Option<String>,
    steps: Vec<FlowStep>,
}

#[derive(Clone, Debug)]
struct FlowStep {
    branches: Vec<FlowNode>,
}

#[derive(Clone, Debug)]
struct FlowNode {
    label: String,
    detail: String,
}

#[derive(Clone, Debug)]
struct KeyFactsData {
    title: Option<String>,
    items: Vec<KeyFact>,
}

#[derive(Clone, Debug)]
struct KeyFact {
    headline: String,
    subtitle: String,
    detail: String,
}

#[derive(Clone, Debug)]
struct CasesData {
    title: Option<String>,
    items: Vec<CaseItem>,
}

#[derive(Clone, Debug)]
struct CaseItem {
    name: String,
    tag: String,
    holding: String,
}

#[derive(Clone, Debug)]
struct FrameworkData {
    title: Option<String>,
    items: Vec<FrameworkItem>,
}

#[derive(Clone, Debug)]
struct FrameworkItem {
    dimension: String,
    body: String,
}

fn parse_visual_block(
    node: &markdown_ast::Node,
    cx: &MarkdownParseContext<'_>,
) -> Option<MarkdownNode> {
    let markdown_ast::Node::Code(code) = node else {
        return None;
    };
    let lang = code.lang.as_deref()?.trim().to_ascii_lowercase();
    let source = cx.node_source(node).unwrap_or(code.value.as_str());
    match lang.as_str() {
        "timeline" => parse_timeline(&code.value, source),
        "compare" => parse_compare(&code.value, source),
        "flow" => parse_flow(&code.value, source),
        "keyfacts" => parse_keyfacts(&code.value, source),
        "cases" => parse_cases(&code.value, source),
        "framework" => parse_framework(&code.value, source),
        _ => None,
    }
}

fn split_title(raw: &str) -> (Option<String>, Vec<&str>) {
    let mut lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let title = if lines.first().is_some_and(|line| line.starts_with("# ")) {
        Some(lines.remove(0).trim_start_matches("# ").trim().to_string())
    } else {
        None
    };
    (title, lines)
}

fn parse_timeline(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let mut items = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() >= 3 {
            items.push(TimelineItem {
                when: parts[0].to_string(),
                title: parts[1].to_string(),
                detail: parts[2..].join(" | "),
            });
        } else if parts.len() == 2 {
            items.push(TimelineItem {
                when: parts[0].to_string(),
                title: parts[1].to_string(),
                detail: String::new(),
            });
        }
    }
    if items.is_empty() {
        return None;
    }
    let plain = items
        .iter()
        .map(|it| format!("{} — {}: {}", it.when, it.title, it.detail))
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        MarkdownNode::new("timeline", TimelineData { title, items })
            .text(plain)
            .markdown(source),
    )
}

fn parse_compare(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let (&head, rest) = lines.split_first()?;
    let headers: Vec<String> = head
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if headers.len() < 2 {
        return None;
    }
    let cols = headers.len();
    let mut rows = Vec::new();
    for line in rest {
        let (criterion, body) = match line.split_once("::") {
            Some((label, rhs)) => (Some(label.trim().to_string()), rhs.trim()),
            None => (None, *line),
        };
        let mut cells: Vec<String> = body.split('|').map(|s| s.trim().to_string()).collect();
        if cells.len() < 2 {
            continue;
        }
        cells.resize(cols, String::new());
        rows.push(CompareRow { criterion, cells });
    }
    if rows.is_empty() {
        return None;
    }
    let plain = rows
        .iter()
        .map(|r| {
            let prefix = r
                .criterion
                .as_deref()
                .map(|c| format!("{c}: "))
                .unwrap_or_default();
            format!("{prefix}{}", r.cells.join(" | "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        MarkdownNode::new(
            "compare",
            CompareData {
                title,
                headers,
                rows,
            },
        )
        .text(plain)
        .markdown(source),
    )
}

fn parse_flow(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let mut steps = Vec::new();
    for line in lines {
        let cleaned = line
            .trim_start_matches("->")
            .trim_start_matches("→")
            .trim_start_matches("↓")
            .trim();
        let branches: Vec<FlowNode> = cleaned
            .split("||")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| match part.split_once('|') {
                Some((label, detail)) => FlowNode {
                    label: label.trim().to_string(),
                    detail: detail.trim().to_string(),
                },
                None => FlowNode {
                    label: part.to_string(),
                    detail: String::new(),
                },
            })
            .collect();
        if !branches.is_empty() {
            steps.push(FlowStep { branches });
        }
    }
    if steps.is_empty() {
        return None;
    }
    let plain = steps
        .iter()
        .map(|s| {
            s.branches
                .iter()
                .map(|b| {
                    if b.detail.is_empty() {
                        b.label.clone()
                    } else {
                        format!("{}: {}", b.label, b.detail)
                    }
                })
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .collect::<Vec<_>>()
        .join(" -> ");
    Some(
        MarkdownNode::new("flow", FlowData { title, steps })
            .text(plain)
            .markdown(source),
    )
}

fn parse_keyfacts(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let mut items = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() >= 3 {
            items.push(KeyFact {
                headline: parts[0].to_string(),
                subtitle: parts[1].to_string(),
                detail: parts[2..].join(" | "),
            });
        } else if parts.len() == 2 {
            items.push(KeyFact {
                headline: parts[0].to_string(),
                subtitle: String::new(),
                detail: parts[1].to_string(),
            });
        }
    }
    if items.is_empty() {
        return None;
    }
    let plain = items
        .iter()
        .map(|f| format!("{} — {} {}", f.headline, f.subtitle, f.detail))
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        MarkdownNode::new("keyfacts", KeyFactsData { title, items })
            .text(plain)
            .markdown(source),
    )
}

fn parse_cases(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let mut items = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() >= 3 {
            items.push(CaseItem {
                name: parts[0].to_string(),
                tag: parts[1].to_string(),
                holding: parts[2..].join(" | "),
            });
        } else if parts.len() == 2 {
            items.push(CaseItem {
                name: parts[0].to_string(),
                tag: String::new(),
                holding: parts[1].to_string(),
            });
        }
    }
    if items.is_empty() {
        return None;
    }
    let plain = items
        .iter()
        .map(|c| format!("{} ({}): {}", c.name, c.tag, c.holding))
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        MarkdownNode::new("cases", CasesData { title, items })
            .text(plain)
            .markdown(source),
    )
}

fn parse_framework(raw: &str, source: &str) -> Option<MarkdownNode> {
    let (title, lines) = split_title(raw);
    let mut items = Vec::new();
    for line in lines {
        if let Some((dim, body)) = line.split_once('|') {
            let dim = dim.trim();
            let body = body.trim();
            if !dim.is_empty() && !body.is_empty() {
                items.push(FrameworkItem {
                    dimension: dim.to_string(),
                    body: body.to_string(),
                });
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let plain = items
        .iter()
        .map(|f| format!("{}: {}", f.dimension, f.body))
        .collect::<Vec<_>>()
        .join("\n");
    Some(
        MarkdownNode::new("framework", FrameworkData { title, items })
            .text(plain)
            .markdown(source),
    )
}

// ---- Native GPUI renderers for the 6 visual blocks ------------------------

fn block_header(icon: IconName, label: &str, color: Hsla, cx: &App) -> Div {
    let t = ui::tokens(cx);
    h_flex()
        .gap(px(7.))
        .pb(px(10.))
        .font_family("Inter")
        .text_size(px(12.))
        .font_semibold()
        .text_color(t.text_2)
        .child(Icon::new(icon).size(px(14.)).text_color(color))
        .child(label.to_string())
}

fn render_timeline(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<TimelineData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let dark = cx.theme().is_dark();
    let pill_text = subject_color.mix_oklab(t.text, 0.72);
    let total = data.items.len();

    let rows = data.items.iter().enumerate().map(|(ix, item)| {
        let last = ix + 1 == total;
        h_flex()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .w(px(82.))
                    .flex_shrink_0()
                    .pt(px(1.))
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .px(px(7.))
                            .py(px(2.))
                            .rounded(px(5.))
                            .bg(subject_color.opacity(if dark { 0.18 } else { 0.11 }))
                            .font_family("Inter")
                            .text_size(px(11.5))
                            .font_semibold()
                            .text_color(pill_text)
                            .child(item.when.clone()),
                    ),
            )
            .child(
                v_flex()
                    .w(px(14.))
                    .flex_shrink_0()
                    .items_center()
                    .pt(px(6.))
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(subject_color)
                            .flex_shrink_0(),
                    )
                    .when(!last, |col| {
                        col.child(div().w(px(1.5)).min_h(px(28.)).flex_1().bg(t.border_2))
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .pb(if last { px(2.) } else { px(14.) })
                    .gap(px(3.))
                    .child(
                        div()
                            .font_family("Inter")
                            .text_size(px(13.5))
                            .line_height(relative(1.35))
                            .font_semibold()
                            .text_color(t.text)
                            .child(item.title.clone()),
                    )
                    .when(!item.detail.is_empty(), |box_el| {
                        box_el.child(
                            div()
                                .font_family("Inter")
                                .text_size(px(12.5))
                                .line_height(relative(1.5))
                                .text_color(t.text_2)
                                .child(item.detail.clone()),
                        )
                    }),
            )
    });

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.card)
        .p(px(16.))
        .child(block_header(
            IconName::Calendar,
            data.title.as_deref().unwrap_or("Timeline"),
            subject_color,
            cx,
        ))
        .children(rows)
        .into_any_element()
}

fn render_compare(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<CompareData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let dark = cx.theme().is_dark();
    let palette = [subject_color, t.accent, t.success];

    let header_cells = data.headers.iter().enumerate().map(|(ix, header)| {
        let col = palette[ix % palette.len()];
        let label_color = col.mix_oklab(t.text, 0.75);
        h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(7.))
            .px(px(12.))
            .py(px(10.))
            .bg(col.opacity(if dark { 0.16 } else { 0.08 }))
            .when(ix > 0, |el| el.border_l_1().border_color(t.border))
            .child(ui::dot(col))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family("Inter")
                    .text_size(px(13.))
                    .line_height(relative(1.3))
                    .font_semibold()
                    .text_color(label_color)
                    .child(header.clone()),
            )
    });

    let body_rows = data.rows.iter().map(|row| {
        let cells = row.cells.iter().enumerate().map(|(ix, cell)| {
            div()
                .flex_1()
                .min_w_0()
                .px(px(12.))
                .py(px(9.))
                .when(ix > 0, |el| el.border_l_1().border_color(t.border))
                .font_family("Inter")
                .text_size(px(12.5))
                .line_height(relative(1.48))
                .text_color(t.text)
                .child(cell.clone())
        });
        v_flex()
            .w_full()
            .border_t_1()
            .border_color(t.border)
            .when_some(row.criterion.as_ref(), |el, label| {
                el.child(
                    div()
                        .w_full()
                        .px(px(12.))
                        .pt(px(7.))
                        .pb(px(3.))
                        .bg(t.sunken)
                        .font_family("Inter")
                        .text_size(px(11.))
                        .font_semibold()
                        .text_color(t.text_3)
                        .child(label.clone()),
                )
            })
            .child(h_flex().w_full().items_stretch().children(cells))
    });

    v_flex()
        .w_full()
        .gap(px(6.))
        .when_some(data.title.as_ref(), |el, title| {
            el.child(
                div()
                    .font_family("Inter")
                    .text_size(px(12.))
                    .font_semibold()
                    .text_color(t.text_2)
                    .child(title.clone()),
            )
        })
        .child(
            v_flex()
                .w_full()
                .rounded(px(10.))
                .border_1()
                .border_color(t.border)
                .bg(t.card)
                .overflow_hidden()
                .child(h_flex().w_full().items_stretch().children(header_cells))
                .children(body_rows),
        )
        .into_any_element()
}

fn render_flow(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<FlowData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let dark = cx.theme().is_dark();
    let badge_text = subject_color.mix_oklab(t.text, 0.72);
    let total = data.steps.len();

    let mut children: Vec<AnyElement> = Vec::new();
    for (ix, step) in data.steps.iter().enumerate() {
        let single = step.branches.len() == 1;
        let cards = step.branches.iter().enumerate().map(|(bix, branch)| {
            let badge = if single {
                format!("{}", ix + 1)
            } else {
                format!("{}.{}", ix + 1, bix + 1)
            };
            v_flex()
                .flex_1()
                .min_w_0()
                .rounded(px(8.))
                .border_1()
                .border_color(t.border_2)
                .bg(t.card)
                .px(px(12.))
                .py(px(10.))
                .gap(px(4.))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(
                            div()
                                .flex_shrink_0()
                                .px(px(6.))
                                .py(px(1.))
                                .rounded(px(4.))
                                .bg(subject_color.opacity(if dark { 0.22 } else { 0.13 }))
                                .font_family("Inter")
                                .text_size(px(11.))
                                .font_semibold()
                                .text_color(badge_text)
                                .child(badge),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family("Inter")
                                .text_size(px(13.))
                                .line_height(relative(1.3))
                                .font_semibold()
                                .text_color(t.text)
                                .child(branch.label.clone()),
                        ),
                )
                .when(!branch.detail.is_empty(), |card| {
                    card.child(
                        div()
                            .font_family("Inter")
                            .text_size(px(12.))
                            .line_height(relative(1.45))
                            .text_color(t.text_2)
                            .child(branch.detail.clone()),
                    )
                })
        });

        children.push(
            h_flex()
                .w_full()
                .items_stretch()
                .gap(px(10.))
                .children(cards)
                .into_any_element(),
        );

        if ix + 1 < total {
            children.push(
                h_flex()
                    .w_full()
                    .justify_center()
                    .py(px(3.))
                    .child(
                        Icon::new(IconName::ArrowDown)
                            .size(px(14.))
                            .text_color(t.text_3),
                    )
                    .into_any_element(),
            );
        }
    }

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.sunken)
        .p(px(14.))
        .when_some(data.title.as_ref(), |el, title| {
            el.child(block_header(IconName::ListTree, title, subject_color, cx))
        })
        .children(children)
        .into_any_element()
}

fn render_keyfacts(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<KeyFactsData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let headline_color = subject_color.mix_oklab(t.text, 0.72);
    let rows = data.items.chunks(2).map(|pair| {
        h_flex()
            .w_full()
            .items_stretch()
            .gap(px(10.))
            .children(pair.iter().map(|fact| {
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.card)
                    .p(px(12.))
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .gap(px(7.))
                            .items_center()
                            .child(ui::dot(subject_color))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(ui::DISPLAY)
                                    .text_size(px(15.5))
                                    .line_height(relative(1.2))
                                    .font_semibold()
                                    .text_color(headline_color)
                                    .child(fact.headline.clone()),
                            ),
                    )
                    .when(!fact.subtitle.is_empty(), |tile| {
                        tile.child(
                            div()
                                .font_family("Inter")
                                .text_size(px(12.5))
                                .line_height(relative(1.3))
                                .font_semibold()
                                .text_color(t.text)
                                .child(fact.subtitle.clone()),
                        )
                    })
                    .when(!fact.detail.is_empty(), |tile| {
                        tile.child(
                            div()
                                .font_family("Inter")
                                .text_size(px(12.))
                                .line_height(relative(1.45))
                                .text_color(t.text_2)
                                .child(fact.detail.clone()),
                        )
                    })
            }))
    });

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.sunken)
        .p(px(12.))
        .gap(px(10.))
        .when_some(data.title.as_ref(), |el, title| {
            el.child(block_header(IconName::Zap, title, subject_color, cx))
        })
        .children(rows)
        .into_any_element()
}

fn render_cases(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<CasesData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let cards = data.items.iter().map(|item| {
        v_flex()
            .w_full()
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.card)
            .px(px(14.))
            .py(px(11.))
            .gap(px(5.))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        Icon::new(IconName::Scale)
                            .size(px(14.))
                            .text_color(subject_color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family("Inter")
                            .text_size(px(13.5))
                            .line_height(relative(1.3))
                            .font_semibold()
                            .text_color(t.text)
                            .child(item.name.clone()),
                    )
                    .when(!item.tag.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(5.))
                                .bg(t.sunken)
                                .border_1()
                                .border_color(t.border)
                                .font_family("Inter")
                                .text_size(px(11.))
                                .font_medium()
                                .text_color(t.text_2)
                                .child(item.tag.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .font_family("Inter")
                    .text_size(px(12.5))
                    .line_height(relative(1.5))
                    .text_color(t.text_2)
                    .child(item.holding.clone()),
            )
    });

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.sunken)
        .p(px(12.))
        .gap(px(8.))
        .child(block_header(
            IconName::Gavel,
            data.title
                .as_deref()
                .unwrap_or("Landmark judgments & committees"),
            subject_color,
            cx,
        ))
        .children(cards)
        .into_any_element()
}

fn render_framework(node: &MarkdownNode, subject_color: Hsla, cx: &App) -> AnyElement {
    let Some(data) = node.data::<FrameworkData>() else {
        return div().into_any_element();
    };
    let t = ui::tokens(cx);
    let palette = [t.accent, subject_color, t.attention, t.success];

    let rows = data.items.chunks(2).enumerate().map(|(row_ix, pair)| {
        h_flex()
            .w_full()
            .items_stretch()
            .gap(px(10.))
            .children(pair.iter().enumerate().map(|(col_ix, item)| {
                let col = palette[(row_ix * 2 + col_ix) % palette.len()];
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.card)
                    .p(px(12.))
                    .gap(px(5.))
                    .child(
                        h_flex()
                            .gap(px(7.))
                            .items_center()
                            .child(ui::dot(col))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family("Inter")
                                    .text_size(px(13.))
                                    .line_height(relative(1.3))
                                    .font_semibold()
                                    .text_color(t.text)
                                    .child(item.dimension.clone()),
                            ),
                    )
                    .child(
                        div()
                            .font_family("Inter")
                            .text_size(px(12.5))
                            .line_height(relative(1.5))
                            .text_color(t.text_2)
                            .child(item.body.clone()),
                    )
            }))
    });

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(t.border)
        .bg(t.sunken)
        .p(px(12.))
        .gap(px(10.))
        .child(block_header(
            IconName::Layers,
            data.title.as_deref().unwrap_or("Mains answer framework"),
            subject_color,
            cx,
        ))
        .children(rows)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        CompareData, FlowData, TimelineData, parse_cases, parse_compare, parse_flow,
        parse_framework, parse_keyfacts, parse_timeline,
    };

    #[test]
    fn custom_block_parsers_extract_structured_rows() {
        let tl = parse_timeline(
            "# Milestones\n1773 | Regulating Act | First control\n1833 | Charter Act | Centralisation",
            "",
        )
        .expect("timeline parses");
        let data = tl.data::<TimelineData>().expect("timeline payload");
        assert_eq!(data.title.as_deref(), Some("Milestones"));
        assert_eq!(data.items.len(), 2);

        let cmp = parse_compare("Lok Sabha | Rajya Sabha\nTerm :: 5 years | Permanent", "")
            .expect("compare parses");
        let cmp_data = cmp.data::<CompareData>().expect("compare payload");
        assert_eq!(cmp_data.headers, vec!["Lok Sabha", "Rajya Sabha"]);
        assert_eq!(cmp_data.rows[0].criterion.as_deref(), Some("Term"));

        let fl = parse_flow(
            "President | Assents\nRS | 245 members || LS | 543 members",
            "",
        )
        .expect("flow parses");
        let fl_data = fl.data::<FlowData>().expect("flow payload");
        assert_eq!(fl_data.steps.len(), 2);
        assert_eq!(fl_data.steps[1].branches.len(), 2);

        assert!(parse_keyfacts("Art 14 | Equality | Rule of law", "").is_some());
        assert!(parse_cases("Kesavananda (1973) | 13-Judge | Basic structure", "").is_some());
        assert!(parse_framework("Way forward | Independent appointments", "").is_some());
    }
}

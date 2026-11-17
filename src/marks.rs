//! Highlights and clips: the student selects text in a reader and keeps it.
//!
//! Every block of Markdown a reader shows is prepared here, so the block owns
//! the text view that renders it and a highlight paints behind the words it
//! covers. A mark keeps the range of the block's rendered text and the words
//! themselves, so it finds its place again after the content is edited.

use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::text::{RangeHighlight, TextView, TextViewState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::content::one_line;
use crate::store::{Mark, MarkKind};

/// How much of a marked passage shows in the bar that saves it.
const QUOTE_CHARS: usize = 90;

/// One block of Markdown in a reader, with the highlights painted on it.
struct Block {
    state: Entity<TextViewState>,
    /// The part of the item it shows, which its marks are filed under.
    part: String,
    text: String,
    /// The ranges painted, so they are set again only when they change.
    painted: Vec<Range<usize>>,
}

/// A passage the student selected, ready to become a highlight or a clip.
pub struct Selection {
    /// The part of the item it is in.
    pub part: String,
    /// Its range in that part's rendered text.
    pub range: Range<usize>,
    pub quote: String,
}

/// The blocks of the item on screen, by key.
#[derive(Default)]
pub struct Blocks {
    open: HashMap<String, Block>,
    /// Text views made since the screen last asked for them, so it can watch
    /// them for the student's selection.
    new: Vec<Entity<TextViewState>>,
}

impl Blocks {
    /// Drops every block, so the next item starts fresh.
    pub fn clear(&mut self) {
        self.open.clear();
        self.new.clear();
    }

    /// The text views made since this was last called, so the screen can
    /// watch them for the student's selection.
    pub fn take_new(&mut self) -> Vec<Entity<TextViewState>> {
        std::mem::take(&mut self.new)
    }

    /// Shows `text` in the block called `key`, with the highlights made in
    /// `part` painted over it. Call before the screen reads the app state:
    /// creating and updating a text view needs it mutably.
    pub fn prepare(
        &mut self,
        key: &str,
        part: &str,
        text: &str,
        marks: &[Mark],
        colour: Hsla,
        cx: &mut App,
    ) {
        if !self.open.contains_key(key) {
            let state = cx.new(|cx| TextViewState::markdown(text, cx));
            self.new.push(state.clone());
            self.open.insert(
                key.to_string(),
                Block {
                    state,
                    part: part.to_string(),
                    text: text.to_string(),
                    painted: Vec::new(),
                },
            );
        }
        let block = self.open.get_mut(key).expect("the block was just made");
        block.part = part.to_string();
        if block.text != text {
            block.text = text.to_string();
            block.painted.clear();
            let text = block.text.clone();
            block
                .state
                .update(cx, |state, cx| state.set_text(&text, cx));
        }
        let wanted = ranges(&block.state, part, marks, cx);
        if block.painted == wanted {
            return;
        }
        let highlights = wanted
            .iter()
            .map(|range| RangeHighlight::new(range.clone(), colour));
        // A range the view rejects is left unpainted, and tried again next
        // time the block is shown.
        let painted = block
            .state
            .update(cx, |state, cx| state.set_range_highlights(highlights, cx));
        block.painted = if painted.is_ok() { wanted } else { Vec::new() };
    }

    /// The block called `key`, ready to go in a reader.
    pub fn view(&self, key: &str) -> AnyElement {
        let Some(block) = self.open.get(key) else {
            return div().into_any_element();
        };
        TextView::new(&block.state).into_any_element()
    }

    /// What the student has selected in the block on screen, if anything.
    pub fn selection(&self, cx: &App) -> Option<Selection> {
        self.open.values().find_map(|block| {
            let view = block.state.read(cx);
            let source = view.selected_source_range()?;
            let rendered = view.rendered_text();
            let range = rendered.range_for_source(source)?;
            let (range, quote) = trimmed(rendered.as_str(), range)?;
            Some(Selection {
                part: block.part.clone(),
                range,
                quote,
            })
        })
    }

    /// How many highlights are painted, which the UI test checks.
    pub fn painted(&self) -> usize {
        self.open.values().map(|block| block.painted.len()).sum()
    }
}

/// The colour a highlight paints behind the words it covers: the
/// `highlight` token from DESIGN.md.
pub fn highlight(cx: &App) -> Hsla {
    crate::ui::tokens(cx).highlight
}

/// The ranges of `part` that "highlight" marks cover in this block's text.
fn ranges(
    state: &Entity<TextViewState>,
    part: &str,
    marks: &[Mark],
    cx: &App,
) -> Vec<Range<usize>> {
    let view = state.read(cx);
    let rendered = view.rendered_text();
    let text = rendered.as_str();
    marks
        .iter()
        .filter(|mark| mark.kind == MarkKind::Highlight && mark.part == part)
        .filter_map(|mark| anchor(text, mark))
        .filter(|range| !range.is_empty())
        .collect()
}

/// Where `mark` sits in `text`, or nowhere when the words it quoted are gone.
fn anchor(text: &str, mark: &Mark) -> Option<Range<usize>> {
    let quote = mark.quote.as_str();
    if quote.is_empty() {
        return None;
    }
    let range = mark.range.clone();
    if range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
        && &text[range.clone()] == quote
    {
        return Some(range);
    }
    // The content changed: take the quote nearest where it used to be.
    text.match_indices(quote)
        .map(|(at, _)| at)
        .min_by_key(|at| at.abs_diff(mark.range.start))
        .map(|at| at..at + quote.len())
}

/// `range` of `text` without the whitespace at either end, with the words it
/// covers.
fn trimmed(text: &str, range: Range<usize>) -> Option<(Range<usize>, String)> {
    let span = text.get(range.clone())?;
    let start = range.start + span.len() - span.trim_start().len();
    let end = range.start + span.trim_end().len();
    let quote = text.get(start..end)?.to_string();
    (!quote.is_empty()).then_some((start..end, quote))
}

/// The marks a reader is making: the selection waiting for a highlight's
/// comment, and the field that comment is written in.
#[derive(Default)]
pub struct Marking {
    pending: Option<Selection>,
    field: Option<Entity<TextareaState>>,
}

impl Marking {
    /// Starts marking `selection`. A highlight opens the comment field and
    /// waits to be saved; a clip is handed straight back.
    pub fn start(
        &mut self,
        kind: MarkKind,
        selection: Selection,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Selection> {
        if kind == MarkKind::Clip {
            return Some(selection);
        }
        let field = cx.new(|cx| TextareaState::new(window, cx));
        field.update(cx, |state, cx| state.focus(window, cx));
        self.field = Some(field);
        self.pending = Some(selection);
        None
    }

    /// The comment written for the pending highlight, if the student wrote
    /// one.
    pub fn comment(&self, cx: &App) -> Option<String> {
        let field = self.field.as_ref()?;
        let value = field.read(cx).value().to_string();
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    }

    /// Takes the pending selection, and closes the bar with it.
    pub fn take(&mut self) -> Option<Selection> {
        self.field = None;
        self.pending.take()
    }

    /// Drops the pending selection without saving it.
    pub fn close(&mut self) {
        self.field = None;
        self.pending = None;
    }

    /// The bar, while a highlight waits for its comment.
    pub fn bar(
        &self,
        on_save: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> Option<AnyElement> {
        let field = self.field.as_ref()?;
        let pending = self.pending.as_ref()?;
        Some(comment_bar(&pending.quote, field, on_save, on_cancel, cx))
    }
}

/// The bar where a selection becomes a highlight, with a comment if the
/// student writes one.
fn comment_bar(
    quote: &str,
    field: &Entity<TextareaState>,
    on_save: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id("highlight-bar")
        .test_support()
        .mx_3()
        .mt_2()
        .p_3()
        .gap_2()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(one_line(quote, QUOTE_CHARS)),
        )
        .child(Textarea::new(field).h(px(56.)))
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("highlight-save")
                        .primary()
                        .small()
                        .label("Save highlight")
                        .on_click(on_save),
                )
                .child(
                    Button::new("highlight-cancel")
                        .ghost()
                        .small()
                        .label("Cancel")
                        .on_click(on_cancel),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("A comment is optional."),
                ),
        )
        .into_any_element()
}

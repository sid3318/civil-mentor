//! The Notebook: the highlights, clips and bookmarks the student has kept,
//! each opening the note or story it came from, and the notes they write
//! themselves.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Library, Target};
use crate::screens::count;
use crate::store::{Mark, MarkKind, MyNote};
use crate::ui;

/// Which of the Notebook's lists is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    All,
    Highlights,
    Clips,
    Bookmarks,
    Notes,
}

/// What the pane beside the list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    /// A highlight or a clip, by its id.
    Mark(i64),
    /// A bookmarked note, story or editorial, by its reading key.
    Bookmark(String),
    /// A note the student wrote, by its id. Zero while it is new.
    Note(i64),
    Blank,
}

pub struct NotebookScreen {
    shell: WeakEntity<Shell>,
    kind: Kind,
    item: Item,
    /// The own note in the pane, with whatever has been typed into it.
    editing: Option<MyNote>,
    title: Option<Entity<InputState>>,
    body: Option<Entity<TextareaState>>,
}

/// The note, story or editorial a mark or bookmark belongs to, with where it
/// opens.
fn source(library: &Library, key: &str) -> Option<(String, Target)> {
    if let Some(ix) = library.notes.iter().position(|note| note.id == key) {
        return Some((library.notes[ix].title.clone(), Target::Note(ix)));
    }
    let (kind, id) = key.split_once(':')?;
    if !matches!(kind, "story" | "editorial") {
        return None;
    }
    for (brief, day) in library.briefs.iter().enumerate() {
        if kind == "story"
            && let Some(story) = day.stories.iter().position(|story| story.id == id)
        {
            return Some((
                day.stories[story].title.clone(),
                Target::Story { brief, story },
            ));
        }
        if kind == "editorial"
            && let Some(editorial) = day.editorials.iter().position(|one| one.id == id)
        {
            return Some((
                day.editorials[editorial].title.clone(),
                Target::Editorial { brief, editorial },
            ));
        }
    }
    None
}

/// A passage, cut to what fits one line of the list.
fn passed(quote: &str) -> String {
    crate::content::one_line(quote, 80)
}

impl NotebookScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        NotebookScreen {
            shell,
            kind: Kind::All,
            item: Item::Blank,
            editing: None,
            title: None,
            body: None,
        }
    }

    /// Opens a note of the student's own, as the search palette asks.
    pub fn open_note(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let note = AppState::get(cx)
            .my_notes
            .iter()
            .find(|note| note.id == id)
            .cloned();
        if let Some(note) = note {
            self.kind = Kind::Notes;
            self.edit(note, window, cx);
        }
        cx.notify();
    }

    /// Puts `note` in the pane to be written.
    fn edit(&mut self, note: MyNote, window: &mut Window, cx: &mut Context<Self>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let body = cx.new(|cx| TextareaState::new(window, cx).placeholder("Write it here"));
        title.update(cx, |state, cx| {
            state.set_value(note.title.clone(), window, cx);
            state.focus(window, cx);
        });
        body.update(cx, |state, cx| {
            state.set_value(note.body.clone(), window, cx);
        });
        self.item = Item::Note(note.id);
        self.editing = Some(note);
        self.title = Some(title);
        self.body = Some(body);
    }

    /// Saves the note in the pane as it stands.
    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.editing.clone() else {
            return;
        };
        let title = self
            .title
            .as_ref()
            .map(|field| field.read(cx).value().to_string())
            .unwrap_or_default();
        let body = self
            .body
            .as_ref()
            .map(|field| field.read(cx).value().to_string())
            .unwrap_or_default();
        let today = AppState::today(cx);
        let note = MyNote {
            title: title.trim().to_string(),
            body,
            updated: today,
            ..note
        };
        if let Some(saved) = AppState::save_my_note(cx, note) {
            self.item = Item::Note(saved.id);
            self.editing = Some(saved);
        }
        cx.notify();
    }

    /// Starts a note that has not been saved yet.
    fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let today = AppState::today(cx);
        self.kind = Kind::Notes;
        self.edit(MyNote::new(today), window, cx);
        cx.notify();
    }

    fn show(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        self.item = item.clone();
        self.editing = None;
        self.title = None;
        self.body = None;
        if let Item::Note(id) = item
            && let Some(note) = AppState::get(cx)
                .my_notes
                .iter()
                .find(|note| note.id == id)
                .cloned()
        {
            self.edit(note, window, cx);
        }
        cx.notify();
    }

    /// The marks the list shows, oldest first.
    fn marks(&self, cx: &App) -> Vec<Mark> {
        let state = AppState::get(cx);
        let mut marks: Vec<Mark> = state.marks.values().flatten().cloned().collect();
        marks.sort_by_key(|mark| mark.id);
        marks.retain(|mark| match self.kind {
            Kind::Highlights => mark.kind == MarkKind::Highlight,
            Kind::Clips => mark.kind == MarkKind::Clip,
            Kind::Bookmarks | Kind::Notes => false,
            Kind::All => true,
        });
        marks
    }

    /// Every bookmarked note, story and editorial, most recently opened first.
    fn bookmarks(&self, cx: &App) -> Vec<(String, String, Target)> {
        let state = AppState::get(cx);
        let mut reading: Vec<(&String, &crate::store::Reading)> = state
            .reading
            .iter()
            .filter(|(_, book)| book.bookmarked)
            .collect();
        reading.sort_by_key(|(_, read)| std::cmp::Reverse(read.last));
        reading
            .into_iter()
            .filter_map(|(key, _)| {
                let (title, target) = source(&state.library, key)?;
                Some((key.clone(), title, target))
            })
            .collect()
    }

    /// The filter and the entries it shows.
    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let muted = t.text_3;
        let row =
            |id: ElementId, title: String, detail: String, item: Item, cx: &mut Context<Self>| {
                let open = matches!(self.item, ref showing if *showing == item);
                v_flex()
                    .id(id)
                    .test_support()
                    .px(px(10.))
                    .py(px(8.))
                    .gap(px(2.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(open, |this| this.bg(t.accent_soft))
                    .hover(|this| this.bg(t.hover))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_medium()
                            .line_clamp(2)
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(detail),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show(item.clone(), window, cx);
                    }))
            };

        let mut rows: Vec<AnyElement> = Vec::new();
        for mark in self.marks(cx) {
            let source = source(&AppState::get(cx).library, &mark.item);
            let detail = match &source {
                Some((title, _)) => format!("{} · {title}", kind_label(mark.kind)),
                None => format!(
                    "{} · the source has left the content",
                    kind_label(mark.kind)
                ),
            };
            rows.push(
                row(
                    ("mark", mark.id as usize).into(),
                    passed(&mark.quote),
                    detail,
                    Item::Mark(mark.id),
                    cx,
                )
                .into_any_element(),
            );
        }
        for (ix, (key, title, _)) in self.bookmarks(cx).into_iter().enumerate() {
            rows.push(
                row(
                    ("bookmark", ix).into(),
                    title,
                    "Bookmark".to_string(),
                    Item::Bookmark(key),
                    cx,
                )
                .into_any_element(),
            );
        }
        if self.kind == Kind::All || self.kind == Kind::Notes {
            for note in AppState::get(cx).my_notes.clone() {
                rows.push(
                    row(
                        ("my-note", note.id as usize).into(),
                        note.label(),
                        format!("Your note · {}", note.updated.format("%-d %b %Y")),
                        Item::Note(note.id),
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }

        v_flex()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(t.border)
            .child(
                v_flex()
                    .id("notebook-list")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .p(px(8.))
                    .gap(px(2.))
                    .overflow_y_scrollbar()
                    .when(rows.is_empty(), |this| {
                        this.child(ui::meta("Nothing here yet.", cx).p_3())
                    })
                    .children(rows),
            )
    }

    /// The entry open on the right.
    fn pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match &self.item {
            Item::Mark(id) => {
                let state = AppState::get(cx);
                let Some(mark) = state
                    .marks
                    .values()
                    .flatten()
                    .find(|mark| mark.id == *id)
                    .cloned()
                else {
                    return blank(cx);
                };
                let source = source(&state.library, &mark.item);
                let open = source.as_ref().map(|(_, target)| *target);
                let shell = self.shell.clone();
                let id = mark.id;
                v_flex()
                    .id("mark-card")
                    .test_support()
                    .p_6()
                    .gap_3()
                    .max_w(px(720.))
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(kind_label(mark.kind).to_uppercase()),
                    )
                    .child(
                        div()
                            .text_lg()
                            .line_height(relative(1.4))
                            .child(mark.quote.clone()),
                    )
                    .when_some(mark.comment.clone(), |this, comment| {
                        this.child(
                            div()
                                .p_3()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().muted)
                                .child(comment),
                        )
                    })
                    .child(div().text_sm().text_color(muted).child(match &source {
                        Some((title, _)) => title.clone(),
                        None => "The source has left the content.".to_string(),
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("open-mark")
                                    .primary()
                                    .small()
                                    .label("Open the source")
                                    .disabled(open.is_none())
                                    .on_click(move |_, _, cx| {
                                        if let Some(target) = open {
                                            let _ = shell
                                                .update(cx, |shell, cx| shell.open(target, cx));
                                        }
                                    }),
                            )
                            .child(
                                Button::new("remove-mark")
                                    .ghost()
                                    .small()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        AppState::remove_mark(cx, id);
                                        this.item = Item::Blank;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .into_any_element()
            }
            Item::Bookmark(key) => {
                let state = AppState::get(cx);
                let source = source(&state.library, key);
                let shell = self.shell.clone();
                let open = source.as_ref().map(|(_, target)| *target);
                v_flex()
                    .p_6()
                    .gap_3()
                    .max_w(px(720.))
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child("BOOKMARK"),
                    )
                    .child(div().text_lg().font_medium().child(match &source {
                        Some((title, _)) => title.clone(),
                        None => "The source has left the content.".to_string(),
                    }))
                    .child(
                        Button::new("open-bookmark")
                            .primary()
                            .small()
                            .label("Open it")
                            .disabled(open.is_none())
                            .on_click(move |_, _, cx| {
                                if let Some(target) = open {
                                    let _ = shell.update(cx, |shell, cx| shell.open(target, cx));
                                }
                            }),
                    )
                    .into_any_element()
            }
            Item::Note(_) => {
                let Some(editing) = self.editing.clone() else {
                    return blank(cx);
                };
                v_flex()
                    .p_6()
                    .gap_3()
                    .max_w(px(720.))
                    .child(
                        div()
                            .id("note-title")
                            .test_support()
                            .child(match &self.title {
                                Some(field) => Input::new(field).into_any_element(),
                                None => div().into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .id("note-body")
                            .test_support()
                            .child(match &self.body {
                                Some(field) => Textarea::new(field).h(px(260.)).into_any_element(),
                                None => div().into_any_element(),
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("save-note")
                                    .primary()
                                    .small()
                                    .label("Save the note")
                                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                            )
                            .child(
                                Button::new("delete-note")
                                    .ghost()
                                    .small()
                                    .label("Delete")
                                    .disabled(editing.id == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        AppState::remove_my_note(cx, editing.id);
                                        this.item = Item::Blank;
                                        this.editing = None;
                                        this.title = None;
                                        this.body = None;
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_xs().text_color(muted).child(if editing.id == 0 {
                                "Not saved yet."
                            } else {
                                "Saved."
                            })),
                    )
                    .into_any_element()
            }
            Item::Blank => blank(cx),
        }
    }
}

fn kind_label(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Highlight => "Highlight",
        MarkKind::Clip => "Clip",
    }
}

/// What the pane says when nothing is open.
fn blank(cx: &App) -> AnyElement {
    let state = AppState::get(cx);
    let highlights = state
        .marks
        .values()
        .flatten()
        .filter(|mark| mark.kind == MarkKind::Highlight)
        .count();
    let clips = state
        .marks
        .values()
        .flatten()
        .filter(|mark| mark.kind == MarkKind::Clip)
        .count();
    let bookmarks = state
        .reading
        .values()
        .filter(|read| read.bookmarked)
        .count();
    v_flex()
        .p_6()
        .gap_2()
        .child(div().text_lg().font_medium().child("Everything you kept"))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} · {} · {} · {}",
                    count(highlights, "highlight", "highlights"),
                    count(clips, "clip", "clips"),
                    count(bookmarks, "bookmark", "bookmarks"),
                    count(
                        state.my_notes.len(),
                        "note of your own",
                        "notes of your own"
                    )
                )),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Select text in a note or a story to highlight it or keep it as a clip."),
        )
        .into_any_element()
}

impl NotebookScreen {
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div().flex().size_full().child(self.list(cx)).child(
            div()
                .id("notebook-pane")
                .test_support()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scrollbar()
                .child(self.pane(cx)),
        )
    }

    fn set_kind(&mut self, kind: Kind, cx: &mut Context<Self>) {
        self.kind = kind;
        self.item = Item::Blank;
        self.editing = None;
        self.title = None;
        self.body = None;
        cx.notify();
    }
}

impl Render for NotebookScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        let filters = [
            ("nb-all", "All", Kind::All),
            ("nb-highlights", "Highlights", Kind::Highlights),
            ("nb-clips", "Clips", Kind::Clips),
            ("nb-bookmarks", "Bookmarks", Kind::Bookmarks),
            ("nb-notes", "My notes", Kind::Notes),
        ]
        .map(|(id, label, choice)| {
            ui::segment(id, label, kind == choice, cx)
                .on_click(cx.listener(move |this, _, _, cx| this.set_kind(choice, cx)))
        });
        let header = ui::header(cx).child(
            h_flex()
                .flex_1()
                .gap_3()
                .child(ui::title(IconName::NotebookText, "Notebook", cx))
                .child(ui::segmented(cx).children(filters))
                .child(div().flex_1())
                .child(
                    Button::new("new-note")
                        .primary()
                        .small()
                        .icon(Icon::new(IconName::SquarePen))
                        .label("New note")
                        .on_click(cx.listener(|this, _, window, cx| this.new_note(window, cx))),
                ),
        );
        v_flex()
            .size_full()
            .child(header)
            .child(div().flex_1().min_h_0().child(self.body(cx)))
    }
}

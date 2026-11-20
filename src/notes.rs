//! The Notes screen: the list of notes, the reader, and the inspector beside
//! it with the outline (DESIGN.md, "Notes").

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Library, Note, Target, sections};
use crate::marks::{self, Blocks, Marking, Selection};
use crate::mentor;
use crate::screens::count;
use crate::store::{MarkKind, NewMark, Study};
use crate::syllabus::Status;
use crate::ui;

/// Reader text sizes, in pixels.
const MIN_SIZE: u32 = 13;
const MAX_SIZE: u32 = 24;
/// Widths of the reading column. The narrow one keeps lines near 75
/// characters at the default size; the wide one suits tables.
const NARROW: f32 = 680.;
const WIDE: f32 = 920.;
/// How many questions the inspector's drill on the note asks.
const DRILL: usize = 10;
/// Reading time stops counting after this long with no scrolling or mouse
/// movement, so a note left open overnight is not a night of study.
pub(crate) const IDLE: Duration = Duration::from_secs(5 * 60);
/// Counted reading time is saved in steps of this many seconds, and when
/// the note is left.
const SAVE_EVERY: u32 = 15;

pub struct NotesScreen {
    shell: WeakEntity<Shell>,
    note: Option<usize>,
    distilled: bool,
    /// The questions whose answers are showing, by index into the library.
    answers: HashSet<usize>,
    /// The reader's children are the header, one per section, the footer and
    /// the questions, so the outline scrolls to a section by its position.
    scroll: ScrollHandle,
    /// Counts the time the note on screen is open, while the student is active.
    clock: Option<ReadingClock>,
    /// The note's sections, as text views a highlight can be made on.
    blocks: Blocks,
    /// The selection waiting for a highlight's comment.
    marking: Marking,
    /// The last passage the student selected in the note.
    selected: Option<Selection>,
    /// The folders open in the list: subjects as `(subject, None)` and
    /// sections as `(subject, Some(section))`.
    open: HashSet<Folder>,
    /// Whether the outline and reading panel shows beside the note.
    inspector: bool,
}

/// A subject, or a section of one, in the list of notes.
type Folder = (String, Option<String>);

/// A subject in the list: its name, and its sections with their notes.
type Subject = (String, Vec<(Option<String>, Vec<usize>)>);

/// Width of the list of notes.
const LIST_WIDTH: f32 = 296.;

/// Counts the time something is open in a reader, while the student is
/// active. The notes and the current-affairs readers each keep one.
pub(crate) struct ReadingClock {
    /// The reading key of what is open.
    pub(crate) item: String,
    kind: Study,
    /// The block of study time in the store, once any time is saved.
    block: Option<i64>,
    /// Up to when the time has been counted.
    counted: Instant,
    /// The last scroll or mouse movement.
    active: Instant,
    /// Where the reader was scrolled at the last tick.
    offset: Point<Pixels>,
    /// Counted and not yet saved.
    unsaved: u32,
    _tick: Task<()>,
}

impl ReadingClock {
    /// Starts counting `item`, calling `tick` on the view every second.
    pub(crate) fn start<V: 'static>(
        item: String,
        kind: Study,
        offset: Point<Pixels>,
        cx: &mut Context<V>,
        tick: fn(&mut V, &mut Context<V>),
    ) -> Self {
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, tick).is_err() {
                    break;
                }
            }
        });
        let now = cx.background_executor().now();
        ReadingClock {
            item,
            kind,
            block: None,
            counted: now,
            active: now,
            offset,
            unsaved: 0,
            _tick: task,
        }
    }

    pub(crate) fn touch(&mut self, now: Instant) {
        self.active = now;
    }

    /// Counts the seconds since the last tick, up to [`IDLE`] after the
    /// student last scrolled or moved the mouse. Returns true when enough
    /// is counted to save.
    pub(crate) fn tick(&mut self, offset: Point<Pixels>, now: Instant) -> bool {
        if offset != self.offset {
            self.offset = offset;
            self.active = now;
        }
        let until = now.min(self.active + IDLE);
        let seconds = until.saturating_duration_since(self.counted).as_secs();
        self.counted += Duration::from_secs(seconds);
        if until < now {
            // The idle stretch is not counted when the student comes back.
            self.counted = now;
        }
        self.unsaved += seconds as u32;
        self.unsaved >= SAVE_EVERY
    }

    /// Saves the time counted so far.
    pub(crate) fn save(&mut self, cx: &mut App) {
        if self.unsaved > 0 {
            AppState::add_reading_time(cx, self.kind, &self.item, &mut self.block, self.unsaved);
            self.unsaved = 0;
        }
    }
}

impl NotesScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        NotesScreen {
            shell,
            note: None,
            distilled: false,
            answers: HashSet::new(),
            scroll: ScrollHandle::new(),
            clock: None,
            blocks: Blocks::default(),
            marking: Marking::default(),
            selected: None,
            open: HashSet::new(),
            inspector: true,
        }
    }

    pub fn note(&self) -> Option<usize> {
        self.note
    }

    /// Opens a note in the reader, and its folders in the list, and records
    /// that the student opened it. Called by the shell, which keeps the back
    /// and forward history.
    pub fn show(&mut self, ix: usize, cx: &mut Context<Self>) {
        let library = &AppState::get(cx).library;
        let id = library.notes[ix].id.clone();
        if self.note != Some(ix) {
            let (subject, section) = place(library, &library.notes[ix]);
            if section.is_some() {
                self.open.insert((subject.clone(), section));
            }
            self.open.insert((subject, None));
            self.note = Some(ix);
            self.answers.clear();
            self.scroll.set_offset(Point::default());
            self.blocks.clear();
            self.marking.close();
            self.selected = None;
        }
        AppState::update_reading(cx, &id, |reading, today| reading.last = today);
        if self.clock.as_ref().is_none_or(|clock| clock.item != id) {
            self.stop_clock(cx);
            let offset = self.scroll.offset();
            self.clock = Some(ReadingClock::start(
                id,
                Study::Reading,
                offset,
                cx,
                Self::tick,
            ));
        }
        cx.notify();
    }

    /// Stops counting reading time and saves what was counted. Called by
    /// the shell when the reader leaves the screen.
    pub fn stop_clock(&mut self, cx: &mut Context<Self>) {
        self.save_time(cx);
        self.clock = None;
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let offset = self.scroll.offset();
        if let Some(clock) = &mut self.clock
            && clock.tick(offset, now)
        {
            clock.save(cx);
        }
    }

    fn save_time(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.save(cx);
        }
    }

    fn touch(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        if let Some(clock) = &mut self.clock {
            clock.touch(now);
        }
    }

    fn set_distilled(&mut self, distilled: bool, cx: &mut Context<Self>) {
        if self.distilled != distilled {
            self.distilled = distilled;
            self.scroll.set_offset(Point::default());
            self.blocks.clear();
            self.marking.close();
            cx.notify();
        }
    }

    /// Every note, in folders like a file browser: the subjects, the sections
    /// in them, then the notes. A click opens or closes a folder.
    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let mut rows = Vec::new();
        for (subject, sections) in tree(library) {
            let folder = (subject, None);
            let open = self.open.contains(&folder);
            rows.push(self.folder(folder.clone(), 0, open, cx).into_any_element());
            if !open {
                continue;
            }
            for (section, notes) in sections {
                let mut depth = 1;
                if section.is_some() {
                    let folder = (folder.0.clone(), section);
                    let open = self.open.contains(&folder);
                    rows.push(self.folder(folder, 1, open, cx).into_any_element());
                    if !open {
                        continue;
                    }
                    depth = 2;
                }
                rows.extend(
                    notes
                        .into_iter()
                        .map(|ix| self.list_row(ix, depth, cx).into_any_element()),
                );
            }
        }
        div()
            .w(px(LIST_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(t.border)
            .overflow_y_scrollbar()
            .id("notes-list")
            .child(v_flex().p(px(8.)).children(rows))
    }

    /// A subject or a section in the list.
    fn folder(
        &self,
        folder: Folder,
        depth: usize,
        open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = ui::tokens(cx);
        let (id, label) = match &folder {
            (subject, Some(section)) => (format!("folder:{subject}/{section}"), section.clone()),
            (subject, None) => (format!("folder:{subject}"), subject.clone()),
        };
        // Subjects are the largest headings, sections the next; both are bold
        // above the notes' plain 13 px.
        let (size, icon) = if depth == 0 { (15., 16.) } else { (13.5, 15.) };
        tree_row(id, depth, cx)
            .text_size(px(size))
            .font_semibold()
            .text_color(t.text)
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(12.))
                .text_color(t.text_4),
            )
            .child(
                Icon::new(if open {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                })
                .size(px(icon))
                .text_color(t.text_3),
            )
            .child(div().flex_1().min_w_0().truncate().child(label))
            .test_support()
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.open.remove(&folder) {
                    this.open.insert(folder.clone());
                }
                cx.notify();
            }))
    }

    /// A note in the list, with how far the student has got with it.
    fn list_row(&self, ix: usize, depth: usize, cx: &App) -> impl IntoElement {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let note = &state.library.notes[ix];
        let title = SharedString::from(note.title.clone());
        let selected = self.note == Some(ix);
        let shell = self.shell.clone();
        tree_row(("note", ix), depth, cx)
            .text_size(px(13.))
            .when(selected, |this| this.bg(t.accent_soft).font_medium())
            // The chevron's room, so a note's icon lines up with a folder's.
            .child(div().w(px(12.)).flex_shrink_0())
            .child(
                Status::of(state.reading.get(&note.id))
                    .icon(cx)
                    .size(px(14.)),
            )
            .child(div().flex_1().min_w_0().truncate().child(title.clone()))
            .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
            .test_support()
            // Not a listener: the shell updates this screen when it opens the note.
            .on_click(move |_, _, cx| {
                let _ = shell.update(cx, |shell, cx| shell.open(Target::Note(ix), cx));
            })
    }

    /// How many of the note's highlights are painted, which the UI test
    /// checks.
    pub fn painted(&self) -> usize {
        self.blocks.painted()
    }

    /// Makes each section a text view the student can select text in, with
    /// their highlights already painted.
    fn prepare_blocks(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.note else {
            return;
        };
        let (sections, marks, colour) = {
            let state = AppState::get(cx);
            let note = &state.library.notes[ix];
            let text = if self.distilled && !note.distilled.is_empty() {
                &note.distilled
            } else {
                &note.body
            };
            let sections: Vec<(String, String)> = sections(text)
                .into_iter()
                .map(|section| {
                    (
                        section.heading.unwrap_or_default().to_string(),
                        section.text.to_string(),
                    )
                })
                .collect();
            (
                sections,
                state.marks_of(&note.id).to_vec(),
                marks::highlight(cx),
            )
        };
        for (k, (part, text)) in sections.iter().enumerate() {
            self.blocks
                .prepare(&k.to_string(), part, text, &marks, colour, cx);
        }
        for state in self.blocks.take_new() {
            cx.observe(&state, |this, _, cx| this.follow(cx)).detach();
        }
    }

    /// Keeps the last selection on hand: marking a passage must not depend on
    /// the click that starts it, which clears the selection first.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if let Some(selection) = self.blocks.selection(cx) {
            self.selected = Some(selection);
            cx.notify();
        }
    }

    /// Highlights the selected text, or clips it, once the angle is on it.
    fn start_mark(&mut self, kind: MarkKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = self.blocks.selection(cx).or_else(|| self.selected.take()) else {
            return;
        };
        if let Some(clip) = self.marking.start(kind, selection, window, cx) {
            self.keep_mark(kind, clip, None, cx);
        }
        cx.notify();
    }

    /// Saves a highlight or clip of the note on screen.
    fn keep_mark(
        &mut self,
        kind: MarkKind,
        selection: Selection,
        comment: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.note else {
            return;
        };
        let item = AppState::get(cx).library.notes[ix].id.clone();
        AppState::add_mark(
            cx,
            NewMark {
                item,
                part: selection.part,
                kind,
                range: selection.range,
                quote: selection.quote,
                comment,
            },
        );
        cx.notify();
    }

    /// The open note, and the inspector beside it.
    fn reader(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.prepare_blocks(cx);
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let Some(ix) = self.note else {
            let read = library
                .notes
                .iter()
                .filter(|note| Status::of(state.reading.get(&note.id)) == Status::Done)
                .count();
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    Icon::new(IconName::BookOpen)
                        .size(px(28.))
                        .text_color(t.text_4),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .font_semibold()
                        .child("Pick a note to read"),
                )
                .child(ui::meta(
                    format!(
                        "You have read {read} of {}.",
                        count(library.notes.len(), "note", "notes")
                    ),
                    cx,
                ))
                .into_any_element();
        };

        let note = &library.notes[ix];
        let reading = state.reading.get(&note.id).copied();
        let size = state.settings.reader_size.clamp(MIN_SIZE, MAX_SIZE);
        let wide = state.settings.reader_wide;
        let distilled = self.distilled && !note.distilled.is_empty();
        let text = if distilled {
            &note.distilled
        } else {
            &note.body
        };
        let sections = sections(text);
        let questions = library.questions_for(note);

        // Every child of the reader is one of these, so the outline can find
        // a section by its position.
        let column = |child: AnyElement| {
            div()
                .w_full()
                .max_w(px(if wide { WIDE } else { NARROW }))
                .mx_auto()
                .px_8()
                .child(child)
        };

        let mut facts = vec![(IconName::Clock, format!("{} min read", note.minutes))];
        if !questions.is_empty() {
            facts.push((
                IconName::CircleQuestionMark,
                count(questions.len(), "question", "questions"),
            ));
        }
        let years: Vec<String> = library
            .years_asked(note)
            .iter()
            .map(u16::to_string)
            .collect();
        if !years.is_empty() {
            facts.push((
                IconName::CalendarClock,
                format!("asked in {}", years.join(", ")),
            ));
        }
        if let Some(updated) = note.updated {
            facts.push((
                IconName::Calendar,
                format!("updated {}", updated.format("%-d %b %Y")),
            ));
        }
        let topic = note.topics.first().cloned().unwrap_or_default();
        let header = v_flex()
            .pt_8()
            .pb_5()
            .gap(px(10.))
            .items_start()
            .child(ui::subject_chip(
                library.subject_label(&topic),
                ui::subject_color(&topic, cx),
                cx,
            ))
            .child(
                div()
                    .font_family(ui::DISPLAY)
                    .text_size(px(size as f32 * 1.75))
                    .line_height(relative(1.2))
                    .font_semibold()
                    .child(note.title.clone()),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_x_4()
                    .gap_y_1()
                    .pb_4()
                    .border_b_1()
                    .border_color(t.border)
                    .text_size(px(12.))
                    .text_color(t.text_3)
                    .children(facts.into_iter().map(|(icon, fact)| {
                        h_flex()
                            .gap(px(5.))
                            .child(Icon::new(icon).size(px(13.)))
                            .child(fact)
                    })),
            );

        let done = reading.and_then(|reading| reading.done);
        let footer = match done {
            None => h_flex()
                .py_8()
                .gap_3()
                .child(
                    Button::new("mark-read")
                        .primary()
                        .icon(Icon::new(IconName::Check))
                        .label("Mark as read")
                        .on_click(toggle_read(&note.id)),
                )
                .child(ui::meta(
                    "It then counts towards your syllabus coverage.",
                    cx,
                )),
            Some(day) => h_flex()
                .py_8()
                .gap_2()
                .text_size(px(13.5))
                .child(
                    Icon::new(IconName::CircleCheck)
                        .size_4()
                        .text_color(t.success),
                )
                .child(format!(
                    "You marked this read on {}.",
                    day.format("%-d %B %Y")
                ))
                .child(
                    Button::new("mark-read")
                        .ghost()
                        .small()
                        .label("Mark as unread")
                        .on_click(toggle_read(&note.id)),
                ),
        };

        let section_views = sections.iter().enumerate().map(|(k, section)| {
            // The trap is the one section set apart, as a callout.
            let trap = section.heading == Some("The trap");
            column(
                div()
                    .when(section.heading.is_some(), |this| this.pt_6())
                    .child(
                        div()
                            .id(("note-section", k))
                            .test_support()
                            .font_family(ui::HAND)
                            .line_height(relative(1.65))
                            .when(trap, |this| {
                                this.px_4()
                                    .py_3()
                                    .rounded(px(8.))
                                    .bg(t.attention.opacity(0.1))
                                    .border_l_2()
                                    .border_color(t.attention)
                            })
                            .child(self.blocks.view(&k.to_string())),
                    )
                    .into_any_element(),
            )
        });
        let reader = div()
            .id("reader")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .text_size(px(size as f32))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(column(header.into_any_element()))
            .children(section_views)
            .child(column(footer.into_any_element()))
            .when(!questions.is_empty(), |reader| {
                reader.child(column(self.questions(&questions, cx).into_any_element()))
            });

        let bar = self.marking.bar(
            cx.listener(|this, _, _, cx| {
                let comment = this.marking.comment(cx);
                if let Some(selection) = this.marking.take() {
                    this.keep_mark(MarkKind::Highlight, selection, comment, cx);
                }
            }),
            cx.listener(|this, _, _, cx| {
                this.marking.close();
                cx.notify();
            }),
            cx,
        );

        // The outline: each heading, then the questions, as reader positions.
        let mut outline: Vec<(SharedString, usize)> = sections
            .iter()
            .enumerate()
            .filter_map(|(k, section)| Some((section.heading?.to_string().into(), k + 1)))
            .collect();
        if !questions.is_empty() {
            outline.push((
                format!("Questions ({})", questions.len()).into(),
                sections.len() + 2,
            ));
        }

        div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                v_flex().flex_1().min_w_0().h_full().children(bar).child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(reader)
                        .vertical_scrollbar(&self.scroll),
                ),
            )
            .when(self.inspector, |this| {
                this.child(self.inspector(ix, outline, cx))
            })
            .into_any_element()
    }

    /// Beside the note: its outline, how far the student has read it, and
    /// a drill on it.
    fn inspector(
        &self,
        ix: usize,
        outline: Vec<(SharedString, usize)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let note = &library.notes[ix];
        let reading = state.reading.get(&note.id).copied();
        let done = reading.and_then(|reading| reading.done).is_some();
        let seconds = reading.map_or(0, |reading| reading.seconds);
        let share = if done {
            1.
        } else {
            (seconds as f32 / (note.minutes.max(1) * 60) as f32).min(0.99)
        };
        let left = note.minutes.saturating_sub(seconds / 60).max(1);
        let topic = note.topics.first().cloned().unwrap_or_default();
        // A note carries Mains and Prelims topics; the drill is on the one
        // with the most questions.
        let drill = note
            .topics
            .iter()
            .map(|topic| mentor::topic_drill(library, topic, DRILL))
            .max_by_key(|plan| plan.count)
            .unwrap_or_else(|| mentor::topic_drill(library, &topic, DRILL));
        let questions = library.questions_for(note).len();
        let entries = outline
            .into_iter()
            .enumerate()
            .map(|(entry, (title, child))| {
                div()
                    .id(("outline", entry))
                    .test_support()
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .text_color(t.text_2)
                    .cursor_pointer()
                    .hover(|this| this.bg(t.hover).text_color(t.text))
                    .child(div().truncate().child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.scroll.scroll_to_top_of_item(child);
                        cx.notify();
                    }))
            });
        v_flex()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(t.border)
            .overflow_y_scrollbar()
            .id("outline")
            .child(
                v_flex()
                    .p(px(12.))
                    .gap(px(2.))
                    .child(ui::caption("On this page", cx).px(px(10.)).pb(px(6.)))
                    .children(entries),
            )
            .child(
                ui::card(cx)
                    .mx(px(12.))
                    .mt(px(6.))
                    .child(
                        h_flex()
                            .gap(px(12.))
                            .p(px(12.))
                            .child(ui::ring(
                                share,
                                40.,
                                5.,
                                if done { t.success } else { t.accent },
                                div(),
                                cx,
                            ))
                            .child(
                                v_flex()
                                    .child(div().text_size(px(13.)).font_medium().child(if done {
                                        "Read".to_string()
                                    } else {
                                        format!("{}% read", (share * 100.).round())
                                    }))
                                    .child(ui::meta(
                                        if done {
                                            "It counts towards your coverage".to_string()
                                        } else {
                                            format!("About {left} min left")
                                        },
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div().px(px(12.)).pb(px(12.)).child(
                            Button::new("inspector-mark-read")
                                .small()
                                .w_full()
                                .when(!done, |this| this.primary())
                                .icon(Icon::new(IconName::Check))
                                .label(if done {
                                    "Mark as unread"
                                } else {
                                    "Mark as read"
                                })
                                .on_click(toggle_read(&note.id)),
                        ),
                    ),
            )
            .when(drill.count > 0, |this| {
                this.child(
                    ui::card(cx)
                        .mx(px(12.))
                        .mt(px(10.))
                        .mb(px(12.))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .p(px(12.))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .size(px(32.))
                                        .rounded(px(8.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(ui::subject_color(&topic, cx).opacity(0.14))
                                        .child(
                                            Icon::new(IconName::SquarePen)
                                                .size(px(15.))
                                                .text_color(ui::subject_color(&topic, cx)),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .child(div().text_size(px(13.)).font_medium().child(count(
                                            questions,
                                            "question",
                                            "questions",
                                        )))
                                        .child(ui::meta("on what this note covers", cx)),
                                ),
                        )
                        .child(
                            div().px(px(12.)).pb(px(12.)).child(
                                Button::new("practise-note")
                                    .small()
                                    .w_full()
                                    .label(format!(
                                        "Practise {}",
                                        count(drill.count, "question", "questions")
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let plan = drill.clone();
                                        let _ =
                                            this.shell.update(cx, |shell, cx| shell.sit(&plan, cx));
                                    })),
                            ),
                        ),
                )
            })
    }

    /// The questions that go with the note, each with its answer behind a button.
    fn questions(&self, questions: &[usize], cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let cards = questions.iter().map(|&ix| {
            let question = &library.questions[ix];
            let shown = self.answers.contains(&ix);
            let options = question.options.iter().enumerate().map(|(option, text)| {
                let right = shown && Some(option) == question.answer;
                h_flex()
                    .items_start()
                    .gap(px(10.))
                    .text_size(px(13.5))
                    .when(right, |this| this.font_medium().text_color(t.success))
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(22.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.))
                            .font_semibold()
                            .border_1()
                            .border_color(if right { t.success } else { t.border_2 })
                            .when(right, |this| this.bg(t.success).text_color(t.on_accent))
                            .child(["A", "B", "C", "D"][option]),
                    )
                    .child(div().min_w_0().pt(px(2.)).child(text.clone()))
            });
            ui::card(cx)
                .p_4()
                .gap(px(12.))
                .when(!question.source.is_empty(), |card| {
                    card.child(ui::meta(question.source.clone(), cx))
                })
                .child(
                    div()
                        .text_size(px(14.))
                        .child(TextView::markdown(("stem", ix), question.stem.clone())),
                )
                .child(v_flex().gap(px(8.)).children(options))
                .when(shown, |card| {
                    card.child(
                        div()
                            .text_size(px(13.5))
                            .text_color(t.text_2)
                            .child(question.explanation.clone()),
                    )
                    .when_some(question.trap.clone(), |card, trap| {
                        card.child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded(px(6.))
                                .bg(t.attention.opacity(0.1))
                                .border_l_2()
                                .border_color(t.attention)
                                .text_size(px(13.))
                                .child(format!("The trap: {trap}")),
                        )
                    })
                })
                .child(
                    h_flex().child(
                        Button::new(("answer", ix))
                            .small()
                            .label(if shown {
                                "Hide the answer"
                            } else {
                                "Show the answer"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.answers.remove(&ix) {
                                    this.answers.insert(ix);
                                }
                                cx.notify();
                            })),
                    ),
                )
        });
        v_flex()
            .pb_16()
            .gap(px(12.))
            .child(
                div()
                    .font_family(ui::DISPLAY)
                    .text_size(px(20.))
                    .font_semibold()
                    .child("Questions on this note"),
            )
            .children(cards)
    }

    fn panes(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex()
            .size_full()
            .on_mouse_move(cx.listener(|this, _, _, cx| this.touch(cx)))
            .child(self.list(cx))
            .child(self.reader(cx))
    }

    /// The header: the breadcrumb to the open note, Read or Distilled, and
    /// the reader's controls.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let Some(ix) = self.note else {
            return ui::header(cx)
                .child(ui::title(IconName::BookOpen, "Notes", cx))
                .into_any_element();
        };
        let note = &library.notes[ix];
        let (subject, section) = place(library, note);
        let bookmarked = state
            .reading
            .get(&note.id)
            .is_some_and(|reading| reading.bookmarked);
        let size = state.settings.reader_size.clamp(MIN_SIZE, MAX_SIZE);
        let wide = state.settings.reader_wide;
        let has_distilled = !note.distilled.is_empty();
        let distilled = self.distilled && has_distilled;
        let chevron = || {
            Icon::new(IconName::ChevronRight)
                .size(px(13.))
                .text_color(t.text_4)
        };
        let icon_button = |id: &'static str, icon: IconName, tip: &'static str| {
            Button::new(id)
                .ghost()
                .small()
                .icon(Icon::new(icon))
                .tooltip(tip)
        };
        ui::header(cx)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .text_color(t.text_3)
                    .child(div().flex_shrink_0().child("Notes"))
                    // The folders the note is in, as in the list.
                    .children(
                        [Some(subject), section]
                            .into_iter()
                            .flatten()
                            .map(|folder| {
                                h_flex()
                                    .flex_shrink_0()
                                    .gap(px(6.))
                                    .child(chevron())
                                    .child(folder)
                            }),
                    )
                    .child(chevron())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_medium()
                            .text_color(t.text)
                            .child(note.title.clone()),
                    )
                    .child(
                        ui::segmented(cx)
                            .ml_2()
                            .child(ui::segment("read-mode", "Read", !distilled, cx).on_click(
                                cx.listener(|this, _, _, cx| this.set_distilled(false, cx)),
                            ))
                            .child(
                                ui::segment("distilled-mode", "Distilled", distilled, cx)
                                    .when(!has_distilled, |this| this.opacity(0.5))
                                    .when(has_distilled, |this| {
                                        this.on_click(cx.listener(|this, _, _, cx| {
                                            this.set_distilled(true, cx)
                                        }))
                                    }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_button("smaller-text", IconName::AArrowDown, "Smaller text")
                            .disabled(size <= MIN_SIZE)
                            .on_click(move |_, _, cx| {
                                AppState::update_settings(cx, |settings| {
                                    settings.reader_size = size - 1
                                });
                            }),
                    )
                    .child(
                        icon_button("larger-text", IconName::AArrowUp, "Larger text")
                            .disabled(size >= MAX_SIZE)
                            .on_click(move |_, _, cx| {
                                AppState::update_settings(cx, |settings| {
                                    settings.reader_size = size + 1
                                });
                            }),
                    )
                    .child(
                        icon_button(
                            "wide",
                            IconName::ChevronsLeftRight,
                            if wide { "Narrow column" } else { "Wide column" },
                        )
                        .selected(wide)
                        .on_click(|_, _, cx| {
                            AppState::update_settings(cx, |settings| {
                                settings.reader_wide = !settings.reader_wide
                            });
                        }),
                    )
                    .child(
                        icon_button(
                            "highlight",
                            IconName::Highlighter,
                            "Highlight the selected text",
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_mark(MarkKind::Highlight, window, cx)
                        })),
                    )
                    .child(
                        icon_button(
                            "clip",
                            IconName::NotebookText,
                            "Keep the selected text as a clip",
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_mark(MarkKind::Clip, window, cx)
                        })),
                    )
                    .child(
                        icon_button(
                            "bookmark",
                            if bookmarked {
                                IconName::BookmarkCheck
                            } else {
                                IconName::Bookmark
                            },
                            if bookmarked {
                                "Remove the bookmark"
                            } else {
                                "Bookmark this note"
                            },
                        )
                        .selected(bookmarked)
                        .on_click({
                            let id = note.id.clone();
                            move |_, _, cx| {
                                AppState::update_reading(cx, &id, |reading, _| {
                                    reading.bookmarked = !reading.bookmarked
                                });
                            }
                        }),
                    )
                    .child(
                        icon_button(
                            "inspector",
                            IconName::PanelRight,
                            "Show or hide the outline",
                        )
                        .selected(self.inspector)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.inspector = !this.inspector;
                            cx.notify();
                        })),
                    ),
            )
            .into_any_element()
    }
}

/// Marks a note read, or unread again, the same from the footer and the
/// inspector.
fn toggle_read(note: &str) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let id = note.to_string();
    move |_, _, cx| {
        AppState::update_reading(cx, &id, |reading, today| {
            reading.done = match reading.done {
                Some(_) => None,
                None => Some(today),
            };
        });
    }
}

/// A row of the list, indented for its depth in the tree.
fn tree_row(id: impl Into<ElementId>, depth: usize, cx: &App) -> Stateful<Div> {
    let t = ui::tokens(cx);
    h_flex()
        .id(id)
        .gap(px(6.))
        .pl(px(6. + 16. * depth as f32))
        .pr(px(8.))
        .py(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|this| this.bg(t.hover))
}

/// The list's folders: each subject with its sections, and the notes in
/// each, in the notes' order. Notes with no section come under `None`.
fn tree(library: &Library) -> Vec<Subject> {
    let mut notes: Vec<usize> = (0..library.notes.len()).collect();
    notes.sort_by_key(|&ix| library.notes[ix].order.unwrap_or(u32::MAX));
    let mut tree: Vec<Subject> = Vec::new();
    for ix in notes {
        let (subject, section) = place(library, &library.notes[ix]);
        let at = match tree.iter().position(|(name, _)| *name == subject) {
            Some(at) => at,
            None => {
                tree.push((subject, Vec::new()));
                tree.len() - 1
            }
        };
        let sections = &mut tree[at].1;
        match sections.iter_mut().find(|(name, _)| *name == section) {
            Some((_, notes)) => notes.push(ix),
            None => sections.push((section, vec![ix])),
        }
    }
    tree
}

/// The subject and section the list files a note under. A note with no
/// subject goes under its paper.
fn place(library: &Library, note: &Note) -> Folder {
    let subject = note.subject.clone().unwrap_or_else(|| {
        let paper: Vec<&str> = note
            .topics
            .first()
            .map_or("", |t| t)
            .split('.')
            .take(2)
            .collect();
        let id = paper.join(".");
        library.topic(&id).map_or(id, |topic| topic.title.clone())
    });
    (subject, note.section.clone())
}

impl Render for NotesScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.header(cx);
        v_flex()
            .size_full()
            .child(header)
            .child(div().flex_1().min_h_0().child(self.panes(cx)))
    }
}

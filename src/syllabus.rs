//! The Syllabus screen. The exam is a tree: two stages, nine papers, then
//! subjects, chapters and topics, with UPSC's own wording on the upper
//! levels. The screen draws one paper of it in one of five designs, picked
//! in its header, so the student can compare them and keep one:
//!
//! - [`flow`]: a canvas of cards joined by connectors, a column per level,
//!   with an overview of the paper beside it;
//! - [`mindmap`]: a node in the middle with its parts branching out on both
//!   sides and their parts beyond them;
//! - [`mosaic`]: the whole exam, one square per topic, filled in as it is read;
//! - [`treemap`]: a node's parts as rectangles sized by how many topics they
//!   hold, with their own parts nested inside;
//! - [`sunburst`]: a node's parts as rings around it, sized the same way.
//!
//! Every design marks how far the student has got, and a topic with a note
//! opens the note.

mod flow;
mod mindmap;
mod mosaic;
mod sunburst;
mod treemap;

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Library, Target, Topic, covers, is_under, within};
use crate::screens::count;
use crate::store::Reading;
use crate::ui;

/// How far the student has got with a note, or with a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A topic that no note covers yet.
    NoNotes,
    NotStarted,
    Reading,
    Done,
}

impl Status {
    /// A note's status: reading once opened, done once marked read.
    pub fn of(reading: Option<&Reading>) -> Status {
        match reading {
            None => Status::NotStarted,
            Some(reading) if reading.done.is_some() => Status::Done,
            Some(_) => Status::Reading,
        }
    }

    /// A topic's status, from the notes that cover it: done once all of them are.
    pub fn of_topic(library: &Library, reading: &HashMap<String, Reading>, topic: &str) -> Status {
        let notes: Vec<Status> = library
            .notes
            .iter()
            .filter(|note| covers(&note.topics, topic))
            .map(|note| Status::of(reading.get(&note.id)))
            .collect();
        if notes.is_empty() {
            Status::NoNotes
        } else if notes.iter().all(|&status| status == Status::Done) {
            Status::Done
        } else if notes.iter().all(|&status| status == Status::NotStarted) {
            Status::NotStarted
        } else {
            Status::Reading
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::NoNotes => "No notes yet",
            Status::NotStarted => "Not started",
            Status::Reading => "In progress",
            Status::Done => "Read",
        }
    }

    pub(crate) fn icon(self, cx: &App) -> Icon {
        let (icon, color) = match self {
            Status::NoNotes => (IconName::Minus, cx.theme().muted_foreground),
            Status::NotStarted => (IconName::Circle, cx.theme().muted_foreground),
            Status::Reading => (IconName::CircleDot, cx.theme().primary),
            Status::Done => (IconName::CircleCheck, cx.theme().success),
        };
        Icon::new(icon).text_color(color)
    }

    /// How strongly a topic takes its subject's colour: solid once read,
    /// half way while reading, a tint with a note waiting, a trace with none.
    fn fill(self) -> f32 {
        match self {
            Status::Done => 1.,
            Status::Reading => 0.55,
            Status::NotStarted => 0.24,
            Status::NoNotes => 0.1,
        }
    }
}

/// Every topic's status, by index into the library's topics.
pub fn statuses(library: &Library, reading: &HashMap<String, Reading>) -> Vec<Status> {
    library
        .topics
        .iter()
        .map(|topic| Status::of_topic(library, reading, &topic.id))
        .collect()
}

/// How many of the bottom-level topics under `under` are done, out of how
/// many. Topics with no notes yet count too, so this is a share of the whole
/// syllabus.
pub fn coverage(library: &Library, statuses: &[Status], under: &str) -> (usize, usize) {
    let mut counts = (0, 0);
    for (topic, &status) in library.topics.iter().zip(statuses) {
        if topic.leaf && within(&topic.id, under) {
            counts.0 += usize::from(status == Status::Done);
            counts.1 += 1;
        }
    }
    counts
}

fn percent((done, total): (usize, usize)) -> f32 {
    if total == 0 {
        0.
    } else {
        done as f32 * 100. / total as f32
    }
}

/// A paper's marks, and whether it only has to be passed, read from UPSC's
/// wording of it: "Paper II-(200 marks) ... a qualifying paper" gives
/// "200 marks · qualifying". The marks are the number before the first "marks".
fn facts(official: &str) -> String {
    let marks = ["marks", "Marks"]
        .iter()
        .filter_map(|word| official.find(word))
        .min()
        .and_then(|at| official[..at].split_whitespace().last())
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_digit()))
        .filter(|digits| !digits.is_empty())
        .map(|digits| format!("{digits} marks"));
    let qualifying = official
        .contains("qualifying")
        .then(|| "qualifying".to_string());
    marks
        .into_iter()
        .chain(qualifying)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A paper's name in the paper strip, under its stage's name: "Prelims GS"
/// is "GS", "CSAT (GS Paper II)" is "CSAT" and "Paper I: Essay" is "Essay".
fn short<'a>(title: &'a str, stage: &str) -> &'a str {
    let title = title.strip_prefix(stage).map_or(title, str::trim_start);
    let title = title.split(" (").next().unwrap_or(title);
    title.rsplit(": ").next().unwrap_or(title)
}

/// UPSC's own wording, set in the reading face between quotation marks.
/// Every quoted line on the screen is UPSC's; the rest is our breakdown.
fn quote(text: &str, size: f32, color: Hsla) -> Div {
    div()
        .font_family(ui::SERIF)
        .text_size(px(size))
        .line_height(relative(1.45))
        .text_color(color)
        .child(format!("“{}”", text.trim()))
}

/// The note a topic opens: the first one on it that is not read yet, or the
/// first one when all of them are.
fn next_note(library: &Library, reading: &HashMap<String, Reading>, topic: &str) -> Option<usize> {
    let notes: Vec<usize> = (0..library.notes.len())
        .filter(|&n| covers(&library.notes[n].topics, topic))
        .collect();
    notes
        .iter()
        .copied()
        .find(|&n| Status::of(reading.get(&library.notes[n].id)) != Status::Done)
        .or(notes.first().copied())
}

/// A topic's status, notes and questions, for its tooltip.
fn describe(library: &Library, reading: &HashMap<String, Reading>, topic: &str) -> String {
    let notes = library
        .notes
        .iter()
        .filter(|note| covers(&note.topics, topic))
        .count();
    let questions = library
        .questions
        .iter()
        .filter(|question| is_under(&question.topics, topic))
        .count();
    let mut parts = vec![
        Status::of_topic(library, reading, topic)
            .label()
            .to_string(),
    ];
    if notes > 0 {
        parts.push(count(notes, "note", "notes"));
    }
    if questions > 0 {
        parts.push(count(questions, "question", "questions"));
    }
    parts.join(" · ")
}

/// A topic's tooltip: its title, which a design may have cut short, and
/// how far the student has got with it. The text is worked out when it
/// shows: a paper has hundreds of topics, and most are never pointed at.
fn topic_tip(id: String) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |window, cx| {
        let state = AppState::get(cx);
        let title = state
            .library
            .topics
            .iter()
            .find(|topic| topic.id == id)
            .map_or("", |topic| topic.title.as_str());
        let about = describe(&state.library, &state.reading, &id);
        Tooltip::new(format!("{title} · {about}")).build(window, cx)
    }
}

/// A tooltip with fixed text.
fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| Tooltip::new(text.clone()).build(window, cx)
}

/// How the bottom-level topics under a node stand, by status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Tally {
    done: usize,
    reading: usize,
    not_started: usize,
    no_notes: usize,
}

impl Tally {
    fn add(&mut self, status: Status) {
        match status {
            Status::Done => self.done += 1,
            Status::Reading => self.reading += 1,
            Status::NotStarted => self.not_started += 1,
            Status::NoNotes => self.no_notes += 1,
        }
    }

    fn merge(&mut self, other: Tally) {
        self.done += other.done;
        self.reading += other.reading;
        self.not_started += other.not_started;
        self.no_notes += other.no_notes;
    }

    fn total(self) -> usize {
        self.done + self.reading + self.not_started + self.no_notes
    }

    /// The share that is read, 0 to 1.
    fn share(self) -> f32 {
        percent((self.done, self.total())) / 100.
    }

    /// How strongly the node takes its subject's colour: the average of its
    /// topics' [`Status::fill`].
    fn fill(self) -> f32 {
        let sum = self.done as f32 * Status::Done.fill()
            + self.reading as f32 * Status::Reading.fill()
            + self.not_started as f32 * Status::NotStarted.fill()
            + self.no_notes as f32 * Status::NoNotes.fill();
        sum / self.total().max(1) as f32
    }
}

/// The syllabus with the student's progress through it, as the designs
/// draw it.
struct Tree<'a> {
    library: &'a Library,
    reading: &'a HashMap<String, Reading>,
    statuses: Vec<Status>,
    /// Each node's topics by status.
    tally: Vec<Tally>,
    parent: Vec<Option<usize>>,
    /// Each node's children, in syllabus order.
    kids: Vec<Vec<usize>>,
}

impl<'a> Tree<'a> {
    fn new(cx: &'a App) -> Self {
        let state = AppState::get(cx);
        let library = &state.library;
        let statuses = statuses(library, &state.reading);
        let index: HashMap<&str, usize> = library
            .topics
            .iter()
            .enumerate()
            .map(|(ix, topic)| (topic.id.as_str(), ix))
            .collect();
        let parent: Vec<Option<usize>> = library
            .topics
            .iter()
            .map(|topic| {
                let (up, _) = topic.id.rsplit_once('.')?;
                index.get(up).copied()
            })
            .collect();
        let mut kids = vec![Vec::new(); library.topics.len()];
        for (ix, up) in parent.iter().enumerate() {
            if let Some(up) = *up {
                kids[up].push(ix);
            }
        }
        // Deepest first, so that every node is summed before its parent.
        let mut order: Vec<usize> = (0..library.topics.len()).collect();
        order.sort_by_key(|&ix| std::cmp::Reverse(library.topics[ix].depth));
        let mut tally = vec![Tally::default(); library.topics.len()];
        for ix in order {
            if library.topics[ix].leaf {
                tally[ix].add(statuses[ix]);
            }
            if let Some(up) = parent[ix] {
                let below = tally[ix];
                tally[up].merge(below);
            }
        }
        Tree {
            library,
            reading: &state.reading,
            statuses,
            tally,
            parent,
            kids,
        }
    }

    fn topic(&self, ix: usize) -> &'a Topic {
        &self.library.topics[ix]
    }

    fn kids(&self, ix: usize) -> &[usize] {
        &self.kids[ix]
    }

    /// The stages, the tree's roots.
    fn stages(&self) -> Vec<usize> {
        (0..self.parent.len())
            .filter(|&ix| self.parent[ix].is_none())
            .collect()
    }

    fn color(&self, ix: usize, cx: &App) -> Hsla {
        ui::subject_color(&self.topic(ix).id, cx)
    }

    /// The nodes from `from` down to `ix`, both included.
    fn path(&self, from: usize, ix: usize) -> Vec<usize> {
        let mut path = vec![ix];
        let mut at = ix;
        while at != from {
            match self.parent[at] {
                Some(up) => {
                    path.push(up);
                    at = up;
                }
                None => break,
            }
        }
        path.reverse();
        path
    }

    /// Where `ix` sits, from its paper down to its parent: "Prelims GS ›
    /// Indian polity and governance › Parliament".
    fn trail(&self, ix: usize) -> String {
        // From the top: `usize::MAX` is above every node.
        let path = self.path(usize::MAX, ix);
        let above = path.get(1..path.len().saturating_sub(1)).unwrap_or(&[]);
        above
            .iter()
            .map(|&node| self.topic(node).title.as_str())
            .collect::<Vec<_>>()
            .join(" › ")
    }

    /// The bottom-level topics under `ix`, in syllabus order.
    fn leaves(&self, ix: usize) -> Vec<usize> {
        if self.topic(ix).leaf {
            return vec![ix];
        }
        self.kids(ix)
            .iter()
            .flat_map(|&kid| self.leaves(kid))
            .collect()
    }

    /// UPSC's wording nearest above `ix`, `ix` included, and its node.
    fn official(&self, ix: usize) -> Option<(usize, &'a str)> {
        let mut at = Some(ix);
        while let Some(node) = at {
            if let Some(official) = self.topic(node).official.as_deref() {
                return Some((node, official));
            }
            at = self.parent[node];
        }
        None
    }

    /// The note a topic opens, when a note covers it.
    fn note(&self, ix: usize) -> Option<usize> {
        (self.statuses[ix] != Status::NoNotes)
            .then(|| next_note(self.library, self.reading, &self.topic(ix).id))
            .flatten()
    }

    /// How many notes are on anything under `ix`, and how many questions.
    fn material(&self, ix: usize) -> (usize, usize) {
        let id = &self.topic(ix).id;
        let notes = self
            .library
            .notes
            .iter()
            .filter(|note| {
                note.topics
                    .iter()
                    .any(|tag| within(tag, id) || within(id, tag))
            })
            .count();
        let questions = self
            .library
            .questions
            .iter()
            .filter(|question| is_under(&question.topics, id))
            .count();
        (notes, questions)
    }

    /// The topics under `ix` the student could read next: the ones with a
    /// note that is not read yet.
    fn next(&self, ix: usize, most: usize) -> Vec<usize> {
        self.leaves(ix)
            .into_iter()
            .filter(|&leaf| matches!(self.statuses[leaf], Status::NotStarted | Status::Reading))
            .take(most)
            .collect()
    }
}

/// A thin bar split by status: read, in progress and not started, on a
/// track that stands for the topics with no notes yet.
fn status_bar(tally: Tally, height: f32, t: &ui::Tokens) -> Div {
    let total = tally.total().max(1) as f32;
    let part =
        |count: usize, color: Hsla| div().h_full().w(relative(count as f32 / total)).bg(color);
    h_flex()
        .w_full()
        .h(px(height))
        .rounded(px(height / 2.))
        .overflow_hidden()
        .bg(t.selected)
        .child(part(tally.done, t.success))
        .child(part(tally.reading, t.accent))
        .child(part(tally.not_started, t.text_4))
}

/// The counts behind a [`status_bar`], each with its colour.
fn status_legend(tally: Tally, t: &ui::Tokens) -> Div {
    let item = |color: Hsla, count: usize, label: &str| {
        h_flex()
            .gap(px(5.))
            .child(div().size(px(7.)).rounded_full().bg(color))
            .child(format!("{count} {label}"))
    };
    h_flex()
        .flex_wrap()
        .gap_x(px(12.))
        .gap_y(px(4.))
        .text_size(px(11.5))
        .text_color(t.text_3)
        .child(item(t.success, tally.done, "read"))
        .child(item(t.accent, tally.reading, "in progress"))
        .child(item(t.text_4, tally.not_started, "not started"))
        .child(item(t.selected, tally.no_notes, "no notes yet"))
}

/// What clicking something does.
type Action = Box<dyn Fn(&mut App)>;

/// What a design needs from the screen once the layout knows its size:
/// the screen's choices, and handles to change them and to open notes.
#[derive(Clone)]
struct View {
    screen: WeakEntity<SyllabusScreen>,
    shell: WeakEntity<Shell>,
    paper: usize,
    /// The node the mind map, treemap and sunburst centre on: the paper,
    /// or a node under it.
    focus: usize,
    /// The flow's open subject and chapter.
    subject: Option<usize>,
    chapter: Option<usize>,
    /// The topic under the pointer, in the mosaic and the sunburst.
    hovered: Option<usize>,
    scroll: ScrollHandle,
}

impl View {
    /// Changes the screen and draws it again.
    fn update(&self, cx: &mut App, change: impl FnOnce(&mut SyllabusScreen)) {
        let _ = self.screen.update(cx, |screen, cx| {
            change(screen);
            cx.notify();
        });
    }

    /// Points at `ix`, or at nothing, drawing again only when that changes.
    fn hover(&self, ix: Option<usize>, cx: &mut App) {
        let _ = self.screen.update(cx, |screen, cx| {
            if screen.hovered != ix {
                screen.hovered = ix;
                cx.notify();
            }
        });
    }

    /// The pointer came onto `ix`, or left it. Leaving one square for the
    /// next can report the arrival first, so leaving clears only `ix`.
    fn pointed(&self, ix: usize, on: bool, cx: &mut App) {
        let _ = self.screen.update(cx, |screen, cx| {
            let next = match (on, screen.hovered) {
                (true, _) => Some(ix),
                (false, Some(was)) if was == ix => None,
                (false, was) => was,
            };
            if screen.hovered != next {
                screen.hovered = next;
                cx.notify();
            }
        });
    }

    fn open_note(&self, note: usize, cx: &mut App) {
        // The shell may update this screen, so this is not a listener.
        let _ = self
            .shell
            .update(cx, |shell, cx| shell.open(Target::Note(note), cx));
    }

    /// Opens what `ix` leads to: a topic's note, or the node as the focus.
    /// Topics with no note lead nowhere.
    fn open(&self, ix: usize, tree: &Tree) -> Option<Action> {
        if tree.topic(ix).leaf {
            let note = tree.note(ix)?;
            let view = self.clone();
            Some(Box::new(move |cx| view.open_note(note, cx)))
        } else {
            let view = self.clone();
            Some(Box::new(move |cx| {
                view.update(cx, |screen| screen.focus_on(ix))
            }))
        }
    }
}

/// The way down from the paper to the focus, each step a link back up, and
/// UPSC's wording for the focus, for the designs that zoom.
fn focus_bar(tree: &Tree, view: &View, cx: &App) -> Div {
    let t = ui::tokens(cx);
    let path = tree.path(view.paper, view.focus);
    let last = path.len() - 1;
    let mut crumbs = h_flex().flex_wrap().gap(px(4.)).text_size(px(12.5));
    for (n, &ix) in path.iter().enumerate() {
        if n > 0 {
            crumbs = crumbs.child(
                Icon::new(IconName::ChevronRight)
                    .size(px(12.))
                    .text_color(t.text_4),
            );
        }
        let view = view.clone();
        crumbs = crumbs.child(
            div()
                .id(("crumb", ix))
                .test_support()
                .px(px(4.))
                .rounded(px(4.))
                .child(tree.topic(ix).title.clone())
                .map(|this| {
                    if n == last {
                        this.text_color(t.text).font_medium()
                    } else {
                        this.text_color(t.text_3)
                            .cursor_pointer()
                            .hover(|this| this.bg(t.hover).text_color(t.text))
                            .on_click(move |_, _, cx| view.update(cx, |screen| screen.focus_on(ix)))
                    }
                }),
        );
    }
    let tally = tree.tally[view.focus];
    v_flex()
        .flex_shrink_0()
        .px(px(16.))
        .py(px(10.))
        .gap(px(4.))
        .border_b_1()
        .border_color(t.border)
        .child(
            h_flex()
                .gap(px(12.))
                .child(div().flex_1().min_w_0().child(crumbs))
                .child(
                    ui::meta(
                        format!("{} of {} topics read", tally.done, tally.total()),
                        cx,
                    )
                    .flex_shrink_0(),
                ),
        )
        .when_some(tree.official(view.focus), |this, (_, official)| {
            this.child(quote(official, 13.5, t.text_2).px(px(4.)).line_clamp(2))
        })
}

/// The five designs, in the order the header offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Design {
    Flow,
    MindMap,
    Mosaic,
    Treemap,
    Sunburst,
}

impl Design {
    const ALL: [Design; 5] = [
        Design::Flow,
        Design::MindMap,
        Design::Mosaic,
        Design::Treemap,
        Design::Sunburst,
    ];

    fn label(self) -> &'static str {
        match self {
            Design::Flow => "Flow",
            Design::MindMap => "Mind map",
            Design::Mosaic => "Mosaic",
            Design::Treemap => "Treemap",
            Design::Sunburst => "Sunburst",
        }
    }
}

/// The syllabus a paper at a time, in the design the student picks.
pub struct SyllabusScreen {
    shell: WeakEntity<Shell>,
    // ponytail: in memory only. The switcher is for choosing a design; once
    // one is chosen the others go, and so does this.
    design: Design,
    /// The open paper, by index into the library's topics; the first paper
    /// until the student picks one.
    paper: Option<usize>,
    /// What the zooming designs centre on, under the paper.
    focus: Option<usize>,
    subject: Option<usize>,
    chapter: Option<usize>,
    hovered: Option<usize>,
    /// The design's scroll, so that a paper or a zoom starts at the top and
    /// the mosaic can jump to a paper.
    scroll: ScrollHandle,
}

impl SyllabusScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        SyllabusScreen {
            shell,
            design: Design::Flow,
            paper: None,
            focus: None,
            subject: None,
            chapter: None,
            hovered: None,
            scroll: ScrollHandle::new(),
        }
    }

    fn set_design(&mut self, design: Design, cx: &mut Context<Self>) {
        self.design = design;
        self.hovered = None;
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// Opens a paper; `order` is its place among all the papers, which is
    /// where the mosaic draws it.
    fn open_paper(&mut self, paper: usize, order: usize, cx: &mut Context<Self>) {
        self.paper = Some(paper);
        self.focus = None;
        self.subject = None;
        self.chapter = None;
        self.hovered = None;
        if self.design == Design::Mosaic {
            self.scroll.scroll_to_top_of_item(order);
        } else {
            self.scroll.set_offset(Point::default());
        }
        cx.notify();
    }

    fn focus_on(&mut self, ix: usize) {
        self.focus = Some(ix);
        self.hovered = None;
        self.scroll.set_offset(Point::default());
    }

    /// The nine papers under their stages, each with how much of it is read.
    fn papers(&self, tree: &Tree, paper: usize, cx: &Context<Self>) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let mut strip = h_flex()
            .id("papers")
            .flex_shrink_0()
            .h(px(40.))
            .px(px(12.))
            .gap(px(2.))
            .border_b_1()
            .border_color(t.border)
            .overflow_x_scroll();
        let mut order = 0;
        for (n, stage) in tree.stages().into_iter().enumerate() {
            let stage_title = &tree.topic(stage).title;
            if n > 0 {
                strip = strip.child(
                    div()
                        .flex_shrink_0()
                        .w(px(1.))
                        .h(px(16.))
                        .mx(px(10.))
                        .bg(t.border_2),
                );
            }
            strip = strip.child(ui::caption(stage_title.clone(), cx).px(px(6.)));
            for &ix in tree.kids(stage) {
                let topic = tree.topic(ix);
                let tally = tree.tally[ix];
                let chosen = ix == paper;
                let at = order;
                order += 1;
                let facts = facts(topic.official.as_deref().unwrap_or_default());
                strip = strip.child(
                    h_flex()
                        .id(("paper", ix))
                        .test_support()
                        .flex_shrink_0()
                        .h_full()
                        .px(px(8.))
                        .gap(px(6.))
                        .border_b_2()
                        .cursor_pointer()
                        .text_size(px(13.))
                        .map(|this| {
                            if chosen {
                                this.border_color(t.accent).text_color(t.text).font_medium()
                            } else {
                                this.border_color(transparent_black())
                                    .text_color(t.text_2)
                                    .hover(|this| this.text_color(t.text))
                            }
                        })
                        .child(ui::ring(tally.share(), 12., 2., t.accent, div(), cx))
                        .child(short(&topic.title, stage_title).to_string())
                        .tooltip(tip(format!(
                            "{} · {facts} · {} of {} topics read",
                            topic.title,
                            tally.done,
                            tally.total()
                        )))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_paper(ix, at, cx))),
                );
            }
        }
        strip
    }
}

impl Render for SyllabusScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let tree = Tree::new(cx);
        let mut whole = Tally::default();
        for stage in tree.stages() {
            whole.merge(tree.tally[stage]);
        }
        let switcher = ui::segmented(cx)
            .ml(px(16.))
            .children(Design::ALL.map(|design| {
                ui::segment(
                    ("design", design as usize),
                    design.label(),
                    design == self.design,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_design(design, cx)))
            }));
        let header = ui::header(cx)
            .child(ui::title(IconName::ListTree, "Syllabus", cx))
            .child(switcher)
            .child(div().flex_1())
            .child(
                h_flex()
                    .gap(px(8.))
                    .pr_1()
                    .child(ui::ring(whole.share(), 18., 3., t.accent, div(), cx))
                    .child(ui::meta(
                        format!("{} of {} topics read", whole.done, whole.total()),
                        cx,
                    )),
            );
        let papers: Vec<usize> = tree
            .stages()
            .into_iter()
            .flat_map(|stage| tree.kids(stage).to_vec())
            .collect();
        let Some(paper) = self
            .paper
            .filter(|ix| papers.contains(ix))
            .or(papers.first().copied())
        else {
            return v_flex().size_full().child(header).child(
                ui::meta(
                    "The syllabus did not load. Settings lists what is wrong with the content.",
                    cx,
                )
                .p_6(),
            );
        };
        let paper_id = &tree.topic(paper).id;
        let focus = self
            .focus
            .filter(|&ix| within(&tree.topic(ix).id, paper_id) && !tree.topic(ix).leaf)
            .unwrap_or(paper);
        let view = View {
            screen: cx.entity().downgrade(),
            shell: self.shell.clone(),
            paper,
            focus,
            subject: self.subject,
            chapter: self.chapter,
            hovered: self.hovered,
            scroll: self.scroll.clone(),
        };
        let body = match self.design {
            Design::Flow => flow::view(&tree, view, cx),
            Design::MindMap => mindmap::view(&tree, view, cx),
            Design::Mosaic => mosaic::view(&tree, view, cx),
            Design::Treemap => treemap::view(&tree, view, cx),
            Design::Sunburst => sunburst::view(&tree, view, cx),
        };
        v_flex()
            .size_full()
            .child(header)
            .child(self.papers(&tree, paper, cx))
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in `gpui_kit::test`, which shadows `#[test]`.
    use super::{Status, Tally, coverage, facts, percent, short, statuses};
    use crate::content::Library;
    use crate::store::Reading;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    #[test]
    fn a_topic_is_covered_once_every_note_on_it_is_read() {
        let syllabus = r#"
[[topic]]
id = "mains"
title = "Mains"
[[topic]]
id = "mains.gs1"
title = "GS-1"
[[topic]]
id = "mains.gs1.geo"
title = "Geography"
[[topic]]
id = "mains.gs1.geo.monsoon"
title = "Monsoon"
[[topic]]
id = "mains.gs1.geo.rivers"
title = "Rivers"
[[topic]]
id = "mains.gs1.geo.rivers.himalayan"
title = "Himalayan rivers"
[[topic]]
id = "mains.gs1.geo.rivers.peninsular"
title = "Peninsular rivers"
[[topic]]
id = "mains.gs1.geo.soils"
title = "Soils"
"#;
        let note = |id: &str, topics: &str| {
            let text = format!("---\nid: {id}\ntitle: {id}\ntopics: [{topics}]\n---\nText.\n");
            (format!("notes/{id}.md"), text)
        };
        let library = Library::from_files([
            ("syllabus.toml".to_string(), syllabus.to_string()),
            note("monsoon", "mains.gs1.geo.monsoon"),
            // A note on a whole chapter covers every topic in it.
            note("rivers", "mains.gs1.geo.rivers"),
            note("ganga", "mains.gs1.geo.rivers.himalayan"),
        ]);
        assert_eq!(library.problems, Vec::<String>::new());
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let mut read = Reading::new(day);
        read.done = Some(day);
        let status = |reading: &HashMap<String, Reading>, topic: &str| {
            let ix = library.topics.iter().position(|t| t.id == topic).unwrap();
            statuses(&library, reading)[ix]
        };
        let covered = |reading: &HashMap<String, Reading>, under: &str| {
            coverage(&library, &statuses(&library, reading), under)
        };

        let mut reading = HashMap::new();
        assert_eq!(
            covered(&reading, "mains.gs1"),
            (0, 4),
            "the bottom-level topics, soils included"
        );
        assert_eq!(status(&reading, "mains.gs1.geo.soils"), Status::NoNotes);
        assert_eq!(
            status(&reading, "mains.gs1.geo.monsoon"),
            Status::NotStarted
        );

        reading.insert("monsoon".to_string(), Reading::new(day));
        assert_eq!(status(&reading, "mains.gs1.geo.monsoon"), Status::Reading);
        reading.insert("monsoon".to_string(), read);
        assert_eq!(covered(&reading, "mains.gs1"), (1, 4));

        reading.insert("rivers".to_string(), read);
        assert_eq!(
            status(&reading, "mains.gs1.geo.rivers.peninsular"),
            Status::Done,
            "the chapter note covers it"
        );
        assert_eq!(
            status(&reading, "mains.gs1.geo.rivers.himalayan"),
            Status::Reading,
            "the chapter note is read, the Ganga note is not"
        );
        assert_eq!(covered(&reading, "mains.gs1"), (2, 4));

        reading.insert("ganga".to_string(), read);
        assert_eq!(covered(&reading, "mains.gs1.geo.rivers"), (2, 2));
        assert_eq!(covered(&reading, "mains.gs1"), (3, 4));
        assert_eq!(percent((3, 4)), 75.);
        assert_eq!(percent((0, 0)), 0., "an empty paper is 0%, not NaN");
    }

    #[test]
    fn each_paper_shows_its_marks_and_whether_it_only_qualifies() {
        let library = Library::embedded();
        let papers: Vec<(&str, String)> = library
            .topics
            .iter()
            .filter(|topic| topic.depth == 1)
            .map(|topic| {
                let official = topic.official.as_deref().unwrap_or_default();
                (topic.id.as_str(), facts(official))
            })
            .collect();
        let expected = [
            ("pre.gs", "200 marks"),
            ("pre.csat", "200 marks · qualifying"),
            ("mains.paper-a", "300 marks · qualifying"),
            ("mains.paper-b", "300 marks · qualifying"),
            ("mains.essay", "250 marks"),
            ("mains.gs1", "250 marks"),
            ("mains.gs2", "250 marks"),
            ("mains.gs3", "250 marks"),
            ("mains.gs4", "250 marks"),
        ];
        let expected: Vec<(&str, String)> = expected
            .iter()
            .map(|(id, facts)| (*id, facts.to_string()))
            .collect();
        assert_eq!(papers, expected);
        assert_eq!(facts("No marks given here"), "", "no number, no marks");
    }

    #[test]
    fn papers_go_by_short_names_under_their_stage() {
        let library = Library::embedded();
        let names: Vec<&str> = library
            .topics
            .iter()
            .filter(|topic| topic.depth == 1)
            .map(|topic| {
                let stage = topic.id.split('.').next().unwrap();
                let stage = &library.topics.iter().find(|t| t.id == stage).unwrap().title;
                short(&topic.title, stage)
            })
            .collect();
        assert_eq!(
            names,
            [
                "GS",
                "CSAT",
                "Indian language",
                "English",
                "Essay",
                "GS-1",
                "GS-2",
                "GS-3",
                "GS-4"
            ]
        );
    }

    #[test]
    fn a_node_is_coloured_by_the_average_of_its_topics() {
        let tally = Tally {
            done: 1,
            reading: 0,
            not_started: 0,
            no_notes: 1,
        };
        assert_eq!(tally.total(), 2);
        assert_eq!(tally.share(), 0.5);
        assert!((tally.fill() - (1. + Status::NoNotes.fill()) / 2.).abs() < 1e-6);
        assert_eq!(Tally::default().fill(), 0., "an empty node has no colour");
    }
}

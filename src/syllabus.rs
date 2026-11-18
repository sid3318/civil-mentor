//! The Syllabus screen. On the left, a map of the exam as UPSC lays it out:
//! each stage in UPSC's words, then its papers with their marks and how much
//! of each is read. On the right, the open paper as a syllabus sheet: UPSC's
//! wording, with every chapter and topic under it, each topic marked by how
//! far the student has got with it.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Library, Target, covers, is_under, within};
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

/// UPSC's own wording, set in the reading face between quotation marks. Every
/// quoted line on the screen is UPSC's; everything else is our breakdown.
fn quote(text: &str, size: f32, color: Hsla) -> Div {
    div()
        .font_family(ui::SERIF)
        .text_size(px(size))
        .line_height(relative(1.45))
        .text_color(color)
        .child(format!("“{}”", text.trim()))
}

/// Marks a topic by its status, the way a student marks a printed syllabus:
/// read topics highlighted in green, those under way in the accent, those
/// with a note not opened yet underlined, and the rest plain.
fn marked<E: Styled>(element: E, status: Status, t: &ui::Tokens) -> E {
    match status {
        Status::Done => element.bg(t.success.opacity(0.16)).text_color(t.text),
        Status::Reading => element.bg(t.accent.opacity(0.14)).text_color(t.text),
        Status::NotStarted => element
            .text_color(t.text)
            .underline()
            .text_decoration_color(t.text_3),
        Status::NoNotes => element.text_color(t.text_2),
    }
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

/// A centred column on the sheet.
fn column() -> Div {
    v_flex().w_full().max_w(px(880.)).mx_auto().px_8()
}

/// Below this width the sheet puts each chapter's title above its topics,
/// not beside them.
const NARROW: Pixels = px(720.);

/// What the parts of the sheet need to draw themselves.
struct Sheet<'a> {
    library: &'a Library,
    reading: &'a HashMap<String, Reading>,
    statuses: &'a [Status],
    /// Each topic's children in syllabus order, by the parent's id.
    children: HashMap<&'a str, Vec<usize>>,
    shell: WeakEntity<Shell>,
    narrow: bool,
    t: ui::Tokens,
}

impl<'a> Sheet<'a> {
    fn new(
        library: &'a Library,
        reading: &'a HashMap<String, Reading>,
        statuses: &'a [Status],
        shell: WeakEntity<Shell>,
        narrow: bool,
        cx: &App,
    ) -> Self {
        let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
        for (ix, topic) in library.topics.iter().enumerate() {
            if let Some((parent, _)) = topic.id.rsplit_once('.') {
                children.entry(parent).or_default().push(ix);
            }
        }
        Sheet {
            library,
            reading,
            statuses,
            children,
            shell,
            narrow,
            t: ui::tokens(cx),
        }
    }

    fn kids(&self, ix: usize) -> &[usize] {
        self.children
            .get(self.library.topics[ix].id.as_str())
            .map_or(&[], Vec::as_slice)
    }

    /// One topic. One with a note opens the first note on it not read yet.
    fn topic(&self, ix: usize) -> impl IntoElement + use<> {
        let t = self.t;
        let topic = &self.library.topics[ix];
        let status = self.statuses[ix];
        let note = (status != Status::NoNotes)
            .then(|| next_note(self.library, self.reading, &topic.id))
            .flatten();
        let shell = self.shell.clone();
        let id = topic.id.clone();
        marked(
            div()
                .id(("topic", ix))
                .test_support()
                // GPUI measures text unwrapped, so without this a long
                // title runs past the edge instead of wrapping.
                .min_w_0()
                .px(px(3.))
                .rounded(px(3.))
                .child(topic.title.clone()),
            status,
            &t,
        )
        .when_some(note, |this, note| {
            this.cursor_pointer()
                .hover(|this| this.underline().text_decoration_color(t.text))
                // Not a listener: the shell may update this screen.
                .on_click(move |_, _, cx| {
                    let _ = shell.update(cx, |shell, cx| shell.open(Target::Note(note), cx));
                })
        })
        // Worked out on hover: a paper has hundreds of topics.
        .tooltip(move |window, cx| {
            let state = AppState::get(cx);
            Tooltip::new(describe(&state.library, &state.reading, &id)).build(window, cx)
        })
    }

    /// Topics running on as one line, split by dots. A topic with subtopics
    /// leads its own line with its title.
    fn flow(&self, lead: Option<usize>, items: &[usize]) -> AnyElement {
        let t = self.t;
        let last = items.len().saturating_sub(1);
        h_flex()
            .flex_wrap()
            .items_start()
            .gap_y(px(3.))
            .when_some(lead, |this, lead| {
                this.child(
                    div()
                        .min_w_0()
                        .pl(px(3.))
                        .pr(px(3.))
                        .text_color(t.text_3)
                        .child(format!("{}:", self.library.topics[lead].title)),
                )
            })
            .children(items.iter().enumerate().map(|(n, &ix)| {
                h_flex()
                    .items_start()
                    .min_w_0()
                    .child(self.topic(ix))
                    .when(n < last, |this| {
                        this.child(div().px(px(3.)).text_color(t.text_4).child("·"))
                    })
            }))
            .into_any_element()
    }

    /// Topics in syllabus order: each run of plain topics is one flowing
    /// line, and each topic with subtopics is a line of its own.
    fn lines(&self, topics: &[usize]) -> Vec<AnyElement> {
        let mut lines = Vec::new();
        let mut run = Vec::new();
        for &ix in topics {
            if self.library.topics[ix].leaf {
                run.push(ix);
                continue;
            }
            if !run.is_empty() {
                lines.push(self.flow(None, &run));
                run.clear();
            }
            lines.push(self.flow(Some(ix), self.kids(ix)));
        }
        if !run.is_empty() {
            lines.push(self.flow(None, &run));
        }
        lines
    }

    /// A row of the sheet: a chapter's title and count on the left, its
    /// topics on the right, or above them when the sheet is narrow. Topics
    /// that sit straight under the subject have no chapter to show.
    fn row(&self, chapter: Option<usize>, topics: &[usize], cx: &App) -> AnyElement {
        let t = self.t;
        let label = chapter.map(|ix| {
            let topic = &self.library.topics[ix];
            let (done, total) = coverage(self.library, self.statuses, &topic.id);
            let title = div()
                .min_w_0()
                .text_size(px(13.))
                .line_height(px(20.))
                .font_medium()
                .text_color(t.text)
                .child(topic.title.clone());
            (title, ui::meta(format!("{done} of {total}"), cx))
        });
        let row = if self.narrow {
            v_flex()
                .gap(px(4.))
                .when_some(label, |this, (title, count)| {
                    this.child(
                        h_flex()
                            .gap(px(8.))
                            .child(title)
                            .child(count.flex_shrink_0()),
                    )
                })
        } else {
            h_flex().items_start().gap(px(20.)).child(
                v_flex()
                    .w(px(184.))
                    .flex_shrink_0()
                    .gap(px(2.))
                    .when_some(label, |this, (title, count)| this.child(title).child(count)),
            )
        };
        row.py(px(10.))
            .border_b_1()
            .border_color(t.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .children(self.lines(topics)),
            )
            .into_any_element()
    }

    /// A subject: UPSC's line above it when it starts one, its heading with
    /// how much of it is read, then its chapters. A line of UPSC's that
    /// covers some of the chapters sits above them.
    fn subject(&self, ix: usize, line: Option<&str>, cx: &App) -> AnyElement {
        let t = self.t;
        let topic = &self.library.topics[ix];
        let (done, total) = coverage(self.library, self.statuses, &topic.id);
        let color = ui::subject_color(&topic.id, cx);
        // A subject with nothing under it is a topic, so it is marked as one.
        let title = if topic.leaf {
            self.topic(ix).into_any_element()
        } else {
            div().child(topic.title.clone()).into_any_element()
        };

        let mut rows = Vec::new();
        let mut run = Vec::new();
        let mut said: Option<&str> = None;
        for &kid in self.kids(ix) {
            let chapter = &self.library.topics[kid];
            let official = chapter.official.as_deref();
            if let Some(official) = official.filter(|&official| Some(official) != said) {
                if !run.is_empty() {
                    rows.push(self.row(None, &run, cx));
                    run.clear();
                }
                rows.push(
                    quote(official, 15., t.text)
                        .pt(px(16.))
                        .pb(px(4.))
                        .into_any_element(),
                );
            }
            said = official;
            if chapter.leaf {
                run.push(kid);
            } else {
                if !run.is_empty() {
                    rows.push(self.row(None, &run, cx));
                    run.clear();
                }
                rows.push(self.row(Some(kid), self.kids(kid), cx));
            }
        }
        if !run.is_empty() {
            rows.push(self.row(None, &run, cx));
        }

        v_flex()
            .pt(px(36.))
            .when_some(line, |this, line| {
                this.child(quote(line, 17., t.text).pb(px(14.)))
            })
            .child(
                h_flex()
                    .id(("heading", ix))
                    .test_support()
                    .gap(px(10.))
                    .pb(px(10.))
                    .border_b_1()
                    .border_color(t.border_3)
                    .child(ui::dot(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(15.))
                            .font_semibold()
                            .text_color(t.text)
                            .child(title),
                    )
                    .child(ui::meta(format!("{done} of {total} read"), cx).flex_shrink_0())
                    .child(div().w(px(64.)).flex_shrink_0().child(ui::progress(
                        percent((done, total)) / 100.,
                        color,
                        cx,
                    ))),
            )
            .children(rows)
            .into_any_element()
    }
}

/// The syllabus a paper at a time, beside a map of the whole exam.
pub struct SyllabusScreen {
    shell: WeakEntity<Shell>,
    /// The paper on the sheet, by index into the library's topics; the first
    /// paper until the student picks one.
    paper: Option<usize>,
    /// The sheet's scroll, so that a paper opens at its top and the subject
    /// list can jump down to a subject.
    scroll: ScrollHandle,
}

impl SyllabusScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        SyllabusScreen {
            shell,
            paper: None,
            scroll: ScrollHandle::new(),
        }
    }

    fn open_paper(&mut self, paper: usize, cx: &mut Context<Self>) {
        self.paper = Some(paper);
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// The exam as UPSC lays it out: each stage in its own words, then its
    /// papers, with their marks and how much of each is read.
    fn map(
        &self,
        paper: usize,
        statuses: &[Status],
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = ui::tokens(cx);
        let library = &AppState::get(cx).library;
        let rows = library
            .topics
            .iter()
            .enumerate()
            .filter(|(_, topic)| topic.depth <= 1)
            .map(|(ix, topic)| {
                if topic.depth == 0 {
                    return v_flex()
                        .gap(px(4.))
                        .px(px(8.))
                        .pt(px(16.))
                        .pb(px(6.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_semibold()
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .when_some(topic.official.as_deref(), |this, official| {
                            this.child(quote(official, 12.5, t.text_3))
                        })
                        .into_any_element();
                }
                let (done, total) = coverage(library, statuses, &topic.id);
                let chosen = ix == paper;
                let tip = format!("{done} of {total} topics read");
                h_flex()
                    .id(("paper", ix))
                    .test_support()
                    .items_start()
                    .gap(px(9.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .map(|this| {
                        if chosen {
                            this.bg(t.accent_soft)
                        } else {
                            this.hover(|this| this.bg(t.hover))
                        }
                    })
                    .child(div().pt(px(1.)).child(ui::ring(
                        percent((done, total)) / 100.,
                        16.,
                        2.5,
                        t.accent,
                        div(),
                        cx,
                    )))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .text_color(t.text)
                                    .when(chosen, |this| this.font_medium())
                                    .child(topic.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(t.text_3)
                                    .child(facts(topic.official.as_deref().unwrap_or_default())),
                            ),
                    )
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_paper(ix, cx)))
                    .into_any_element()
            });
        v_flex()
            .w(px(256.))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(t.border)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .id("exam-map")
                    .child(
                        v_flex().px(px(10.)).pb_4().children(rows).child(
                            // ponytail: content/syllabus.toml leaves them out
                            // for now; drop this line when they are added.
                            ui::meta(
                                "Papers VI and VII, the optional subject, are not in the app yet.",
                                cx,
                            )
                            .px(px(8.))
                            .pt(px(12.)),
                        ),
                    ),
            )
    }

    /// The open paper: its title and UPSC's line for it, a list of its
    /// subjects to jump to, then each subject in full. It is laid out for
    /// the width it gets, which only the layout knows.
    fn sheet(&self, paper: usize, statuses: Vec<Status>) -> impl IntoElement + use<> {
        let (shell, scroll) = (self.shell.clone(), self.scroll.clone());
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(container_query({
                let scroll = scroll.clone();
                move |size, _, cx| {
                    sheet_body(paper, &statuses, shell, scroll, size.width < NARROW, cx)
                }
            }))
            .vertical_scrollbar(&scroll)
    }
}

/// The sheet itself, for [`SyllabusScreen::sheet`].
fn sheet_body(
    paper: usize,
    statuses: &[Status],
    shell: WeakEntity<Shell>,
    scroll: ScrollHandle,
    narrow: bool,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let state = AppState::get(cx);
    let library = &state.library;
    let sheet = Sheet::new(library, &state.reading, statuses, shell, narrow, cx);
    let t = sheet.t;
    let topic = &library.topics[paper];
    let (done, total) = coverage(library, statuses, &topic.id);
    let subjects = sheet.kids(paper);

    // A subject with nothing under it needs no jump: it is a line long.
    let jumps: Vec<AnyElement> = subjects
        .iter()
        .enumerate()
        .filter(|(_, subject)| !library.topics[**subject].leaf)
        .map(|(n, &ix)| {
            let subject = &library.topics[ix];
            let scroll = scroll.clone();
            h_flex()
                .id(("jump", ix))
                .test_support()
                .h(px(26.))
                .px(px(8.))
                .gap(px(6.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.5))
                .text_color(t.text_2)
                .hover(|this| this.bg(t.hover).text_color(t.text))
                .child(ui::dot(ui::subject_color(&subject.id, cx)))
                .child(subject.title.clone())
                .on_click(move |_, window, _| {
                    // The heading is the sheet's first child, then a child per subject.
                    scroll.scroll_to_top_of_item(n + 1);
                    window.refresh();
                })
                .into_any_element()
        })
        .collect();
    let legend = h_flex()
        .flex_wrap()
        .gap_x(px(14.))
        .gap_y(px(4.))
        .text_size(px(12.))
        .child(quote("UPSC's own words", 13., t.text))
        .children(
            [
                Status::Done,
                Status::Reading,
                Status::NotStarted,
                Status::NoNotes,
            ]
            .map(|status| {
                marked(div().px(px(3.)).rounded(px(3.)), status, &t).child(status.label())
            }),
        );
    let head = column()
        .pt(px(28.))
        .gap(px(16.))
        .child(
            h_flex()
                .items_start()
                .gap(px(16.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(8.))
                        .child(
                            div()
                                .font_family(ui::DISPLAY)
                                .text_size(px(26.))
                                .line_height(px(32.))
                                .font_semibold()
                                .text_color(t.text)
                                .child(topic.title.clone()),
                        )
                        .when_some(topic.official.as_deref(), |this, official| {
                            this.child(quote(official, 16., t.text_2))
                        }),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap(px(10.))
                        .pt(px(2.))
                        .child(
                            v_flex()
                                .items_end()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_medium()
                                        .text_color(t.text)
                                        .child(format!("{:.0}% read", percent((done, total)))),
                                )
                                .child(ui::meta(format!("{done} of {total} topics"), cx)),
                        )
                        .child(ui::ring(
                            percent((done, total)) / 100.,
                            36.,
                            4.,
                            t.accent,
                            div(),
                            cx,
                        )),
                ),
        )
        .when(jumps.len() > 2, |this| {
            this.child(h_flex().flex_wrap().gap(px(2.)).ml(px(-8.)).children(jumps))
        })
        .child(legend);

    // Subjects that share a line of UPSC's follow one another; the first
    // of them carries it.
    let mut said: Option<&str> = None;
    let blocks: Vec<AnyElement> = subjects
        .iter()
        .map(|&subject| {
            let official = library.topics[subject].official.as_deref();
            let line = official.filter(|&official| Some(official) != said);
            said = official;
            column()
                .child(sheet.subject(subject, line, cx))
                .into_any_element()
        })
        .collect();
    let foot = column().pt(px(28.)).pb(px(48.)).child(ui::meta(
        "Quoted lines are UPSC's own wording, from the official exam notice. The chapters and \
             topics under them are Civil Mentor's breakdown.",
        cx,
    ));

    div()
        .id("sheet")
        .size_full()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .child(head)
        .children(blocks)
        .child(foot)
}

impl Render for SyllabusScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let statuses = statuses(library, &state.reading);
        let (done, total) = library
            .topics
            .iter()
            .filter(|topic| topic.depth == 0)
            .map(|stage| coverage(library, &statuses, &stage.id))
            .fold((0, 0), |(done, total), (d, t)| (done + d, total + t));
        let header = ui::header(cx)
            .child(ui::title(IconName::ListTree, "Syllabus", cx))
            .child(div().flex_1())
            .child(
                h_flex()
                    .gap(px(8.))
                    .pr_1()
                    .child(ui::ring(
                        percent((done, total)) / 100.,
                        18.,
                        3.,
                        t.accent,
                        div(),
                        cx,
                    ))
                    .child(ui::meta(format!("{done} of {total} topics read"), cx)),
            );
        let paper = self
            .paper
            .filter(|&ix| library.topics.get(ix).is_some_and(|topic| topic.depth == 1))
            .or_else(|| library.topics.iter().position(|topic| topic.depth == 1));
        let body = match paper {
            Some(paper) => div()
                .flex()
                .size_full()
                .child(self.map(paper, &statuses, cx))
                .child(self.sheet(paper, statuses))
                .into_any_element(),
            None => column()
                .pt_6()
                .child(ui::meta(
                    "The syllabus did not load. Settings lists what is wrong with the content.",
                    cx,
                ))
                .into_any_element(),
        };
        v_flex()
            .size_full()
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in `gpui_kit::test`, which shadows `#[test]`.
    use super::{Status, coverage, facts, percent, statuses};
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
}

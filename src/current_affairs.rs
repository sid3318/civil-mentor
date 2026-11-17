//! The Current affairs screen: a day's brief with the date bar and a
//! calendar, its stories grouped by subject and its editorials, the story
//! reader in Full and Rapid modes, the daily quiz, and the monthly issue
//! with its own quiz.

use chrono::{Datelike as _, NaiveDate};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, StyledExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::{Screen, Shell};
use crate::content::{Brief, Story, Target};
use crate::exam::LETTERS;
use crate::marks::{Blocks, Marking, Selection};
use crate::mentor::{self, SUBJECTS, TestKind, TestPlan};
use crate::notes::ReadingClock;
use crate::review::marks;
use crate::screens::count;
use crate::store::{Attempt, MarkKind, NewMark, Study};
use crate::ui;

/// The reading column, as wide as the notes reader's narrow one.
const COLUMN: f32 = 720.;

/// What the reader pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The day at a glance, before a story is opened.
    Day,
    Story(usize),
    Editorial(usize),
    /// The monthly issue for the month of the brief on screen.
    Month,
}

pub struct CurrentAffairsScreen {
    shell: WeakEntity<Shell>,
    /// The brief on screen, by index into the library. `None` only when
    /// there are no briefs at all.
    brief: Option<usize>,
    view: View,
    rapid: bool,
    calendar: bool,
    /// Whether the open story's practice answer is showing.
    answer: bool,
    scroll: ScrollHandle,
    clock: Option<ReadingClock>,
    /// The open story or editorial, as text views a highlight can be made on.
    blocks: Blocks,
    /// The reading key of the item those blocks belong to.
    marked: Option<String>,
    /// The selection waiting for a highlight's comment.
    marking: Marking,
    /// The last passage the student selected in the item.
    selected: Option<Selection>,
}

/// The story as the parts the reader marks, in the order it shows them.
fn story_parts(story: &Story) -> Vec<(&'static str, String)> {
    let mut parts = vec![
        ("context", story.context.clone()),
        ("why", story.why.clone()),
    ];
    if !story.facts.is_empty() {
        let facts: Vec<String> = story.facts.iter().map(|fact| format!("- {fact}")).collect();
        parts.push(("facts", facts.join("\n")));
    }
    if let Some(trap) = &story.trap {
        parts.push(("trap", trap.clone()));
    }
    if let Some(pyq) = &story.pyq {
        parts.push(("pyq", pyq.clone()));
    }
    if let Some(mains) = &story.mains {
        parts.push(("mains", mains.clone()));
    }
    parts
}

/// The subject a story is listed under: the first of its topics in a
/// subject other than current affairs, which every story is.
fn subject_of(story: &Story) -> &'static str {
    story
        .topics
        .iter()
        .filter_map(|topic| mentor::subject_of_topic(topic))
        .find(|subject| subject.id != "current")
        .map_or("Other", |subject| subject.name)
}

/// Items grouped by the subject of their story, in [`SUBJECTS`] order.
fn by_subject<T>(
    items: impl IntoIterator<Item = (T, &'static str)>,
) -> Vec<(&'static str, Vec<T>)> {
    let mut groups: Vec<(&'static str, Vec<T>)> = Vec::new();
    for (item, subject) in items {
        match groups.iter_mut().find(|(name, _)| *name == subject) {
            Some((_, list)) => list.push(item),
            None => groups.push((subject, vec![item])),
        }
    }
    let rank = |name: &str| {
        SUBJECTS
            .iter()
            .position(|s| s.name == name)
            .unwrap_or(SUBJECTS.len())
    };
    groups.sort_by_key(|(name, _)| rank(name));
    groups
}

/// The order a day's stories are read in: the list's, by subject.
fn reading_order(day: &Brief) -> Vec<usize> {
    by_subject(
        day.stories
            .iter()
            .enumerate()
            .map(|(ix, story)| (ix, subject_of(story))),
    )
    .into_iter()
    .flat_map(|(_, stories)| stories)
    .collect()
}

/// Finished sittings of one kind of test, newest first.
pub(crate) fn sittings(kind: &TestKind, cx: &App) -> Vec<Attempt> {
    AppState::get(cx)
        .store
        .finished_kind(&kind.key())
        .unwrap_or_default()
}

/// "4 of 5 right, 7.33 of 10 marks".
pub(crate) fn quiz_score(test: &Attempt) -> String {
    let score = mentor::score(test);
    format!(
        "{} of {} right, {} of {} marks",
        score.right,
        test.answers.len(),
        marks(score.marks),
        marks(mentor::full_marks(test))
    )
}

/// A section label in the story reader: "Context", "Key facts".
fn heading(text: &str, cx: &App) -> Div {
    div()
        .pt_6()
        .pb(px(6.))
        .text_size(px(12.))
        .font_semibold()
        .text_color(ui::tokens(cx).text_3)
        .child(text.to_string())
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(13.))
        .text_color(ui::tokens(cx).text_3)
        .child(text.into())
}

fn bullet(text: &str) -> Div {
    h_flex()
        .items_start()
        .gap_2()
        .child(div().flex_shrink_0().child("•"))
        .child(div().min_w_0().child(text.to_string()))
}

impl CurrentAffairsScreen {
    pub fn new(shell: WeakEntity<Shell>, cx: &App) -> Self {
        let today = AppState::today(cx);
        CurrentAffairsScreen {
            shell,
            brief: AppState::get(cx).library.todays_brief(today),
            view: View::Day,
            rapid: false,
            calendar: false,
            answer: false,
            scroll: ScrollHandle::new(),
            clock: None,
            blocks: Blocks::default(),
            marked: None,
            marking: Marking::default(),
            selected: None,
        }
    }

    /// The brief on screen, by index into the library.
    pub fn brief(&self) -> Option<usize> {
        self.brief
    }

    /// The story or editorial open in the reader.
    pub fn selected(&self) -> Option<Target> {
        let brief = self.brief?;
        match self.view {
            View::Story(story) => Some(Target::Story { brief, story }),
            View::Editorial(editorial) => Some(Target::Editorial { brief, editorial }),
            View::Day | View::Month => None,
        }
    }

    pub fn rapid(&self) -> bool {
        self.rapid
    }

    /// Shows the day at a glance, or the monthly issue if it is open.
    /// Called by the shell when the section is opened with nothing selected.
    pub fn show_day(&mut self, cx: &mut Context<Self>) {
        self.stop_clock(cx);
        if matches!(self.view, View::Story(_) | View::Editorial(_)) {
            self.view = View::Day;
            self.scroll.set_offset(Point::default());
        }
        cx.notify();
    }

    /// Moves to another day's brief, at a glance.
    pub fn set_brief(&mut self, brief: usize, cx: &mut Context<Self>) {
        self.stop_clock(cx);
        self.brief = Some(brief);
        self.view = View::Day;
        self.calendar = false;
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// Opens a story or editorial, records that it was read and starts
    /// counting the time. Called by the shell, which keeps the history.
    pub fn show(&mut self, target: Target, cx: &mut Context<Self>) {
        let library = &AppState::get(cx).library;
        let (brief, view, key) = match target {
            Target::Story { brief, story } => (
                brief,
                View::Story(story),
                library.briefs[brief].stories[story].reading_key(),
            ),
            Target::Editorial { brief, editorial } => (
                brief,
                View::Editorial(editorial),
                library.briefs[brief].editorials[editorial].reading_key(),
            ),
            Target::Note(_) | Target::Question(_) => return,
        };
        if self.brief != Some(brief) || self.view != view {
            self.brief = Some(brief);
            self.view = view;
            self.answer = false;
            self.scroll.set_offset(Point::default());
        }
        self.rapid = false;
        AppState::update_reading(cx, &key, |reading, today| {
            reading.last = today;
            reading.done.get_or_insert(today);
        });
        if self.clock.as_ref().is_none_or(|clock| clock.item != key) {
            self.stop_clock(cx);
            let offset = self.scroll.offset();
            self.clock = Some(ReadingClock::start(
                key,
                Study::Story,
                offset,
                cx,
                Self::tick,
            ));
        }
        cx.notify();
    }

    /// Stops counting reading time and saves what was counted.
    pub fn stop_clock(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.save(cx);
        }
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

    fn touch(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        if let Some(clock) = &mut self.clock {
            clock.touch(now);
        }
    }

    /// A click handler that opens `target` through the shell, so that back
    /// and forward know about it. Not a listener: the shell updates this
    /// screen when it opens the target.
    fn opener(&self, target: Target) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let shell = self.shell.clone();
        move |_, _, cx| {
            let _ = shell.update(cx, |shell, cx| shell.open(target, cx));
        }
    }

    /// A click handler that moves to another brief and makes the section,
    /// not a story, the current place.
    fn go_to_brief(
        &self,
        brief: usize,
        cx: &Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let (this, shell) = (cx.weak_entity(), self.shell.clone());
        move |_, _, cx| {
            let _ = this.update(cx, |this, cx| this.set_brief(brief, cx));
            let _ = shell.update(cx, |shell, cx| shell.go(Screen::CurrentAffairs, cx));
        }
    }

    /// A click handler that sits a quiz in the exam room.
    fn sitter(&self, kind: TestKind) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let shell = self.shell.clone();
        move |_, _, cx| {
            let plan = TestPlan::standard(kind.clone(), &AppState::get(cx).library);
            let _ = shell.update(cx, |shell, cx| shell.sit(&plan, cx));
        }
    }

    fn result_opener(&self, id: i64) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let shell = self.shell.clone();
        move |_, _, cx| {
            let _ = shell.update(cx, |shell, cx| shell.open_result(id, cx));
        }
    }

    /// Full or Rapid reading of the day's stories.
    fn set_mode(&mut self, rapid: bool, cx: &mut Context<Self>) {
        self.rapid = rapid;
        if self.view == View::Month {
            self.view = View::Day;
        }
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// The header: the day with the days either side, the calendar, Full
    /// or Rapid, the month's issue, and how much of the day is read.
    fn date_bar(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let day = &library.briefs[brief];
        let read = day
            .stories
            .iter()
            .filter(|story| mentor::story_read(&state.reading, story))
            .count();
        let month = self.view == View::Month;
        let older = brief + 1 < library.briefs.len();
        let rapid = self.rapid;
        h_flex()
            .flex_1()
            .gap_1()
            .child(
                Button::new("older-brief")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::ChevronLeft))
                    .tooltip("The day before")
                    .disabled(!older)
                    .when(older, |this| this.on_click(self.go_to_brief(brief + 1, cx))),
            )
            .child(
                div()
                    .id("brief-date")
                    .test_support()
                    .px_1()
                    .text_size(px(13.5))
                    .font_semibold()
                    .child(day.date.format("%a, %-d %b %Y").to_string()),
            )
            .child(
                Button::new("newer-brief")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::ChevronRight))
                    .tooltip("The day after")
                    .disabled(brief == 0)
                    .when(brief > 0, |this| {
                        this.on_click(self.go_to_brief(brief - 1, cx))
                    }),
            )
            .child(
                Button::new("calendar")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Calendar))
                    .tooltip("Every day's brief")
                    .selected(self.calendar)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.calendar = !this.calendar;
                        cx.notify();
                    })),
            )
            .child(
                ui::segmented(cx)
                    .ml_2()
                    .child(
                        ui::segment("full-mode", "Full", !rapid, cx)
                            .on_click(cx.listener(|this, _, _, cx| this.set_mode(false, cx))),
                    )
                    .child(
                        ui::segment("rapid-mode", "Rapid", rapid, cx)
                            .on_click(cx.listener(|this, _, _, cx| this.set_mode(true, cx))),
                    ),
            )
            .child(
                Button::new("monthly-issue")
                    .ghost()
                    .small()
                    .ml_1()
                    .icon(Icon::new(IconName::Newspaper))
                    .label(format!("{} issue", day.date.format("%B")))
                    .selected(month)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view = if month { View::Day } else { View::Month };
                        this.scroll.set_offset(Point::default());
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .id("brief-count")
                    .test_support()
                    .gap(px(8.))
                    .pr_1()
                    .child(ui::ring(
                        read as f32 / day.stories.len().max(1) as f32,
                        18.,
                        3.,
                        t.accent,
                        div(),
                        cx,
                    ))
                    .child(ui::meta(
                        format!(
                            "{read} of {} read",
                            count(day.stories.len(), "story", "stories")
                        ),
                        cx,
                    )),
            )
    }

    /// Every month that has briefs, newest first, with the days that have
    /// one to click.
    fn calendar(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let library = &AppState::get(cx).library;
        let mut months: Vec<(i32, u32)> = library
            .briefs
            .iter()
            .map(|b| (b.date.year(), b.date.month()))
            .collect();
        months.dedup();
        let selected = library.briefs[brief].date;
        let theme = cx.theme();
        let grids: Vec<Div> = months
            .into_iter()
            .map(|(year, month)| {
                let first = NaiveDate::from_ymd_opt(year, month, 1).expect("a brief's month");
                let blank = first.weekday().num_days_from_monday() as usize;
                let mut cells: Vec<AnyElement> = (0..blank)
                    .map(|_| div().size(px(30.)).into_any_element())
                    .collect();
                for date in (1..=31).filter_map(|d| NaiveDate::from_ymd_opt(year, month, d)) {
                    let cell = div()
                        .size(px(30.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radius)
                        .text_xs()
                        .child(date.day().to_string());
                    cells.push(match library.brief_on(date) {
                        Some(ix) => cell
                            .id(("calendar-day", date.num_days_from_ce() as u64))
                            .test_support()
                            .cursor_pointer()
                            .font_semibold()
                            .bg(theme.accent)
                            .when(date == selected, |this| {
                                this.border_2().border_color(theme.ring)
                            })
                            .on_click(self.go_to_brief(ix, cx))
                            .into_any_element(),
                        None => cell.text_color(theme.muted_foreground).into_any_element(),
                    });
                }
                let weekdays = ["M", "T", "W", "T", "F", "S", "S"].map(|d| {
                    div()
                        .w(px(30.))
                        .text_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(d)
                });
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .child(first.format("%B %Y").to_string()),
                    )
                    .child(h_flex().gap_1().children(weekdays))
                    .child(
                        div()
                            .w(px(30. * 7. + 4. * 6.))
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .children(cells),
                    )
            })
            .collect();
        h_flex()
            .id("brief-calendar")
            .test_support()
            .flex_shrink_0()
            .items_start()
            .gap_8()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .children(grids)
            .child(muted(
                "Days with a brief are shaded. Pick one to read it.",
                cx,
            ))
    }

    /// The day's stories by subject, its editorials and its quiz.
    fn list(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let t = ui::tokens(cx);
        let state = AppState::get(cx);
        let day = &state.library.briefs[brief];
        let row = |id: ElementId, title: &str, read: bool, selected: bool, target: Target| {
            h_flex()
                .id(id)
                .test_support()
                .items_start()
                .gap(px(9.))
                .px(px(8.))
                .py(px(7.))
                .rounded(px(6.))
                .text_size(px(13.))
                .cursor_pointer()
                .hover(|this| this.bg(t.hover))
                .when(selected, |this| this.bg(t.accent_soft).font_medium())
                .child(
                    div().pt(px(1.)).child(
                        Icon::new(if read {
                            IconName::CircleCheck
                        } else {
                            IconName::Circle
                        })
                        .size(px(15.))
                        .text_color(if read {
                            t.success
                        } else {
                            t.text_4
                        }),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(title.to_string()),
                )
                .on_click(self.opener(target))
        };
        let groups = by_subject(
            day.stories
                .iter()
                .enumerate()
                .map(|(ix, story)| (ix, subject_of(story))),
        );
        let header = |text: String, color: Option<Hsla>| {
            h_flex()
                .gap(px(7.))
                .px(px(8.))
                .pt(px(14.))
                .pb(px(4.))
                .when_some(color, |this, color| this.child(ui::dot(color)))
                .child(ui::caption(text, cx))
        };
        let kind = TestKind::Quiz(day.date);
        let questions = state.library.brief_questions(day).len();
        let last = sittings(&kind, cx).into_iter().next();
        v_flex()
            .w(px(280.))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(t.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .px(px(8.))
                    .pb_2()
                    .overflow_y_scrollbar()
                    .id("ca-list")
                    .children(groups.into_iter().map(|(subject, stories)| {
                        let color = stories
                            .first()
                            .and_then(|&ix| {
                                day.stories[ix]
                                    .topics
                                    .iter()
                                    .find(|topic| !topic.starts_with("pre.gs.current"))
                            })
                            .map(|topic| ui::subject_color(topic, cx));
                        v_flex().child(header(subject.to_string(), color)).children(
                            stories.into_iter().map(|ix| {
                                let story = &day.stories[ix];
                                row(
                                    ("story", ix).into(),
                                    &story.title,
                                    mentor::story_read(&state.reading, story),
                                    self.view == View::Story(ix),
                                    Target::Story { brief, story: ix },
                                )
                            }),
                        )
                    }))
                    .when(!day.editorials.is_empty(), |list| {
                        list.child(header(
                            format!("Editorials ({})", day.editorials.len()),
                            None,
                        ))
                        .children(day.editorials.iter().enumerate().map(|(ix, editorial)| {
                            let read = state
                                .reading
                                .get(&editorial.reading_key())
                                .is_some_and(|r| r.done.is_some());
                            row(
                                ("editorial", ix).into(),
                                &editorial.title,
                                read,
                                self.view == View::Editorial(ix),
                                Target::Editorial {
                                    brief,
                                    editorial: ix,
                                },
                            )
                        }))
                    }),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .p(px(12.))
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(t.border)
                    .child(
                        Button::new("take-quiz")
                            .primary()
                            .small()
                            .w_full()
                            .label(format!("Take the day's quiz · {questions} Q"))
                            .disabled(questions == 0)
                            .on_click(self.sitter(kind)),
                    )
                    .when_some(last, |this, test| {
                        this.child(
                            div()
                                .id("quiz-last")
                                .test_support()
                                .text_size(px(12.))
                                .text_color(t.text_3)
                                .cursor_pointer()
                                .hover(|this| this.text_color(t.accent))
                                .child(format!("Last time: {}. Review it", quiz_score(&test)))
                                .on_click(self.result_opener(test.id)),
                        )
                    }),
            )
    }

    fn reader(&mut self, brief: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match self.view {
            View::Month => self.month(brief, cx).into_any_element(),
            _ if self.rapid => self.rapid_page(brief, cx).into_any_element(),
            View::Story(story) => self.story(brief, story, cx).into_any_element(),
            View::Editorial(editorial) => self.editorial(brief, editorial, cx).into_any_element(),
            View::Day => self.day(brief, cx).into_any_element(),
        };
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .id("ca-reader")
                    .test_support()
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .w_full()
                            .max_w(px(COLUMN))
                            .mx_auto()
                            .px_8()
                            .child(page),
                    ),
            )
            .vertical_scrollbar(&self.scroll)
    }

    /// The day at a glance: what is in it and where to start.
    fn day(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let state = AppState::get(cx);
        let day = &state.library.briefs[brief];
        let order = reading_order(day);
        let unread = order
            .iter()
            .copied()
            .find(|&ix| !mentor::story_read(&state.reading, &day.stories[ix]));
        v_flex()
            .id("brief-day")
            .test_support()
            .py_8()
            .gap_4()
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .child(format!("The brief for {}", day.date.format("%A, %-d %B %Y"))),
            )
            .child(muted(
                format!(
                    "{} and {}. Each story has what to know for Prelims and a practice question.",
                    count(day.stories.len(), "story", "stories"),
                    count(day.editorials.len(), "editorial", "editorials")
                ),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .when(!day.stories.is_empty(), |this| {
                        this.child(
                            Button::new("start-reading")
                                .primary()
                                .label(if unread.is_some() {
                                    "Read the next story"
                                } else {
                                    "Read it again"
                                })
                                .on_click(self.opener(Target::Story {
                                    brief,
                                    story: unread.unwrap_or(order[0]),
                                })),
                        )
                    })
                    .child(
                        Button::new("day-quiz")
                            .outline()
                            .label("Take the day's quiz")
                            .disabled(state.library.brief_questions(day).is_empty())
                            .on_click(self.sitter(TestKind::Quiz(day.date))),
                    ),
            )
            .child(
                v_flex()
                    .pt_4()
                    .gap_3()
                    .children(order.iter().map(|&ix| {
                        let story = &day.stories[ix];
                        v_flex()
                            .id(("day-story", ix))
                            .test_support()
                            .gap_1()
                            .p_3()
                            .rounded(cx.theme().radius_lg)
                            .border_1()
                            .border_color(cx.theme().border)
                            .cursor_pointer()
                            .hover(|this| this.bg(cx.theme().accent))
                            .child(muted(subject_of(story), cx))
                            .child(div().font_medium().child(story.title.clone()))
                            .child(div().text_sm().child(story.context.clone()))
                            .on_click(self.opener(Target::Story { brief, story: ix }))
                    })),
            )
            .child(muted(
                "Sample brief: the stories are illustrative, written to show the format. They are not real news.",
                cx,
            ))
    }

    /// Rapid mode: every story's title and key facts, one after another.
    fn rapid_page(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let state = AppState::get(cx);
        let day = &state.library.briefs[brief];
        let keys: Vec<String> = day.stories.iter().map(Story::reading_key).collect();
        let all_read = day
            .stories
            .iter()
            .all(|story| mentor::story_read(&state.reading, story));
        v_flex()
            .id("rapid")
            .test_support()
            .py_8()
            .gap_5()
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .child(format!("Rapid read · {}", day.date.format("%-d %B %Y"))),
            )
            .children(reading_order(day).into_iter().map(|ix| {
                let story = &day.stories[ix];
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .id(("rapid-story", ix))
                            .test_support()
                            .font_semibold()
                            .cursor_pointer()
                            .hover(|this| this.underline())
                            .child(story.title.clone())
                            .on_click(self.opener(Target::Story { brief, story: ix })),
                    )
                    .children(story.facts.iter().map(|fact| bullet(fact).text_sm()))
            }))
            .child(h_flex().pt_2().pb_8().child(if all_read {
                muted("Every story in this brief is read.", cx).into_any_element()
            } else {
                Button::new("rapid-mark-read")
                    .outline()
                    .icon(Icon::new(IconName::Check))
                    .label("Mark them all read")
                    .on_click(move |_, _, cx| {
                        for key in &keys {
                            AppState::update_reading(cx, key, |reading, today| {
                                reading.last = today;
                                reading.done.get_or_insert(today);
                            });
                        }
                    })
                    .into_any_element()
            }))
    }

    fn bookmark(&self, key: String, cx: &App) -> Button {
        let saved = AppState::get(cx)
            .reading
            .get(&key)
            .is_some_and(|reading| reading.bookmarked);
        Button::new("ca-bookmark")
            .small()
            .icon(Icon::new(if saved {
                IconName::BookmarkCheck
            } else {
                IconName::Bookmark
            }))
            .label(if saved { "Saved" } else { "Save" })
            .selected(saved)
            .tooltip(if saved {
                "Remove the bookmark"
            } else {
                "Bookmark this"
            })
            .on_click(move |_, _, cx| {
                AppState::update_reading(cx, &key, |reading, _| {
                    reading.bookmarked = !reading.bookmarked
                });
            })
    }

    /// The paper and subject of the first Mains topic, the date, and the
    /// notes linked.
    fn byline(
        &self,
        topics: &[String],
        notes: &[String],
        date: NaiveDate,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let library = &AppState::get(cx).library;
        let paper = topics
            .iter()
            .find(|t| t.starts_with("mains."))
            .or(topics.first())
            .map(|t| library.subject_label(t))
            .unwrap_or_default();
        let color = topics
            .iter()
            .find(|t| t.starts_with("mains."))
            .or(topics.first())
            .map(|t| ui::subject_color(t, cx))
            .unwrap_or(ui::tokens(cx).text_3);
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(ui::subject_chip(paper, color, cx))
            .child(ui::meta(date.format("%-d %b %Y").to_string(), cx))
            .children(notes.iter().enumerate().filter_map(|(k, id)| {
                let ix = library.notes.iter().position(|note| &note.id == id)?;
                Some(
                    Button::new(("story-note", k))
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(IconName::BookOpen))
                        .label(format!("Note: {}", library.notes[ix].title))
                        .on_click(self.opener(Target::Note(ix))),
                )
            }))
    }

    /// How many of the item's highlights are painted, which the UI test
    /// checks.
    pub fn painted(&self) -> usize {
        self.blocks.painted()
    }

    /// Makes the open story or editorial a set of text views the student can
    /// select text in, with their highlights painted.
    fn prepare_blocks(&mut self, brief: usize, cx: &mut Context<Self>) {
        let (key, parts, marks, colour) = {
            let state = AppState::get(cx);
            let day = &state.library.briefs[brief];
            let (key, parts) = match self.view {
                View::Story(ix) => {
                    let story = &day.stories[ix];
                    (story.reading_key(), story_parts(story))
                }
                View::Editorial(ix) => {
                    let editorial = &day.editorials[ix];
                    (editorial.reading_key(), vec![("", editorial.body.clone())])
                }
                View::Day | View::Month => return,
            };
            let marks = state.marks_of(&key).to_vec();
            (key, parts, marks, crate::marks::highlight(cx))
        };
        if self.marked.as_deref() != Some(key.as_str()) {
            self.blocks.clear();
            self.marking.close();
            self.selected = None;
            self.marked = Some(key);
        }
        for (part, text) in &parts {
            self.blocks.prepare(part, part, text, &marks, colour, cx);
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

    /// Highlights the selected text, or clips it.
    fn start_mark(&mut self, kind: MarkKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = self.blocks.selection(cx).or_else(|| self.selected.take()) else {
            return;
        };
        if let Some(clip) = self.marking.start(kind, selection, window, cx) {
            self.keep_mark(kind, clip, None, cx);
        }
        cx.notify();
    }

    /// Saves a highlight or clip of the item on screen.
    fn keep_mark(
        &mut self,
        kind: MarkKind,
        selection: Selection,
        comment: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.marked.clone() else {
            return;
        };
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
    }

    /// The buttons that mark the selected text, and the bar where a
    /// highlight waits for its comment.
    fn mark_tools(&self, cx: &Context<Self>) -> impl IntoElement {
        let button = |label: &'static str, icon: IconName, kind: MarkKind, cx: &Context<Self>| {
            Button::new(label)
                .ghost()
                .xsmall()
                .icon(Icon::new(icon))
                .tooltip("Select some text first, then mark it")
                .on_click(cx.listener(move |this, _, window, cx| this.start_mark(kind, window, cx)))
        };
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .pt_2()
                    .gap_1()
                    .child(
                        button(
                            "story-highlight",
                            IconName::PenLine,
                            MarkKind::Highlight,
                            cx,
                        )
                        .label("Highlight"),
                    )
                    .child(
                        button("story-clip", IconName::NotebookText, MarkKind::Clip, cx)
                            .label("Clip"),
                    ),
            )
            .children(self.marking.bar(
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
            ))
    }

    fn story(&mut self, brief: usize, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_blocks(brief, cx);
        let state = AppState::get(cx);
        let library = &state.library;
        let day = &library.briefs[brief];
        let story = &day.stories[ix];
        let t = ui::tokens(cx);
        let text = |label: &str, part: &str| {
            v_flex().child(heading(label, cx)).child(
                div()
                    .id(SharedString::from(format!("story-{part}")))
                    .test_support()
                    .font_family(ui::SERIF)
                    .text_size(px(17.))
                    .line_height(relative(1.6))
                    .child(self.blocks.view(part)),
            )
        };
        let question = story
            .question
            .as_deref()
            .and_then(|id| library.question(id));
        let shown = self.answer;
        let order = reading_order(day);
        let after = order.iter().position(|&story| story == ix).map(|at| at + 1);
        let next = if let Some(&story) = after.and_then(|at| order.get(at)) {
            Some(Target::Story { brief, story })
        } else if !day.editorials.is_empty() {
            Some(Target::Editorial {
                brief,
                editorial: 0,
            })
        } else {
            None
        };
        let toggle = cx.listener(|this, _, _, cx| {
            this.answer = !this.answer;
            cx.notify();
        });
        v_flex()
            .id("story")
            .test_support()
            .py_8()
            .gap_1()
            .child(self.byline(&story.topics, &story.notes, day.date, cx))
            .child(
                div()
                    .pt_3()
                    .font_family(ui::DISPLAY)
                    .text_size(px(26.))
                    .line_height(relative(1.25))
                    .font_semibold()
                    .child(story.title.clone()),
            )
            .child(self.mark_tools(cx))
            .child(text("Context", "context"))
            .child(text("Why it matters", "why"))
            .child(
                v_flex()
                    .id("key-facts")
                    .test_support()
                    .child(heading("Key facts", cx))
                    .child(
                        div()
                            .font_family(ui::SERIF)
                            .text_size(px(16.5))
                            .line_height(relative(1.6))
                            .child(self.blocks.view("facts")),
                    ),
            )
            .when(story.trap.is_some(), |this| {
                this.child(
                    v_flex()
                        .mt_5()
                        .px_4()
                        .py_3()
                        .gap_1()
                        .rounded(px(8.))
                        .bg(t.attention.opacity(0.1))
                        .border_l_2()
                        .border_color(t.attention)
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .font_semibold()
                                .child(
                                    Icon::new(IconName::TriangleAlert)
                                        .size(px(13.))
                                        .text_color(t.attention),
                                )
                                .child("The trap"),
                        )
                        .child(div().text_size(px(14.5)).child(self.blocks.view("trap"))),
                )
            })
            .when(story.pyq.is_some(), |this| {
                this.child(text("PYQ pattern", "pyq"))
            })
            .when_some(question, |this, question| {
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
                                .child(LETTERS[option]),
                        )
                        .child(div().min_w_0().pt(px(2.)).child(text.clone()))
                });
                this.child(heading("Practice question", cx)).child(
                    v_flex()
                        .id("practice-question")
                        .test_support()
                        .p_4()
                        .gap(px(12.))
                        .rounded(px(8.))
                        .bg(t.card)
                        .border_1()
                        .border_color(t.border)
                        .shadow(ui::lift(cx))
                        .child(
                            div()
                                .text_size(px(14.5))
                                .child(TextView::markdown("story-stem", question.stem.clone())),
                        )
                        .child(v_flex().gap(px(8.)).children(options))
                        .when(shown, |card| {
                            card.child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(t.text_2)
                                    .child(question.explanation.clone()),
                            )
                        })
                        .child(
                            h_flex().child(
                                Button::new("story-answer")
                                    .small()
                                    .label(if shown {
                                        "Hide the answer"
                                    } else {
                                        "Show the answer"
                                    })
                                    .on_click(toggle),
                            ),
                        ),
                )
            })
            .when(story.mains.is_some(), |this| {
                this.child(heading("Mains question", cx))
                    .child(
                        div()
                            .font_family(ui::SERIF)
                            .text_size(px(17.))
                            .font_medium()
                            .child(self.blocks.view("mains")),
                    )
                    .when(!story.frame.is_empty(), |this| {
                        this.child(muted("Answer frame", cx).pt_3().pb_1())
                            .children(story.frame.iter().enumerate().map(|(k, point)| {
                                h_flex()
                                    .items_start()
                                    .gap(px(10.))
                                    .py(px(4.))
                                    .text_size(px(14.))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .size(px(20.))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(t.accent_soft)
                                            .text_color(t.accent)
                                            .text_size(px(11.))
                                            .font_semibold()
                                            .child((k + 1).to_string()),
                                    )
                                    .child(div().min_w_0().child(point.clone()))
                            }))
                    })
            })
            .child(
                h_flex()
                    .py_8()
                    .gap_2()
                    .child(self.bookmark(story.reading_key(), cx))
                    .child(div().flex_1())
                    .when_some(next, |this, next| {
                        this.child(
                            Button::new("next-story")
                                .primary()
                                .small()
                                .label("Next story")
                                .icon(Icon::new(IconName::ArrowRight))
                                .on_click(self.opener(next)),
                        )
                    }),
            )
    }

    fn editorial(&mut self, brief: usize, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_blocks(brief, cx);
        let library = &AppState::get(cx).library;
        let day = &library.briefs[brief];
        let editorial = &day.editorials[ix];
        v_flex()
            .id("editorial")
            .test_support()
            .py_8()
            .gap_2()
            .child(self.byline(&editorial.topics, &editorial.notes, day.date, cx))
            .child(
                div()
                    .text_2xl()
                    .line_height(relative(1.25))
                    .font_semibold()
                    .child(editorial.title.clone()),
            )
            .child(muted(
                "Editorial: our own analysis of an issue in the news.",
                cx,
            ))
            .child(self.mark_tools(cx))
            .child(
                div()
                    .id("editorial-body")
                    .test_support()
                    .child(self.blocks.view("")),
            )
            .child(
                h_flex()
                    .py_8()
                    .child(self.bookmark(editorial.reading_key(), cx)),
            )
    }

    /// The monthly issue: every story of the month by subject, and its quiz.
    fn month(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let date = library.briefs[brief].date;
        let stories = library.month_stories(date.year(), date.month());
        let mut days: Vec<usize> = stories.iter().map(|(b, _)| *b).collect();
        days.dedup();
        let kind = TestKind::Month(date.year(), date.month());
        let questions = mentor::pool(library, &kind, &[]).len();
        let last = sittings(&kind, cx).into_iter().next();
        let groups = by_subject(
            stories
                .iter()
                .map(|&(b, s)| ((b, s), subject_of(&library.briefs[b].stories[s]))),
        );
        let theme = cx.theme();
        v_flex()
            .id("monthly")
            .test_support()
            .py_8()
            .gap_2()
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .child(format!("Monthly issue: {}", date.format("%B %Y"))),
            )
            .child(muted(
                format!(
                    "{} from {}, by subject.",
                    count(stories.len(), "story", "stories"),
                    count(days.len(), "brief", "briefs")
                ),
                cx,
            ))
            .child(
                h_flex()
                    .pt_2()
                    .gap_3()
                    .child(
                        Button::new("monthly-quiz")
                            .primary()
                            .label(format!("Take the monthly quiz · {questions} Q"))
                            .disabled(questions == 0)
                            .on_click(self.sitter(kind)),
                    )
                    .when_some(last, |this, test| {
                        this.child(
                            div()
                                .id("monthly-last")
                                .test_support()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .cursor_pointer()
                                .hover(|this| this.underline())
                                .child(format!("Last time: {}", quiz_score(&test)))
                                .on_click(self.result_opener(test.id)),
                        )
                    }),
            )
            .children(groups.into_iter().map(|(subject, items)| {
                v_flex()
                    .child(heading(subject, cx))
                    .children(items.into_iter().map(|(b, s)| {
                        let story = &library.briefs[b].stories[s];
                        let read = mentor::story_read(&state.reading, story);
                        h_flex()
                            .id(SharedString::from(format!("month-{}", story.id)))
                            .test_support()
                            .gap_3()
                            .py_1p5()
                            .border_t_1()
                            .border_color(theme.border)
                            .text_sm()
                            .cursor_pointer()
                            .hover(|this| this.bg(theme.accent))
                            .child(
                                div()
                                    .w(px(52.))
                                    .flex_shrink_0()
                                    .text_color(theme.muted_foreground)
                                    .child(library.briefs[b].date.format("%-d %b").to_string()),
                            )
                            .child(div().flex_1().min_w_0().child(story.title.clone()))
                            .when(read, |this| {
                                this.child(
                                    Icon::new(IconName::CircleCheck)
                                        .small()
                                        .text_color(theme.success),
                                )
                            })
                            .on_click(self.opener(Target::Story { brief: b, story: s }))
                    }))
            }))
            .child(div().h(px(32.)))
    }
}

impl CurrentAffairsScreen {
    fn panes(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Some(brief) = self.brief else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(div().text_lg().font_medium().child("No briefs yet"))
                .child(muted("The content pack has no current affairs.", cx))
                .into_any_element();
        };
        v_flex()
            .size_full()
            .on_mouse_move(cx.listener(|this, _, _, cx| this.touch(cx)))
            .when(self.calendar, |this| this.child(self.calendar(brief, cx)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.list(brief, cx))
                    .child(self.reader(brief, cx)),
            )
            .into_any_element()
    }
}

impl Render for CurrentAffairsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = match self.brief {
            Some(brief) => ui::header(cx).child(self.date_bar(brief, cx)),
            None => ui::header(cx).child(ui::title(IconName::Newspaper, "Current affairs", cx)),
        };
        v_flex()
            .size_full()
            .child(header)
            .child(div().flex_1().min_h_0().child(self.panes(cx)))
    }
}

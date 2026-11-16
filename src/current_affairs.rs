//! The Current affairs screen: a day's brief with the date bar and a
//! calendar, its stories grouped by subject and its editorials, the story
//! reader in Full and Rapid modes, the daily quiz, and the monthly issue
//! with its own quiz.

use chrono::{Datelike as _, Local, NaiveDate};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
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
use crate::mentor::{self, SUBJECTS, TestKind, TestPlan};
use crate::notes::ReadingClock;
use crate::review::marks;
use crate::screens::count;
use crate::store::{Attempt, Study};

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

fn heading(text: &str, cx: &App) -> Div {
    div()
        .pt_6()
        .pb_1()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_uppercase())
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
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
        let today = Local::now().date_naive();
        CurrentAffairsScreen {
            shell,
            brief: AppState::get(cx).library.todays_brief(today),
            view: View::Day,
            rapid: false,
            calendar: false,
            answer: false,
            scroll: ScrollHandle::new(),
            clock: None,
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

    fn date_bar(&self, brief: usize, cx: &Context<Self>) -> impl IntoElement {
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
        h_flex()
            .flex_shrink_0()
            .h(px(48.))
            .px_3()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
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
                    .w(px(150.))
                    .text_center()
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
                    .label("Calendar")
                    .selected(self.calendar)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.calendar = !this.calendar;
                        cx.notify();
                    })),
            )
            .child(div().w(px(8.)))
            .child(
                ButtonGroup::new("ca-mode")
                    .outline()
                    .small()
                    .child(Button::new("full-mode").label("Full").selected(!self.rapid))
                    .child(
                        Button::new("rapid-mode")
                            .label("Rapid")
                            .selected(self.rapid),
                    )
                    .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                        if let Some(&button) = clicked.first() {
                            this.rapid = button == 1;
                            if this.view == View::Month {
                                this.view = View::Day;
                            }
                            this.scroll.set_offset(Point::default());
                            cx.notify();
                        }
                    })),
            )
            .child(div().w(px(8.)))
            .child(
                Button::new("monthly-issue")
                    .outline()
                    .small()
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
                div()
                    .id("brief-count")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} · {read} read",
                        count(day.stories.len(), "story", "stories")
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
        let state = AppState::get(cx);
        let day = &state.library.briefs[brief];
        let theme = cx.theme();
        let row = |id: ElementId, title: &str, read: bool, selected: bool, target: Target| {
            h_flex()
                .id(id)
                .test_support()
                .items_start()
                .gap_2()
                .px_2()
                .py_1()
                .rounded(theme.radius)
                .text_sm()
                .cursor_pointer()
                .hover(|this| this.bg(theme.accent))
                .when(selected, |this| this.bg(theme.accent).font_medium())
                .child(
                    div().pt(px(2.)).child(
                        Icon::new(if read {
                            IconName::CircleCheck
                        } else {
                            IconName::Circle
                        })
                        .small()
                        .when(read, |icon| icon.text_color(theme.success)),
                    ),
                )
                .child(div().flex_1().min_w_0().child(title.to_string()))
                .on_click(self.opener(target))
        };
        let groups = by_subject(
            day.stories
                .iter()
                .enumerate()
                .map(|(ix, story)| (ix, subject_of(story))),
        );
        let header = |text: String| {
            div()
                .px_2()
                .pt_3()
                .pb_1()
                .text_xs()
                .font_medium()
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let kind = TestKind::Quiz(day.date);
        let questions = state.library.brief_questions(day).len();
        let last = sittings(&kind, cx).into_iter().next();
        v_flex()
            .w(px(270.))
            .flex_shrink_0()
            .h_full()
            .p_2()
            .border_r_1()
            .border_color(theme.border)
            .overflow_y_scrollbar()
            .id("ca-list")
            .children(groups.into_iter().map(|(subject, stories)| {
                v_flex()
                    .child(header(subject.to_string()))
                    .children(stories.into_iter().map(|ix| {
                        let story = &day.stories[ix];
                        row(
                            ("story", ix).into(),
                            &story.title,
                            mentor::story_read(&state.reading, story),
                            self.view == View::Story(ix),
                            Target::Story { brief, story: ix },
                        )
                    }))
            }))
            .when(!day.editorials.is_empty(), |list| {
                list.child(header(format!("Editorials ({})", day.editorials.len())))
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
            })
            .child(
                v_flex()
                    .pt_4()
                    .px_2()
                    .gap_2()
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
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .cursor_pointer()
                                .hover(|this| this.underline())
                                .child(format!("Last time: {}. Review it", quiz_score(&test)))
                                .on_click(self.result_opener(test.id)),
                        )
                    }),
            )
    }

    fn reader(&self, brief: usize, cx: &mut Context<Self>) -> impl IntoElement {
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
            .outline()
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
        h_flex()
            .flex_wrap()
            .gap_2()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(format!("{paper} · {}", date.format("%-d %b %Y")))
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

    fn story(&self, brief: usize, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let state = AppState::get(cx);
        let library = &state.library;
        let day = &library.briefs[brief];
        let story = &day.stories[ix];
        let theme = cx.theme();
        let text = |label: &str, body: &str| {
            v_flex()
                .child(heading(label, cx))
                .child(div().child(body.to_string()))
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
                    .pt_2()
                    .text_2xl()
                    .line_height(relative(1.25))
                    .font_semibold()
                    .child(story.title.clone()),
            )
            .child(text("Context", &story.context))
            .child(text("Why it matters", &story.why))
            .child(
                v_flex()
                    .id("key-facts")
                    .test_support()
                    .child(heading("Key facts", cx))
                    .children(story.facts.iter().map(|fact| bullet(fact))),
            )
            .when_some(story.trap.clone(), |this, trap| {
                this.child(
                    v_flex()
                        .mt_4()
                        .p_3()
                        .gap_1()
                        .rounded(theme.radius_lg)
                        .bg(theme.muted)
                        .child(div().text_xs().font_semibold().child("THE TRAP"))
                        .child(div().child(trap)),
                )
            })
            .when_some(story.pyq.clone(), |this, pyq| {
                this.child(text("PYQ pattern", &pyq))
            })
            .when_some(question, |this, question| {
                let options = question.options.iter().enumerate().map(|(option, text)| {
                    let right = shown && Some(option) == question.answer;
                    h_flex()
                        .items_start()
                        .gap_2()
                        .when(right, |this| this.font_medium().text_color(theme.success))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(28.))
                                .child(format!("({})", LETTERS[option])),
                        )
                        .child(div().min_w_0().child(text.clone()))
                        .when(right, |this| this.child(Icon::new(IconName::Check)))
                });
                this.child(heading("Practice question", cx)).child(
                    v_flex()
                        .id("practice-question")
                        .test_support()
                        .p_4()
                        .gap_3()
                        .rounded(theme.radius_lg)
                        .border_1()
                        .border_color(theme.border)
                        .child(TextView::markdown("story-stem", question.stem.clone()))
                        .child(v_flex().gap_1().children(options))
                        .when(shown, |card| {
                            card.child(div().child(question.explanation.clone()))
                        })
                        .child(
                            h_flex().child(
                                Button::new("story-answer")
                                    .outline()
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
            .when_some(story.mains.clone(), |this, mains| {
                this.child(heading("Mains question", cx))
                    .child(div().font_medium().child(mains))
                    .when(!story.frame.is_empty(), |this| {
                        this.child(muted("Answer frame", cx).pt_2()).children(
                            story.frame.iter().enumerate().map(|(k, point)| {
                                h_flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .w(px(20.))
                                            .child(format!("{}.", k + 1)),
                                    )
                                    .child(div().min_w_0().child(point.clone()))
                            }),
                        )
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
                                .label("Next")
                                .icon(Icon::new(IconName::ChevronRight))
                                .on_click(self.opener(next)),
                        )
                    }),
            )
    }

    fn editorial(&self, brief: usize, ix: usize, cx: &Context<Self>) -> impl IntoElement {
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
            .child(TextView::markdown(
                SharedString::from(format!("editorial-{}", editorial.id)),
                editorial.body.clone(),
            ))
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

impl Render for CurrentAffairsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.date_bar(brief, cx))
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

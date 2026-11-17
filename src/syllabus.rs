//! The Syllabus screen: the whole UPSC syllabus as a tree, with how far the
//! student has got with every topic and how much of each paper is covered.

use std::collections::{HashMap, HashSet};

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::app::Shell;
use crate::content::{Library, Target, covers, is_under, within};
use crate::screens::{count, page};
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

/// The syllabus as a tree that folds. Papers start folded; a row opens to
/// show what is under it, and a bottom-level topic opens its note.
pub struct SyllabusScreen {
    shell: WeakEntity<Shell>,
    /// Topics whose children are showing, by index into the library.
    open: HashSet<usize>,
}

impl SyllabusScreen {
    pub fn new(shell: WeakEntity<Shell>) -> Self {
        SyllabusScreen {
            shell,
            open: HashSet::new(),
        }
    }
}

impl SyllabusScreen {
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = AppState::get(cx);
        let (library, reading) = (&state.library, &state.reading);
        let statuses = statuses(library, reading);
        let t = ui::tokens(cx);
        let muted = t.text_3;

        // Stages and papers always show; any other topic shows when its parent
        // shows and is open. Parents come before children, so one pass does it.
        let mut showing_children: HashSet<&str> = HashSet::new();
        let mut rows = Vec::new();
        for (ix, topic) in library.topics.iter().enumerate() {
            let parent = topic.id.rsplit_once('.').map(|(parent, _)| parent);
            if topic.depth > 1 && !parent.is_some_and(|p| showing_children.contains(p)) {
                continue;
            }
            if self.open.contains(&ix) {
                showing_children.insert(&topic.id);
            }

            if topic.depth == 0 {
                rows.push(
                    v_flex()
                        .pt_6()
                        .pb_3()
                        .gap(px(2.))
                        .child(
                            div()
                                .font_family(ui::DISPLAY)
                                .text_size(px(20.))
                                .font_semibold()
                                .child(topic.title.clone()),
                        )
                        .when_some(topic.official.clone(), |this, official| {
                            this.child(ui::meta(official, cx))
                        })
                        .into_any_element(),
                );
                continue;
            }

            let indent = px(10. + 20. * (topic.depth - 1) as f32);
            let row = h_flex()
                .id(("topic", ix))
                .test_support()
                .items_start()
                .gap_3()
                .py(px(7.))
                .pr_3()
                .pl(indent)
                .rounded(px(6.))
                .text_size(px(13.5))
                .when(topic.depth == 1, |this| {
                    this.py(px(10.))
                        .border_t_1()
                        .border_color(t.border)
                        .rounded_none()
                });

            if !topic.leaf {
                let open = self.open.contains(&ix);
                let covered = coverage(library, &statuses, &topic.id);
                rows.push(
                    row.cursor_pointer()
                        .hover(|this| this.bg(cx.theme().accent))
                        .child(
                            div().pt(px(3.)).child(
                                Icon::new(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .small()
                                .text_color(muted),
                            ),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .when(topic.depth <= 2, |this| this.font_medium())
                                        .when(topic.depth == 1, |this| this.text_size(px(15.)))
                                        .child(topic.title.clone()),
                                )
                                // UPSC's wording shows once the student opens the row.
                                .when_some(
                                    topic.official.clone().filter(|_| open),
                                    |this, official| {
                                        this.child(
                                            div()
                                                .mt_1()
                                                .pl_3()
                                                .border_l_2()
                                                .border_color(t.border_3)
                                                .text_size(px(12.5))
                                                .text_color(muted)
                                                .child(official),
                                        )
                                    },
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .pt(px(2.))
                                .text_size(px(12.))
                                .text_color(muted)
                                .child(format!("{} of {} read", covered.0, covered.1)),
                        )
                        .child(
                            div()
                                .id(("coverage", ix))
                                .flex_shrink_0()
                                .w(px(80.))
                                .pt(px(9.))
                                .child(ui::progress(
                                    percent(covered) / 100.,
                                    if topic.depth == 1 {
                                        t.accent
                                    } else {
                                        ui::subject_color(&topic.id, cx)
                                    },
                                    cx,
                                )),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.open.remove(&ix) {
                                this.open.insert(ix);
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                continue;
            }

            let notes: Vec<usize> = (0..library.notes.len())
                .filter(|&n| covers(&library.notes[n].topics, &topic.id))
                .collect();
            let questions = library
                .questions
                .iter()
                .filter(|q| is_under(&q.topics, &topic.id))
                .count();
            let status = statuses[ix];
            // The row opens the first note on the topic that is not read yet.
            let next = notes
                .iter()
                .copied()
                .find(|&n| Status::of(reading.get(&library.notes[n].id)) != Status::Done)
                .or(notes.first().copied());
            let linked = [
                (!notes.is_empty()).then(|| count(notes.len(), "note", "notes")),
                (questions > 0).then(|| count(questions, "question", "questions")),
            ];
            let linked: Vec<String> = linked.into_iter().flatten().collect();
            let shell = self.shell.clone();
            rows.push(
                row.child(div().pt(px(3.)).child(status.icon(cx).small()))
                    .child(div().flex_1().min_w_0().child(topic.title.clone()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(linked.join(" · ")),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(96.))
                            .text_size(px(12.5))
                            .when(status == Status::NoNotes, |this| this.text_color(muted))
                            .child(status.label()),
                    )
                    .when_some(next, |row, note| {
                        row.cursor_pointer()
                            .hover(|this| this.bg(cx.theme().accent))
                            // Not a listener: the shell may update this screen.
                            .on_click(move |_, _, cx| {
                                let _ = shell
                                    .update(cx, |shell, cx| shell.open(Target::Note(note), cx));
                            })
                    })
                    .into_any_element(),
            );
        }

        let (done, total) = library
            .topics
            .iter()
            .filter(|topic| topic.depth == 0)
            .map(|stage| coverage(library, &statuses, &stage.id))
            .fold((0, 0), |(done, total), (d, t)| (done + d, total + t));
        let summary = format!(
            "You have read {done} of {total} topics. A topic counts once every note on it is \
             marked read. Open a paper to see its subjects, chapters and topics."
        );
        let papers: Vec<AnyElement> = library
            .topics
            .iter()
            .enumerate()
            .filter(|(_, topic)| topic.depth == 1)
            .map(|(ix, paper)| {
                let covered = coverage(library, &statuses, &paper.id);
                let open = self.open.contains(&ix);
                ui::card(cx)
                    .id(("paper-card", ix))
                    .w(px(148.))
                    .flex_shrink_0()
                    .p(px(12.))
                    .gap(px(6.))
                    .cursor_pointer()
                    .when(open, |this| this.border_color(t.accent))
                    .hover(|this| this.border_color(t.border_3))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.5))
                            .font_medium()
                            .child(paper.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_semibold()
                            .child(format!("{:.0}%", percent(covered))),
                    )
                    .child(ui::progress(percent(covered) / 100., t.accent, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.open.remove(&ix) {
                            this.open.insert(ix);
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        page(summary, cx)
            .child(h_flex().flex_wrap().gap(px(10.)).children(papers))
            .child(v_flex().children(rows))
    }
}

impl Render for SyllabusScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = ui::header(cx).child(ui::title(IconName::ListTree, "Syllabus", cx));
        ui::screen(header, ("screen", 1usize), self.body(cx))
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in `gpui_kit::test`, which shadows `#[test]`.
    use super::{Status, coverage, percent, statuses};
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
}

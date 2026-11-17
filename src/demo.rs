//! Made-up history for trying the app before there is any of one's own:
//! 30 days of tests, reading, study time and reviews, so that Analysis,
//! Today and Revision have
//! something to show. It is built from the real content, so every question
//! and note it names exists. The store saves every row with the demo mark,
//! and removing the history deletes those rows and nothing else.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate, NaiveTime};

use crate::content::Library;
use crate::mentor::{self, Random, TestKind, TestPlan, chapter_of};
use crate::store::{Attempt, Confidence, Demo, Reading, Review, Study};

/// How many days back the history reaches.
pub const DAYS: i64 = 30;
/// Days back with no study, so the calendar has gaps. The last nine days
/// are all studied, so the streak shows.
const RESTS: [i64; 6] = [27, 23, 20, 16, 13, 10];

/// The share of questions the made-up student gets right in each subject.
fn skill(subject: &str, topic: &str) -> f64 {
    if chapter_of(topic) == "pre.gs.polity.parliament" {
        return 0.3;
    }
    match subject {
        "polity" => 0.74,
        "environment" => 0.64,
        "geography" => 0.6,
        "science" => 0.58,
        "history" => 0.52,
        "economy" => 0.42,
        _ => 0.5,
    }
}

/// 30 days of history ending yesterday. The same content gives the same history.
pub fn history(library: &Library, today: NaiveDate) -> Demo {
    let mut random = Random(0x5EED);
    let rotation = [
        TestKind::Micro,
        TestKind::Subject("polity".into()),
        TestKind::Subject("economy".into()),
        TestKind::Mini,
        TestKind::Subject("history".into()),
        TestKind::Subject("geography".into()),
        TestKind::Subject("environment".into()),
        TestKind::Subject("science".into()),
        TestKind::Subject("polity".into()),
        TestKind::Micro,
    ];
    let read_notes: Vec<&crate::content::Note> = library
        .notes
        .iter()
        .enumerate()
        .filter(|(ix, _)| ix % 3 != 2)
        .map(|(_, note)| note)
        .collect();

    let mut demo = Demo::default();
    let studied = (1..=DAYS).rev().filter(|back| !RESTS.contains(back));
    for (n, back) in studied.enumerate() {
        let day = today - Duration::days(back);

        let plan = TestPlan::standard(rotation[n % rotation.len()].clone(), library);
        if plan.fits(library) {
            let questions = mentor::pick(library, &plan, &[], random.next());
            let evening = NaiveTime::from_hms_opt(19, 0, 0).expect("a valid time");
            let mut attempt =
                mentor::new_attempt(library, &plan, &questions, day.and_time(evening));
            for (answer, &ix) in attempt.answers.iter_mut().zip(&questions) {
                let right = library.questions[ix].answer.unwrap_or(0);
                answer.visited = true;
                answer.seconds = 45 + (random.next() % 50) as u32;
                let roll = random.unit();
                let (confidence, odds) = if roll < 0.08 {
                    answer.seconds = 20;
                    continue;
                } else if roll < 0.5 {
                    (
                        Confidence::Sure,
                        (skill(&answer.subject, &answer.topic) + 0.2).min(0.95),
                    )
                } else if roll < 0.82 {
                    (
                        Confidence::FiftyFifty,
                        skill(&answer.subject, &answer.topic),
                    )
                } else {
                    (Confidence::Guess, 0.2)
                };
                let correct = random.unit() < odds;
                let chosen = if correct {
                    right
                } else {
                    (right + 1 + (random.next() % 3) as usize) % 4
                };
                answer.chosen = Some(chosen);
                answer.correct = Some(correct);
                answer.confidence = Some(confidence);
            }
            attempt.seconds_used = attempt
                .answers
                .iter()
                .map(|a| a.seconds)
                .sum::<u32>()
                .min(attempt.seconds_allowed);
            attempt.current = attempt.answers.len().saturating_sub(1);
            attempt.finished =
                Some(attempt.started + Duration::seconds(attempt.seconds_used.into()));
            demo.sessions
                .push((day, Study::Test, format!("demo-{n}"), attempt.seconds_used));
            demo.attempts.push(attempt);
        }

        if let Some(note) = read_notes.get(n % read_notes.len().max(1)) {
            let seconds = 60 * (20 + (random.next() % 31) as u32);
            demo.sessions
                .push((day, Study::Reading, note.id.clone(), seconds));
        }
    }

    for (ix, note) in read_notes.iter().enumerate() {
        let first = today - Duration::days((DAYS - 2 * ix as i64).max(2));
        let last = (first + Duration::days(4)).min(today - Duration::days(1));
        let done = (ix % 2 == 0).then_some(last);
        let reading = Reading {
            first,
            last,
            done,
            bookmarked: false,
            seconds: note.minutes * 60 / if done.is_some() { 1 } else { 2 },
            demo: true,
        };
        demo.readings.push((note.id.clone(), reading));
    }
    demo.reviews = reviews(&demo.attempts, today);
    demo
}

/// The made-up student's review boxes. Each test's mistakes join on the day
/// it was sat, and every studied day starts with the reviews due, seven in
/// ten answered right.
fn reviews(attempts: &[Attempt], today: NaiveDate) -> Vec<(String, Review)> {
    let mut random = Random(0xB0C5);
    let mut boxes: HashMap<String, Review> = HashMap::new();
    for back in (1..=DAYS).rev().filter(|back| !RESTS.contains(back)) {
        let day = today - Duration::days(back);
        let mut due: Vec<String> = boxes
            .iter()
            .filter(|(_, review)| review.due.is_some_and(|due| due <= day))
            .map(|(key, _)| key.clone())
            .collect();
        due.sort();
        for key in due {
            let next = mentor::review(boxes.get(&key), random.unit() < 0.7, day);
            boxes.insert(key, next);
        }
        for attempt in attempts
            .iter()
            .filter(|attempt| attempt.finished.is_some_and(|f| f.date() == day))
        {
            let mistakes = mentor::mistakes(attempt, &boxes, day);
            boxes.extend(mistakes);
        }
    }
    let mut reviews: Vec<(String, Review)> = boxes
        .into_iter()
        .map(|(key, review)| {
            (
                key,
                Review {
                    demo: true,
                    ..review
                },
            )
        })
        .collect();
    reviews.sort_by(|a, b| a.0.cmp(&b.0));
    reviews
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::store::Past;

    #[test]
    fn demo_history_fills_thirty_days_from_the_real_content() {
        let library = Library::embedded();
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let demo = history(&library, today);
        assert_eq!(demo, history(&library, today), "the same every time");
        assert_eq!(demo.attempts.len(), DAYS as usize - RESTS.len());
        for attempt in &demo.attempts {
            let finished = attempt
                .finished
                .expect("every demo test is finished")
                .date();
            assert!(finished < today && finished >= today - Duration::days(DAYS));
            assert!(attempt.seconds_used <= attempt.seconds_allowed);
            for answer in &attempt.answers {
                assert!(library.question(&answer.question).is_some());
            }
        }
        for (note, reading) in &demo.readings {
            assert!(library.note(note).is_some() && reading.demo);
            assert!(reading.last < today);
        }

        // The made-up student is weak on Parliament, so a drill gets planned.
        let history: Vec<Past> = demo
            .attempts
            .iter()
            .flat_map(|attempt| &attempt.answers)
            .filter_map(|answer| {
                Some(Past {
                    question: answer.question.clone(),
                    topic: answer.topic.clone(),
                    correct: answer.correct?,
                })
            })
            .collect();
        let weak = mentor::weak_topics(&history);
        assert!(
            weak.contains(&"pre.gs.polity.parliament".to_string()),
            "{weak:?}"
        );

        // The last nine days are all studied, so the streak shows.
        let mut study: BTreeMap<NaiveDate, u32> = BTreeMap::new();
        for (day, _, _, seconds) in &demo.sessions {
            *study.entry(*day).or_default() += seconds;
        }
        assert_eq!(mentor::streak(&study, today), 9);
        assert_eq!(study.len(), DAYS as usize - RESTS.len());

        // Every mistake is in the reviews, spread over the boxes, and some
        // fall due today.
        let wrong: std::collections::HashSet<String> = demo
            .attempts
            .iter()
            .flat_map(|attempt| &attempt.answers)
            .filter(|answer| answer.correct == Some(false))
            .map(|answer| mentor::mistake_key(&answer.question))
            .collect();
        let keys: std::collections::HashSet<String> =
            demo.reviews.iter().map(|(key, _)| key.clone()).collect();
        assert_eq!(keys, wrong);
        assert!(demo.reviews.iter().all(|(_, review)| review.demo));
        let levels: std::collections::HashSet<u8> = demo
            .reviews
            .iter()
            .map(|(_, review)| review.level)
            .collect();
        assert!(levels.len() >= 3, "{levels:?}");
        let due_today = demo
            .reviews
            .iter()
            .filter(|(_, review)| review.due == Some(today))
            .count();
        assert!(due_today > 0);
        assert!(
            demo.reviews
                .iter()
                .all(|(_, r)| r.due.is_none_or(|due| due >= today))
        );
    }
}

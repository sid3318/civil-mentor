//! The mentor: plain, deterministic rules over the content and the student's
//! record. Phase 2 adds how a test is marked and which questions it asks.
//! The daily plan, insights and review dates come with later phases.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDateTime;

use crate::content::{Library, Question, within};
use crate::store::{Answer, Attempt, Past};

/// Prelims allows 2 hours for 100 questions.
pub const SECONDS_PER_QUESTION: u32 = 72;
/// GS Paper I marking: 2 for a right answer, a third of that off for a wrong one.
pub const GS_RIGHT: f64 = 2.0;
pub const GS_WRONG: f64 = GS_RIGHT / 3.0;

/// A topic is weak below this share right, once it has this many answers.
pub const WEAK_BELOW: f64 = 0.5;
pub const WEAK_AFTER: usize = 8;

/// How the app groups Prelims GS for drills and marks: the subjects a student
/// thinks in, each made of one or more syllabus subjects.
#[derive(Debug, PartialEq, Eq)]
pub struct Subject {
    /// Stored with every answer, so it must never change.
    pub id: &'static str,
    pub name: &'static str,
    nodes: &'static [&'static str],
}

pub const SUBJECTS: [Subject; 7] = [
    Subject {
        id: "history",
        name: "History and culture",
        nodes: &[
            "pre.gs.ancient",
            "pre.gs.medieval",
            "pre.gs.modern",
            "pre.gs.culture",
        ],
    },
    Subject {
        id: "geography",
        name: "Geography",
        nodes: &[
            "pre.gs.indian-geography",
            "pre.gs.physical-geography",
            "pre.gs.world-geography",
        ],
    },
    Subject {
        id: "polity",
        name: "Polity",
        nodes: &["pre.gs.polity"],
    },
    Subject {
        id: "economy",
        name: "Economy",
        nodes: &["pre.gs.economy"],
    },
    Subject {
        id: "environment",
        name: "Environment",
        nodes: &["pre.gs.environment"],
    },
    Subject {
        id: "science",
        name: "Science and technology",
        nodes: &["pre.gs.general-science", "pre.gs.science-tech"],
    },
    Subject {
        id: "current",
        name: "Current affairs",
        nodes: &["pre.gs.current"],
    },
];

/// The subject of a question: the first of its topics that falls in one.
pub fn subject_of(question: &Question) -> Option<&'static Subject> {
    question.topics.iter().find_map(|topic| {
        SUBJECTS
            .iter()
            .find(|subject| subject.nodes.iter().any(|node| within(topic, node)))
    })
}

pub fn subject(id: &str) -> Option<&'static Subject> {
    SUBJECTS.iter().find(|subject| subject.id == id)
}

/// The topic an answer is filed under: the question's first Prelims topic.
pub fn topic_of(question: &Question) -> &str {
    question
        .topics
        .iter()
        .find(|topic| topic.starts_with("pre."))
        .or(question.topics.first())
        .map_or("", String::as_str)
}

/// The kinds of test the exam room sits. A new kind is a new variant here,
/// with its questions in [`pool`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestKind {
    Micro,
    Mini,
    Mock,
    /// A drill on one subject, by [`Subject::id`].
    Subject(String),
    /// The student picks the subjects, the number of questions and the time.
    Custom,
    /// Weighted towards what the student gets wrong and their weak topics.
    Mix,
    /// A real UPSC paper, by year, in its own order.
    Paper(u16),
}

impl TestKind {
    /// How the kind is written in the store.
    pub fn key(&self) -> String {
        match self {
            TestKind::Micro => "micro".into(),
            TestKind::Mini => "mini".into(),
            TestKind::Mock => "mock".into(),
            TestKind::Subject(id) => format!("subject:{id}"),
            TestKind::Custom => "custom".into(),
            TestKind::Mix => "mix".into(),
            TestKind::Paper(year) => format!("paper:{year}"),
        }
    }

    pub fn parse(key: &str) -> Option<TestKind> {
        Some(match key {
            "micro" => TestKind::Micro,
            "mini" => TestKind::Mini,
            "mock" => TestKind::Mock,
            "custom" => TestKind::Custom,
            "mix" => TestKind::Mix,
            _ => match key.split_once(':')? {
                ("subject", id) => TestKind::Subject(id.to_string()),
                ("paper", year) => TestKind::Paper(year.parse().ok()?),
                _ => return None,
            },
        })
    }

    /// The standard number of questions. A paper asks all of its own.
    pub fn questions(&self) -> usize {
        match self {
            TestKind::Micro => 10,
            TestKind::Mini => 30,
            TestKind::Mock | TestKind::Paper(_) => 100,
            TestKind::Subject(_) | TestKind::Custom | TestKind::Mix => 20,
        }
    }

    pub fn title(&self) -> String {
        match self {
            TestKind::Micro => "Micro test".into(),
            TestKind::Mini => "Mini test".into(),
            TestKind::Mock => "Full mock".into(),
            TestKind::Subject(id) => match subject(id) {
                Some(subject) => format!("Subject drill: {}", subject.name),
                None => "Subject drill".into(),
            },
            TestKind::Custom => "Custom drill".into(),
            TestKind::Mix => "Personalised mix".into(),
            TestKind::Paper(year) => format!("UPSC Prelims {year}, GS Paper I"),
        }
    }
}

/// What to sit: the kind, the subjects a custom drill draws on, how many
/// questions and how long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPlan {
    pub kind: TestKind,
    /// Subject ids. Only a custom drill uses them.
    pub subjects: Vec<String>,
    pub count: usize,
    pub seconds: u32,
}

impl TestPlan {
    /// A test of `kind` at its standard size and 72 seconds a question.
    pub fn standard(kind: TestKind, library: &Library) -> TestPlan {
        let count = match &kind {
            TestKind::Paper(_) => pool(library, &kind, &[]).len(),
            kind => kind.questions(),
        };
        TestPlan {
            kind,
            subjects: Vec::new(),
            count,
            seconds: count as u32 * SECONDS_PER_QUESTION,
        }
    }

    /// Whether the question bank has enough questions for it.
    pub fn fits(&self, library: &Library) -> bool {
        self.count > 0 && pool(library, &self.kind, &self.subjects).len() >= self.count
    }
}

/// The questions a test of `kind` can draw on, by index into the library.
/// Dropped questions only appear in their own paper.
pub fn pool(library: &Library, kind: &TestKind, subjects: &[String]) -> Vec<usize> {
    let scored = |ix: &usize| library.questions[*ix].answer.is_some();
    let in_subject =
        |ix: usize, id: &str| subject_of(&library.questions[ix]).is_some_and(|s| s.id == id);
    let all = 0..library.questions.len();
    match kind {
        TestKind::Paper(year) => all
            .filter(|&ix| library.questions[ix].year() == Some(*year))
            .collect(),
        TestKind::Subject(id) => all
            .filter(scored)
            .filter(|&ix| in_subject(ix, id))
            .collect(),
        TestKind::Custom => all
            .filter(scored)
            .filter(|&ix| subjects.iter().any(|id| in_subject(ix, id)))
            .collect(),
        TestKind::Micro | TestKind::Mini | TestKind::Mock | TestKind::Mix => {
            all.filter(scored).collect()
        }
    }
}

/// Weights for choosing questions: those got wrong last time come back most,
/// unseen ones next, and ones got right rarely.
const WRONG_WEIGHT: f64 = 2.0;
const UNSEEN_WEIGHT: f64 = 1.0;
const RIGHT_WEIGHT: f64 = 0.2;
/// The personalised mix asks more on weak topics.
const WEAK_WEIGHT: f64 = 1.5;

/// The topics whose share right is under [`WEAK_BELOW`] over at least
/// [`WEAK_AFTER`] answers.
pub fn weak_topics(history: &[Past]) -> Vec<String> {
    let mut tally: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for past in history {
        let (right, total) = tally.entry(&past.topic).or_default();
        *right += usize::from(past.correct);
        *total += 1;
    }
    tally
        .into_iter()
        .filter(|(_, (right, total))| {
            *total >= WEAK_AFTER && (*right as f64) < WEAK_BELOW * *total as f64
        })
        .map(|(topic, _)| topic.to_string())
        .collect()
}

/// Chooses the questions for a test, by index into the library. A paper is
/// asked whole and in order. Any other test samples its pool without
/// repeats, weighted by the student's last outcome on each question (and,
/// for the personalised mix, by weak topic). The same seed gives the same
/// test. `history` is oldest first, as [`crate::store::Store::answered`] gives it.
pub fn pick(library: &Library, plan: &TestPlan, history: &[Past], seed: u64) -> Vec<usize> {
    let pool = pool(library, &plan.kind, &plan.subjects);
    if matches!(plan.kind, TestKind::Paper(_)) {
        return pool;
    }
    let mut last: HashMap<&str, bool> = HashMap::new();
    for past in history {
        last.insert(&past.question, past.correct);
    }
    let weak = match plan.kind {
        TestKind::Mix => weak_topics(history),
        _ => Vec::new(),
    };

    // Weighted sampling without replacement (Efraimidis and Spirakis): each
    // question draws ln(u) / weight and the largest keys win.
    let mut random = Random(seed);
    let mut keyed: Vec<(f64, usize)> = pool
        .into_iter()
        .map(|ix| {
            let question = &library.questions[ix];
            let mut weight = match last.get(question.id.as_str()) {
                Some(false) => WRONG_WEIGHT,
                None => UNSEEN_WEIGHT,
                Some(true) => RIGHT_WEIGHT,
            };
            if weak.iter().any(|topic| topic == topic_of(question)) {
                weight *= WEAK_WEIGHT;
            }
            (random.unit().ln() / weight, ix)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    keyed
        .into_iter()
        .take(plan.count)
        .map(|(_, ix)| ix)
        .collect()
}

/// SplitMix64: small, fast and the same on every machine, which is all a
/// repeatable test needs.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in (0, 1].
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }
}

/// A new, unsaved sitting of `questions` (indices into the library) under `plan`.
pub fn new_attempt(
    library: &Library,
    plan: &TestPlan,
    questions: &[usize],
    now: NaiveDateTime,
) -> Attempt {
    let answers = questions
        .iter()
        .map(|&ix| {
            let question = &library.questions[ix];
            Answer {
                question: question.id.clone(),
                subject: subject_of(question).map_or("", |s| s.id).to_string(),
                topic: topic_of(question).to_string(),
                ..Answer::default()
            }
        })
        .collect();
    Attempt {
        id: 0,
        kind: plan.kind.key(),
        title: plan.kind.title(),
        started: now,
        finished: None,
        seconds_allowed: plan.seconds,
        seconds_used: 0,
        right_marks: GS_RIGHT,
        wrong_marks: GS_WRONG,
        current: 0,
        answers,
    }
}

/// The marks for a set of answers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Score {
    pub right: usize,
    pub wrong: usize,
    pub blank: usize,
    /// Answered, but UPSC dropped the question, so it carries no marks.
    pub dropped: usize,
    pub marks: f64,
}

impl Score {
    fn add(&mut self, answer: &Answer, right_marks: f64, wrong_marks: f64) {
        match (answer.chosen, answer.correct) {
            (None, _) => self.blank += 1,
            (Some(_), Some(true)) => {
                self.right += 1;
                self.marks += right_marks;
            }
            (Some(_), Some(false)) => {
                self.wrong += 1;
                self.marks -= wrong_marks;
            }
            (Some(_), None) => self.dropped += 1,
        }
    }
}

/// The net score of a sitting, with the marks it records: right adds
/// `right_marks`, wrong takes off `wrong_marks`, blank gives nothing.
pub fn score(attempt: &Attempt) -> Score {
    let mut score = Score::default();
    for answer in &attempt.answers {
        score.add(answer, attempt.right_marks, attempt.wrong_marks);
    }
    score
}

/// The score split by any property of an answer, such as its subject or
/// confidence, in key order.
pub fn score_by<K: Ord>(attempt: &Attempt, key: impl Fn(&Answer) -> K) -> Vec<(K, Score)> {
    let mut groups: BTreeMap<K, Score> = BTreeMap::new();
    for answer in &attempt.answers {
        groups.entry(key(answer)).or_default().add(
            answer,
            attempt.right_marks,
            attempt.wrong_marks,
        );
    }
    groups.into_iter().collect()
}

/// The most a sitting can score.
pub fn full_marks(attempt: &Attempt) -> f64 {
    attempt.answers.len() as f64 * attempt.right_marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Confidence;
    use chrono::NaiveDate;

    fn answer(chosen: Option<usize>, correct: Option<bool>) -> Answer {
        Answer {
            chosen,
            correct,
            ..Answer::default()
        }
    }

    fn nine_am() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 15)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    fn attempt(answers: Vec<Answer>, right: f64, wrong: f64) -> Attempt {
        Attempt {
            id: 1,
            kind: "micro".into(),
            title: "Micro test".into(),
            started: nine_am(),
            finished: Some(nine_am()),
            seconds_allowed: 720,
            seconds_used: 600,
            right_marks: right,
            wrong_marks: wrong,
            current: 0,
            answers,
        }
    }

    /// Worked by hand: 10 questions, 6 right, 3 wrong, 1 blank.
    /// 6 x 2 = 12, less 3 x 2/3 = 2, so the net score is 10 out of 20.
    #[test]
    fn the_score_matches_a_hand_worked_case() {
        let mut answers = vec![answer(Some(0), Some(true)); 6];
        answers.extend(vec![answer(Some(1), Some(false)); 3]);
        answers.push(answer(None, None));
        let test = attempt(answers, GS_RIGHT, GS_WRONG);
        let score = score(&test);
        assert_eq!((score.right, score.wrong, score.blank), (6, 3, 1));
        assert!((score.marks - 10.0).abs() < 1e-9, "{}", score.marks);
        assert_eq!(full_marks(&test), 20.0);
    }

    #[test]
    fn the_marks_come_from_the_attempt() {
        // CSAT marks 2.5 right and a third of that wrong: one of each nets 1.67.
        let test = attempt(
            vec![answer(Some(0), Some(true)), answer(Some(2), Some(false))],
            2.5,
            2.5 / 3.0,
        );
        assert!((score(&test).marks - 2.5 * 2.0 / 3.0).abs() < 1e-9);
        // A dropped question carries no marks either way.
        let test = attempt(vec![answer(Some(3), None)], GS_RIGHT, GS_WRONG);
        let score = score(&test);
        assert_eq!((score.dropped, score.marks), (1, 0.0));
    }

    #[test]
    fn the_score_splits_by_confidence() {
        let tagged = |confidence, correct| Answer {
            confidence: Some(confidence),
            ..answer(Some(0), Some(correct))
        };
        let test = attempt(
            vec![
                tagged(Confidence::Sure, true),
                tagged(Confidence::Sure, true),
                tagged(Confidence::Guess, false),
                tagged(Confidence::Guess, false),
                tagged(Confidence::Guess, true),
            ],
            GS_RIGHT,
            GS_WRONG,
        );
        let split = score_by(&test, |a| a.confidence);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].0, Some(Confidence::Sure));
        assert!((split[0].1.marks - 4.0).abs() < 1e-9);
        assert_eq!(split[1].0, Some(Confidence::Guess));
        assert!((split[1].1.marks - (2.0 - 4.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn kinds_survive_the_store() {
        for kind in [
            TestKind::Micro,
            TestKind::Mini,
            TestKind::Mock,
            TestKind::Subject("polity".into()),
            TestKind::Custom,
            TestKind::Mix,
            TestKind::Paper(2025),
        ] {
            assert_eq!(TestKind::parse(&kind.key()), Some(kind));
        }
        assert_eq!(TestKind::parse("paper:soon"), None);
        assert_eq!(TestKind::parse("quiz"), None);
    }

    /// 20 questions on rights, 10 on courts, and a 10-question 2025 paper on
    /// the economy whose last question was dropped.
    fn bank() -> Library {
        let syllabus = r#"
topic = [
  { id = "pre", title = "Prelims" },
  { id = "pre.gs", title = "GS" },
  { id = "pre.gs.polity", title = "Polity" },
  { id = "pre.gs.polity.rights", title = "Rights" },
  { id = "pre.gs.polity.courts", title = "Courts" },
  { id = "pre.gs.economy", title = "Economy" },
]"#;
        let mut questions = String::new();
        for n in 0..40 {
            let (topic, source) = match n {
                0..20 => ("pre.gs.polity.rights", "Civil Mentor sample"),
                20..30 => ("pre.gs.polity.courts", "Civil Mentor sample"),
                _ => ("pre.gs.economy", "UPSC Prelims 2025"),
            };
            let answer = if n == 39 {
                "dropped = true"
            } else {
                "answer = \"A\""
            };
            questions.push_str(&format!(
                "[[question]]\nid = \"q{n:02}\"\ntopics = [\"{topic}\"]\nsource = \"{source}\"\n\
                 stem = \"?\"\noptions = [\"1\", \"2\", \"3\", \"4\"]\n{answer}\nexplanation = \".\"\n\n"
            ));
        }
        let library = Library::from_files([
            ("syllabus.toml".to_string(), syllabus.to_string()),
            ("questions/bank.toml".to_string(), questions),
        ]);
        assert_eq!(library.problems, Vec::<String>::new());
        library
    }

    fn ids(library: &Library, picked: &[usize]) -> Vec<String> {
        picked
            .iter()
            .map(|&ix| library.questions[ix].id.clone())
            .collect()
    }

    #[test]
    fn pick_is_repeatable_and_never_repeats_a_question() {
        let library = bank();
        let plan = TestPlan::standard(TestKind::Micro, &library);
        assert_eq!((plan.count, plan.seconds), (10, 720));
        let first = pick(&library, &plan, &[], 7);
        assert_eq!(first, pick(&library, &plan, &[], 7), "same seed, same test");
        assert_ne!(first, pick(&library, &plan, &[], 8), "another seed differs");
        let mut unique = first.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 10);
        let all = pick(&library, &TestPlan { count: 100, ..plan }, &[], 1);
        assert_eq!(
            all.len(),
            39,
            "capped at the pool, which leaves out the dropped one"
        );
        assert!(!ids(&library, &all).contains(&"q39".to_string()));
    }

    #[test]
    fn a_paper_is_asked_whole_and_in_order() {
        let library = bank();
        let plan = TestPlan::standard(TestKind::Paper(2025), &library);
        assert_eq!((plan.count, plan.seconds), (10, 720));
        let picked = ids(&library, &pick(&library, &plan, &[], 99));
        let expected: Vec<String> = (30..40).map(|n| format!("q{n:02}")).collect();
        assert_eq!(picked, expected, "the dropped question stays in its paper");
        assert_eq!(library.papers(), vec![(2025, (30..40).collect())]);
    }

    #[test]
    fn drills_draw_on_their_subjects_and_grey_out_when_short() {
        let library = bank();
        let polity = TestPlan::standard(TestKind::Subject("polity".into()), &library);
        assert!(polity.fits(&library), "30 polity questions fill 20");
        let picked = pick(&library, &polity, &[], 3);
        assert!(picked.iter().all(|&ix| ix < 30), "only polity: {picked:?}");
        let economy = TestPlan::standard(TestKind::Subject("economy".into()), &library);
        assert!(
            !economy.fits(&library),
            "9 scored economy questions cannot fill 20"
        );
        let custom = TestPlan {
            kind: TestKind::Custom,
            subjects: vec!["economy".into()],
            count: 5,
            seconds: 300,
        };
        assert!(custom.fits(&library));
        assert!(pick(&library, &custom, &[], 3).iter().all(|&ix| ix >= 30));
        let no_subject = TestPlan {
            subjects: Vec::new(),
            ..custom
        };
        assert!(!no_subject.fits(&library), "a custom drill needs a subject");
    }

    #[test]
    fn wrong_answers_come_back_more_often_than_right_ones() {
        let library = bank();
        let past = |id: &str, correct| Past {
            question: id.into(),
            topic: "pre.gs.polity.rights".into(),
            correct,
        };
        // q00 was got wrong; q01 was got wrong once, then right.
        let history = vec![past("q01", false), past("q00", false), past("q01", true)];
        let plan = TestPlan::standard(TestKind::Micro, &library);
        let (mut wrong, mut right) = (0, 0);
        for seed in 0..400 {
            let picked = ids(&library, &pick(&library, &plan, &history, seed));
            wrong += usize::from(picked.contains(&"q00".to_string()));
            right += usize::from(picked.contains(&"q01".to_string()));
        }
        assert!(
            wrong > 4 * right,
            "wrong {wrong} times, right {right} times"
        );
    }

    #[test]
    fn the_mix_leans_on_weak_topics() {
        let library = bank();
        let past = |n: usize, topic: &str, correct| Past {
            question: format!("q{n:02}"),
            topic: topic.into(),
            correct,
        };
        // Courts: 2 right of 8, so weak. Rights: 8 of 8.
        let mut history: Vec<Past> = (20..28)
            .map(|n| past(n, "pre.gs.polity.courts", n < 22))
            .collect();
        history.extend((0..8).map(|n| past(n, "pre.gs.polity.rights", true)));
        assert_eq!(weak_topics(&history), ["pre.gs.polity.courts"]);
        assert_eq!(
            weak_topics(&history[..7]),
            Vec::<String>::new(),
            "too few answers"
        );

        let courts_asked = |kind: TestKind| {
            let plan = TestPlan {
                count: 20,
                ..TestPlan::standard(kind, &library)
            };
            (0..200)
                .map(|seed| {
                    let picked = pick(&library, &plan, &history, seed);
                    picked.iter().filter(|&&ix| (20..30).contains(&ix)).count()
                })
                .sum::<usize>()
        };
        let (mix, plain) = (courts_asked(TestKind::Mix), courts_asked(TestKind::Mock));
        assert!(
            mix > plain,
            "the mix asked courts {mix} times, a plain test {plain}"
        );
    }

    #[test]
    fn a_new_attempt_files_each_answer_under_its_subject_and_topic() {
        let library = bank();
        let plan = TestPlan::standard(TestKind::Micro, &library);
        let test = new_attempt(&library, &plan, &[0, 35], nine_am());
        assert_eq!(test.kind, "micro");
        assert_eq!((test.seconds_allowed, test.started), (720, nine_am()));
        assert_eq!(test.answers[0].subject, "polity");
        assert_eq!(test.answers[0].topic, "pre.gs.polity.rights");
        assert_eq!(test.answers[1].subject, "economy");
        assert_eq!(test.answers[1].chosen, None);
    }
}

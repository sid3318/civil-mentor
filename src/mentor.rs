//! The mentor: plain, deterministic rules over the content and the student's
//! record: how a test is marked and which questions it asks (Phase 2), and
//! today's plan, the insights and the streak (Phase 3). Review dates come
//! with Phase 5. Rules that depend on the date take it as an argument.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike as _, NaiveDate, NaiveDateTime};

use crate::content::{Library, Note, Question, within};
use crate::store::{Answer, Attempt, Confidence, Past, PlanTask, Reading};
use crate::syllabus::{self, Status};

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

/// The subject a syllabus topic falls in.
pub fn subject_of_topic(topic: &str) -> Option<&'static Subject> {
    SUBJECTS
        .iter()
        .find(|subject| subject.nodes.iter().any(|node| within(topic, node)))
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
    /// A drill on one topic, by its syllabus id (see [`chapter_of`]).
    Topic(String),
    /// The first-launch test: questions spread across every subject.
    Diagnostic,
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
            TestKind::Topic(id) => format!("topic:{id}"),
            TestKind::Diagnostic => "diagnostic".into(),
        }
    }

    pub fn parse(key: &str) -> Option<TestKind> {
        Some(match key {
            "micro" => TestKind::Micro,
            "mini" => TestKind::Mini,
            "mock" => TestKind::Mock,
            "custom" => TestKind::Custom,
            "mix" => TestKind::Mix,
            "diagnostic" => TestKind::Diagnostic,
            _ => match key.split_once(':')? {
                ("subject", id) => TestKind::Subject(id.to_string()),
                ("paper", year) => TestKind::Paper(year.parse().ok()?),
                ("topic", id) => TestKind::Topic(id.to_string()),
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
            TestKind::Subject(_) | TestKind::Custom | TestKind::Mix | TestKind::Diagnostic => 20,
            TestKind::Topic(_) => DRILL_QUESTIONS,
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
            TestKind::Topic(_) => "Topic drill".into(),
            TestKind::Diagnostic => "Diagnostic test".into(),
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
            TestKind::Topic(_) => pool(library, &kind, &[]).len().min(kind.questions()),
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
        TestKind::Topic(id) => all
            .filter(scored)
            .filter(|&ix| library.questions[ix].topics.iter().any(|t| within(t, id)))
            .collect(),
        TestKind::Micro
        | TestKind::Mini
        | TestKind::Mock
        | TestKind::Mix
        | TestKind::Diagnostic => all.filter(scored).collect(),
    }
}

/// Weights for choosing questions: those got wrong last time come back most,
/// unseen ones next, and ones got right rarely.
const WRONG_WEIGHT: f64 = 2.0;
const UNSEEN_WEIGHT: f64 = 1.0;
const RIGHT_WEIGHT: f64 = 0.2;
/// The personalised mix asks more on weak topics.
const WEAK_WEIGHT: f64 = 1.5;

/// The topic an answer counts towards in analysis: the chapter of the
/// syllabus its topic sits in (`pre.gs.polity.parliament`), which is the level
/// a student thinks of as a topic. An id above that level is its own.
pub fn chapter_of(topic: &str) -> &str {
    match topic.match_indices('.').nth(3) {
        Some((ix, _)) => &topic[..ix],
        None => topic,
    }
}

/// How the student has done on one topic, counted at the [`chapter_of`] level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicScore {
    pub topic: String,
    pub right: usize,
    pub total: usize,
}

impl TopicScore {
    pub fn share(&self) -> f64 {
        self.right as f64 / self.total.max(1) as f64
    }

    /// Weakest first: the lower share right, then the more answers, then by id.
    fn weakness(&self, other: &TopicScore) -> Ordering {
        (self.right * other.total)
            .cmp(&(other.right * self.total))
            .then(other.total.cmp(&self.total))
            .then(self.topic.cmp(&other.topic))
    }
}

/// Right and total answers for every topic answered, by topic id.
pub fn topic_scores(history: &[Past]) -> Vec<TopicScore> {
    let mut tally: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for past in history {
        let (right, total) = tally.entry(chapter_of(&past.topic)).or_default();
        *right += usize::from(past.correct);
        *total += 1;
    }
    tally
        .into_iter()
        .map(|(topic, (right, total))| TopicScore {
            topic: topic.to_string(),
            right,
            total,
        })
        .collect()
}

/// The topics whose share right is under [`WEAK_BELOW`] over at least
/// [`WEAK_AFTER`] answers, by id.
pub fn weak_topics(history: &[Past]) -> Vec<String> {
    topic_scores(history)
        .into_iter()
        .filter(|s| s.total >= WEAK_AFTER && (s.right as f64) < WEAK_BELOW * s.total as f64)
        .map(|s| s.topic)
        .collect()
}

/// Chooses the questions for a test, by index into the library. A paper is
/// asked whole and in order. Any other test samples its pool without
/// repeats, weighted by the student's last outcome on each question (and,
/// for the personalised mix, by weak topic). The same seed gives the same
/// test. `history` is oldest first, as [`crate::store::Store::answered`] gives it.
pub fn pick(library: &Library, plan: &TestPlan, history: &[Past], seed: u64) -> Vec<usize> {
    let pool = pool(library, &plan.kind, &plan.subjects);
    match plan.kind {
        TestKind::Paper(_) => return pool,
        TestKind::Diagnostic => return spread(library, pool, plan.count, seed),
        _ => {}
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
            if weak.iter().any(|topic| within(topic_of(question), topic)) {
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

/// Up to `count` questions from `pool`, one subject at a time in
/// [`SUBJECTS`] order, so every subject is asked about evenly. Within a
/// subject the order is shuffled by `seed`.
fn spread(library: &Library, pool: Vec<usize>, count: usize, seed: u64) -> Vec<usize> {
    let mut random = Random(seed);
    let mut subjects: Vec<Vec<usize>> = SUBJECTS
        .iter()
        .map(|subject| {
            let mut keyed: Vec<(u64, usize)> = pool
                .iter()
                .filter(|&&ix| subject_of(&library.questions[ix]) == Some(subject))
                .map(|&ix| (random.next(), ix))
                .collect();
            keyed.sort();
            // Popped from the end, so the smallest key comes out first.
            keyed.into_iter().rev().map(|(_, ix)| ix).collect()
        })
        .collect();
    let mut picked = Vec::with_capacity(count);
    while picked.len() < count && subjects.iter().any(|s| !s.is_empty()) {
        for questions in &mut subjects {
            if picked.len() < count
                && let Some(ix) = questions.pop()
            {
                picked.push(ix);
            }
        }
    }
    picked
}

/// SplitMix64: small, fast and the same on every machine, which is all a
/// repeatable test needs.
pub(crate) struct Random(pub(crate) u64);

impl Random {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in (0, 1].
    pub(crate) fn unit(&mut self) -> f64 {
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
        title: match &plan.kind {
            TestKind::Topic(id) => format!("Topic drill: {}", topic_title(library, id)),
            kind => kind.title(),
        },
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

impl Score {
    /// Questions given an answer, right, wrong or dropped.
    pub fn answered(&self) -> usize {
        self.right + self.wrong + self.dropped
    }

    fn merge(&mut self, other: &Score) {
        self.right += other.right;
        self.wrong += other.wrong;
        self.blank += other.blank;
        self.dropped += other.dropped;
        self.marks += other.marks;
    }
}

// ---- today's plan ----------------------------------------------------------

/// The questions in a drill on the plan, and in a drill a Drill button starts.
pub const DRILL_QUESTIONS: usize = 10;
/// A plan drill shorter than this is left out, unless the topic has fewer questions.
const MIN_DRILL_QUESTIONS: usize = 5;
/// No task is planned for less time than this.
const MIN_TASK_MINUTES: u32 = 5;

/// The kinds of task in a day's plan, in the order the plan fills them.
/// Today's brief (Phase 4), due reviews (Phase 5) and a Mains answer
/// (Phase 6) become variants here, each with its step in [`today_plan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// Continue the note in progress, or start the next unread one.
    Note,
    /// A drill on the weakest topic.
    Drill,
}

impl TaskKind {
    /// How the kind is written in the store.
    pub fn key(self) -> &'static str {
        match self {
            TaskKind::Note => "note",
            TaskKind::Drill => "drill",
        }
    }

    pub fn parse(key: &str) -> Option<TaskKind> {
        match key {
            "note" => Some(TaskKind::Note),
            "drill" => Some(TaskKind::Drill),
            _ => None,
        }
    }
}

/// A day's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub day: NaiveDate,
    pub tasks: Vec<PlanTask>,
}

impl Plan {
    pub fn minutes(&self) -> u32 {
        self.tasks.iter().map(|task| task.minutes).sum()
    }
}

/// The plan for `today`, sized to `daily_minutes`. It fills the time in the
/// order of "Mentor rules": the note in progress, or else the next unread
/// note in the least-covered subject; then a drill on the weakest topic
/// with at least [`WEAK_AFTER`] answers. Each task takes what is left of the
/// day and is shortened to fit, or left out below its least useful size.
/// The caller saves it, so it stays frozen for the day.
pub fn today_plan(
    library: &Library,
    reading: &HashMap<String, Reading>,
    history: &[Past],
    daily_minutes: u32,
    today: NaiveDate,
) -> Plan {
    let mut left = daily_minutes;
    let mut tasks = Vec::new();
    let steps: [&dyn Fn(u32) -> Option<PlanTask>; 2] =
        [&|left| note_task(library, reading, left), &|left| {
            drill_task(library, history, left)
        }];
    for step in steps {
        if let Some(task) = step(left) {
            left -= task.minutes;
            tasks.push(task);
        }
    }
    Plan { day: today, tasks }
}

fn note_task(library: &Library, reading: &HashMap<String, Reading>, left: u32) -> Option<PlanTask> {
    if left < MIN_TASK_MINUTES {
        return None;
    }
    let (note, verb) = match in_progress(library, reading) {
        Some(ix) => (&library.notes[ix], "Continue"),
        None => (next_unread(library, reading)?, "Read"),
    };
    let spent = reading.get(&note.id).map_or(0, |r| r.seconds / 60);
    let minutes = note
        .minutes
        .saturating_sub(spent)
        .max(MIN_TASK_MINUTES)
        .min(left);
    Some(PlanTask {
        kind: TaskKind::Note.key().into(),
        target: note.id.clone(),
        title: format!("{verb}: {}", note.title),
        minutes,
        done: false,
    })
}

/// The note in progress: opened and not marked read, the most recently
/// opened first. By index into the library.
pub fn in_progress(library: &Library, reading: &HashMap<String, Reading>) -> Option<usize> {
    library
        .notes
        .iter()
        .enumerate()
        .filter_map(|(ix, note)| {
            let record = reading.get(&note.id).filter(|r| r.done.is_none())?;
            Some(((record.last, Reverse(ix)), ix))
        })
        .max_by_key(|(key, _)| *key)
        .map(|(_, ix)| ix)
}

/// The note to open for a topic: the first that covers it or a part of it.
pub fn note_for_topic(library: &Library, topic: &str) -> Option<usize> {
    library.notes.iter().position(|note| {
        note.topics
            .iter()
            .any(|t| within(t, topic) || within(topic, t))
    })
}

/// The first unopened note, in syllabus order, of the subject with the
/// smallest share of its syllabus read. Ties go to the subject that comes
/// first in the syllabus.
fn next_unread<'a>(library: &'a Library, reading: &HashMap<String, Reading>) -> Option<&'a Note> {
    let statuses = syllabus::statuses(library, reading);
    let position = |id: &str| library.topics.iter().position(|t| t.id == id);
    let mut shares: HashMap<&str, (usize, usize)> = HashMap::new();
    library
        .notes
        .iter()
        .enumerate()
        .filter(|(_, note)| !reading.contains_key(&note.id))
        .filter_map(|(ix, note)| {
            let first = note.topics.first()?;
            let subject = subject_node(first);
            let share = *shares
                .entry(subject)
                .or_insert_with(|| syllabus::coverage(library, &statuses, subject));
            Some((share, position(subject), position(first), ix, note))
        })
        .min_by(|a, b| {
            let ((done_a, total_a), (done_b, total_b)) = (a.0, b.0);
            (done_a * total_b.max(1))
                .cmp(&(done_b * total_a.max(1)))
                .then((a.1, a.2, a.3).cmp(&(b.1, b.2, b.3)))
        })
        .map(|candidate| candidate.4)
}

/// The subject a topic sits in: `mains.gs2.polity` for any topic under it.
fn subject_node(topic: &str) -> &str {
    match topic.match_indices('.').nth(2) {
        Some((ix, _)) => &topic[..ix],
        None => topic,
    }
}

fn drill_task(library: &Library, history: &[Past], left: u32) -> Option<PlanTask> {
    let weakest = topic_scores(history)
        .into_iter()
        .filter(|score| score.total >= WEAK_AFTER)
        .min_by(TopicScore::weakness)?;
    let pool = pool(library, &TestKind::Topic(weakest.topic.clone()), &[]).len();
    let fit = (left * 60 / SECONDS_PER_QUESTION) as usize;
    let questions = DRILL_QUESTIONS.min(pool).min(fit);
    if questions == 0 || questions < MIN_DRILL_QUESTIONS.min(pool) {
        return None;
    }
    Some(PlanTask {
        kind: TaskKind::Drill.key().into(),
        title: format!(
            "Drill: {}, {questions} questions",
            topic_title(library, &weakest.topic)
        ),
        target: weakest.topic,
        minutes: (questions as u32 * SECONDS_PER_QUESTION).div_ceil(60),
        done: false,
    })
}

/// The test a drill task sits: as many questions as fit its minutes.
pub fn task_drill(library: &Library, task: &PlanTask) -> TestPlan {
    topic_drill(
        library,
        &task.target,
        (task.minutes * 60 / SECONDS_PER_QUESTION) as usize,
    )
}

/// A drill of up to `questions` on one topic, at 72 seconds a question.
pub fn topic_drill(library: &Library, topic: &str, questions: usize) -> TestPlan {
    let kind = TestKind::Topic(topic.to_string());
    let count = questions.min(pool(library, &kind, &[]).len());
    TestPlan {
        kind,
        subjects: Vec::new(),
        count,
        seconds: count as u32 * SECONDS_PER_QUESTION,
    }
}

/// A topic's title from the syllabus, or its id if the syllabus lacks it.
pub fn topic_title(library: &Library, id: &str) -> String {
    library
        .topic(id)
        .map_or_else(|| id.to_string(), |topic| topic.title.clone())
}

/// Whether a task in the plan for `day` is done: ticked by the student, or
/// done another way. A note is done once it is marked read, and a drill
/// once a drill on its topic is submitted that day. `tests` are finished sittings.
pub fn task_done(
    task: &PlanTask,
    day: NaiveDate,
    reading: &HashMap<String, Reading>,
    tests: &[Attempt],
) -> bool {
    task.done
        || match TaskKind::parse(&task.kind) {
            Some(TaskKind::Note) => reading.get(&task.target).is_some_and(|r| r.done.is_some()),
            Some(TaskKind::Drill) => {
                let key = TestKind::Topic(task.target.clone()).key();
                tests
                    .iter()
                    .any(|t| t.kind == key && t.finished.is_some_and(|f| f.date() == day))
            }
            None => false,
        }
}

/// The number of days in a row, ending today or yesterday, with any study
/// recorded. `study` is seconds by day.
pub fn streak(study: &BTreeMap<NaiveDate, u32>, today: NaiveDate) -> u32 {
    let studied = |day: NaiveDate| study.get(&day).is_some_and(|&seconds| seconds > 0);
    let mut day = today;
    if !studied(day) {
        match day.pred_opt() {
            Some(yesterday) => day = yesterday,
            None => return 0,
        }
    }
    let mut days = 0;
    while studied(day) {
        days += 1;
        match day.pred_opt() {
            Some(before) => day = before,
            None => break,
        }
    }
    days
}

/// One question from the bank for the day, the same all day.
pub fn question_of_the_day(library: &Library, today: NaiveDate) -> Option<usize> {
    let scored: Vec<usize> = (0..library.questions.len())
        .filter(|&ix| library.questions[ix].answer.is_some())
        .collect();
    if scored.is_empty() {
        return None;
    }
    let pick = Random(today.num_days_from_ce() as u64).next() % scored.len() as u64;
    Some(scored[pick as usize])
}

// ---- insights --------------------------------------------------------------

/// Insights about speed, confidence and attempts read the last this many tests.
pub const RECENT_TESTS: usize = 10;
/// The most high-yield topics listed.
const HIGH_YIELD: usize = 5;
/// A topic is high-yield once UPSC has asked at least this many questions on it.
const HIGH_YIELD_AFTER: usize = 2;

/// What the mentor makes of the student's record. See "Mentor rules".
#[derive(Debug, Clone, PartialEq)]
pub struct Insights {
    /// Topics under 50% right over at least 8 answers, weakest first.
    pub weak: Vec<TopicScore>,
    /// How many recent tests the next four figures cover.
    pub tests: usize,
    /// Marks won or lost at each confidence level in those tests, Sure,
    /// 50-50 and Guess first and untagged answers (`None`) last. A level
    /// with only blanks is left out.
    pub confidence: Vec<(Option<Confidence>, Score)>,
    /// Every answer in those tests together: what was attempted and what it scored.
    pub overall: Score,
    /// Seconds spent per question asked, against the 72-second budget.
    pub seconds_per_question: Option<f64>,
    /// Prelims topics UPSC asks about often that the student has not
    /// finished reading, with the number of past questions, most asked first.
    pub high_yield: Vec<(String, usize)>,
    /// Seconds studied in the 7 days ending today.
    pub week_seconds: u32,
    /// The daily study time times 7, in seconds.
    pub week_goal: u32,
    pub today_seconds: u32,
}

/// A piece of advice for the Today screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub text: String,
    /// The topic it is about, for its Open note and Drill buttons.
    pub topic: Option<String>,
}

/// The insights under "Mentor rules", from the answer `history` (oldest
/// first), the finished `tests` (newest first), seconds studied by day,
/// and the daily goal.
pub fn insights(
    library: &Library,
    reading: &HashMap<String, Reading>,
    history: &[Past],
    tests: &[Attempt],
    study: &BTreeMap<NaiveDate, u32>,
    daily_minutes: u32,
    today: NaiveDate,
) -> Insights {
    let mut weak: Vec<TopicScore> = topic_scores(history)
        .into_iter()
        .filter(|s| s.total >= WEAK_AFTER && (s.right as f64) < WEAK_BELOW * s.total as f64)
        .collect();
    weak.sort_by(TopicScore::weakness);

    let recent = &tests[..tests.len().min(RECENT_TESTS)];
    let mut by_confidence: BTreeMap<(bool, Option<Confidence>), Score> = BTreeMap::new();
    let mut overall = Score::default();
    for test in recent {
        overall.merge(&score(test));
        for (confidence, part) in score_by(test, |a| a.confidence) {
            by_confidence
                .entry((confidence.is_none(), confidence))
                .or_default()
                .merge(&part);
        }
    }
    let confidence = by_confidence
        .into_iter()
        .filter(|(_, score)| score.answered() > 0)
        .map(|((_, level), score)| (level, score))
        .collect();
    let asked: usize = recent.iter().map(|t| t.answers.len()).sum();
    let seconds: u32 = recent.iter().map(|t| t.seconds_used).sum();
    let seconds_per_question = (asked > 0).then(|| seconds as f64 / asked as f64);

    let statuses = syllabus::statuses(library, reading);
    let mut asked_on: BTreeMap<&str, usize> = BTreeMap::new();
    for question in library.questions.iter().filter(|q| q.year().is_some()) {
        let chapters: HashSet<&str> = question
            .topics
            .iter()
            .filter(|t| t.starts_with("pre."))
            .map(|t| chapter_of(t))
            .collect();
        for chapter in chapters {
            *asked_on.entry(chapter).or_default() += 1;
        }
    }
    let read = |topic: &str| {
        library
            .topics
            .iter()
            .position(|t| t.id == topic)
            .is_some_and(|ix| statuses[ix] == Status::Done)
    };
    let mut high_yield: Vec<(String, usize)> = asked_on
        .into_iter()
        .filter(|&(topic, asked)| asked >= HIGH_YIELD_AFTER && !read(topic))
        .map(|(topic, asked)| (topic.to_string(), asked))
        .collect();
    high_yield.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    high_yield.truncate(HIGH_YIELD);

    let week_start = today - chrono::Duration::days(6);
    Insights {
        weak,
        tests: recent.len(),
        confidence,
        overall,
        seconds_per_question,
        high_yield,
        week_seconds: study.range(week_start..=today).map(|(_, s)| s).sum(),
        week_goal: daily_minutes * 60 * 7,
        today_seconds: study.get(&today).copied().unwrap_or(0),
    }
}

impl Insights {
    /// The average marks an attempted question earns: positive when
    /// attempting more should raise the net score.
    pub fn marks_per_attempt(&self) -> Option<f64> {
        let attempted = self.overall.answered();
        (attempted > 0).then(|| self.overall.marks / attempted as f64)
    }

    /// The insights as advice, most useful first: the weakest topic, what
    /// guessing does, attempts against marks, speed, study time against the
    /// goal, then the most-asked unread topic.
    pub fn cards(&self, library: &Library) -> Vec<Card> {
        let mut cards = Vec::new();
        let card = |text: String| Card { text, topic: None };
        if let Some(weak) = self.weak.first() {
            cards.push(Card {
                text: format!(
                    "{}: {}% right over {} questions.",
                    topic_title(library, &weak.topic),
                    percent(weak.right, weak.total),
                    weak.total
                ),
                topic: Some(weak.topic.clone()),
            });
        }
        let guesses = self
            .confidence
            .iter()
            .find(|(level, _)| *level == Some(Confidence::Guess));
        if let Some((_, guess)) = guesses.filter(|(_, g)| g.answered() > 0 && g.marks != 0.0) {
            let per_test = guess.marks / self.tests.max(1) as f64;
            cards.push(card(if per_test < 0.0 {
                format!(
                    "Guesses cost you {} marks per test. Skip a question unless you can rule out two options.",
                    one_decimal(-per_test)
                )
            } else {
                format!(
                    "Guesses gain you {} marks per test. Keep guessing once you can rule out an option.",
                    one_decimal(per_test)
                )
            }));
        }
        if let Some(per_attempt) = self.marks_per_attempt() {
            let asked = self.overall.answered() + self.overall.blank;
            let attempted = percent(self.overall.answered(), asked);
            let right = percent(self.overall.right, self.overall.answered());
            cards.push(card(if per_attempt > 0.0 {
                format!(
                    "You attempt {attempted}% of questions and get {right}% of those right, so each attempt earns {} marks. Attempting more should raise your score.",
                    one_decimal(per_attempt)
                )
            } else {
                format!(
                    "You attempt {attempted}% of questions and get {right}% of those right, so each attempt loses {} marks. Attempt fewer, more carefully.",
                    one_decimal(-per_attempt)
                )
            }));
        }
        if let Some(pace) = self.seconds_per_question {
            let budget = SECONDS_PER_QUESTION as f64;
            cards.push(card(format!(
                "You spend {} seconds a question against the {} second budget. {}",
                pace.round(),
                SECONDS_PER_QUESTION,
                if pace > budget {
                    "Speed up to finish the paper."
                } else {
                    "That leaves time to go back to marked questions."
                }
            )));
        }
        if self.week_seconds > 0 {
            cards.push(card(format!(
                "You studied {} in the last 7 days, {}% of your {} goal.",
                hours(self.week_seconds),
                percent(self.week_seconds as usize, self.week_goal as usize),
                hours(self.week_goal)
            )));
        }
        if let Some((topic, asked)) = self.high_yield.first() {
            cards.push(Card {
                text: format!(
                    "{}: {asked} past UPSC questions, and you have not read it yet.",
                    topic_title(library, topic)
                ),
                topic: Some(topic.clone()),
            });
        }
        cards
    }
}

/// A whole-number percentage, rounded.
pub fn percent(part: usize, whole: usize) -> usize {
    (part * 100 + whole / 2).checked_div(whole).unwrap_or(0)
}

/// "0.3", "1.2", "4".
fn one_decimal(value: f64) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches(".0").to_string()
}

/// "3 h 10 min", "45 min".
pub fn hours(seconds: u32) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
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
            TestKind::Topic("pre.gs.polity.parliament".into()),
            TestKind::Diagnostic,
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
    fn the_diagnostic_spreads_its_questions_across_the_subjects() {
        let library = bank();
        let plan = TestPlan::standard(TestKind::Diagnostic, &library);
        assert_eq!((plan.count, plan.seconds), (20, 20 * 72));
        let picked = pick(&library, &plan, &[], 5);
        assert_eq!(picked, pick(&library, &plan, &[], 5), "repeatable");
        // Polity and economy take turns until the 9 scored economy
        // questions run out; polity fills the rest.
        let economy = picked.iter().filter(|&&ix| ix >= 30).count();
        assert_eq!((picked.len(), economy), (20, 9));
        assert!(picked[0] < 30 && picked[1] >= 30, "polity, then economy");
        let mut unique = picked.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 20, "no question twice");
    }

    #[test]
    fn weak_topics_are_counted_by_chapter() {
        assert_eq!(
            chapter_of("pre.gs.polity.parliament.bills.money-bills"),
            "pre.gs.polity.parliament"
        );
        assert_eq!(
            chapter_of("pre.gs.polity.parliament"),
            "pre.gs.polity.parliament"
        );
        assert_eq!(chapter_of("pre.gs.polity"), "pre.gs.polity");
        let past = |topic: &str, correct| Past {
            question: "q".into(),
            topic: topic.into(),
            correct,
        };
        // Two subtopics of one chapter: 3 right of 8 together, so weak.
        let mut history: Vec<Past> = (0..4)
            .map(|n| past("pre.gs.polity.parliament.bills", n == 0))
            .collect();
        history.extend((0..4).map(|n| past("pre.gs.polity.parliament.committees", n < 2)));
        assert_eq!(weak_topics(&history), ["pre.gs.polity.parliament"]);
        assert_eq!(
            topic_scores(&history),
            [TopicScore {
                topic: "pre.gs.polity.parliament".into(),
                right: 3,
                total: 8
            }]
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

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn the_streak_counts_days_in_a_row_ending_today_or_yesterday() {
        let study = |days: &[(u32, u32)]| -> BTreeMap<NaiveDate, u32> {
            days.iter().map(|&(d, s)| (day(d), s)).collect()
        };
        assert_eq!(streak(&study(&[]), day(16)), 0);
        // 14, 15 and 16: three days, today included.
        assert_eq!(streak(&study(&[(14, 60), (15, 60), (16, 60)]), day(16)), 3);
        // Nothing yet today: the run ending yesterday still counts.
        assert_eq!(streak(&study(&[(14, 60), (15, 60)]), day(16)), 2);
        // A gap on the 13th ends the run; the 12th is not counted.
        assert_eq!(streak(&study(&[(12, 60), (14, 60), (15, 60)]), day(16)), 2);
        // Last studied two days ago: the streak is broken.
        assert_eq!(streak(&study(&[(13, 60), (14, 60)]), day(16)), 0);
        // A day with a block of no time is not a day of study.
        assert_eq!(streak(&study(&[(15, 0), (16, 30)]), day(16)), 1);
    }

    /// Polity (rights and courts) and economy (inflation), a note on each
    /// chapter, 10 sample questions each on courts and rights, and a 2025
    /// paper with 3 questions on inflation and 2 on courts.
    fn plan_library() -> Library {
        let syllabus = r#"
topic = [
  { id = "pre", title = "Prelims" },
  { id = "pre.gs", title = "GS" },
  { id = "pre.gs.polity", title = "Polity" },
  { id = "pre.gs.polity.rights", title = "Rights" },
  { id = "pre.gs.polity.rights.equality", title = "Equality" },
  { id = "pre.gs.polity.rights.writs", title = "Writs" },
  { id = "pre.gs.polity.courts", title = "Courts" },
  { id = "pre.gs.polity.courts.supreme", title = "Supreme Court" },
  { id = "pre.gs.economy", title = "Economy" },
  { id = "pre.gs.economy.inflation", title = "Inflation" },
  { id = "pre.gs.economy.inflation.cpi", title = "CPI" },
]"#;
        let note = |id: &str, topic: &str| {
            format!("---\nid: {id}\ntitle: {id}\ntopics: [{topic}]\n---\n## One\nText.\n")
        };
        let mut questions = String::new();
        for n in 0..25 {
            let (topic, source) = match n {
                0..10 => ("pre.gs.polity.courts.supreme", "Civil Mentor sample"),
                10..20 => ("pre.gs.polity.rights.writs", "Civil Mentor sample"),
                20..23 => ("pre.gs.economy.inflation.cpi", "UPSC Prelims 2025"),
                _ => ("pre.gs.polity.courts.supreme", "UPSC Prelims 2025"),
            };
            questions.push_str(&format!(
                "[[question]]\nid = \"p{n:02}\"\ntopics = [\"{topic}\"]\nsource = \"{source}\"\n\
                 stem = \"?\"\noptions = [\"1\", \"2\", \"3\", \"4\"]\nanswer = \"A\"\nexplanation = \".\"\n\n"
            ));
        }
        let mut library = Library::from_files([
            ("syllabus.toml".to_string(), syllabus.to_string()),
            (
                "notes/rights.md".to_string(),
                note("rights", "pre.gs.polity.rights"),
            ),
            (
                "notes/courts.md".to_string(),
                note("courts", "pre.gs.polity.courts"),
            ),
            (
                "notes/inflation.md".to_string(),
                note("inflation", "pre.gs.economy.inflation"),
            ),
            ("questions/bank.toml".to_string(), questions),
        ]);
        assert_eq!(library.problems, Vec::<String>::new());
        for (id, minutes) in [("rights", 30), ("courts", 20), ("inflation", 25)] {
            let ix = library.notes.iter().position(|n| n.id == id).unwrap();
            library.notes[ix].minutes = minutes;
        }
        library
    }

    fn opened(last: u32, seconds: u32, done: bool) -> Reading {
        Reading {
            first: day(1),
            last: day(last),
            done: done.then(|| day(last)),
            bookmarked: false,
            seconds,
            demo: false,
        }
    }

    fn answers_on(topic: &str, right: usize, total: usize) -> Vec<Past> {
        (0..total)
            .map(|n| Past {
                question: format!("{topic}-{n}"),
                topic: topic.into(),
                correct: n < right,
            })
            .collect()
    }

    fn titles(plan: &Plan) -> Vec<(String, u32)> {
        plan.tasks
            .iter()
            .map(|t| (t.title.clone(), t.minutes))
            .collect()
    }

    #[test]
    fn a_new_student_is_planned_the_first_note_of_the_first_subject() {
        let library = plan_library();
        // Polity has 0 of 3 topics read and economy 0 of 1: a tie, so the
        // subject first in the syllabus, and its first note. No topic has
        // 8 answers, so no drill.
        let plan = today_plan(&library, &HashMap::new(), &[], 90, day(16));
        assert_eq!(plan.day, day(16));
        assert_eq!(titles(&plan), [("Read: rights".to_string(), 30)]);
        assert_eq!(plan.tasks[0].kind, "note");
        assert_eq!(plan.tasks[0].target, "rights");
        assert_eq!(plan.minutes(), 30);
    }

    #[test]
    fn the_plan_reads_the_least_covered_subject_and_drills_the_weakest_topic() {
        let library = plan_library();
        // Rights is read, so polity has 2 of its 3 topics read and economy
        // 0 of 1: inflation comes next, 25 minutes.
        let reading = HashMap::from([("rights".to_string(), opened(10, 1800, true))]);
        // Courts 2 of 8 right (25%), rights 6 of 10 (60%): courts is the
        // weakest topic with 8 answers. 10 questions at 72 s is 12 minutes.
        let mut history = answers_on("pre.gs.polity.courts.supreme", 2, 8);
        history.extend(answers_on("pre.gs.polity.rights.writs", 6, 10));
        let plan = today_plan(&library, &reading, &history, 90, day(16));
        assert_eq!(
            titles(&plan),
            [
                ("Read: inflation".to_string(), 25),
                ("Drill: Courts, 10 questions".to_string(), 12)
            ]
        );
        assert_eq!(plan.tasks[1].kind, "drill");
        assert_eq!(plan.tasks[1].target, "pre.gs.polity.courts");
        let drill = task_drill(&library, &plan.tasks[1]);
        assert_eq!(drill.kind, TestKind::Topic("pre.gs.polity.courts".into()));
        assert_eq!((drill.count, drill.seconds), (10, 720));
        assert!(drill.fits(&library), "12 courts questions fill 10");
    }

    #[test]
    fn the_plan_continues_the_note_in_progress_and_fits_the_daily_time() {
        let library = plan_library();
        // Courts was opened on the 15th with 10 minutes read; inflation on
        // the 14th. Courts is the latest, with 20 - 10 = 10 minutes left.
        let reading = HashMap::from([
            ("courts".to_string(), opened(15, 600, false)),
            ("inflation".to_string(), opened(14, 0, false)),
        ]);
        let history = answers_on("pre.gs.polity.courts.supreme", 2, 8);
        // 15 minutes: the note takes 10, and 5 minutes fit 4 questions,
        // under the 5 a drill needs. No drill.
        let plan = today_plan(&library, &reading, &history, 15, day(16));
        assert_eq!(titles(&plan), [("Continue: courts".to_string(), 10)]);
        // 20 minutes: 10 left fit 8 questions, 576 s, which is 10 minutes.
        let plan = today_plan(&library, &reading, &history, 20, day(16));
        assert_eq!(
            titles(&plan),
            [
                ("Continue: courts".to_string(), 10),
                ("Drill: Courts, 8 questions".to_string(), 10)
            ]
        );
        assert_eq!(task_drill(&library, &plan.tasks[1]).count, 8);
        // 4 minutes is under the shortest task: nothing is planned.
        assert_eq!(
            today_plan(&library, &reading, &history, 4, day(16)).tasks,
            []
        );
    }

    #[test]
    fn a_task_is_done_when_ticked_or_done_another_way() {
        let task = |kind: TaskKind, target: &str| PlanTask {
            kind: kind.key().into(),
            target: target.into(),
            title: String::new(),
            minutes: 10,
            done: false,
        };
        let note = task(TaskKind::Note, "courts");
        let mut reading = HashMap::from([("courts".to_string(), opened(16, 60, false))]);
        assert!(!task_done(&note, day(16), &reading, &[]));
        assert!(task_done(
            &PlanTask {
                done: true,
                ..note.clone()
            },
            day(16),
            &reading,
            &[]
        ));
        reading.insert("courts".into(), opened(16, 60, true));
        assert!(task_done(&note, day(16), &reading, &[]), "marked read");

        let drill = task(TaskKind::Drill, "pre.gs.polity.courts");
        let mut sat = attempt(vec![], GS_RIGHT, GS_WRONG);
        sat.kind = "topic:pre.gs.polity.courts".into();
        sat.finished = Some(day(15).and_hms_opt(20, 0, 0).unwrap());
        assert!(
            !task_done(&drill, day(16), &reading, &[sat.clone()]),
            "sat yesterday"
        );
        sat.finished = Some(day(16).and_hms_opt(20, 0, 0).unwrap());
        assert!(task_done(&drill, day(16), &reading, &[sat]), "sat today");
    }

    /// Two tests, worked by hand.
    ///
    /// The newest, 10 questions in 900 s: Sure 3 right and 1 wrong
    /// (6 - 2/3 = 5.33), Guess 1 right and 4 wrong (2 - 8/3 = -0.67), 1 blank.
    /// The older, 5 questions in 300 s: 50-50 2 right and 1 wrong
    /// (4 - 2/3 = 3.33), 1 untagged wrong (-0.67), 1 blank.
    ///
    /// Together 6 right, 7 wrong, 2 blank: 12 - 14/3 = 7.33 marks from 13
    /// attempts, 0.56 an attempt; 1200 s over 15 questions is 80 s each.
    fn two_tests() -> Vec<Attempt> {
        let tagged = |confidence: Option<Confidence>, correct: Option<bool>| Answer {
            confidence,
            ..answer(correct.map(|_| 0), correct)
        };
        let some = |n: usize, confidence, correct| vec![tagged(confidence, correct); n];
        let newest = [
            some(3, Some(Confidence::Sure), Some(true)),
            some(1, Some(Confidence::Sure), Some(false)),
            some(1, Some(Confidence::Guess), Some(true)),
            some(4, Some(Confidence::Guess), Some(false)),
            some(1, None, None),
        ]
        .concat();
        let older = [
            some(2, Some(Confidence::FiftyFifty), Some(true)),
            some(1, Some(Confidence::FiftyFifty), Some(false)),
            some(1, None, Some(false)),
            some(1, None, None),
        ]
        .concat();
        let mut newest = attempt(newest, GS_RIGHT, GS_WRONG);
        newest.seconds_used = 900;
        let mut older = attempt(older, GS_RIGHT, GS_WRONG);
        older.seconds_used = 300;
        vec![newest, older]
    }

    #[test]
    fn insights_match_a_hand_worked_record() {
        let library = plan_library();
        let mut history = answers_on("pre.gs.polity.courts.supreme", 2, 8);
        history.extend(answers_on("pre.gs.polity.rights.writs", 6, 10));
        // 1800 s today and 3600 s six days ago count; 7200 s seven days ago does not.
        let study = BTreeMap::from([(day(9), 7200), (day(10), 3600), (day(16), 1800)]);
        let reading = HashMap::from([("inflation".to_string(), opened(12, 1500, true))]);
        let insights = insights(
            &library,
            &reading,
            &history,
            &two_tests(),
            &study,
            60,
            day(16),
        );

        assert_eq!(
            insights.weak,
            [TopicScore {
                topic: "pre.gs.polity.courts".into(),
                right: 2,
                total: 8
            }],
            "rights is 60%, so only courts is weak"
        );
        assert_eq!(insights.tests, 2);
        let marks: Vec<(Option<Confidence>, usize, usize, f64)> = insights
            .confidence
            .iter()
            .map(|(level, s)| (*level, s.right, s.wrong, (s.marks * 100.0).round() / 100.0))
            .collect();
        assert_eq!(
            marks,
            [
                (Some(Confidence::Sure), 3, 1, 5.33),
                (Some(Confidence::FiftyFifty), 2, 1, 3.33),
                (Some(Confidence::Guess), 1, 4, -0.67),
                (None, 0, 1, -0.67),
            ]
        );
        let overall = insights.overall;
        assert_eq!((overall.right, overall.wrong, overall.blank), (6, 7, 2));
        assert!((overall.marks - (12.0 - 14.0 / 3.0)).abs() < 1e-9);
        assert!((insights.marks_per_attempt().unwrap() - (22.0 / 3.0) / 13.0).abs() < 1e-9);
        assert_eq!(insights.seconds_per_question, Some(80.0));
        // Inflation is read, so of the past-paper topics only courts is left.
        assert_eq!(
            insights.high_yield,
            [("pre.gs.polity.courts".to_string(), 2)]
        );
        assert_eq!(
            (
                insights.week_seconds,
                insights.week_goal,
                insights.today_seconds
            ),
            (5400, 60 * 60 * 7, 1800)
        );

        let cards: Vec<String> = insights
            .cards(&library)
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert_eq!(
            cards,
            [
                "Courts: 25% right over 8 questions.",
                "Guesses cost you 0.3 marks per test. Skip a question unless you can rule out two options.",
                "You attempt 87% of questions and get 46% of those right, so each attempt earns 0.6 marks. Attempting more should raise your score.",
                "You spend 80 seconds a question against the 72 second budget. Speed up to finish the paper.",
                "You studied 1 h 30 min in the last 7 days, 21% of your 7 h goal.",
                "Courts: 2 past UPSC questions, and you have not read it yet.",
            ]
        );
        assert_eq!(
            insights.cards(&library)[0].topic.as_deref(),
            Some("pre.gs.polity.courts")
        );
    }

    #[test]
    fn no_record_gives_no_advice() {
        let library = plan_library();
        let insights = insights(
            &library,
            &HashMap::new(),
            &[],
            &[],
            &BTreeMap::new(),
            90,
            day(16),
        );
        assert_eq!(insights.tests, 0);
        assert_eq!(insights.seconds_per_question, None);
        assert_eq!(insights.marks_per_attempt(), None);
        // Unread, inflation (3 past questions) comes before courts (2).
        assert_eq!(
            insights.high_yield,
            [
                ("pre.gs.economy.inflation".to_string(), 3),
                ("pre.gs.polity.courts".to_string(), 2)
            ]
        );
        assert_eq!(
            insights.cards(&library).len(),
            1,
            "only the high-yield topic"
        );
    }

    #[test]
    fn the_question_of_the_day_holds_all_day_and_is_scored() {
        let library = bank();
        let today = question_of_the_day(&library, day(16)).unwrap();
        assert_eq!(question_of_the_day(&library, day(16)), Some(today));
        assert!(library.questions[today].answer.is_some());
        let week: HashSet<usize> = (10..17)
            .filter_map(|d| question_of_the_day(&library, day(d)))
            .collect();
        assert!(week.len() > 1, "it changes from day to day");
    }
}

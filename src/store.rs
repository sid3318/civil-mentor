//! Everything the student does, kept in one SQLite file in the OS app-data folder.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension as _, params};

/// Schema changes, oldest first. Never edit an entry that has shipped: add a new
/// one. `PRAGMA user_version` records how many have been applied to a file.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT",
    // One row per note the student has opened. Dates are YYYY-MM-DD.
    "CREATE TABLE reading (
        item TEXT PRIMARY KEY,
        first TEXT NOT NULL,
        last TEXT NOT NULL,
        done TEXT,
        bookmarked INTEGER NOT NULL DEFAULT 0
    ) STRICT",
    // One row per test sitting, and one per question in it. Answers are
    // saved as they are given, so an unfinished sitting can be resumed.
    // Times are local, YYYY-MM-DD HH:MM:SS; `finished` is empty until it is
    // submitted. Marks are kept per sitting so other markings can be added.
    "CREATE TABLE attempt (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        started TEXT NOT NULL,
        finished TEXT,
        seconds_allowed INTEGER NOT NULL,
        seconds_used INTEGER NOT NULL DEFAULT 0,
        right_marks REAL NOT NULL,
        wrong_marks REAL NOT NULL,
        current INTEGER NOT NULL DEFAULT 0
    ) STRICT;
    CREATE TABLE answer (
        attempt INTEGER NOT NULL REFERENCES attempt (id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        question TEXT NOT NULL,
        subject TEXT NOT NULL,
        topic TEXT NOT NULL,
        chosen INTEGER,
        correct INTEGER,
        confidence TEXT,
        marked INTEGER NOT NULL DEFAULT 0,
        struck INTEGER NOT NULL DEFAULT 0,
        visited INTEGER NOT NULL DEFAULT 0,
        seconds INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (attempt, position)
    ) STRICT",
    // Study time: one row per block of time spent reading a note or in a
    // test, counted as it goes. The day's plan, made once a day and then
    // frozen. Time spent on each note.
    "CREATE TABLE session (
        id INTEGER PRIMARY KEY,
        day TEXT NOT NULL,
        kind TEXT NOT NULL,
        item TEXT NOT NULL,
        seconds INTEGER NOT NULL DEFAULT 0
    ) STRICT;
    CREATE INDEX session_day ON session (day);
    CREATE TABLE plan_task (
        day TEXT NOT NULL,
        position INTEGER NOT NULL,
        kind TEXT NOT NULL,
        target TEXT NOT NULL,
        title TEXT NOT NULL,
        minutes INTEGER NOT NULL,
        done INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (day, position)
    ) STRICT;
    ALTER TABLE reading ADD COLUMN seconds INTEGER NOT NULL DEFAULT 0",
    // Rows made up by the demo history carry this mark, so removing them
    // never touches the student's own.
    "ALTER TABLE attempt ADD COLUMN demo INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE reading ADD COLUMN demo INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE session ADD COLUMN demo INTEGER NOT NULL DEFAULT 0",
    // Spaced review: one row per mistake (`question:<id>`) or flashcard
    // (`card:<id>`) in the five boxes. `due` is empty once it has left box 5.
    "CREATE TABLE review (
        item TEXT PRIMARY KEY,
        box INTEGER NOT NULL,
        due TEXT,
        added TEXT NOT NULL,
        reviewed TEXT,
        demo INTEGER NOT NULL DEFAULT 0
    ) STRICT",
    // What the student marks and writes: a highlight or clip is a range of one
    // part of a note, story or editorial, and `my_note` is a note of their own.
    "CREATE TABLE mark (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        item TEXT NOT NULL,
        part TEXT NOT NULL,
        kind TEXT NOT NULL,
        start INTEGER NOT NULL,
        end INTEGER NOT NULL,
        quote TEXT NOT NULL,
        comment TEXT,
        added TEXT NOT NULL
    ) STRICT;
    CREATE INDEX mark_by_item ON mark (item);
    CREATE TABLE my_note (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT NOT NULL,
        body TEXT NOT NULL,
        added TEXT NOT NULL,
        updated TEXT NOT NULL
    ) STRICT",
    // A Mains answer, saved as it is typed. `submitted` is empty while it is
    // a draft; `seconds` is the time on its clock, overrun included; `points`
    // lists the points to cover the student ticked, by position, separated by
    // commas; the last seven columns are the student's own scores, 0 to 5,
    // empty until given.
    "CREATE TABLE mains_answer (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        question TEXT NOT NULL,
        text TEXT NOT NULL,
        started TEXT NOT NULL,
        submitted TEXT,
        seconds INTEGER NOT NULL DEFAULT 0,
        points TEXT NOT NULL DEFAULT '',
        directive INTEGER,
        introduction INTEGER,
        content INTEGER,
        dimensions INTEGER,
        examples INTEGER,
        conclusion INTEGER,
        presentation INTEGER
    ) STRICT;
    CREATE INDEX mains_answer_by_question ON mains_answer (question)",
];

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// ponytail: queries run on the UI thread. They take microseconds at one
// student's volume; move them to the background executor if one ever shows up
// in a frame profile.
pub struct Store {
    conn: Connection,
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    Light,
    Dark,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System];

    pub fn name(self) -> &'static str {
        match self {
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::System => "System",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The day of the Prelims paper the student is preparing for.
    pub exam_date: NaiveDate,
    /// How long the student means to study each day. Today's plan is sized to it.
    pub daily_minutes: u32,
    pub theme: ThemeChoice,
    /// Body text size in the reader, in pixels.
    pub reader_size: u32,
    /// Whether the reader uses the wide column.
    pub reader_wide: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // A placeholder until the student sets their own date in Settings.
            exam_date: NaiveDate::from_ymd_opt(2027, 5, 23).expect("valid date"),
            daily_minutes: 90,
            theme: ThemeChoice::Light,
            reader_size: 17,
            reader_wide: false,
        }
    }
}

/// What the student has done with one note. A note with no `Reading` has
/// never been opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// The day it was first opened.
    pub first: NaiveDate,
    /// The day it was last opened.
    pub last: NaiveDate,
    /// The day the student marked it read.
    pub done: Option<NaiveDate>,
    pub bookmarked: bool,
    /// Time spent with the note open, counted while the student is active.
    pub seconds: u32,
    /// Made up by the demo history. The student's own use of the note
    /// replaces it with a real record.
    pub demo: bool,
}

impl Reading {
    /// A note opened for the first time today.
    pub fn new(today: NaiveDate) -> Reading {
        Reading {
            first: today,
            last: today,
            done: None,
            bookmarked: false,
            seconds: 0,
            demo: false,
        }
    }
}

/// Where a mistake or a flashcard stands in the five review boxes. Moved
/// along by [`crate::mentor::review`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    /// The box it is in, 1 to 5.
    pub level: u8,
    /// The day it is next asked, or `None` once it has left box 5.
    pub due: Option<NaiveDate>,
    /// The day it joined the reviews.
    pub added: NaiveDate,
    /// The day it was last answered in a review session.
    pub reviewed: Option<NaiveDate>,
    /// Made up by the demo history. Answering it makes it the student's own.
    pub demo: bool,
}

/// A highlight or a clip: a piece of a note, story or editorial the student
/// marked, with what they wrote about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub id: i64,
    /// The reading key of the item it was made in, such as `story:ca-1-2`.
    pub item: String,
    /// Which part of the item: a note section's heading, or a story part.
    /// Empty for the part before a note's first heading, or a whole body.
    pub part: String,
    pub kind: MarkKind,
    /// The byte range of the part's text as it rendered when it was marked.
    pub range: Range<usize>,
    /// The marked text itself, to find it again after the content is edited.
    pub quote: String,
    pub comment: Option<String>,
    /// The day it was marked.
    pub added: NaiveDate,
}

/// A highlight is a marked passage; a clip is one kept as a card to read on
/// its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Highlight,
    Clip,
}

impl MarkKind {
    pub fn key(self) -> &'static str {
        match self {
            MarkKind::Highlight => "highlight",
            MarkKind::Clip => "clip",
        }
    }

    fn parse(key: &str) -> Option<MarkKind> {
        match key {
            "highlight" => Some(MarkKind::Highlight),
            "clip" => Some(MarkKind::Clip),
            _ => None,
        }
    }
}

/// A highlight or clip that has not been saved yet.
#[derive(Debug, Clone)]
pub struct NewMark {
    pub item: String,
    pub part: String,
    pub kind: MarkKind,
    pub range: Range<usize>,
    pub quote: String,
    pub comment: Option<String>,
}

/// A note the student wrote themselves: a title and a Markdown body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyNote {
    /// Zero for a note that has not been saved yet.
    pub id: i64,
    pub title: String,
    pub body: String,
    pub added: NaiveDate,
    pub updated: NaiveDate,
}

impl MyNote {
    /// A new note, empty, written today.
    pub fn new(today: NaiveDate) -> MyNote {
        MyNote {
            id: 0,
            title: String::new(),
            body: String::new(),
            added: today,
            updated: today,
        }
    }

    /// The first line of the body, for a list that has no title yet.
    pub fn label(&self) -> String {
        let title = self.title.trim();
        if !title.is_empty() {
            return title.to_string();
        }
        let first = self.body.lines().find(|line| !line.trim().is_empty());
        first.unwrap_or("Untitled note").trim().to_string()
    }
}

/// How many dimensions a Mains answer is scored on. Their names are
/// [`crate::mentor::DIMENSIONS`].
pub const SCORES: usize = 7;

/// An answer the student wrote to a Mains question, saved as it is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainsAnswer {
    /// Zero until it is first saved.
    pub id: i64,
    /// The question's id.
    pub question: String,
    pub text: String,
    pub started: NaiveDateTime,
    /// Empty while it is a draft.
    pub submitted: Option<NaiveDateTime>,
    /// Time on the clock, overrun included.
    pub seconds: u32,
    /// The points to cover the student says they made, by position in the
    /// question's list.
    pub points: Vec<usize>,
    /// The student's own score on each dimension, 0 to 5.
    pub scores: [Option<u8>; SCORES],
}

impl MainsAnswer {
    /// A draft begun at `started`, with nothing written.
    pub fn new(question: &str, started: NaiveDateTime) -> MainsAnswer {
        MainsAnswer {
            id: 0,
            question: question.to_string(),
            text: String::new(),
            started,
            submitted: None,
            seconds: 0,
            points: Vec::new(),
            scores: [None; SCORES],
        }
    }

    /// The total of the seven scores, out of 35, once every one is given.
    pub fn total(&self) -> Option<u32> {
        self.scores
            .iter()
            .map(|score| score.map(u32::from))
            .sum::<Option<u32>>()
    }
}

/// How sure the student was of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confidence {
    Sure,
    FiftyFifty,
    Guess,
}

impl Confidence {
    pub const ALL: [Confidence; 3] = [Confidence::Sure, Confidence::FiftyFifty, Confidence::Guess];

    pub fn label(self) -> &'static str {
        match self {
            Confidence::Sure => "Sure",
            Confidence::FiftyFifty => "50-50",
            Confidence::Guess => "Guess",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Confidence::Sure => "sure",
            Confidence::FiftyFifty => "fifty-fifty",
            Confidence::Guess => "guess",
        }
    }
}

/// One test sitting: its questions in order and what the student did with each.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// Set by the store when the sitting is started.
    pub id: i64,
    /// What kind of test it is, as [`crate::mentor::TestKind::key`] writes it.
    pub kind: String,
    pub title: String,
    pub started: NaiveDateTime,
    /// `None` while the test is still open.
    pub finished: Option<NaiveDateTime>,
    pub seconds_allowed: u32,
    /// Time on the clock so far. The test has `seconds_allowed - seconds_used` left.
    pub seconds_used: u32,
    pub right_marks: f64,
    /// Taken off for a wrong answer, as a positive number.
    pub wrong_marks: f64,
    /// The question on screen when the student last left, by position.
    pub current: usize,
    pub answers: Vec<Answer>,
}

impl Attempt {
    pub fn seconds_left(&self) -> u32 {
        self.seconds_allowed.saturating_sub(self.seconds_used)
    }

    pub fn answered(&self) -> usize {
        self.answers.iter().filter(|a| a.chosen.is_some()).count()
    }
}

/// One question in a sitting. `subject` and `topic` are copied from the
/// question when the test starts, so analysis does not need the content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub question: String,
    pub subject: String,
    pub topic: String,
    /// The option chosen, 0 to 3, or `None` while blank.
    pub chosen: Option<usize>,
    /// `None` while blank, and for a question UPSC dropped from its key.
    pub correct: Option<bool>,
    pub confidence: Option<Confidence>,
    /// Marked for review.
    pub marked: bool,
    /// Options struck out, one bit per option.
    pub struck: u8,
    pub visited: bool,
    /// Time spent on this question.
    pub seconds: u32,
}

impl Answer {
    pub fn is_struck(&self, option: usize) -> bool {
        self.struck & (1 << option) != 0
    }

    pub fn toggle_strike(&mut self, option: usize) {
        self.struck ^= 1 << option;
    }
}

/// An answered question from any sitting, for choosing what to ask next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Past {
    pub question: String,
    pub topic: String,
    pub correct: bool,
}

/// Where the student's data lives: `~/Library/Application Support/CivilMentor`
/// on macOS, `%APPDATA%\CivilMentor` on Windows, `~/.local/share/CivilMentor` on Linux.
pub fn default_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("this system has no app-data folder")?;
    Ok(dir.join("CivilMentor").join("civil-mentor.db"))
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("could not open {}", path.display()))?;
        Store::prepare(conn, Some(path.to_path_buf()))
    }

    /// A store that vanishes when the app closes. Used by tests, and as the
    /// fallback when the real file cannot be opened.
    pub fn in_memory() -> Store {
        let conn = Connection::open_in_memory().expect("SQLite can open an in-memory database");
        Store::prepare(conn, None).expect("the schema applies to an empty database")
    }

    fn prepare(conn: Connection, path: Option<PathBuf>) -> Result<Store> {
        // WAL keeps the file consistent if the app is killed mid-write.
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let applied: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let applied = applied as usize;
        if applied > MIGRATIONS.len() {
            // Opening it anyway could write old-shaped rows into a newer schema.
            bail!("the data file was written by a newer version of Civil Mentor");
        }
        for (ix, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (ix + 1) as u32)?;
            tx.commit()?;
        }
        Ok(Store { conn, path })
    }

    /// The file behind this store, or `None` for an in-memory store.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// The saved settings. A value that is missing or unreadable falls back to
    /// its default, so one bad row cannot stop the app from starting.
    pub fn settings(&self) -> Settings {
        let read = |key: &str| self.get(key).ok().flatten();
        let defaults = Settings::default();
        Settings {
            exam_date: read("exam_date")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.exam_date),
            daily_minutes: read("daily_minutes")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.daily_minutes),
            theme: read("theme")
                .and_then(|value| ThemeChoice::ALL.into_iter().find(|t| t.name() == value))
                .unwrap_or(defaults.theme),
            reader_size: read("reader_size")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.reader_size),
            reader_wide: read("reader_wide")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.reader_wide),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.set("exam_date", &settings.exam_date.to_string())?;
        self.set("daily_minutes", &settings.daily_minutes.to_string())?;
        self.set("theme", settings.theme.name())?;
        self.set("reader_size", &settings.reader_size.to_string())?;
        self.set("reader_wide", &settings.reader_wide.to_string())?;
        Ok(())
    }

    /// Every note the student has opened, by note id. A row whose dates
    /// cannot be read is left out rather than stopping the rest loading.
    pub fn readings(&self) -> Result<HashMap<String, Reading>> {
        let mut query = self
            .conn
            .prepare("SELECT item, first, last, done, bookmarked, seconds, demo FROM reading")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, bool>(6)?,
            ))
        })?;
        let mut readings = HashMap::new();
        for row in rows {
            let (item, first, last, done, bookmarked, seconds, demo) = row?;
            let (Ok(first), Ok(last)) = (first.parse(), last.parse()) else {
                continue;
            };
            let done = done.and_then(|done| done.parse().ok());
            readings.insert(
                item,
                Reading {
                    first,
                    last,
                    done,
                    bookmarked,
                    seconds,
                    demo,
                },
            );
        }
        Ok(readings)
    }

    pub fn save_reading(&self, item: &str, reading: &Reading) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading (item, first, last, done, bookmarked, seconds, demo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (item) DO UPDATE SET first = excluded.first, last = excluded.last,
                 done = excluded.done, bookmarked = excluded.bookmarked,
                 seconds = excluded.seconds, demo = excluded.demo",
            params![
                item,
                reading.first.to_string(),
                reading.last.to_string(),
                reading.done.map(|done| done.to_string()),
                reading.bookmarked,
                reading.seconds,
                reading.demo
            ],
        )?;
        Ok(())
    }
}

// ---- tests and answers -----------------------------------------------------

impl Store {
    /// Saves a new sitting with all its questions and returns its id.
    pub fn start_attempt(&self, attempt: &Attempt) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let id = insert_attempt(&tx, attempt, false)?;
        tx.commit()?;
        Ok(id)
    }

    /// Saves one answer as it stands, with the clock and the question on
    /// screen, in one transaction. Called on every change and every tick,
    /// so a crash loses at most a second.
    pub fn save_answer(&self, attempt: &Attempt, position: usize) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE attempt SET seconds_used = ?2, current = ?3 WHERE id = ?1",
            params![attempt.id, attempt.seconds_used, attempt.current as i64],
        )?;
        if let Some(answer) = attempt.answers.get(position) {
            write_answer(&tx, attempt.id, position, answer)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Closes a sitting that has `finished` set.
    pub fn finish_attempt(&self, attempt: &Attempt) -> Result<()> {
        let Some(finished) = attempt.finished else {
            bail!("the test has not been submitted");
        };
        self.conn.execute(
            "UPDATE attempt SET finished = ?2, seconds_used = ?3 WHERE id = ?1",
            params![
                attempt.id,
                finished.format(TIME_FORMAT).to_string(),
                attempt.seconds_used
            ],
        )?;
        Ok(())
    }

    /// One sitting with its answers, to resume it or show its result.
    pub fn attempt(&self, id: i64) -> Result<Option<Attempt>> {
        Ok(self.attempts_where("id = ?1", params![id])?.pop())
    }

    /// Sittings that were started and never submitted, newest first.
    pub fn unfinished(&self) -> Result<Vec<Attempt>> {
        self.attempts_where("finished IS NULL", [])
    }

    /// The most recent submitted sittings, newest first.
    pub fn finished(&self, limit: usize) -> Result<Vec<Attempt>> {
        self.attempts_where(
            "finished IS NOT NULL ORDER BY finished DESC, id DESC LIMIT ?1",
            params![limit as i64],
        )
    }

    /// Submitted sittings whose kind matches a GLOB pattern, such as
    /// `quiz:*` or one exact kind, newest first.
    pub fn finished_kind(&self, pattern: &str) -> Result<Vec<Attempt>> {
        self.attempts_where(
            "finished IS NOT NULL AND kind GLOB ?1 ORDER BY finished DESC, id DESC",
            params![pattern],
        )
    }

    fn attempts_where(&self, filter: &str, args: impl rusqlite::Params) -> Result<Vec<Attempt>> {
        let order = if filter.contains("ORDER BY") {
            ""
        } else {
            " ORDER BY id DESC"
        };
        let mut query = self.conn.prepare(&format!(
            "SELECT id, kind, title, started, finished, seconds_allowed, seconds_used,
                 right_marks, wrong_marks, current
             FROM attempt WHERE {filter}{order}"
        ))?;
        let rows = query.query_map(args, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, u32>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })?;
        let mut attempts = Vec::new();
        for row in rows {
            let (id, kind, title, started, finished, allowed, used, right, wrong, current) = row?;
            let started = NaiveDateTime::parse_from_str(&started, TIME_FORMAT)
                .with_context(|| format!("test {id} has an unreadable start time"))?;
            let finished = match finished {
                Some(text) => Some(
                    NaiveDateTime::parse_from_str(&text, TIME_FORMAT)
                        .with_context(|| format!("test {id} has an unreadable finish time"))?,
                ),
                None => None,
            };
            attempts.push(Attempt {
                id,
                kind,
                title,
                started,
                finished,
                seconds_allowed: allowed,
                seconds_used: used,
                right_marks: right,
                wrong_marks: wrong,
                current: current.max(0) as usize,
                answers: self.answers(id)?,
            });
        }
        Ok(attempts)
    }

    fn answers(&self, attempt: i64) -> Result<Vec<Answer>> {
        let mut query = self.conn.prepare(
            "SELECT question, subject, topic, chosen, correct, confidence, marked, struck,
                 visited, seconds
             FROM answer WHERE attempt = ?1 ORDER BY position",
        )?;
        let rows = query.query_map([attempt], |row| {
            let confidence: Option<String> = row.get(5)?;
            Ok(Answer {
                question: row.get(0)?,
                subject: row.get(1)?,
                topic: row.get(2)?,
                chosen: row.get::<_, Option<i64>>(3)?.map(|c| c as usize),
                correct: row.get(4)?,
                confidence: confidence
                    .and_then(|key| Confidence::ALL.into_iter().find(|c| c.key() == key)),
                marked: row.get(6)?,
                struck: row.get(7)?,
                visited: row.get(8)?,
                seconds: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The questions whose latest answer in the student's own submitted tests
    /// was wrong, the most recent first. Demo tests are left out.
    pub fn past_mistakes(&self) -> Result<Vec<String>> {
        let mut query = self.conn.prepare(
            "SELECT answer.question, answer.correct FROM answer
             JOIN attempt ON attempt.id = answer.attempt
             WHERE attempt.demo = 0 AND attempt.finished IS NOT NULL
                 AND answer.correct IS NOT NULL
             ORDER BY attempt.finished DESC, attempt.id DESC, answer.position",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })?;
        let mut seen = std::collections::HashSet::new();
        let mut mistakes = Vec::new();
        for row in rows {
            let (question, correct) = row?;
            if seen.insert(question.clone()) && !correct {
                mistakes.push(question);
            }
        }
        Ok(mistakes)
    }

    /// Every answered question with a right or wrong outcome, oldest first.
    pub fn answered(&self) -> Result<Vec<Past>> {
        let mut query = self.conn.prepare(
            "SELECT question, topic, correct FROM answer
             WHERE correct IS NOT NULL ORDER BY attempt, position",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Past {
                question: row.get(0)?,
                topic: row.get(1)?,
                correct: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

// ---- study time, the day's plan and demo history ------------------------------

// ---- review boxes -----------------------------------------------------------

impl Store {
    /// Every mistake and flashcard in the reviews, by item key. A row whose
    /// dates cannot be read is left out rather than stopping the rest loading.
    pub fn reviews(&self) -> Result<HashMap<String, Review>> {
        let mut query = self
            .conn
            .prepare("SELECT item, box, due, added, reviewed, demo FROM review")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u8>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, bool>(5)?,
            ))
        })?;
        let mut reviews = HashMap::new();
        for row in rows {
            let (item, level, due, added, reviewed, demo) = row?;
            let Ok(added) = added.parse() else {
                continue;
            };
            let due = match due.map(|due| due.parse()) {
                Some(Ok(due)) => Some(due),
                Some(Err(_)) => continue,
                None => None,
            };
            reviews.insert(
                item,
                Review {
                    level,
                    due,
                    added,
                    reviewed: reviewed.and_then(|day| day.parse().ok()),
                    demo,
                },
            );
        }
        Ok(reviews)
    }

    /// Saves where each item stands, in one transaction.
    pub fn save_reviews(&self, reviews: &[(String, Review)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (item, review) in reviews {
            write_review(&tx, item, review)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Whether mistakes made before the reviews existed have been brought in.
    pub fn mistakes_backfilled(&self) -> bool {
        self.get("mistakes_backfilled").ok().flatten().is_some()
    }

    /// Brings in mistakes made before the reviews existed, once: saves
    /// `reviews` and records the day, so it never runs again.
    pub fn backfill_mistakes(&self, day: NaiveDate, reviews: &[(String, Review)]) -> Result<()> {
        self.save_reviews(reviews)?;
        self.set("mistakes_backfilled", &day.to_string())
    }
}

fn write_review(conn: &Connection, item: &str, review: &Review) -> Result<()> {
    conn.execute(
        "INSERT INTO review (item, box, due, added, reviewed, demo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (item) DO UPDATE SET box = excluded.box, due = excluded.due,
             added = excluded.added, reviewed = excluded.reviewed, demo = excluded.demo",
        params![
            item,
            review.level,
            review.due.map(|due| due.to_string()),
            review.added.to_string(),
            review.reviewed.map(|day| day.to_string()),
            review.demo
        ],
    )?;
    Ok(())
}

// ---- marks and the student's own notes -------------------------------------

impl Store {
    /// Every highlight and clip, oldest first. A row whose day cannot be read
    /// is left out rather than stopping the rest loading.
    pub fn marks(&self) -> Result<Vec<Mark>> {
        let mut query = self.conn.prepare(
            "SELECT id, item, part, kind, start, end, quote, comment, added FROM mark ORDER BY id",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut marks = Vec::new();
        for row in rows {
            let (id, item, part, kind, start, end, quote, comment, added) = row?;
            let (Some(kind), Ok(added)) = (MarkKind::parse(&kind), added.parse()) else {
                continue;
            };
            marks.push(Mark {
                id,
                item,
                part,
                kind,
                range: usize::try_from(start).unwrap_or_default()
                    ..usize::try_from(end).unwrap_or_default(),
                quote,
                comment,
                added,
            });
        }
        Ok(marks)
    }

    /// Saves a highlight or clip and returns it with its id.
    pub fn add_mark(&self, mark: NewMark, added: NaiveDate) -> Result<Mark> {
        self.conn.execute(
            "INSERT INTO mark (item, part, kind, start, end, quote, comment, added)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                mark.item,
                mark.part,
                mark.kind.key(),
                mark.range.start as i64,
                mark.range.end as i64,
                mark.quote,
                mark.comment,
                added.to_string()
            ],
        )?;
        Ok(Mark {
            id: self.conn.last_insert_rowid(),
            item: mark.item,
            part: mark.part,
            kind: mark.kind,
            range: mark.range,
            quote: mark.quote,
            comment: mark.comment,
            added,
        })
    }

    /// Deletes one highlight or clip.
    pub fn remove_mark(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM mark WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Every note the student wrote, newest first.
    pub fn my_notes(&self) -> Result<Vec<MyNote>> {
        let mut query = self
            .conn
            .prepare("SELECT id, title, body, added, updated FROM my_note ORDER BY id DESC")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows {
            let (id, title, body, added, updated) = row?;
            let (Ok(added), Ok(updated)) = (added.parse(), updated.parse()) else {
                continue;
            };
            notes.push(MyNote {
                id,
                title,
                body,
                added,
                updated,
            });
        }
        Ok(notes)
    }

    /// Saves a note of the student's own and returns it with its id.
    pub fn save_my_note(&self, note: &MyNote) -> Result<MyNote> {
        let mut note = note.clone();
        if note.id == 0 {
            self.conn.execute(
                "INSERT INTO my_note (title, body, added, updated) VALUES (?1, ?2, ?3, ?4)",
                params![
                    note.title,
                    note.body,
                    note.added.to_string(),
                    note.updated.to_string()
                ],
            )?;
            note.id = self.conn.last_insert_rowid();
        } else {
            self.conn.execute(
                "UPDATE my_note SET title = ?2, body = ?3, added = ?4, updated = ?5 WHERE id = ?1",
                params![
                    note.id,
                    note.title,
                    note.body,
                    note.added.to_string(),
                    note.updated.to_string()
                ],
            )?;
        }
        Ok(note)
    }

    /// Deletes one note of the student's own.
    pub fn remove_my_note(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM my_note WHERE id = ?1", [id])?;
        Ok(())
    }
}

// ---- Mains answers ----------------------------------------------------------

const SCORE_COLUMNS: &str =
    "directive, introduction, content, dimensions, examples, conclusion, presentation";

impl Store {
    /// Every Mains answer, drafts included, oldest first.
    pub fn mains_answers(&self) -> Result<Vec<MainsAnswer>> {
        let mut query = self.conn.prepare(&format!(
            "SELECT id, question, text, started, submitted, seconds, points, {SCORE_COLUMNS}
             FROM mains_answer ORDER BY id"
        ))?;
        let rows = query.query_map([], |row| {
            let mut scores = [None; SCORES];
            for (ix, score) in scores.iter_mut().enumerate() {
                *score = row.get::<_, Option<u8>>(7 + ix)?;
            }
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, String>(6)?,
                scores,
            ))
        })?;
        let mut answers = Vec::new();
        for row in rows {
            let (id, question, text, started, submitted, seconds, points, scores) = row?;
            let started = NaiveDateTime::parse_from_str(&started, TIME_FORMAT)
                .with_context(|| format!("Mains answer {id} has an unreadable start time"))?;
            let submitted = match submitted {
                Some(time) => Some(
                    NaiveDateTime::parse_from_str(&time, TIME_FORMAT).with_context(|| {
                        format!("Mains answer {id} has an unreadable submission time")
                    })?,
                ),
                None => None,
            };
            answers.push(MainsAnswer {
                id,
                question,
                text,
                started,
                submitted,
                seconds,
                points: points
                    .split(',')
                    .filter_map(|point| point.trim().parse().ok())
                    .collect(),
                scores,
            });
        }
        Ok(answers)
    }

    /// Saves a Mains answer as it stands and returns it with its id. Called
    /// on every keystroke and every tick of its clock, so a crash loses at
    /// most a second.
    pub fn save_mains_answer(&self, answer: &MainsAnswer) -> Result<MainsAnswer> {
        let mut answer = answer.clone();
        let points = answer
            .points
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let started = answer.started.format(TIME_FORMAT).to_string();
        let [
            directive,
            introduction,
            content,
            dimensions,
            examples,
            conclusion,
            presentation,
        ] = answer.scores;
        let tx = self.conn.unchecked_transaction()?;
        if answer.id == 0 {
            tx.execute(
                "INSERT INTO mains_answer (question, text, started) VALUES (?1, '', ?2)",
                params![answer.question, started],
            )?;
            answer.id = tx.last_insert_rowid();
        }
        let changed = tx.execute(
            "UPDATE mains_answer SET question = ?2, text = ?3, started = ?4, submitted = ?5,
                 seconds = ?6, points = ?7, directive = ?8, introduction = ?9, content = ?10,
                 dimensions = ?11, examples = ?12, conclusion = ?13, presentation = ?14
             WHERE id = ?1",
            params![
                answer.id,
                answer.question,
                answer.text,
                started,
                answer.submitted.map(|t| t.format(TIME_FORMAT).to_string()),
                answer.seconds,
                points,
                directive,
                introduction,
                content,
                dimensions,
                examples,
                conclusion,
                presentation,
            ],
        )?;
        if changed == 0 {
            bail!("Mains answer {} is not in the data file", answer.id);
        }
        tx.commit()?;
        Ok(answer)
    }
}

/// What a block of study time was spent on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Study {
    /// A note open in the reader. The item is the note's id.
    Reading,
    /// A test in the exam room. The item is the sitting's id.
    Test,
    /// A story or editorial open in the current-affairs reader. The item is
    /// its reading key.
    Story,
    /// Today's review session. The item is `session`.
    Review,
    /// Writing a Mains answer. The item is the answer's id.
    Mains,
    /// A focus block from the sidebar timer, counted only where nothing
    /// else counted the same minutes. The item is the block's length in
    /// minutes.
    Focus,
}

impl Study {
    pub fn key(self) -> &'static str {
        match self {
            Study::Reading => "reading",
            Study::Test => "test",
            Study::Story => "story",
            Study::Review => "review",
            Study::Mains => "mains",
            Study::Focus => "focus",
        }
    }
}

/// One task in a day's plan. `kind` is what [`crate::mentor::TaskKind::key`] writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTask {
    pub kind: String,
    /// What the task opens: a note id, or the topic a drill is on.
    pub target: String,
    pub title: String,
    pub minutes: u32,
    /// Ticked by the student.
    pub done: bool,
}

/// Made-up history, so that Analysis and Today have something to show.
/// Every row it adds carries the demo mark.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Demo {
    /// Finished sittings with their answers.
    pub attempts: Vec<Attempt>,
    pub readings: Vec<(String, Reading)>,
    /// The day, what the time went on, the item and the seconds.
    pub sessions: Vec<(NaiveDate, Study, String, u32)>,
    /// Mistakes and flashcards in the review boxes, by item key.
    pub reviews: Vec<(String, Review)>,
}

impl Store {
    /// Starts a block of study time on `day` and returns its id.
    pub fn start_session(&self, day: NaiveDate, kind: Study, item: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO session (day, kind, item) VALUES (?1, ?2, ?3)",
            params![day.to_string(), kind.key(), item],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Adds time to a block started with [`Store::start_session`].
    pub fn add_session_seconds(&self, id: i64, seconds: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE session SET seconds = seconds + ?2 WHERE id = ?1",
            params![id, seconds],
        )?;
        Ok(())
    }

    /// The seconds studied on every day with any study recorded.
    pub fn study_days(&self) -> Result<BTreeMap<NaiveDate, u32>> {
        let mut query = self.conn.prepare(
            "SELECT day, SUM(seconds) FROM session GROUP BY day HAVING SUM(seconds) > 0",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?;
        let mut days = BTreeMap::new();
        for row in rows {
            let (day, seconds) = row?;
            if let Ok(day) = day.parse() {
                days.insert(day, seconds);
            }
        }
        Ok(days)
    }

    /// The plan saved for `day`, in order, or `None` if none was made.
    pub fn plan(&self, day: NaiveDate) -> Result<Option<Vec<PlanTask>>> {
        let mut query = self.conn.prepare(
            "SELECT kind, target, title, minutes, done FROM plan_task
             WHERE day = ?1 ORDER BY position",
        )?;
        let rows = query.query_map([day.to_string()], |row| {
            Ok(PlanTask {
                kind: row.get(0)?,
                target: row.get(1)?,
                title: row.get(2)?,
                minutes: row.get(3)?,
                done: row.get(4)?,
            })
        })?;
        let tasks: Vec<PlanTask> = rows.collect::<rusqlite::Result<_>>()?;
        Ok((!tasks.is_empty()).then_some(tasks))
    }

    /// Saves the plan for `day`, replacing any made before.
    pub fn save_plan(&self, day: NaiveDate, tasks: &[PlanTask]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM plan_task WHERE day = ?1", [day.to_string()])?;
        for (position, task) in tasks.iter().enumerate() {
            tx.execute(
                "INSERT INTO plan_task (day, position, kind, target, title, minutes, done)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    day.to_string(),
                    position as i64,
                    task.kind,
                    task.target,
                    task.title,
                    task.minutes,
                    task.done
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_task_done(&self, day: NaiveDate, position: usize, done: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE plan_task SET done = ?3 WHERE day = ?1 AND position = ?2",
            params![day.to_string(), position as i64, done],
        )?;
        Ok(())
    }

    /// Forgets the plan for `day`, so the next look at Today makes it afresh.
    pub fn clear_plan(&self, day: NaiveDate) -> Result<()> {
        self.conn
            .execute("DELETE FROM plan_task WHERE day = ?1", [day.to_string()])?;
        Ok(())
    }

    /// Saves made-up history with the demo mark. A note or review item the
    /// student has a record for keeps the student's own.
    pub fn add_demo(&self, demo: &Demo) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for attempt in &demo.attempts {
            insert_attempt(&tx, attempt, true)?;
        }
        for (item, reading) in &demo.readings {
            tx.execute(
                "INSERT INTO reading (item, first, last, done, bookmarked, seconds, demo)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
                 ON CONFLICT (item) DO NOTHING",
                params![
                    item,
                    reading.first.to_string(),
                    reading.last.to_string(),
                    reading.done.map(|done| done.to_string()),
                    reading.bookmarked,
                    reading.seconds
                ],
            )?;
        }
        for (day, kind, item, seconds) in &demo.sessions {
            tx.execute(
                "INSERT INTO session (day, kind, item, seconds, demo) VALUES (?1, ?2, ?3, ?4, 1)",
                params![day.to_string(), kind.key(), item, seconds],
            )?;
        }
        for (item, review) in &demo.reviews {
            tx.execute(
                "INSERT INTO review (item, box, due, added, reviewed, demo)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)
                 ON CONFLICT (item) DO NOTHING",
                params![
                    item,
                    review.level,
                    review.due.map(|due| due.to_string()),
                    review.added.to_string(),
                    review.reviewed.map(|day| day.to_string())
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes every row the demo history added, and nothing else.
    pub fn remove_demo(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "DELETE FROM answer WHERE attempt IN (SELECT id FROM attempt WHERE demo = 1);
             DELETE FROM attempt WHERE demo = 1;
             DELETE FROM reading WHERE demo = 1;
             DELETE FROM session WHERE demo = 1;
             DELETE FROM review WHERE demo = 1;",
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Whether demo history is loaded.
    pub fn has_demo(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM attempt WHERE demo = 1)
                 OR EXISTS (SELECT 1 FROM session WHERE demo = 1)",
            [],
            |row| row.get(0),
        )?)
    }

    /// Whether the student has started any test of their own.
    pub fn has_own_tests(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM attempt WHERE demo = 0)",
            [],
            |row| row.get(0),
        )?)
    }

    /// Whether the student has taken or declined the first-launch diagnostic.
    pub fn diagnostic_settled(&self) -> bool {
        self.get("diagnostic").ok().flatten().is_some()
    }

    /// Records that the diagnostic was taken or declined, so it is not offered again.
    pub fn settle_diagnostic(&self, taken: bool) -> Result<()> {
        self.set("diagnostic", if taken { "taken" } else { "declined" })
    }
}

/// Writes a sitting and all its questions, and returns its id.
fn insert_attempt(conn: &Connection, attempt: &Attempt, demo: bool) -> Result<i64> {
    conn.execute(
        "INSERT INTO attempt (kind, title, started, finished, seconds_allowed, seconds_used,
             right_marks, wrong_marks, current, demo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            attempt.kind,
            attempt.title,
            attempt.started.format(TIME_FORMAT).to_string(),
            attempt.finished.map(|t| t.format(TIME_FORMAT).to_string()),
            attempt.seconds_allowed,
            attempt.seconds_used,
            attempt.right_marks,
            attempt.wrong_marks,
            attempt.current as i64,
            demo,
        ],
    )?;
    let id = conn.last_insert_rowid();
    for (position, answer) in attempt.answers.iter().enumerate() {
        conn.execute(
            "INSERT INTO answer (attempt, position, question, subject, topic)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                position as i64,
                answer.question,
                answer.subject,
                answer.topic
            ],
        )?;
        write_answer(conn, id, position, answer)?;
    }
    Ok(id)
}

fn write_answer(conn: &Connection, attempt: i64, position: usize, answer: &Answer) -> Result<()> {
    conn.execute(
        "UPDATE answer SET chosen = ?3, correct = ?4, confidence = ?5, marked = ?6, struck = ?7,
             visited = ?8, seconds = ?9
         WHERE attempt = ?1 AND position = ?2",
        params![
            attempt,
            position as i64,
            answer.chosen.map(|c| c as i64),
            answer.correct,
            answer.confidence.map(Confidence::key),
            answer.marked,
            answer.struck,
            answer.visited,
            answer.seconds,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed() -> Settings {
        Settings {
            exam_date: NaiveDate::from_ymd_opt(2028, 6, 4).unwrap(),
            daily_minutes: 240,
            theme: ThemeChoice::Dark,
            reader_size: 19,
            reader_wide: true,
        }
    }

    #[test]
    fn a_new_store_returns_the_defaults() {
        assert_eq!(Store::in_memory().settings(), Settings::default());
    }

    #[test]
    fn settings_survive_closing_and_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-test-{}", std::process::id()));
        let path = dir.join("nested").join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);

        let store = Store::open(&path).expect("creates the folder and the file");
        assert_eq!(store.path(), Some(path.as_path()));
        store.save_settings(&changed()).unwrap();
        drop(store);

        let store = Store::open(&path).expect("reopens without re-running migrations");
        assert_eq!(store.settings(), changed());
        // Saving twice updates the row instead of failing on the primary key.
        store.save_settings(&Settings::default()).unwrap();
        assert_eq!(store.settings(), Settings::default());
        drop(store);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_value_falls_back_to_its_default() {
        let store = Store::in_memory();
        store.save_settings(&changed()).unwrap();
        store.set("exam_date", "not a date").unwrap();
        store.set("theme", "Sepia").unwrap();
        let settings = store.settings();
        assert_eq!(settings.exam_date, Settings::default().exam_date);
        assert_eq!(settings.theme, Settings::default().theme);
        assert_eq!(settings.daily_minutes, 240, "good rows are still read");
    }

    #[test]
    fn readings_are_saved_updated_and_read_back() {
        let store = Store::in_memory();
        assert_eq!(store.readings().unwrap(), HashMap::new());
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();

        let mut reading = Reading::new(day(14));
        store.save_reading("fr", &reading).unwrap();
        reading.last = day(15);
        reading.done = Some(day(15));
        reading.bookmarked = true;
        store.save_reading("fr", &reading).unwrap();
        store
            .save_reading("monsoon", &Reading::new(day(15)))
            .unwrap();

        let readings = store.readings().unwrap();
        assert_eq!(readings.len(), 2, "saving again updates the row");
        assert_eq!(readings["fr"], reading);
        assert_eq!(readings["monsoon"].done, None);

        // A row with a broken date is skipped; the others still load.
        store
            .conn
            .execute(
                "UPDATE reading SET first = 'soon' WHERE item = 'monsoon'",
                [],
            )
            .unwrap();
        assert_eq!(store.readings().unwrap().len(), 1);
    }

    fn sitting(questions: usize) -> Attempt {
        let at = |h, m| {
            NaiveDate::from_ymd_opt(2026, 11, 15)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap()
        };
        Attempt {
            id: 0,
            kind: "subject:polity".into(),
            title: "Subject drill: Polity".into(),
            started: at(9, 0),
            finished: None,
            seconds_allowed: questions as u32 * 72,
            seconds_used: 0,
            right_marks: 2.0,
            wrong_marks: 2.0 / 3.0,
            current: 0,
            answers: (0..questions)
                .map(|n| Answer {
                    question: format!("polity-{n:04}"),
                    subject: "polity".into(),
                    topic: "pre.gs.polity.rights".into(),
                    ..Answer::default()
                })
                .collect(),
        }
    }

    #[test]
    fn a_test_is_started_answered_resumed_after_a_crash_and_finished() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-attempt-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);

        let store = Store::open(&path).unwrap();
        let mut test = sitting(3);
        test.id = store.start_attempt(&test).unwrap();
        assert!(test.id > 0);

        // The student answers the first question, strikes an option on the
        // second and marks it, and the clock runs.
        test.answers[0].chosen = Some(1);
        test.answers[0].correct = Some(true);
        test.answers[0].confidence = Some(Confidence::FiftyFifty);
        test.answers[0].visited = true;
        test.answers[0].seconds = 40;
        test.seconds_used = 40;
        store.save_answer(&test, 0).unwrap();
        test.current = 1;
        test.answers[1].toggle_strike(2);
        test.answers[1].toggle_strike(3);
        test.answers[1].toggle_strike(3);
        test.answers[1].marked = true;
        test.answers[1].visited = true;
        test.seconds_used = 55;
        store.save_answer(&test, 1).unwrap();
        assert!(test.answers[1].is_struck(2) && !test.answers[1].is_struck(3));
        // The app dies here, before the test is submitted.
        drop(store);

        let store = Store::open(&path).unwrap();
        let unfinished = store.unfinished().unwrap();
        assert_eq!(
            unfinished,
            vec![test.clone()],
            "everything up to the crash is back"
        );
        assert_eq!(unfinished[0].seconds_left(), 3 * 72 - 55);
        assert_eq!(unfinished[0].answered(), 1);
        assert_eq!(store.finished(10).unwrap(), Vec::new());

        test.answers[2].chosen = Some(0);
        test.answers[2].correct = Some(false);
        store.save_answer(&test, 2).unwrap();
        assert!(store.finish_attempt(&test).is_err(), "needs a finish time");
        test.finished = Some(test.started + chrono::Duration::minutes(3));
        store.finish_attempt(&test).unwrap();
        assert_eq!(store.unfinished().unwrap(), Vec::new());
        assert_eq!(store.finished(10).unwrap(), vec![test.clone()]);
        assert_eq!(store.attempt(test.id).unwrap(), Some(test.clone()));
        assert_eq!(store.attempt(test.id + 1).unwrap(), None);

        // What the mentor reads to choose the next questions.
        let past: Vec<(String, bool)> = store
            .answered()
            .unwrap()
            .into_iter()
            .map(|p| (p.question, p.correct))
            .collect();
        assert_eq!(
            past,
            [
                ("polity-0000".to_string(), true),
                ("polity-0002".to_string(), false)
            ]
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn newer_sittings_come_first_and_the_list_is_limited() {
        let store = Store::in_memory();
        for minute in 0..3 {
            let mut test = sitting(1);
            test.title = format!("test {minute}");
            test.id = store.start_attempt(&test).unwrap();
            test.finished = Some(test.started + chrono::Duration::minutes(minute + 1));
            store.finish_attempt(&test).unwrap();
        }
        let titles: Vec<String> = store
            .finished(2)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["test 2", "test 1"]);
    }

    #[test]
    fn a_path_that_cannot_be_used_is_an_error_not_a_panic() {
        // A regular file where the data folder should be.
        let blocker =
            std::env::temp_dir().join(format!("civil-mentor-blocker-{}", std::process::id()));
        std::fs::write(&blocker, "not a folder").unwrap();
        let err = Store::open(&blocker.join("civil-mentor.db"))
            .err()
            .expect("must not open");
        assert!(err.to_string().contains("could not create"), "{err}");
        std::fs::remove_file(&blocker).unwrap();
    }

    #[test]
    fn a_phase_2_store_upgrades_without_losing_a_row() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-upgrade-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A file as Phase 2 left it: the first three migrations and a row in every table.
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute_batch(
            "INSERT INTO setting VALUES ('daily_minutes', '120');
             INSERT INTO reading VALUES ('fr', '2026-11-10', '2026-11-12', '2026-11-12', 1);
             INSERT INTO attempt (id, kind, title, started, finished, seconds_allowed,
                 seconds_used, right_marks, wrong_marks, current)
             VALUES (7, 'micro', 'Micro test', '2026-11-12 09:00:00', '2026-11-12 09:10:00',
                 720, 600, 2.0, 0.6666666666666666, 1);
             INSERT INTO answer (attempt, position, question, subject, topic, chosen, correct,
                 confidence, visited, seconds)
             VALUES (7, 0, 'polity-0001', 'polity', 'pre.gs.polity.rights', 2, 1, 'sure', 1, 40),
                    (7, 1, 'polity-0002', 'polity', 'pre.gs.polity.rights', NULL, NULL, NULL, 1, 20);",
        )
        .unwrap();
        drop(conn);

        let store = Store::open(&path).expect("the old file upgrades");
        let version: u32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        assert_eq!(store.settings().daily_minutes, 120);
        let fr = store.readings().unwrap()["fr"];
        assert_eq!(fr.done, NaiveDate::from_ymd_opt(2026, 11, 12));
        assert!(fr.bookmarked);
        assert_eq!((fr.seconds, fr.demo), (0, false), "new columns start empty");
        let test = store.attempt(7).unwrap().expect("the test is still there");
        assert_eq!(
            (test.seconds_used, test.current, test.answers.len()),
            (600, 1, 2)
        );
        assert_eq!(test.answers[0].chosen, Some(2));
        assert_eq!(test.answers[0].confidence, Some(Confidence::Sure));
        assert_eq!(store.answered().unwrap().len(), 1);
        assert!(store.has_own_tests().unwrap());
        assert!(!store.has_demo().unwrap(), "old rows are the student's own");
        assert_eq!(store.study_days().unwrap(), BTreeMap::new());
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn study_time_adds_up_by_day() {
        let store = Store::in_memory();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let reading = store.start_session(day(15), Study::Reading, "fr").unwrap();
        store.add_session_seconds(reading, 15).unwrap();
        store.add_session_seconds(reading, 45).unwrap();
        let test = store.start_session(day(15), Study::Test, "7").unwrap();
        store.add_session_seconds(test, 600).unwrap();
        let idle = store.start_session(day(14), Study::Reading, "fr").unwrap();
        let next = store
            .start_session(day(16), Study::Reading, "dpsp")
            .unwrap();
        store.add_session_seconds(next, 30).unwrap();
        assert_ne!(idle, next);
        assert_eq!(
            store.study_days().unwrap(),
            BTreeMap::from([(day(15), 660), (day(16), 30)]),
            "a block with no time is no study"
        );
    }

    #[test]
    fn a_plan_is_saved_once_and_ticked() {
        let store = Store::in_memory();
        let day = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        assert_eq!(store.plan(day).unwrap(), None);
        let task = |kind: &str, minutes| PlanTask {
            kind: kind.into(),
            target: format!("{kind}-target"),
            title: format!("{kind} title"),
            minutes,
            done: false,
        };
        let tasks = vec![task("note", 20), task("drill", 12)];
        store.save_plan(day, &tasks).unwrap();
        assert_eq!(store.plan(day).unwrap(), Some(tasks.clone()));
        assert_eq!(store.plan(day.succ_opt().unwrap()).unwrap(), None);
        store.set_task_done(day, 1, true).unwrap();
        assert!(store.plan(day).unwrap().unwrap()[1].done);
        store.clear_plan(day).unwrap();
        assert_eq!(store.plan(day).unwrap(), None);
    }

    #[test]
    fn a_phase_4_store_upgrades_without_losing_a_row() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-phase4-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A file as Phase 4 left it: real, demo and story reading rows, a
        // real and a demo test with a wrong answer each, study time and a plan.
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 5).unwrap();
        conn.execute_batch(
            "INSERT INTO setting VALUES ('diagnostic', 'declined');
             INSERT INTO reading VALUES
                 ('fr', '2026-11-10', '2026-11-12', '2026-11-12', 1, 900, 0),
                 ('monsoon', '2026-11-01', '2026-11-03', NULL, 0, 600, 1),
                 ('story:ca-1', '2026-11-14', '2026-11-14', '2026-11-14', 1, 120, 0);
             INSERT INTO attempt (id, kind, title, started, finished, seconds_allowed,
                 seconds_used, right_marks, wrong_marks, current, demo)
             VALUES (7, 'micro', 'Micro test', '2026-11-12 09:00:00', '2026-11-12 09:10:00',
                     720, 600, 2.0, 0.6666666666666666, 1, 0),
                    (8, 'mini', 'Demo', '2026-11-02 19:00:00', '2026-11-02 19:20:00',
                     1440, 1200, 2.0, 0.6666666666666666, 0, 1);
             INSERT INTO answer (attempt, position, question, subject, topic, chosen, correct,
                 confidence, visited, seconds)
             VALUES (7, 0, 'polity-0001', 'polity', 'pre.gs.polity.rights', 2, 0, 'sure', 1, 40),
                    (7, 1, 'polity-0002', 'polity', 'pre.gs.polity.rights', 1, 1, NULL, 1, 20),
                    (8, 0, 'economy-0001', 'economy', 'pre.gs.economy', 0, 0, 'guess', 1, 30);
             INSERT INTO session (day, kind, item, seconds, demo)
             VALUES ('2026-11-12', 'test', '7', 600, 0), ('2026-11-02', 'test', 'demo-1', 1200, 1),
                    ('2026-11-14', 'story', 'story:ca-1', 120, 0);
             INSERT INTO plan_task VALUES ('2026-11-14', 0, 'note', 'fr', 'Read: fr', 30, 1);",
        )
        .unwrap();
        drop(conn);

        let store = Store::open(&path).expect("the Phase 4 file upgrades");
        let version: u32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        let readings = store.readings().unwrap();
        assert_eq!(readings.len(), 3);
        assert_eq!(
            (
                readings["fr"].seconds,
                readings["fr"].bookmarked,
                readings["fr"].demo
            ),
            (900, true, false)
        );
        assert!(readings["monsoon"].demo);
        assert!(readings["story:ca-1"].bookmarked);
        assert_eq!(store.finished(10).unwrap().len(), 2);
        assert!(store.has_demo().unwrap() && store.has_own_tests().unwrap());
        assert_eq!(store.study_days().unwrap().values().sum::<u32>(), 1920);
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        assert_eq!(store.plan(day(14)).unwrap().unwrap()[0].target, "fr");
        assert!(store.diagnostic_settled());
        assert_eq!(
            store.reviews().unwrap(),
            HashMap::new(),
            "reviews start empty"
        );
        assert_eq!(
            store.past_mistakes().unwrap(),
            ["polity-0001"],
            "the student's own wrong answer, not the demo's"
        );
        assert!(!store.mistakes_backfilled());
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn marks_and_own_notes_are_saved_and_read_back() {
        let store = Store::in_memory();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let mark = store
            .add_mark(
                NewMark {
                    item: "gs2-polity-fundamental-rights".into(),
                    part: "1. Quick anchor".into(),
                    kind: MarkKind::Highlight,
                    range: 4..21,
                    quote: "Article 32 is itself".into(),
                    comment: Some("Ask why it is called the soul".into()),
                },
                day(17),
            )
            .unwrap();
        assert_eq!(mark.id, 1);
        let clip = store
            .add_mark(
                NewMark {
                    item: "story:ca-1-1".into(),
                    part: "facts".into(),
                    kind: MarkKind::Clip,
                    range: 0..12,
                    quote: "- The Act cut".into(),
                    comment: None,
                },
                day(17),
            )
            .unwrap();
        let marks = store.marks().unwrap();
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0], mark);
        assert_eq!(marks[1], clip);
        assert_eq!(marks[1].kind, MarkKind::Clip);
        assert_eq!(marks[1].comment, None);
        store.remove_mark(mark.id).unwrap();
        assert_eq!(store.marks().unwrap(), [clip]);

        let mut note = MyNote::new(day(17));
        note.title = "Drainage".into();
        note.body = "Khadar and bhangar.".into();
        let saved = store.save_my_note(&note).unwrap();
        assert!(saved.id > 0, "a new note is given an id");
        let mut edited = saved.clone();
        edited.body = "Khadar is the new alluvium.".into();
        edited.updated = day(18);
        store.save_my_note(&edited).unwrap();
        assert_eq!(edited.label(), "Drainage");
        assert_eq!(store.my_notes().unwrap(), [edited]);
        store.remove_my_note(saved.id).unwrap();
        assert!(store.my_notes().unwrap().is_empty());
    }

    #[test]
    fn a_phase_5_store_upgrades_without_losing_a_row() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-phase5-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A file as Phase 5.1 and 5.2 left it: a review row and a note read.
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute_batch(
            "INSERT INTO reading VALUES
                 ('fr', '2026-11-10', '2026-11-12', '2026-11-12', 1, 900, 0);
             INSERT INTO review VALUES
                 ('question:polity-0001', 2, '2026-11-18', '2026-11-16', '2026-11-17', 0);",
        )
        .unwrap();
        drop(conn);

        let store = Store::open(&path).expect("the Phase 5 file upgrades");
        let version: u32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        assert_eq!(store.reviews().unwrap().len(), 1, "the boxes are kept");
        assert_eq!(store.readings().unwrap().len(), 1, "the reading is kept");
        assert!(store.marks().unwrap().is_empty(), "marks start empty");
        assert!(
            store.my_notes().unwrap().is_empty(),
            "own notes start empty"
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mains_answers_are_saved_as_typed_and_survive_a_crash() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-mains-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);
        let at = |d, h, m| {
            NaiveDate::from_ymd_opt(2026, 11, d)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap()
        };

        // A draft is saved the moment it is begun, then with every keystroke.
        let store = Store::open(&path).unwrap();
        let draft = store
            .save_mains_answer(&MainsAnswer::new("gs2-governor-discretion", at(16, 9, 0)))
            .unwrap();
        assert!(draft.id > 0, "a new answer is given an id");
        let typed = MainsAnswer {
            text: "The Governor is the constitutional head".into(),
            seconds: 95,
            ..draft.clone()
        };
        assert_eq!(store.save_mains_answer(&typed).unwrap(), typed);
        let other = store
            .save_mains_answer(&MainsAnswer::new("gs3-heatwaves", at(16, 10, 0)))
            .unwrap();
        drop(store);

        // The app is killed and reopened: the words and the clock are there.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.mains_answers().unwrap(), [typed.clone(), other]);

        // Submitted, then scored with two points ticked.
        let scored = MainsAnswer {
            submitted: Some(at(16, 9, 12)),
            seconds: 702,
            points: vec![0, 3],
            scores: [
                Some(4),
                Some(3),
                Some(3),
                Some(2),
                Some(1),
                Some(3),
                Some(4),
            ],
            ..typed.clone()
        };
        store.save_mains_answer(&scored).unwrap();
        let saved = store.mains_answers().unwrap();
        assert_eq!(saved[0], scored);
        assert_eq!(saved[0].total(), Some(20));
        assert_eq!(typed.total(), None, "no total until every score is given");
        assert!(
            store
                .save_mains_answer(&MainsAnswer { id: 99, ..typed })
                .is_err(),
            "an answer that is not in the file is not quietly made"
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every row of every table, as text, to compare a file before and after
    /// an upgrade.
    fn dump(conn: &Connection) -> Vec<(String, Vec<String>)> {
        let tables: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        tables
            .into_iter()
            .map(|table| {
                let mut query = conn
                    .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                    .unwrap();
                let columns = query.column_count();
                let rows = query
                    .query_map([], |row| {
                        (0..columns)
                            .map(|ix| {
                                row.get::<_, rusqlite::types::Value>(ix)
                                    .map(|value| format!("{value:?}"))
                            })
                            .collect::<rusqlite::Result<Vec<_>>>()
                            .map(|values| values.join(" | "))
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                (table, rows)
            })
            .collect()
    }

    #[test]
    fn a_phase_5_store_takes_mains_answers_without_losing_a_row() {
        let dir = std::env::temp_dir().join(format!("civil-mentor-phase6-{}", std::process::id()));
        let path = dir.join("civil-mentor.db");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A file as Phase 5 left it, with a row in every table: real and
        // demo tests, readings, study time and review boxes, a plan, a
        // highlight, a clip and a note of the student's own.
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..7] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 7).unwrap();
        conn.execute_batch(
            "INSERT INTO setting VALUES ('diagnostic', 'taken'), ('daily_minutes', '180'),
                 ('mistakes_backfilled', '2026-11-16');
             INSERT INTO reading VALUES
                 ('fr', '2026-11-10', '2026-11-12', '2026-11-12', 1, 900, 0),
                 ('monsoon', '2026-11-01', '2026-11-03', NULL, 0, 600, 1);
             INSERT INTO attempt (id, kind, title, started, finished, seconds_allowed,
                 seconds_used, right_marks, wrong_marks, current, demo)
             VALUES (7, 'micro', 'Micro test', '2026-11-12 09:00:00', '2026-11-12 09:10:00',
                     720, 600, 2.0, 0.6666666666666666, 1, 0),
                    (8, 'mini', 'Demo', '2026-11-02 19:00:00', '2026-11-02 19:20:00',
                     1440, 1200, 2.0, 0.6666666666666666, 0, 1),
                    (9, 'micro', 'Micro test', '2026-11-16 08:00:00', NULL,
                     720, 30, 2.0, 0.6666666666666666, 0, 0);
             INSERT INTO answer (attempt, position, question, subject, topic, chosen, correct,
                 confidence, marked, struck, visited, seconds)
             VALUES (7, 0, 'polity-0001', 'polity', 'pre.gs.polity.rights', 2, 0, 'sure', 0, 0, 1, 40),
                    (7, 1, 'polity-0002', 'polity', 'pre.gs.polity.rights', 1, 1, NULL, 1, 5, 1, 20),
                    (8, 0, 'economy-0001', 'economy', 'pre.gs.economy', 0, 0, 'guess', 0, 0, 1, 30),
                    (9, 0, 'history-0001', 'history', 'pre.gs.history', NULL, NULL, NULL, 0, 0, 1, 30);
             INSERT INTO session (day, kind, item, seconds, demo)
             VALUES ('2026-11-12', 'test', '7', 600, 0), ('2026-11-02', 'test', 'demo-1', 1200, 1),
                    ('2026-11-16', 'review', 'session', 240, 0), ('2026-11-16', 'focus', '25', 900, 0);
             INSERT INTO plan_task VALUES ('2026-11-16', 0, 'review', '6', 'Review 6 mistakes', 6, 1),
                    ('2026-11-16', 1, 'note', 'fr', 'Read: fr', 30, 0);
             INSERT INTO review VALUES
                 ('question:polity-0001', 2, '2026-11-19', '2026-11-13', '2026-11-16', 0),
                 ('card:economy-repo', 5, NULL, '2026-10-01', '2026-11-15', 0),
                 ('question:economy-0001', 1, '2026-11-17', '2026-11-02', NULL, 1);
             INSERT INTO mark (item, part, kind, start, end, quote, comment, added) VALUES
                 ('fr', '0', 'highlight', 4, 30, 'Part III of the Constitution', 'Why III?', '2026-11-16'),
                 ('story:2026-11-14-ca-1', 'context', 'clip', 0, 12, 'The Cabinet', NULL, '2026-11-16');
             INSERT INTO my_note (title, body, added, updated)
             VALUES ('Writs', 'Five writs under **Article 32**.', '2026-11-15', '2026-11-16');",
        )
        .unwrap();
        let before = dump(&conn);
        drop(conn);

        let store = Store::open(&path).expect("the Phase 5 file upgrades");
        let version: u32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        let after: Vec<(String, Vec<String>)> = dump(&store.conn)
            .into_iter()
            .filter(|(table, _)| table != "mains_answer")
            .collect();
        assert_eq!(after, before, "every row of every table is as it was");
        assert!(before.iter().all(|(_, rows)| !rows.is_empty()));
        assert_eq!(store.mains_answers().unwrap(), [], "no Mains answers yet");
        assert!(store.has_demo().unwrap() && store.has_own_tests().unwrap());
        assert_eq!(store.unfinished().unwrap().len(), 1, "the test in progress");
        assert_eq!(store.reviews().unwrap().len(), 3);
        assert_eq!(store.marks().unwrap().len(), 2);
        assert_eq!(store.my_notes().unwrap()[0].title, "Writs");

        // And the new table works on the upgraded file.
        let at = NaiveDate::from_ymd_opt(2026, 11, 17)
            .unwrap()
            .and_hms_opt(7, 30, 0)
            .unwrap();
        let answer = store
            .save_mains_answer(&MainsAnswer::new("gs4-probity", at))
            .unwrap();
        assert_eq!(store.mains_answers().unwrap(), [answer]);
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reviews_are_saved_updated_and_backfilled_once() {
        let store = Store::in_memory();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let first = Review {
            level: 1,
            due: Some(day(17)),
            added: day(16),
            reviewed: None,
            demo: false,
        };
        let retired = Review {
            level: 5,
            due: None,
            added: day(1),
            reviewed: Some(day(16)),
            demo: false,
        };
        store
            .save_reviews(&[("question:a".into(), first), ("card:b".into(), retired)])
            .unwrap();
        let moved = Review {
            level: 2,
            due: Some(day(20)),
            reviewed: Some(day(17)),
            ..first
        };
        store.save_reviews(&[("question:a".into(), moved)]).unwrap();
        let reviews = store.reviews().unwrap();
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews["question:a"], moved);
        assert_eq!(reviews["card:b"], retired);

        assert!(!store.mistakes_backfilled());
        store.backfill_mistakes(day(16), &[]).unwrap();
        assert!(
            store.mistakes_backfilled(),
            "recorded even with nothing to bring in"
        );
    }

    #[test]
    fn removing_demo_history_never_touches_a_real_row() {
        let store = Store::in_memory();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();

        // The student's own: a finished test, a note read, and study time.
        let mut real = sitting(2);
        real.answers[0].chosen = Some(1);
        real.answers[0].correct = Some(true);
        real.id = store.start_attempt(&real).unwrap();
        real.finished = Some(real.started + chrono::Duration::minutes(2));
        store.finish_attempt(&real).unwrap();
        let mut fr = Reading::new(day(10));
        fr.done = Some(day(12));
        fr.seconds = 900;
        store.save_reading("fr", &fr).unwrap();
        let block = store.start_session(day(12), Study::Reading, "fr").unwrap();
        store.add_session_seconds(block, 900).unwrap();
        store.settle_diagnostic(false).unwrap();
        let mistake = Review {
            level: 3,
            due: Some(day(20)),
            added: day(5),
            reviewed: Some(day(13)),
            demo: false,
        };
        store
            .save_reviews(&[("question:q0".into(), mistake)])
            .unwrap();
        let kept = store
            .add_mark(
                NewMark {
                    item: "fr".into(),
                    part: "".into(),
                    kind: MarkKind::Highlight,
                    range: 0..9,
                    quote: "Federalism".into(),
                    comment: None,
                },
                day(12),
            )
            .unwrap();
        let mut mine = MyNote::new(day(12));
        mine.title = "My own".into();
        let mine = store.save_my_note(&mine).unwrap();
        let written = day(12).and_hms_opt(18, 0, 0).unwrap();
        let mut answer = MainsAnswer::new("gs2-governor-discretion", written);
        answer.text = "The Governor links the Union and the states.".into();
        answer.submitted = Some(written + chrono::Duration::minutes(11));
        answer.seconds = 660;
        answer.points = vec![1];
        answer.scores = [Some(3); SCORES];
        let answer = store.save_mains_answer(&answer).unwrap();
        let mut draft = MainsAnswer::new("gs3-heatwaves", written);
        draft.text = "Heatwaves are".into();
        let draft = store.save_mains_answer(&draft).unwrap();
        let mains_block = store.start_session(day(12), Study::Mains, "1").unwrap();
        store.add_session_seconds(mains_block, 660).unwrap();
        let before = (
            store.finished(100).unwrap(),
            store.readings().unwrap(),
            store.study_days().unwrap(),
            store.answered().unwrap(),
            store.reviews().unwrap(),
            store.marks().unwrap(),
            store.my_notes().unwrap(),
            store.mains_answers().unwrap(),
        );
        assert!(!store.has_demo().unwrap());

        // Demo history that overlaps it: a test, a record for the same note
        // and for another, and study on the same day.
        let mut made_up = sitting(3);
        made_up.title = "Demo".into();
        made_up.answers[2].chosen = Some(0);
        made_up.answers[2].correct = Some(false);
        made_up.finished = Some(made_up.started + chrono::Duration::minutes(3));
        let mut monsoon = Reading::new(day(8));
        monsoon.demo = true;
        let made_up_mistake = Review {
            level: 1,
            due: Some(day(14)),
            added: day(13),
            reviewed: None,
            demo: true,
        };
        let demo = Demo {
            attempts: vec![made_up],
            readings: vec![("fr".into(), monsoon), ("monsoon".into(), monsoon)],
            sessions: vec![(day(12), Study::Test, "demo-1".into(), 300)],
            reviews: vec![
                ("question:q0".into(), made_up_mistake),
                ("question:q2".into(), made_up_mistake),
            ],
        };
        store.add_demo(&demo).unwrap();
        assert!(store.has_demo().unwrap());
        assert_eq!(store.finished(100).unwrap().len(), 2);
        let readings = store.readings().unwrap();
        assert_eq!(
            readings["fr"], fr,
            "the student's own record stays as it was"
        );
        assert!(readings["monsoon"].demo);
        assert_eq!(store.study_days().unwrap()[&day(12)], 1860);
        let reviews = store.reviews().unwrap();
        assert_eq!(
            reviews["question:q0"], mistake,
            "the student's own box stays"
        );
        assert!(reviews["question:q2"].demo);

        store.remove_demo().unwrap();
        let after = (
            store.finished(100).unwrap(),
            store.readings().unwrap(),
            store.study_days().unwrap(),
            store.answered().unwrap(),
            store.reviews().unwrap(),
            store.marks().unwrap(),
            store.my_notes().unwrap(),
            store.mains_answers().unwrap(),
        );
        assert_eq!(after, before, "every real row is back exactly");
        assert_eq!(after.5, [kept], "the highlight is untouched");
        assert_eq!(
            after.6,
            [mine],
            "the note of the student's own is untouched"
        );
        assert_eq!(
            after.7,
            [answer, draft],
            "the student's Mains answer and draft are untouched"
        );
        assert!(!store.has_demo().unwrap());
        assert!(store.has_own_tests().unwrap());
        assert!(store.diagnostic_settled());
        let orphans: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM answer WHERE attempt NOT IN (SELECT id FROM attempt)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0, "the demo test's answers went with it");
    }

    #[test]
    fn a_file_from_a_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", (MIGRATIONS.len() + 1) as u32)
            .unwrap();
        let err = Store::prepare(conn, None).err().expect("must not open");
        assert!(err.to_string().contains("newer version"), "{err}");
    }
}
